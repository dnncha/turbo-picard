#![forbid(unsafe_code)]

use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::record::Aux;
use rust_htslib::bam::{self, Read, index};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;
use turbo_picard_core::hts_io;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;

const DUPLICATE_FLAG: u16 = 0x400;
const UNMAPPED_FLAG: u16 = 0x4;
type LibraryId = u32;
type InternedBytesId = u32;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkDuplicatesSummary {
    pub library: String,
    pub unpaired_reads_examined: u64,
    pub read_pairs_examined: u64,
    paired_records_examined: u64,
    pub secondary_or_supplementary_records: u64,
    pub unpaired_duplicate_records: u64,
    pub duplicate_pair_records: u64,
    pub read_pair_optical_duplicates: u64,
    pub unmapped_records: u64,
    duplicate_set_histogram: BTreeMap<u64, DuplicateSetCounts>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DuplicateSetCounts {
    all_sets: u64,
    non_optical_sets: u64,
}

#[derive(Debug)]
pub enum MarkDuplicatesError {
    UnsupportedInputFormat(String),
    Io(std::io::Error),
    Htslib(rust_htslib::errors::Error),
    Operation(String),
    MalformedSam { line_number: usize, reason: String },
}

impl fmt::Display for MarkDuplicatesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedInputFormat(path) => write!(
                f,
                "unsupported MarkDuplicates input format for {path}; this engine supports BAM inputs and single SAM text input"
            ),
            Self::Io(error) => write!(f, "{error}"),
            Self::Htslib(error) => write!(f, "{error}"),
            Self::Operation(message) => write!(f, "{message}"),
            Self::MalformedSam {
                line_number,
                reason,
            } => write!(f, "malformed SAM at line {line_number}: {reason}"),
        }
    }
}

impl std::error::Error for MarkDuplicatesError {}

impl From<std::io::Error> for MarkDuplicatesError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rust_htslib::errors::Error> for MarkDuplicatesError {
    fn from(value: rust_htslib::errors::Error) -> Self {
        Self::Htslib(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DuplicateKey {
    reference_name: String,
    position: i64,
    mate_reference_name: String,
    mate_position: i64,
    template_length: i64,
    reverse_strand: bool,
    barcode: Option<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct BamDuplicateKey {
    library_id: LibraryId,
    reference_id: i32,
    position: i64,
    mate_reference_id: i32,
    mate_position: i64,
    template_length: i64,
    reverse_strand: bool,
    barcode_id: Option<InternedBytesId>,
}

#[derive(Debug, Clone)]
struct DuplicateCandidate {
    record_index: usize,
    library_id: LibraryId,
    qname_id: InternedBytesId,
    flags: u16,
    reference_id: i32,
    five_prime_position: i64,
    _mate_reference_id: i32,
    _mate_position: i64,
    _template_length: i64,
    duplicate_score: u64,
    optical_location: Option<ReadLocation>,
    barcode_id: Option<InternedBytesId>,
}

impl DuplicateCandidate {
    fn from_record(
        record_index: usize,
        record: &bam::Record,
        library_id: LibraryId,
        qname_id: InternedBytesId,
        barcode_id: Option<InternedBytesId>,
    ) -> Self {
        Self {
            record_index,
            library_id,
            qname_id,
            flags: record.flags(),
            reference_id: record.tid(),
            five_prime_position: unclipped_record_position(record),
            _mate_reference_id: record.mtid(),
            _mate_position: record.mpos(),
            _template_length: record.insert_size(),
            duplicate_score: quality_score(record),
            optical_location: parse_read_location(record.qname()),
            barcode_id,
        }
    }

    fn is_pair(&self) -> bool {
        duplicate_candidate_is_pair(self.flags)
    }

    fn reverse_strand(&self) -> bool {
        self.flags & 0x10 != 0
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RecordDecision {
    duplicate: bool,
    optical_duplicate: bool,
    duplicate_set_size: Option<i32>,
    duplicate_set_index: Option<i32>,
}

#[derive(Debug, Default)]
struct ByteInterner {
    ids: HashMap<Vec<u8>, InternedBytesId>,
    values: Vec<Vec<u8>>,
}

impl ByteInterner {
    fn intern(&mut self, value: &[u8]) -> InternedBytesId {
        if let Some(id) = self.ids.get(value) {
            return *id;
        }
        let id = InternedBytesId::try_from(self.values.len()).expect("interned id fits in u32");
        let owned = value.to_vec();
        self.values.push(owned.clone());
        self.ids.insert(owned, id);
        id
    }
}

pub fn run(config: &MarkDuplicatesConfig) -> Result<MarkDuplicatesSummary, MarkDuplicatesError> {
    if config
        .inputs
        .iter()
        .all(|input| hts_io::is_hts_container_input(input))
    {
        return run_hts_container(config);
    }

    if config.inputs.len() > 1 {
        return Err(MarkDuplicatesError::UnsupportedInputFormat(
            config.inputs.join(","),
        ));
    }
    ensure_sam_input(&config.input)?;

    let input = fs::read_to_string(&config.input)?;
    let mut seen = HashMap::<DuplicateKey, usize>::default();
    let mut output = String::with_capacity(input.len());
    let mut summary = MarkDuplicatesSummary {
        library: "Unknown Library".to_string(),
        unpaired_reads_examined: 0,
        read_pairs_examined: 0,
        paired_records_examined: 0,
        secondary_or_supplementary_records: 0,
        unpaired_duplicate_records: 0,
        duplicate_pair_records: 0,
        read_pair_optical_duplicates: 0,
        unmapped_records: 0,
        duplicate_set_histogram: BTreeMap::new(),
    };

    for (line_index, line) in input.lines().enumerate() {
        let line_number = line_index + 1;
        if line.starts_with('@') {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        let mut fields = line.split('\t').map(str::to_string).collect::<Vec<_>>();
        if fields.len() < 11 {
            return Err(MarkDuplicatesError::MalformedSam {
                line_number,
                reason: "expected at least 11 tab-delimited fields".to_string(),
            });
        }

        let mut flag = fields[1]
            .parse::<u16>()
            .map_err(|_| MarkDuplicatesError::MalformedSam {
                line_number,
                reason: format!("invalid FLAG value: {}", fields[1]),
            })?;

        if flag & UNMAPPED_FLAG != 0 {
            summary.unmapped_records += 1;
            output.push_str(line);
            output.push('\n');
            continue;
        }
        if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
            summary.secondary_or_supplementary_records += 1;
            output.push_str(line);
            output.push('\n');
            continue;
        }

        if duplicate_candidate_is_pair(flag) {
            summary.paired_records_examined += 1;
            if flag & FIRST_IN_PAIR_FLAG != 0 {
                summary.read_pairs_examined += 1;
            }
        } else {
            summary.unpaired_reads_examined += 1;
        }
        let key = duplicate_key(&fields, flag, line_number, config)?;
        let seen_count = seen.entry(key).or_insert(0);
        let duplicate = *seen_count > 0;
        *seen_count += 1;

        if duplicate {
            if duplicate_candidate_is_pair(flag) {
                summary.duplicate_pair_records += 1;
            } else {
                summary.unpaired_duplicate_records += 1;
            }
            flag |= DUPLICATE_FLAG;
            fields[1] = flag.to_string();
        }

        if !(duplicate && config.remove_duplicates) {
            if config.tagging_policy.as_deref() == Some("All") && flag & DUPLICATE_FLAG != 0 {
                add_duplicate_type_tag_to_sam_fields(&mut fields);
            }
            if config.add_pg_tag_to_reads {
                add_program_group_to_sam_fields(&mut fields);
            }
            output.push_str(&fields.join("\t"));
            output.push('\n');
        }
    }

    if config.add_pg_tag_to_reads {
        add_program_group_to_sam_header(&mut output);
    }
    fs::write(&config.output, output)?;
    fs::write(&config.metrics_file, metrics_text(&summary))?;
    Ok(summary)
}

fn ensure_sam_input(input: &str) -> Result<(), MarkDuplicatesError> {
    if Path::new(input)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("sam"))
    {
        Ok(())
    } else {
        Err(MarkDuplicatesError::UnsupportedInputFormat(
            input.to_string(),
        ))
    }
}

fn markdup_reference(config: &MarkDuplicatesConfig) -> Option<&str> {
    config.reference_sequence.as_deref()
}

fn open_markdup_reader(
    config: &MarkDuplicatesConfig,
    path: &str,
) -> Result<bam::Reader, MarkDuplicatesError> {
    hts_io::open_reader(path, markdup_reference(config)).map_err(MarkDuplicatesError::Operation)
}

fn open_markdup_writer(
    config: &MarkDuplicatesConfig,
    output: &str,
    header: &bam::Header,
) -> Result<bam::Writer, MarkDuplicatesError> {
    let format =
        hts_io::writer_format_for_output(output).map_err(MarkDuplicatesError::Operation)?;
    hts_io::open_writer(
        output,
        header,
        format,
        markdup_reference(config),
        config.compression_level,
    )
    .map_err(MarkDuplicatesError::Operation)
}

fn run_hts_container(
    config: &MarkDuplicatesConfig,
) -> Result<MarkDuplicatesSummary, MarkDuplicatesError> {
    let first_input = &config.inputs[0];
    let mut reader = open_markdup_reader(config, first_input)?;
    let mut library_registry = LibraryRegistry::new();
    let first_library_lookup = library_lookup(reader.header(), &mut library_registry);
    let library = library_registry
        .summary(first_library_lookup.first_library_id)
        .library
        .clone();
    let mut header = bam::Header::from_template(reader.header());
    let mut known_read_groups = read_group_ids(reader.header());
    for input in config.inputs.iter().skip(1) {
        let reader = open_markdup_reader(config, input)?;
        append_missing_read_groups(&mut header, reader.header(), &mut known_read_groups);
    }
    if config.add_pg_tag_to_reads {
        push_markdup_pg_header_if_needed(&mut header);
    }
    let mut writer = open_markdup_writer(config, &config.output, &header)?;
    let mut records = Vec::new();
    let mut candidates = Vec::new();
    let mut qnames = ByteInterner::default();
    let mut barcodes = ByteInterner::default();
    let mut summary = MarkDuplicatesSummary {
        library,
        unpaired_reads_examined: 0,
        read_pairs_examined: 0,
        paired_records_examined: 0,
        secondary_or_supplementary_records: 0,
        unpaired_duplicate_records: 0,
        duplicate_pair_records: 0,
        read_pair_optical_duplicates: 0,
        unmapped_records: 0,
        duplicate_set_histogram: BTreeMap::new(),
    };

    read_bam_records(
        &mut reader,
        &mut BamRecordSink {
            records: &mut records,
            candidates: &mut candidates,
            qnames: &mut qnames,
            barcodes: &mut barcodes,
        },
        &first_library_lookup,
        &mut library_registry,
        &mut summary,
        config,
    )?;
    for input in config.inputs.iter().skip(1) {
        let mut reader = open_markdup_reader(config, input)?;
        let input_library_lookup = library_lookup(reader.header(), &mut library_registry);
        read_bam_records(
            &mut reader,
            &mut BamRecordSink {
                records: &mut records,
                candidates: &mut candidates,
                qnames: &mut qnames,
                barcodes: &mut barcodes,
            },
            &input_library_lookup,
            &mut library_registry,
            &mut summary,
            config,
        )?;
    }

    let duplicate_groups = duplicate_groups(&candidates, config.max_records_in_ram)?;
    let mut decisions = vec![RecordDecision::default(); records.len()];

    for group in &duplicate_groups {
        if group.len() < 2 {
            continue;
        }
        let paired_set_size = paired_duplicate_set_size(group, &candidates);
        if !has_multiple_read_names(group, &candidates) {
            if let Some(set_size) = paired_set_size {
                add_duplicate_set(&mut summary, set_size, Some(set_size));
                if let Some(candidate_index) = group.first() {
                    add_duplicate_set(
                        library_registry.summary_mut(candidates[*candidate_index].library_id),
                        set_size,
                        Some(set_size),
                    );
                }
            }
            continue;
        }

        let representative_candidate_index =
            best_duplicate_representative_index(group, &candidates);
        let representative_qname_id = candidates[representative_candidate_index].qname_id;
        let optical_duplicates =
            optical_duplicate_record_indices(group, &candidates, representative_qname_id, config);
        if let Some(set_size) = paired_set_size {
            let optical_names = u64::try_from(optical_duplicates.read_names).unwrap_or(u64::MAX);
            let non_optical_size = (optical_names < set_size).then_some(set_size - optical_names);
            add_duplicate_set(&mut summary, set_size, non_optical_size);
            if let Some(candidate_index) = group.first() {
                let library_summary =
                    library_registry.summary_mut(candidates[*candidate_index].library_id);
                add_duplicate_set(library_summary, set_size, non_optical_size);
            }
        }
        summary.read_pair_optical_duplicates += optical_duplicates.read_names as u64;
        if let Some(candidate_index) = group.first() {
            library_registry
                .summary_mut(candidates[*candidate_index].library_id)
                .read_pair_optical_duplicates += optical_duplicates.read_names as u64;
        }
        for index in optical_duplicates.record_indices {
            decisions[index].optical_duplicate = true;
        }
        if config.tag_duplicate_set_members && !config.remove_duplicates {
            add_duplicate_set_member_tags(
                group,
                &candidates,
                &mut decisions,
                representative_qname_id,
            );
        }

        for candidate_index in group.iter().copied() {
            if candidates[candidate_index].qname_id == representative_qname_id {
                continue;
            }
            let candidate = &candidates[candidate_index];
            let index = candidate.record_index;
            if candidate.is_pair() {
                summary.duplicate_pair_records += 1;
                library_registry
                    .summary_mut(candidate.library_id)
                    .duplicate_pair_records += 1;
            } else {
                summary.unpaired_duplicate_records += 1;
                library_registry
                    .summary_mut(candidate.library_id)
                    .unpaired_duplicate_records += 1;
            }
            decisions[index].duplicate = true;
        }
    }
    mark_fragment_duplicate_groups(
        &candidates,
        &mut decisions,
        &mut summary,
        &mut library_registry,
    );

    {
        if config.inputs.len() > 1 {
            let mut marked_records = records.into_iter().zip(decisions).collect::<Vec<_>>();
            marked_records.sort_by(|(left, left_decision), (right, right_decision)| {
                compare_bam_output_order(left, *left_decision, right, *right_decision)
            });
            write_bam_records(marked_records, config, &mut writer)?;
        } else {
            write_bam_records(records.into_iter().zip(decisions), config, &mut writer)?;
        }
    }
    drop(writer);

    let mut library_summaries = library_registry.summary_refs();
    library_summaries.sort_by(|left, right| left.library.cmp(&right.library));
    fs::write(
        &config.metrics_file,
        metrics_text_for_libraries(library_summaries),
    )?;
    if config.create_md5_file {
        write_md5_sidecar(&config.output)?;
    }
    if config.create_index {
        index::build(
            &config.output,
            Some(&picard_bai_path(&config.output)),
            index::Type::Bai,
            turbo_picard_core::bgzf_threads::htslib_worker_threads(),
        )?;
    }
    Ok(summary)
}

struct BamRecordSink<'a> {
    records: &'a mut Vec<bam::Record>,
    candidates: &'a mut Vec<DuplicateCandidate>,
    qnames: &'a mut ByteInterner,
    barcodes: &'a mut ByteInterner,
}

fn read_bam_records<R: bam::Read>(
    reader: &mut R,
    sink: &mut BamRecordSink<'_>,
    library_lookup: &LibraryLookup,
    library_registry: &mut LibraryRegistry,
    summary: &mut MarkDuplicatesSummary,
    config: &MarkDuplicatesConfig,
) -> Result<(), MarkDuplicatesError> {
    for result in reader.records() {
        let mut record = result?;
        let flag = record.flags() & !DUPLICATE_FLAG;
        if record.flags() != flag {
            record.set_flags(flag);
        }
        let library_id = record_library_id(&record, library_lookup);
        let record_index = sink.records.len();

        if flag & UNMAPPED_FLAG != 0 {
            summary.unmapped_records += 1;
            library_registry.summary_mut(library_id).unmapped_records += 1;
            sink.records.push(record);
            continue;
        }
        if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
            summary.secondary_or_supplementary_records += 1;
            library_registry
                .summary_mut(library_id)
                .secondary_or_supplementary_records += 1;
            sink.records.push(record);
            continue;
        }

        if duplicate_candidate_is_pair(flag) {
            summary.paired_records_examined += 1;
            library_registry
                .summary_mut(library_id)
                .paired_records_examined += 1;
            if flag & FIRST_IN_PAIR_FLAG != 0 {
                summary.read_pairs_examined += 1;
                library_registry.summary_mut(library_id).read_pairs_examined += 1;
            }
        } else {
            summary.unpaired_reads_examined += 1;
            library_registry
                .summary_mut(library_id)
                .unpaired_reads_examined += 1;
        }
        let qname_id = sink.qnames.intern(record.qname());
        let barcode_id = bam_barcode(&record, config).map(|barcode| sink.barcodes.intern(&barcode));
        sink.candidates.push(DuplicateCandidate::from_record(
            record_index,
            &record,
            library_id,
            qname_id,
            barcode_id,
        ));
        sink.records.push(record);
    }

    Ok(())
}

fn compare_bam_output_order(
    left: &bam::Record,
    left_decision: RecordDecision,
    right: &bam::Record,
    right_decision: RecordDecision,
) -> Ordering {
    left.tid()
        .cmp(&right.tid())
        .then_with(|| left.pos().cmp(&right.pos()))
        .then_with(|| left.qname().cmp(right.qname()))
        .then_with(|| {
            effective_record_flags(left, left_decision)
                .cmp(&effective_record_flags(right, right_decision))
        })
}

fn effective_record_flags(record: &bam::Record, decision: RecordDecision) -> u16 {
    if decision.duplicate {
        record.flags() | DUPLICATE_FLAG
    } else {
        record.flags()
    }
}

fn write_bam_records(
    records: impl IntoIterator<Item = (bam::Record, RecordDecision)>,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    for (record, decision) in records {
        write_bam_record(record, decision, config, writer)?;
    }
    Ok(())
}

fn write_bam_record(
    mut record: bam::Record,
    decision: RecordDecision,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    let flags = effective_record_flags(&record, decision);
    if (config.remove_duplicates && decision.duplicate)
        || (config.remove_sequencing_duplicates && decision.optical_duplicate)
    {
        return Ok(());
    }
    if flags != record.flags() {
        record.set_flags(flags);
    }
    if config.clear_dt {
        clear_duplicate_type_tag(&mut record)?;
    }
    if let Some(duplicate_type) =
        duplicate_type_tag(config, record.flags(), decision.optical_duplicate)
    {
        add_duplicate_type_tag(&mut record, duplicate_type)?;
    }
    if let Some(duplicate_set_size) = decision.duplicate_set_size {
        replace_i32_aux(&mut record, b"DS", duplicate_set_size)?;
    }
    if let Some(duplicate_set_index) = decision.duplicate_set_index {
        replace_i32_aux(&mut record, b"DI", duplicate_set_index)?;
    }
    if config.add_pg_tag_to_reads {
        add_program_group_to_bam_record(&mut record)?;
    }
    writer.write(&record)?;
    Ok(())
}

fn clear_duplicate_type_tag(record: &mut bam::Record) -> Result<(), MarkDuplicatesError> {
    if record.aux(b"DT").is_ok() {
        record.remove_aux(b"DT")?;
    }
    Ok(())
}

fn duplicate_type_tag(
    config: &MarkDuplicatesConfig,
    flags: u16,
    is_optical_duplicate: bool,
) -> Option<&str> {
    if flags & DUPLICATE_FLAG == 0 {
        return None;
    }
    if is_optical_duplicate {
        return match config.tagging_policy.as_deref() {
            Some("All" | "OpticalOnly") => Some("SQ"),
            _ => None,
        };
    }
    if config.tagging_policy.as_deref() == Some("All") {
        Some("LB")
    } else {
        None
    }
}

fn add_duplicate_type_tag(
    record: &mut bam::Record,
    duplicate_type: &str,
) -> Result<(), MarkDuplicatesError> {
    if record.aux(b"DT").is_ok() {
        record.remove_aux(b"DT")?;
    }
    record.push_aux(b"DT", Aux::String(duplicate_type))?;
    Ok(())
}

struct OpticalDuplicateRecords {
    read_names: usize,
    record_indices: Vec<usize>,
}

fn optical_duplicate_record_indices(
    group: &[usize],
    candidates: &[DuplicateCandidate],
    representative_qname_id: InternedBytesId,
    config: &MarkDuplicatesConfig,
) -> OpticalDuplicateRecords {
    if config.read_name_regex.as_deref() == Some("null") {
        return OpticalDuplicateRecords {
            read_names: 0,
            record_indices: Vec::new(),
        };
    }
    let Some(representative_location) = group
        .iter()
        .find(|index| candidates[**index].qname_id == representative_qname_id)
        .and_then(|index| candidates[*index].optical_location)
    else {
        return OpticalDuplicateRecords {
            read_names: 0,
            record_indices: Vec::new(),
        };
    };
    let pixel_distance = i64::from(config.optical_duplicate_pixel_distance.unwrap_or(100));
    let mut optical_names = HashSet::<InternedBytesId>::default();
    let mut record_indices = Vec::<usize>::new();

    for index in group.iter().copied() {
        let candidate = &candidates[index];
        if candidate.qname_id == representative_qname_id {
            continue;
        }
        if optical_names.contains(&candidate.qname_id) {
            record_indices.push(candidate.record_index);
            continue;
        }
        let Some(location) = candidate.optical_location else {
            continue;
        };
        if representative_location.is_within(&location, pixel_distance) {
            optical_names.insert(candidate.qname_id);
            record_indices.push(candidate.record_index);
        }
    }

    OpticalDuplicateRecords {
        read_names: optical_names.len(),
        record_indices,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReadLocation {
    tile: i64,
    x: i64,
    y: i64,
}

impl ReadLocation {
    fn is_within(&self, other: &Self, pixel_distance: i64) -> bool {
        self.tile == other.tile
            && (self.x - other.x).abs() <= pixel_distance
            && (self.y - other.y).abs() <= pixel_distance
    }
}

fn parse_read_location(name: &[u8]) -> Option<ReadLocation> {
    let text = std::str::from_utf8(name).ok()?;
    let mut parts = text.rsplit(':');
    let y = parts.next()?.parse::<i64>().ok()?;
    let x = parts.next()?.parse::<i64>().ok()?;
    let tile = parts.next()?.parse::<i64>().ok()?;
    Some(ReadLocation { tile, x, y })
}

fn add_duplicate_set_member_tags(
    group: &[usize],
    candidates: &[DuplicateCandidate],
    decisions: &mut [RecordDecision],
    representative_qname_id: InternedBytesId,
) {
    if !group.iter().any(|index| candidates[*index].is_pair()) {
        return;
    }

    let mut member_names = HashSet::<InternedBytesId>::default();
    for index in group.iter().copied() {
        member_names.insert(candidates[index].qname_id);
    }
    if member_names.len() < 2 {
        return;
    }

    let duplicate_set_index = group
        .iter()
        .copied()
        .filter(|index| candidates[*index].qname_id == representative_qname_id)
        .map(|index| candidates[index].record_index)
        .min()
        .unwrap_or(candidates[group[0]].record_index);
    let duplicate_set_size = i32::try_from(member_names.len()).unwrap_or(i32::MAX);
    let duplicate_set_index = i32::try_from(duplicate_set_index).unwrap_or(i32::MAX);

    for index in group.iter().copied() {
        let record_index = candidates[index].record_index;
        decisions[record_index].duplicate_set_size = Some(duplicate_set_size);
        decisions[record_index].duplicate_set_index = Some(duplicate_set_index);
    }
}

fn replace_i32_aux(
    record: &mut bam::Record,
    tag: &[u8],
    value: i32,
) -> Result<(), MarkDuplicatesError> {
    if record.aux(tag).is_ok() {
        record.remove_aux(tag)?;
    }
    record.push_aux(tag, Aux::I32(value))?;
    Ok(())
}

fn add_duplicate_type_tag_to_sam_fields(fields: &mut Vec<String>) {
    fields.retain(|field| !field.starts_with("DT:Z:"));
    fields.push("DT:Z:LB".to_string());
}

fn add_program_group_to_bam_record(record: &mut bam::Record) -> Result<(), MarkDuplicatesError> {
    if record.aux(b"PG").is_ok() {
        record.remove_aux(b"PG")?;
    }
    record.push_aux(b"PG", Aux::String("MarkDuplicates"))?;
    Ok(())
}

fn add_program_group_to_sam_fields(fields: &mut Vec<String>) {
    fields.retain(|field| !field.starts_with("PG:Z:"));
    fields.push("PG:Z:MarkDuplicates".to_string());
}

fn add_program_group_to_sam_header(output: &mut String) {
    if output
        .lines()
        .any(|line| line.starts_with("@PG") && line.contains("ID:MarkDuplicates"))
    {
        return;
    }
    let insert_at = output
        .lines()
        .take_while(|line| line.starts_with('@'))
        .map(|line| line.len() + 1)
        .sum::<usize>();
    output.insert_str(insert_at, "@PG\tID:MarkDuplicates\tPN:MarkDuplicates\n");
}

fn push_markdup_pg_header_if_needed(header: &mut bam::Header) {
    let header_bytes = header.to_bytes();
    let header_text = String::from_utf8_lossy(&header_bytes);
    if header_text
        .lines()
        .any(|line| line.starts_with("@PG") && line.contains("ID:MarkDuplicates"))
    {
        return;
    }
    header.push_record(
        HeaderRecord::new(b"PG")
            .push_tag(b"ID", "MarkDuplicates")
            .push_tag(b"PN", "MarkDuplicates"),
    );
}

fn picard_bai_path(output: &str) -> String {
    Path::new(output)
        .with_extension("bai")
        .display()
        .to_string()
}

fn write_md5_sidecar(output: &str) -> Result<(), MarkDuplicatesError> {
    let bytes = fs::read(output)?;
    let digest = md5::compute(bytes);
    fs::write(format!("{output}.md5"), format!("{digest:x}"))?;
    Ok(())
}

fn duplicate_key(
    fields: &[String],
    flag: u16,
    line_number: usize,
    config: &MarkDuplicatesConfig,
) -> Result<DuplicateKey, MarkDuplicatesError> {
    let reverse_strand = flag & 0x10 != 0;
    let position = parse_sam_integer(&fields[3], "POS", line_number)? - 1;
    let mate_position = parse_sam_integer(&fields[7], "MATE_POS", line_number)?;
    let template_length = parse_sam_integer(&fields[8], "TLEN", line_number)?;
    Ok(DuplicateKey {
        reference_name: fields[2].clone(),
        position: unclipped_five_prime_position(position, &fields[5], reverse_strand, line_number)?,
        mate_reference_name: fields[6].clone(),
        mate_position,
        template_length,
        reverse_strand,
        barcode: sam_barcode(fields, config),
    })
}

fn parse_sam_integer(
    value: &str,
    field_name: &str,
    line_number: usize,
) -> Result<i64, MarkDuplicatesError> {
    value
        .parse::<i64>()
        .map_err(|_| MarkDuplicatesError::MalformedSam {
            line_number,
            reason: format!("invalid {field_name} value: {value}"),
        })
}

fn duplicate_groups(
    candidates: &[DuplicateCandidate],
    max_displaced_pair_records: usize,
) -> Result<Vec<Vec<usize>>, MarkDuplicatesError> {
    let mut keyed_pairs = collate_pair_key_rows(candidates, max_displaced_pair_records)?;
    keyed_pairs.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(collect_sorted_pair_groups(keyed_pairs))
}

fn collate_pair_key_rows(
    candidates: &[DuplicateCandidate],
    max_displaced_pair_records: usize,
) -> Result<Vec<(BamDuplicateKey, [usize; 2])>, MarkDuplicatesError> {
    let mut paired_by_name = HashMap::<InternedBytesId, usize>::default();
    let mut keyed_pairs = Vec::<(BamDuplicateKey, [usize; 2])>::new();
    let mut candidate_index = 0usize;
    let max_displaced_pair_records = max_displaced_pair_records.max(1);

    while candidate_index < candidates.len() {
        if !candidates[candidate_index].is_pair() {
            candidate_index += 1;
            continue;
        }
        let qname_id = candidates[candidate_index].qname_id;
        let mut run_end = candidate_index + 1;
        while run_end < candidates.len() && candidates[run_end].qname_id == qname_id {
            run_end += 1;
        }
        if run_end > candidate_index + 1 && !paired_by_name.contains_key(&qname_id) {
            if let Some(unpaired_index) =
                collate_adjacent_pair_run(candidate_index..run_end, candidates, &mut keyed_pairs)
            {
                insert_pending_pair_candidate(
                    qname_id,
                    unpaired_index,
                    &mut paired_by_name,
                    max_displaced_pair_records,
                )?;
            }
        } else {
            for index in candidate_index..run_end {
                if candidates[index].is_pair() {
                    collate_displaced_pair_candidate(
                        index,
                        candidates,
                        &mut paired_by_name,
                        &mut keyed_pairs,
                        max_displaced_pair_records,
                    )?;
                }
            }
        }
        candidate_index = run_end;
    }

    Ok(keyed_pairs)
}

fn collate_adjacent_pair_run(
    candidate_indices: std::ops::Range<usize>,
    candidates: &[DuplicateCandidate],
    keyed_pairs: &mut Vec<(BamDuplicateKey, [usize; 2])>,
) -> Option<usize> {
    let mut first_index = None::<usize>;
    for candidate_index in candidate_indices {
        if !candidates[candidate_index].is_pair() {
            continue;
        }
        if let Some(first) = first_index.take() {
            push_pair_key_row(first, candidate_index, candidates, keyed_pairs);
        } else {
            first_index = Some(candidate_index);
        }
    }
    first_index
}

fn collate_displaced_pair_candidate(
    candidate_index: usize,
    candidates: &[DuplicateCandidate],
    paired_by_name: &mut HashMap<InternedBytesId, usize>,
    keyed_pairs: &mut Vec<(BamDuplicateKey, [usize; 2])>,
    max_displaced_pair_records: usize,
) -> Result<(), MarkDuplicatesError> {
    let candidate = &candidates[candidate_index];
    if let Some(first_index) = paired_by_name.remove(&candidate.qname_id) {
        push_pair_key_row(first_index, candidate_index, candidates, keyed_pairs);
    } else {
        insert_pending_pair_candidate(
            candidate.qname_id,
            candidate_index,
            paired_by_name,
            max_displaced_pair_records,
        )?;
    }
    Ok(())
}

fn insert_pending_pair_candidate(
    qname_id: InternedBytesId,
    candidate_index: usize,
    paired_by_name: &mut HashMap<InternedBytesId, usize>,
    max_displaced_pair_records: usize,
) -> Result<(), MarkDuplicatesError> {
    if paired_by_name.len() >= max_displaced_pair_records {
        return Err(MarkDuplicatesError::Operation(format!(
            "MarkDuplicates displaced pair cache exceeded MAX_RECORDS_IN_RAM={max_displaced_pair_records}; external qname collation is not implemented yet"
        )));
    }
    paired_by_name.insert(qname_id, candidate_index);
    Ok(())
}

fn push_pair_key_row(
    first_index: usize,
    second_index: usize,
    candidates: &[DuplicateCandidate],
    keyed_pairs: &mut Vec<(BamDuplicateKey, [usize; 2])>,
) {
    let candidate_indices = [first_index, second_index];
    let barcode = first_barcode(candidates, &candidate_indices);
    let key = pair_duplicate_key_bam(
        &candidates[first_index],
        &candidates[second_index],
        candidates[first_index].library_id,
        barcode,
    );
    keyed_pairs.push((key, candidate_indices));
}

#[cfg(test)]
fn collate_pair_key_rows_legacy(
    candidates: &[DuplicateCandidate],
) -> Vec<(BamDuplicateKey, [usize; 2])> {
    let mut paired_by_name = HashMap::<InternedBytesId, usize>::default();
    let mut keyed_pairs = Vec::<(BamDuplicateKey, [usize; 2])>::new();

    for (candidate_index, candidate) in candidates.iter().enumerate() {
        if candidate.is_pair() {
            if let Some(first_index) = paired_by_name.remove(&candidate.qname_id) {
                push_pair_key_row(first_index, candidate_index, candidates, &mut keyed_pairs);
            } else {
                paired_by_name.insert(candidate.qname_id, candidate_index);
            }
        }
    }

    keyed_pairs
}

fn collect_sorted_pair_groups(keyed_pairs: Vec<(BamDuplicateKey, [usize; 2])>) -> Vec<Vec<usize>> {
    let mut groups = Vec::<Vec<usize>>::new();
    let mut current_key = None::<BamDuplicateKey>;

    for (key, pair_indices) in keyed_pairs {
        if current_key.as_ref() == Some(&key) {
            groups
                .last_mut()
                .expect("current key has a group")
                .extend(pair_indices);
        } else {
            groups.push(pair_indices.into());
            current_key = Some(key);
        }
    }

    groups
}

fn fragment_duplicate_groups(candidates: &[DuplicateCandidate]) -> Vec<Vec<usize>> {
    let mut keyed_fragments = candidates
        .iter()
        .enumerate()
        .map(|(candidate_index, candidate)| {
            (fragment_duplicate_key_bam(candidate), candidate_index)
        })
        .collect::<Vec<_>>();
    keyed_fragments.sort_by(|left, right| left.0.cmp(&right.0));
    collect_sorted_fragment_groups(keyed_fragments)
}

fn collect_sorted_fragment_groups(
    keyed_fragments: Vec<(BamDuplicateKey, usize)>,
) -> Vec<Vec<usize>> {
    let mut groups = Vec::<Vec<usize>>::new();
    let mut current_key = None::<BamDuplicateKey>;

    for (key, candidate_index) in keyed_fragments {
        if current_key.as_ref() == Some(&key) {
            groups
                .last_mut()
                .expect("current key has a group")
                .push(candidate_index);
        } else {
            groups.push(vec![candidate_index]);
            current_key = Some(key);
        }
    }

    groups
}

fn mark_fragment_duplicate_groups(
    candidates: &[DuplicateCandidate],
    decisions: &mut [RecordDecision],
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
) {
    let fragment_groups = fragment_duplicate_groups(candidates);

    for group in &fragment_groups {
        if group.len() < 2 || !has_multiple_read_names(group, candidates) {
            continue;
        }

        let contains_complete_pair = group
            .iter()
            .any(|candidate_index| candidates[*candidate_index].is_pair());
        if contains_complete_pair {
            for candidate_index in group.iter().copied() {
                if candidates[candidate_index].is_pair() {
                    continue;
                }
                mark_unpaired_duplicate_record(
                    candidate_index,
                    candidates,
                    decisions,
                    summary,
                    library_registry,
                );
            }
            continue;
        }

        let representative_index = best_duplicate_representative_index(group, candidates);
        let representative_qname_id = candidates[representative_index].qname_id;
        for candidate_index in group.iter().copied() {
            if candidates[candidate_index].qname_id == representative_qname_id {
                continue;
            }
            mark_unpaired_duplicate_record(
                candidate_index,
                candidates,
                decisions,
                summary,
                library_registry,
            );
        }
    }
}

fn mark_unpaired_duplicate_record(
    candidate_index: usize,
    candidates: &[DuplicateCandidate],
    decisions: &mut [RecordDecision],
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
) {
    let candidate = &candidates[candidate_index];
    let record_index = candidate.record_index;
    if decisions[record_index].duplicate {
        return;
    }
    summary.unpaired_duplicate_records += 1;
    library_registry
        .summary_mut(candidate.library_id)
        .unpaired_duplicate_records += 1;
    decisions[record_index].duplicate = true;
}

fn sam_barcode(fields: &[String], config: &MarkDuplicatesConfig) -> Option<Vec<u8>> {
    if let Some(tag) = config.barcode_tag.as_deref() {
        return sam_tag_value(fields, tag);
    }

    combined_barcode(
        config
            .read_one_barcode_tag
            .as_deref()
            .and_then(|tag| sam_tag_value(fields, tag)),
        config
            .read_two_barcode_tag
            .as_deref()
            .and_then(|tag| sam_tag_value(fields, tag)),
    )
}

fn sam_tag_value(fields: &[String], tag: &str) -> Option<Vec<u8>> {
    let prefix = format!("{tag}:Z:");
    fields.iter().skip(11).find_map(|field| {
        field
            .strip_prefix(&prefix)
            .map(|value| value.as_bytes().to_vec())
    })
}

fn first_barcode(candidates: &[DuplicateCandidate], indices: &[usize]) -> Option<InternedBytesId> {
    indices
        .iter()
        .find_map(|index| candidates[*index].barcode_id)
}

fn bam_barcode(record: &bam::Record, config: &MarkDuplicatesConfig) -> Option<Vec<u8>> {
    if let Some(tag) = config.barcode_tag.as_deref() {
        return bam_tag_value(record, tag);
    }

    combined_barcode(
        config
            .read_one_barcode_tag
            .as_deref()
            .and_then(|tag| bam_tag_value(record, tag)),
        config
            .read_two_barcode_tag
            .as_deref()
            .and_then(|tag| bam_tag_value(record, tag)),
    )
}

fn bam_tag_value(record: &bam::Record, tag: &str) -> Option<Vec<u8>> {
    match record.aux(tag.as_bytes()) {
        Ok(Aux::String(value)) => Some(value.as_bytes().to_vec()),
        Ok(Aux::Char(value)) => Some(vec![value]),
        _ => None,
    }
}

fn combined_barcode<T>(read_one: Option<T>, read_two: Option<T>) -> Option<Vec<u8>>
where
    T: AsRef<[u8]>,
{
    match (read_one, read_two) {
        (None, None) => None,
        (read_one, read_two) => {
            let mut barcode = Vec::new();
            if let Some(value) = read_one {
                barcode.extend_from_slice(value.as_ref());
            }
            barcode.push(b'|');
            if let Some(value) = read_two {
                barcode.extend_from_slice(value.as_ref());
            }
            Some(barcode)
        }
    }
}

fn fragment_duplicate_key_bam(candidate: &DuplicateCandidate) -> BamDuplicateKey {
    BamDuplicateKey {
        library_id: candidate.library_id,
        reference_id: candidate.reference_id,
        position: candidate.five_prime_position,
        mate_reference_id: -1,
        mate_position: -1,
        template_length: 0,
        reverse_strand: candidate.reverse_strand(),
        barcode_id: candidate.barcode_id,
    }
}

fn pair_duplicate_key_bam(
    first: &DuplicateCandidate,
    second: &DuplicateCandidate,
    library_id: LibraryId,
    barcode_id: Option<InternedBytesId>,
) -> BamDuplicateKey {
    let (left, right) = if (first.reference_id, first.five_prime_position)
        <= (second.reference_id, second.five_prime_position)
    {
        (first, second)
    } else {
        (second, first)
    };

    BamDuplicateKey {
        library_id,
        reference_id: left.reference_id,
        position: left.five_prime_position,
        mate_reference_id: right.reference_id,
        mate_position: right.five_prime_position,
        template_length: pair_orientation_code(left, right),
        reverse_strand: false,
        barcode_id,
    }
}

fn pair_orientation_code(left: &DuplicateCandidate, right: &DuplicateCandidate) -> i64 {
    let left_reverse = i64::from(left.reverse_strand());
    let right_reverse = i64::from(right.reverse_strand());
    (left_reverse << 1) | right_reverse
}

fn unclipped_record_position(record: &bam::Record) -> i64 {
    let reverse_strand = record.flags() & 0x10 != 0;
    let cigar = record.raw_cigar();
    if reverse_strand {
        let reference_len: i64 = cigar
            .iter()
            .filter(|operation| raw_cigar_consumes_reference(**operation))
            .map(|operation| raw_cigar_len(*operation))
            .sum();
        let trailing_clip: i64 = cigar
            .iter()
            .rev()
            .take_while(|operation| raw_cigar_is_clip(**operation))
            .map(|operation| raw_cigar_len(*operation))
            .sum();
        record.pos() + reference_len + trailing_clip - 1
    } else {
        let leading_clip: i64 = cigar
            .iter()
            .take_while(|operation| raw_cigar_is_clip(**operation))
            .map(|operation| raw_cigar_len(*operation))
            .sum();
        record.pos() - leading_clip
    }
}

fn raw_cigar_len(operation: u32) -> i64 {
    i64::from(operation >> 4)
}

fn raw_cigar_op(operation: u32) -> u32 {
    operation & 0x0f
}

fn raw_cigar_consumes_reference(operation: u32) -> bool {
    matches!(raw_cigar_op(operation), 0 | 2 | 3 | 7 | 8)
}

fn raw_cigar_is_clip(operation: u32) -> bool {
    matches!(raw_cigar_op(operation), 4 | 5)
}

fn unclipped_five_prime_position(
    position: i64,
    cigar: &str,
    reverse_strand: bool,
    line_number: usize,
) -> Result<i64, MarkDuplicatesError> {
    let operations = parse_cigar(cigar, line_number)?;
    if reverse_strand {
        let reference_len: i64 = operations
            .iter()
            .filter(|(_, op)| matches!(op, 'M' | 'D' | 'N' | '=' | 'X'))
            .map(|(len, _)| *len)
            .sum();
        let trailing_clip = operations
            .iter()
            .rev()
            .take_while(|(_, op)| matches!(op, 'S' | 'H'))
            .map(|(len, _)| *len)
            .sum::<i64>();
        Ok(position + reference_len + trailing_clip - 1)
    } else {
        let leading_clip = operations
            .iter()
            .take_while(|(_, op)| matches!(op, 'S' | 'H'))
            .map(|(len, _)| *len)
            .sum::<i64>();
        Ok(position - leading_clip)
    }
}

fn parse_cigar(cigar: &str, line_number: usize) -> Result<Vec<(i64, char)>, MarkDuplicatesError> {
    let mut operations = Vec::new();
    let mut length = 0i64;
    let mut seen_operation = false;

    for character in cigar.bytes() {
        if character.is_ascii_digit() {
            length = length
                .checked_mul(10)
                .and_then(|value| value.checked_add(i64::from(character - b'0')))
                .filter(|value| *value >= 0)
                .ok_or_else(|| MarkDuplicatesError::MalformedSam {
                    line_number,
                    reason: format!("invalid CIGAR value: {cigar}"),
                })?;
            continue;
        }
        if length == 0 {
            return Err(MarkDuplicatesError::MalformedSam {
                line_number,
                reason: format!("invalid CIGAR value: {cigar}"),
            });
        }
        if !matches!(
            character as char,
            'M' | 'I' | 'D' | 'N' | 'S' | 'H' | 'P' | '=' | 'X'
        ) {
            return Err(MarkDuplicatesError::MalformedSam {
                line_number,
                reason: format!("invalid CIGAR value: {cigar}"),
            });
        }
        operations.push((length, character as char));
        length = 0;
        seen_operation = true;
    }

    if !seen_operation {
        return Err(MarkDuplicatesError::MalformedSam {
            line_number,
            reason: format!("invalid CIGAR value: {cigar}"),
        });
    }

    if length != 0 {
        return Err(MarkDuplicatesError::MalformedSam {
            line_number,
            reason: format!("invalid CIGAR value: {cigar}"),
        });
    }

    Ok(operations)
}

fn quality_score(record: &bam::Record) -> u64 {
    record
        .qual()
        .iter()
        .filter(|quality| **quality >= 15)
        .map(|quality| u64::from(*quality))
        .sum()
}

fn has_multiple_read_names(group: &[usize], candidates: &[DuplicateCandidate]) -> bool {
    let Some(first_index) = group.first() else {
        return false;
    };
    let first_name = candidates[*first_index].qname_id;
    group
        .iter()
        .skip(1)
        .any(|index| candidates[*index].qname_id != first_name)
}

fn paired_duplicate_set_size(group: &[usize], candidates: &[DuplicateCandidate]) -> Option<u64> {
    if !group.iter().any(|index| candidates[*index].is_pair()) {
        return None;
    }
    let mut names = HashSet::<InternedBytesId>::default();
    for index in group.iter().copied() {
        names.insert(candidates[index].qname_id);
    }
    u64::try_from(names.len()).ok().filter(|size| *size > 0)
}

fn best_duplicate_representative_index(
    group: &[usize],
    candidates: &[DuplicateCandidate],
) -> usize {
    let mut scores_by_name = HashMap::<InternedBytesId, (usize, u64)>::default();

    for index in group.iter().copied() {
        let candidate = &candidates[index];
        let score = candidate.duplicate_score;
        let name = candidate.qname_id;
        let entry = scores_by_name.entry(name).or_insert((index, 0));
        entry.1 += score;
    }

    scores_by_name
        .into_values()
        .max_by(|left, right| {
            left.1.cmp(&right.1).then_with(|| {
                candidates[right.0]
                    .record_index
                    .cmp(&candidates[left.0].record_index)
            })
        })
        .map(|(index, _)| index)
        .expect("non-empty duplicate group")
}

fn add_duplicate_set(
    summary: &mut MarkDuplicatesSummary,
    all_set_size: u64,
    non_optical_set_size: Option<u64>,
) {
    if all_set_size == 0 {
        return;
    }
    summary
        .duplicate_set_histogram
        .entry(all_set_size)
        .or_default()
        .all_sets += 1;
    if let Some(non_optical_set_size) = non_optical_set_size.filter(|size| *size > 0) {
        summary
            .duplicate_set_histogram
            .entry(non_optical_set_size)
            .or_default()
            .non_optical_sets += 1;
    }
}

fn metrics_text(summary: &MarkDuplicatesSummary) -> String {
    metrics_text_for_libraries(std::iter::once(summary))
}

fn metrics_text_for_libraries<'a>(
    summaries: impl IntoIterator<Item = &'a MarkDuplicatesSummary>,
) -> String {
    let summaries = summaries.into_iter().collect::<Vec<_>>();
    let mut output = concat!(
        "## METRICS CLASS\tpicard.sam.DuplicationMetrics\n",
        "LIBRARY\tUNPAIRED_READS_EXAMINED\tREAD_PAIRS_EXAMINED\tSECONDARY_OR_SUPPLEMENTARY_RDS\tUNMAPPED_READS\tUNPAIRED_READ_DUPLICATES\tREAD_PAIR_DUPLICATES\tREAD_PAIR_OPTICAL_DUPLICATES\tPERCENT_DUPLICATION\tESTIMATED_LIBRARY_SIZE\n",
    )
    .to_string();

    for summary in &summaries {
        output.push_str(&metrics_row(summary));
    }
    let histogram = combined_duplicate_set_histogram(summaries.iter().copied());
    if !histogram.is_empty() {
        output.push_str("\n## HISTOGRAM\tjava.lang.Double\n");
        output.push_str("BIN\tCoverageMult\tall_sets\tnon_optical_sets\n");
        let read_pairs_examined = summaries
            .iter()
            .map(|summary| {
                summary
                    .effective_read_pairs_examined()
                    .saturating_sub(summary.read_pair_optical_duplicates)
            })
            .sum::<u64>();
        let read_pair_duplicates = summaries
            .iter()
            .map(|summary| summary.read_pair_duplicates())
            .sum::<u64>();
        let unique_read_pairs = read_pairs_examined.saturating_sub(read_pair_duplicates);
        let estimated_library_size = estimate_library_size(read_pairs_examined, unique_read_pairs);
        let max_set_size = histogram.keys().copied().max().unwrap_or_default().max(100);
        for set_size in 1..=max_set_size {
            let counts = histogram.get(&set_size).copied().unwrap_or_default();
            let coverage_mult = estimated_library_size
                .map(|library_size| {
                    estimate_roi(
                        library_size,
                        set_size as f64,
                        read_pairs_examined,
                        unique_read_pairs,
                    )
                })
                .map(format_metric_float)
                .unwrap_or_default();
            output.push_str(&format!(
                "{:.1}\t{}\t{}\t{}\n",
                set_size as f64, coverage_mult, counts.all_sets, counts.non_optical_sets
            ));
        }
    }

    output
}

fn metrics_row(summary: &MarkDuplicatesSummary) -> String {
    let read_pairs_examined = summary.effective_read_pairs_examined();
    let duplicate_fragments =
        summary.unpaired_duplicate_records + (summary.read_pair_duplicates() * 2);
    let examined_fragments = summary.unpaired_reads_examined + (read_pairs_examined * 2);
    let percent_duplication = if examined_fragments == 0 {
        0.0
    } else {
        duplicate_fragments as f64 / examined_fragments as f64
    };
    let read_pair_duplicates = summary.read_pair_duplicates();
    let estimated_library_size = estimate_library_size(
        summary
            .effective_read_pairs_examined()
            .saturating_sub(summary.read_pair_optical_duplicates),
        summary
            .effective_read_pairs_examined()
            .saturating_sub(read_pair_duplicates),
    )
    .map(|value| value.to_string())
    .unwrap_or_default();

    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        summary.library,
        summary.unpaired_reads_examined,
        read_pairs_examined,
        summary.secondary_or_supplementary_records,
        summary.unmapped_records,
        summary.unpaired_duplicate_records,
        summary.read_pair_duplicates(),
        summary.read_pair_optical_duplicates,
        format_metric_float(percent_duplication),
        estimated_library_size
    )
}

fn combined_duplicate_set_histogram<'a>(
    summaries: impl IntoIterator<Item = &'a MarkDuplicatesSummary>,
) -> BTreeMap<u64, DuplicateSetCounts> {
    let mut combined = BTreeMap::<u64, DuplicateSetCounts>::new();
    for summary in summaries {
        for (set_size, counts) in &summary.duplicate_set_histogram {
            let target = combined.entry(*set_size).or_default();
            target.all_sets += counts.all_sets;
            target.non_optical_sets += counts.non_optical_sets;
        }
    }
    combined
}

fn format_metric_float(value: f64) -> String {
    let formatted = format!("{value:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

fn estimate_library_size(read_pairs: u64, unique_read_pairs: u64) -> Option<u64> {
    let read_pair_duplicates = read_pairs.checked_sub(unique_read_pairs)?;
    if read_pairs == 0 || read_pair_duplicates == 0 {
        return None;
    }

    let unique_read_pairs = unique_read_pairs as f64;
    let read_pairs = read_pairs as f64;
    let mut lower = 1.0;
    let mut upper = 100.0;
    if unique_read_pairs >= read_pairs
        || estimate_library_size_function(lower * unique_read_pairs, unique_read_pairs, read_pairs)
            < 0.0
    {
        return None;
    }

    while estimate_library_size_function(upper * unique_read_pairs, unique_read_pairs, read_pairs)
        > 0.0
    {
        upper *= 10.0;
    }

    for _ in 0..40 {
        let midpoint = (lower + upper) / 2.0;
        let value = estimate_library_size_function(
            midpoint * unique_read_pairs,
            unique_read_pairs,
            read_pairs,
        );
        match value.total_cmp(&0.0) {
            Ordering::Equal => break,
            Ordering::Greater => lower = midpoint,
            Ordering::Less => upper = midpoint,
        }
    }

    Some((unique_read_pairs * (lower + upper) / 2.0) as u64)
}

fn estimate_roi(
    estimated_library_size: u64,
    coverage_multiple: f64,
    read_pairs: u64,
    unique_read_pairs: u64,
) -> f64 {
    let library_size = estimated_library_size as f64;
    let read_pairs = read_pairs as f64;
    let unique_read_pairs = unique_read_pairs as f64;
    library_size * (1.0 - (-(coverage_multiple * read_pairs) / library_size).exp())
        / unique_read_pairs
}

fn estimate_library_size_function(
    library_size: f64,
    unique_read_pairs: f64,
    read_pairs: f64,
) -> f64 {
    unique_read_pairs / library_size - 1.0 + (-read_pairs / library_size).exp()
}

const PAIRED_FLAG: u16 = 0x1;
const MATE_UNMAPPED_FLAG: u16 = 0x8;
const FIRST_IN_PAIR_FLAG: u16 = 0x40;
const SECONDARY_OR_SUPPLEMENTARY_FLAGS: u16 = 0x100 | 0x800;

fn duplicate_candidate_is_pair(flag: u16) -> bool {
    flag & PAIRED_FLAG != 0 && flag & MATE_UNMAPPED_FLAG == 0
}

impl MarkDuplicatesSummary {
    fn read_pair_duplicates(&self) -> u64 {
        self.duplicate_pair_records / 2
    }

    fn effective_read_pairs_examined(&self) -> u64 {
        if self.paired_records_examined > 0 {
            self.paired_records_examined / 2
        } else {
            self.read_pairs_examined
        }
    }
}

struct LibraryLookup {
    by_read_group: HashMap<Vec<u8>, LibraryId>,
    first_library_id: LibraryId,
    unknown_id: LibraryId,
}

struct LibraryRegistry {
    ids_by_name: HashMap<String, LibraryId>,
    summaries: Vec<MarkDuplicatesSummary>,
}

impl LibraryRegistry {
    fn new() -> Self {
        Self {
            ids_by_name: HashMap::default(),
            summaries: Vec::new(),
        }
    }

    fn intern(&mut self, library: &str) -> LibraryId {
        if let Some(id) = self.ids_by_name.get(library) {
            return *id;
        }
        let id = LibraryId::try_from(self.summaries.len()).expect("library id fits in u32");
        self.ids_by_name.insert(library.to_string(), id);
        self.summaries.push(MarkDuplicatesSummary {
            library: library.to_string(),
            unpaired_reads_examined: 0,
            read_pairs_examined: 0,
            paired_records_examined: 0,
            secondary_or_supplementary_records: 0,
            unpaired_duplicate_records: 0,
            duplicate_pair_records: 0,
            read_pair_optical_duplicates: 0,
            unmapped_records: 0,
            duplicate_set_histogram: BTreeMap::new(),
        });
        id
    }

    fn summary(&self, id: LibraryId) -> &MarkDuplicatesSummary {
        &self.summaries[id as usize]
    }

    fn summary_mut(&mut self, id: LibraryId) -> &mut MarkDuplicatesSummary {
        &mut self.summaries[id as usize]
    }

    fn summary_refs(&self) -> Vec<&MarkDuplicatesSummary> {
        self.summaries
            .iter()
            .filter(|summary| !summary.is_empty())
            .collect()
    }
}

impl MarkDuplicatesSummary {
    fn is_empty(&self) -> bool {
        self.unpaired_reads_examined == 0
            && self.read_pairs_examined == 0
            && self.secondary_or_supplementary_records == 0
            && self.unpaired_duplicate_records == 0
            && self.duplicate_pair_records == 0
            && self.read_pair_optical_duplicates == 0
            && self.unmapped_records == 0
            && self.paired_records_examined == 0
            && self.duplicate_set_histogram.is_empty()
    }
}

fn library_lookup(header: &bam::HeaderView, registry: &mut LibraryRegistry) -> LibraryLookup {
    let mut lookup = HashMap::<Vec<u8>, LibraryId>::default();
    let unknown_id = registry.intern("Unknown Library");
    let mut first_library_id = None::<LibraryId>;
    let header_text = String::from_utf8_lossy(header.as_bytes());
    for line in header_text.lines() {
        if !line.starts_with("@RG\t") {
            continue;
        }
        let mut read_group = None::<Vec<u8>>;
        let mut library_name = None::<&str>;
        for field in line.split('\t') {
            if let Some(id) = field.strip_prefix("ID:") {
                read_group = Some(id.as_bytes().to_vec());
            }
            if let Some(library) = field.strip_prefix("LB:") {
                library_name = Some(library);
            }
        }
        if let Some(read_group) = read_group {
            let library_id = registry.intern(library_name.unwrap_or("Unknown Library"));
            first_library_id.get_or_insert(library_id);
            lookup.insert(read_group, library_id);
        }
    }

    LibraryLookup {
        by_read_group: lookup,
        first_library_id: first_library_id.unwrap_or(unknown_id),
        unknown_id,
    }
}

fn read_group_ids(header: &bam::HeaderView) -> HashSet<Vec<u8>> {
    let header_text = String::from_utf8_lossy(header.as_bytes());
    header_text
        .lines()
        .filter(|line| line.starts_with("@RG\t"))
        .filter_map(read_group_id)
        .collect()
}

fn append_missing_read_groups(
    header: &mut bam::Header,
    source: &bam::HeaderView,
    known_read_groups: &mut HashSet<Vec<u8>>,
) {
    let header_text = String::from_utf8_lossy(source.as_bytes());
    for line in header_text.lines().filter(|line| line.starts_with("@RG\t")) {
        let Some(read_group_id) = read_group_id(line) else {
            continue;
        };
        if !known_read_groups.insert(read_group_id) {
            continue;
        }
        let mut record = HeaderRecord::new(b"RG");
        for field in line.split('\t').skip(1) {
            if let Some((tag, value)) = field.split_once(':') {
                record.push_tag(tag.as_bytes(), value);
            }
        }
        header.push_record(&record);
    }
}

fn read_group_id(line: &str) -> Option<Vec<u8>> {
    line.split('\t')
        .find_map(|field| field.strip_prefix("ID:"))
        .map(|id| id.as_bytes().to_vec())
}

fn record_library_id(record: &bam::Record, lookup: &LibraryLookup) -> LibraryId {
    match record.aux(b"RG") {
        Ok(Aux::String(read_group)) => lookup
            .by_read_group
            .get(read_group.as_bytes())
            .copied()
            .unwrap_or(lookup.unknown_id),
        _ => lookup.unknown_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sam_markdup_config() -> MarkDuplicatesConfig {
        MarkDuplicatesConfig {
            input: String::new(),
            inputs: Vec::new(),
            output: String::new(),
            metrics_file: String::new(),
            max_records_in_ram: 500_000,
            remove_duplicates: false,
            remove_sequencing_duplicates: false,
            assume_sorted: true,
            assume_sort_order: None,
            validation_stringency: None,
            quiet: true,
            create_index: false,
            create_md5_file: false,
            add_pg_tag_to_reads: true,
            tag_duplicate_set_members: false,
            duplicate_scoring_strategy: None,
            read_name_regex: None,
            tagging_policy: None,
            barcode_tag: None,
            read_one_barcode_tag: None,
            read_two_barcode_tag: None,
            clear_dt: true,
            optical_duplicate_pixel_distance: None,
            compression_level: None,
            reference_sequence: None,
        }
    }

    fn valid_sam_fields() -> Vec<String> {
        vec![
            "r1".to_string(),
            "0".to_string(),
            "chr1".to_string(),
            "1".to_string(),
            "60".to_string(),
            "10M".to_string(),
            "*".to_string(),
            "0".to_string(),
            "0".to_string(),
            "NNNNNNNNNN".to_string(),
            "*".to_string(),
        ]
    }

    fn assert_malformed_sam_err(
        result: Result<DuplicateKey, MarkDuplicatesError>,
        expected_line: usize,
        field: &str,
    ) {
        let error = result.expect_err("expected malformed SAM error");
        let MarkDuplicatesError::MalformedSam {
            line_number,
            reason,
        } = error
        else {
            panic!("expected malformed SAM error");
        };
        assert_eq!(line_number, expected_line);
        assert!(reason.contains(field));
    }

    fn record_with_name_and_qualities(qname: &[u8], qualities: &[u8], flags: u16) -> bam::Record {
        let mut record = bam::Record::new();
        let sequence = vec![b'A'; qualities.len()];
        record.set(qname, None, &sequence, qualities);
        record.set_flags(flags);
        record
    }

    fn candidates_for_records(records: &[bam::Record]) -> Vec<DuplicateCandidate> {
        candidates_and_qnames_for_records(records).0
    }

    fn candidates_and_qnames_for_records(
        records: &[bam::Record],
    ) -> (Vec<DuplicateCandidate>, ByteInterner) {
        let mut qnames = ByteInterner::default();
        let candidates = records
            .iter()
            .enumerate()
            .map(|(index, record)| {
                let qname_id = qnames.intern(record.qname());
                DuplicateCandidate::from_record(index, record, 0, qname_id, None)
            })
            .collect();
        (candidates, qnames)
    }

    #[test]
    fn best_duplicate_representative_index_aggregates_score_by_read_name() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[20], 0),
            record_with_name_and_qualities(b"dup-a", &[15], 0),
            record_with_name_and_qualities(b"dup-b", &[35], 0),
            record_with_name_and_qualities(b"dup-c", &[5], 0),
        ];
        let candidates = candidates_for_records(&records);

        let representative_index = best_duplicate_representative_index(&[0, 1, 2, 3], &candidates);
        assert_eq!(representative_index, 0);
    }

    #[test]
    fn best_duplicate_representative_index_tie_keeps_first_seen_index() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[20], 0),
            record_with_name_and_qualities(b"dup-b", &[10], 0),
            record_with_name_and_qualities(b"dup-c", &[10], 0),
        ];
        let candidates = candidates_for_records(&records);

        let representative_index = best_duplicate_representative_index(&[0, 1, 2], &candidates);
        assert_eq!(representative_index, 0);
    }

    #[test]
    fn paired_duplicate_set_size_uses_unique_read_names_for_pairs() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[10], 0x1),
            record_with_name_and_qualities(b"dup-a", &[20], 0x1),
            record_with_name_and_qualities(b"dup-b", &[30], 0x0),
            record_with_name_and_qualities(b"dup-b", &[40], 0x1),
        ];
        let candidates = candidates_for_records(&records);

        assert_eq!(
            paired_duplicate_set_size(&[0, 1, 2, 3], &candidates),
            Some(2)
        );
    }

    #[test]
    fn paired_duplicate_set_size_none_without_paired_candidate() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[10], 0x0),
            record_with_name_and_qualities(b"dup-a", &[20], 0x0),
        ];
        let candidates = candidates_for_records(&records);

        assert_eq!(paired_duplicate_set_size(&[0, 1], &candidates), None);
    }

    fn record_with_name_and_flags(qname: &[u8], flags: u16) -> bam::Record {
        let mut record = bam::Record::new();
        let qualities = [0x1f_u8];
        record.set(qname, None, &vec![b'A'; qualities.len()], &qualities);
        record.set_flags(flags);
        record.set_tid(0);
        record.set_pos(0);
        record.set_mtid(-1);
        record.set_mpos(-1);
        record.set_insert_size(0);
        record
    }

    fn record_with_name_flags_and_position(qname: &[u8], flags: u16, pos: i64) -> bam::Record {
        let mut record = record_with_name_and_flags(qname, flags);
        record.set_pos(pos);
        record
    }

    #[test]
    fn duplicate_groups_sorts_pair_keys_and_preserves_equal_key_order() {
        let records = [
            record_with_name_flags_and_position(b"z-pair", 0x1, 10),
            record_with_name_flags_and_position(b"z-pair", 0x1, 20),
            record_with_name_flags_and_position(b"a-pair", 0x1, 10),
            record_with_name_flags_and_position(b"a-pair", 0x1, 20),
            record_with_name_flags_and_position(b"later-pair", 0x1, 30),
            record_with_name_flags_and_position(b"later-pair", 0x1, 40),
        ];
        let candidates = candidates_for_records(&records);

        let groups = duplicate_groups(&candidates, 500_000).expect("pair grouping succeeds");

        assert_eq!(groups, vec![vec![0, 1, 2, 3], vec![4, 5]]);
    }

    #[test]
    fn pair_collation_matches_legacy_for_adjacent_and_displaced_qnames() {
        let records = [
            record_with_name_flags_and_position(b"displaced", 0x1, 10),
            record_with_name_flags_and_position(b"adjacent", 0x1, 30),
            record_with_name_flags_and_position(b"adjacent", 0x1, 40),
            record_with_name_flags_and_position(b"mixed", 0x1, 50),
            record_with_name_flags_and_position(b"mixed", 0x0, 55),
            record_with_name_flags_and_position(b"mixed", 0x1, 60),
            record_with_name_flags_and_position(b"other", 0x1, 70),
            record_with_name_flags_and_position(b"displaced", 0x1, 20),
        ];
        let candidates = candidates_for_records(&records);

        assert_eq!(
            collate_pair_key_rows(&candidates, 500_000).expect("pair collation succeeds"),
            collate_pair_key_rows_legacy(&candidates)
        );
    }

    #[test]
    fn pair_collation_preserves_prior_unresolved_candidate_before_adjacent_run() {
        let records = [
            record_with_name_flags_and_position(b"repeat", 0x1, 10),
            record_with_name_flags_and_position(b"other", 0x1, 30),
            record_with_name_flags_and_position(b"repeat", 0x1, 20),
            record_with_name_flags_and_position(b"repeat", 0x1, 25),
        ];
        let candidates = candidates_for_records(&records);

        assert_eq!(
            collate_pair_key_rows(&candidates, 500_000).expect("pair collation succeeds"),
            collate_pair_key_rows_legacy(&candidates)
        );
    }

    #[test]
    fn pair_collation_errors_when_displaced_cache_exceeds_limit() {
        let records = [
            record_with_name_flags_and_position(b"pending-a", 0x1, 10),
            record_with_name_flags_and_position(b"pending-b", 0x1, 20),
            record_with_name_flags_and_position(b"pending-c", 0x1, 30),
        ];
        let candidates = candidates_for_records(&records);

        let error = collate_pair_key_rows(&candidates, 2).expect_err("cache limit is enforced");

        assert!(
            error
                .to_string()
                .contains("external qname collation is not implemented yet")
        );
    }

    #[test]
    fn fragment_duplicate_groups_sorts_keys_and_preserves_equal_key_order() {
        let records = [
            record_with_name_flags_and_position(b"later-a", 0x0, 30),
            record_with_name_flags_and_position(b"dup-a", 0x0, 10),
            record_with_name_flags_and_position(b"dup-b", 0x0, 10),
            record_with_name_flags_and_position(b"later-b", 0x0, 30),
        ];
        let candidates = candidates_for_records(&records);

        let groups = fragment_duplicate_groups(&candidates);

        assert_eq!(groups, vec![vec![1, 2], vec![0, 3]]);
    }

    #[test]
    fn add_duplicate_set_member_tags_uses_unique_read_names_for_duplicate_set_size() {
        let records = [
            record_with_name_and_flags(b"dup-a", 0x1),
            record_with_name_and_flags(b"dup-a", 0x1),
            record_with_name_and_flags(b"dup-b", 0x1),
            record_with_name_and_flags(b"dup-c", 0x1),
        ];
        let candidates = candidates_for_records(&records);
        let mut decisions = vec![RecordDecision::default(); records.len()];

        add_duplicate_set_member_tags(
            &[0, 1, 2, 3],
            &candidates,
            &mut decisions,
            candidates[0].qname_id,
        );

        for index in [0usize, 1, 2, 3] {
            assert_eq!(decisions[index].duplicate_set_index, Some(0));
            assert_eq!(decisions[index].duplicate_set_size, Some(3));
        }
    }

    #[test]
    fn add_duplicate_set_member_tags_skips_groups_without_paired_record() {
        let records = [
            record_with_name_and_flags(b"dup-a", 0x0),
            record_with_name_and_flags(b"dup-a", 0x0),
        ];
        let candidates = candidates_for_records(&records);
        let mut decisions = vec![RecordDecision::default(); records.len()];

        add_duplicate_set_member_tags(&[0, 1], &candidates, &mut decisions, candidates[0].qname_id);

        assert!(decisions[0].duplicate_set_index.is_none());
        assert!(decisions[1].duplicate_set_index.is_none());
    }

    #[test]
    fn optical_duplicate_records_count_unique_interned_read_names() {
        let records = [
            record_with_name_and_flags(b"INST:1:FC:1:1101:100:100", 0x1),
            record_with_name_and_flags(b"INST:1:FC:1:1101:105:105", 0x1),
            record_with_name_and_flags(b"INST:1:FC:1:1101:105:105", 0x1),
        ];
        let candidates = candidates_for_records(&records);
        let config = MarkDuplicatesConfig {
            read_name_regex: None,
            optical_duplicate_pixel_distance: Some(100),
            ..sam_markdup_config()
        };

        assert_eq!(
            candidates[0].optical_location,
            Some(ReadLocation {
                tile: 1101,
                x: 100,
                y: 100
            })
        );
        let optical = optical_duplicate_record_indices(&[0, 1, 2], &candidates, 0, &config);

        assert_eq!(optical.read_names, 1);
        assert_eq!(optical.record_indices, vec![1, 2]);
    }

    #[test]
    fn duplicate_key_rejects_invalid_position() {
        let mut fields = valid_sam_fields();
        fields[3] = "bad".to_string();

        let key = duplicate_key(&fields, 0, 7, &sam_markdup_config());
        assert_malformed_sam_err(key, 7, "POS");
    }

    #[test]
    fn duplicate_key_rejects_invalid_mate_position() {
        let mut fields = valid_sam_fields();
        fields[7] = "bad".to_string();

        let key = duplicate_key(&fields, 0, 11, &sam_markdup_config());
        assert_malformed_sam_err(key, 11, "MATE_POS");
    }

    #[test]
    fn duplicate_key_rejects_invalid_template_length() {
        let mut fields = valid_sam_fields();
        fields[8] = "bad".to_string();

        let key = duplicate_key(&fields, 0, 13, &sam_markdup_config());
        assert_malformed_sam_err(key, 13, "TLEN");
    }

    #[test]
    fn duplicate_key_rejects_invalid_cigar() {
        let mut fields = valid_sam_fields();
        fields[5] = "10M5".to_string();

        let key = duplicate_key(&fields, 0, 17, &sam_markdup_config());
        assert_malformed_sam_err(key, 17, "CIGAR");
    }

    #[test]
    fn duplicate_key_rejects_zero_length_cigar_op() {
        let mut fields = valid_sam_fields();
        fields[5] = "0M".to_string();

        let key = duplicate_key(&fields, 0, 19, &sam_markdup_config());
        assert_malformed_sam_err(key, 19, "CIGAR");
    }

    #[test]
    fn duplicate_key_rejects_empty_cigar() {
        let mut fields = valid_sam_fields();
        fields[5] = "".to_string();

        let key = duplicate_key(&fields, 0, 23, &sam_markdup_config());
        assert_malformed_sam_err(key, 23, "CIGAR");
    }

    #[test]
    fn duplicate_key_rejects_star_cigar() {
        let mut fields = valid_sam_fields();
        fields[5] = "*".to_string();

        let key = duplicate_key(&fields, 0, 31, &sam_markdup_config());
        assert_malformed_sam_err(key, 31, "CIGAR");
    }
}
