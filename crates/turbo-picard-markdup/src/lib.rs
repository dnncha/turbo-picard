#![forbid(unsafe_code)]

use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::record::Aux;
use rust_htslib::bam::{self, Read, index};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Read as IoRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use turbo_picard_core::external_sort::{ExternalSortConfig, ExternalSorter};
use turbo_picard_core::hts_io;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;

const DUPLICATE_FLAG: u16 = 0x400;
const UNMAPPED_FLAG: u16 = 0x4;
const DECISION_BITS_PER_WORD: usize = u64::BITS as usize;
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

#[cfg(test)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct SamTextDuplicateKey {
    reference_id: InternedBytesId,
    position: i64,
    mate_reference_id: InternedBytesId,
    mate_position: i64,
    template_length: i64,
    reverse_strand: bool,
    barcode_id: Option<InternedBytesId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct FragmentDuplicateKey {
    library_id: LibraryId,
    reference_id: i32,
    position: i64,
    barcode_id: Option<InternedBytesId>,
}

impl FragmentDuplicateKey {
    fn duplicate_key(self, reverse_strand: bool) -> BamDuplicateKey {
        BamDuplicateKey {
            library_id: self.library_id,
            reference_id: self.reference_id,
            position: self.position,
            mate_reference_id: -1,
            mate_position: -1,
            template_length: 0,
            reverse_strand,
            barcode_id: self.barcode_id,
        }
    }
}

#[derive(Debug, Clone)]
struct DuplicateCandidate {
    record_index: usize,
    qname_id: InternedBytesId,
    flags: CandidateFlags,
    duplicate_score: u64,
    optical_location: Option<ReadLocation>,
    fragment_key: FragmentDuplicateKey,
}

#[derive(Debug, Clone, Copy, Default)]
struct CandidateFlags(u8);

impl CandidateFlags {
    const PAIR: u8 = 0b0000_0001;
    const REVERSE_STRAND: u8 = 0b0000_0010;

    fn from_record_flags(flags: u16) -> Self {
        let mut candidate_flags = 0;
        if duplicate_candidate_is_pair(flags) {
            candidate_flags |= Self::PAIR;
        }
        if flags & 0x10 != 0 {
            candidate_flags |= Self::REVERSE_STRAND;
        }
        Self(candidate_flags)
    }

    fn is_pair(self) -> bool {
        self.0 & Self::PAIR != 0
    }

    fn reverse_strand(self) -> bool {
        self.0 & Self::REVERSE_STRAND != 0
    }
}

impl DuplicateCandidate {
    fn from_record(
        record_index: usize,
        record: &bam::Record,
        library_id: LibraryId,
        qname_id: InternedBytesId,
        barcode_id: Option<InternedBytesId>,
        parse_optical_location: bool,
    ) -> Self {
        let flags = record.flags();
        let reference_id = record.tid();
        let five_prime_position = unclipped_record_position(record);
        let candidate_flags = CandidateFlags::from_record_flags(flags);
        Self {
            record_index,
            qname_id,
            flags: candidate_flags,
            duplicate_score: quality_score(record),
            optical_location: parse_optical_location
                .then(|| parse_read_location(record.qname()))
                .flatten(),
            fragment_key: FragmentDuplicateKey {
                library_id,
                reference_id,
                position: five_prime_position,
                barcode_id,
            },
        }
    }

    fn is_pair(&self) -> bool {
        self.flags.is_pair()
    }

    fn reverse_strand(&self) -> bool {
        self.flags.reverse_strand()
    }

    fn library_id(&self) -> LibraryId {
        self.fragment_key.library_id
    }

    fn fragment_duplicate_key(&self) -> BamDuplicateKey {
        self.fragment_key.duplicate_key(self.reverse_strand())
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct RecordDecision {
    flags: u8,
    duplicate_set: Option<DuplicateSetTag>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DuplicateSetTag {
    size: i32,
    index: i32,
}

impl RecordDecision {
    const DUPLICATE: u8 = 0b0000_0001;
    const OPTICAL_DUPLICATE: u8 = 0b0000_0010;

    fn duplicate(self) -> bool {
        self.flags & Self::DUPLICATE != 0
    }

    fn optical_duplicate(self) -> bool {
        self.flags & Self::OPTICAL_DUPLICATE != 0
    }
}

#[derive(Debug, Clone)]
struct DuplicateDecisions {
    record_count: usize,
    duplicate_flags: Vec<u64>,
    optical_duplicate_flags: Vec<u64>,
    duplicate_sets: HashMap<usize, DuplicateSetTag>,
}

impl DuplicateDecisions {
    fn new(record_count: usize) -> Self {
        Self {
            record_count,
            duplicate_flags: vec![0; bitset_words(record_count)],
            optical_duplicate_flags: vec![0; bitset_words(record_count)],
            duplicate_sets: HashMap::default(),
        }
    }

    fn len(&self) -> usize {
        self.record_count
    }

    fn decision(&self, record_index: usize) -> Option<RecordDecision> {
        (record_index < self.record_count).then(|| {
            let mut flags = 0u8;
            if bitset_get(&self.duplicate_flags, record_index) {
                flags |= RecordDecision::DUPLICATE;
            }
            if bitset_get(&self.optical_duplicate_flags, record_index) {
                flags |= RecordDecision::OPTICAL_DUPLICATE;
            }
            RecordDecision {
                flags,
                duplicate_set: self.duplicate_sets.get(&record_index).copied(),
            }
        })
    }

    fn duplicate(&self, record_index: usize) -> bool {
        record_index < self.record_count && bitset_get(&self.duplicate_flags, record_index)
    }

    fn mark_duplicate(&mut self, record_index: usize) {
        if record_index < self.record_count {
            bitset_set(&mut self.duplicate_flags, record_index);
        }
    }

    fn mark_optical_duplicate(&mut self, record_index: usize) {
        if record_index < self.record_count {
            bitset_set(&mut self.optical_duplicate_flags, record_index);
        }
    }

    fn set_duplicate_set(&mut self, record_index: usize, size: i32, index: i32) {
        if record_index < self.record_count {
            self.duplicate_sets
                .insert(record_index, DuplicateSetTag { size, index });
        }
    }
}

fn bitset_words(bits: usize) -> usize {
    bits.div_ceil(DECISION_BITS_PER_WORD)
}

fn bitset_get(words: &[u64], bit: usize) -> bool {
    words
        .get(bit / DECISION_BITS_PER_WORD)
        .is_some_and(|word| word & bitset_mask(bit) != 0)
}

fn bitset_set(words: &mut [u64], bit: usize) {
    if let Some(word) = words.get_mut(bit / DECISION_BITS_PER_WORD) {
        *word |= bitset_mask(bit);
    }
}

fn bitset_mask(bit: usize) -> u64 {
    1u64 << (bit % DECISION_BITS_PER_WORD)
}

#[derive(Debug)]
struct OutputRecordLocator {
    record_index: usize,
    offset: i64,
    position: i64,
    input_index: u32,
    reference_id: i32,
    qname_id: InternedBytesId,
    flags: u16,
}

#[derive(Debug, Default)]
struct ByteInterner {
    ids: HashMap<Rc<[u8]>, InternedBytesId>,
    values: Vec<Rc<[u8]>>,
}

impl ByteInterner {
    fn intern(&mut self, value: &[u8]) -> InternedBytesId {
        if let Some(id) = self.ids.get(value) {
            return *id;
        }
        let id = InternedBytesId::try_from(self.values.len()).expect("interned id fits in u32");
        let owned = Rc::<[u8]>::from(value);
        self.values.push(Rc::clone(&owned));
        self.ids.insert(owned, id);
        id
    }

    fn get(&self, id: InternedBytesId) -> Result<&[u8], MarkDuplicatesError> {
        self.values
            .get(usize::try_from(id).map_err(|_| {
                MarkDuplicatesError::Operation(
                    "interned MarkDuplicates byte id exceeds usize".to_string(),
                )
            })?)
            .map(Rc::as_ref)
            .ok_or_else(|| {
                MarkDuplicatesError::Operation(format!(
                    "interned MarkDuplicates byte id {id} is out of range"
                ))
            })
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

    run_sam_text(config)
}

fn run_sam_text(
    config: &MarkDuplicatesConfig,
) -> Result<MarkDuplicatesSummary, MarkDuplicatesError> {
    let input = File::open(&config.input)?;
    let mut reader = BufReader::with_capacity(1024 * 1024, input);
    let mut output = AtomicSamTextOutput::create(&config.output)?;
    let mut seen = HashMap::<SamTextDuplicateKey, usize>::default();
    let mut key_interner = ByteInterner::default();
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
    let mut header_lines = Vec::<String>::new();
    let mut header_written = false;
    let mut line = String::new();
    let mut line_number = 0usize;

    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        line_number += 1;
        let line = line.trim_end_matches(['\r', '\n']);
        if line.starts_with('@') {
            if header_written {
                output.write_line(line)?;
            } else {
                header_lines.push(line.to_string());
            }
            continue;
        }
        if !header_written {
            output.write_header_lines(&header_lines, config.add_pg_tag_to_reads)?;
            header_written = true;
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
            output.write_line(line)?;
            continue;
        }
        if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
            summary.secondary_or_supplementary_records += 1;
            output.write_line(line)?;
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
        let key = sam_text_duplicate_key(&fields, flag, line_number, config, &mut key_interner)?;
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
            output.write_line(&fields.join("\t"))?;
        }
    }

    if !header_written {
        output.write_header_lines(&header_lines, config.add_pg_tag_to_reads)?;
    }
    fs::write(&config.metrics_file, metrics_text(&summary))?;
    output.persist()?;
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
    let mut candidates = Vec::new();
    let mut qnames = ByteInterner::default();
    let mut barcodes = ByteInterner::default();
    let mut record_count = 0usize;
    let seekable_multi_bam_output = config.inputs.len() > 1 && all_inputs_are_bam(config);
    let retain_records = config.inputs.len() > 1 && !seekable_multi_bam_output;
    let mut records = Vec::new();
    let mut output_locs = Vec::new();
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
            records: if retain_records {
                Some(&mut records)
            } else {
                None
            },
            output_locs: if seekable_multi_bam_output {
                Some(&mut output_locs)
            } else {
                None
            },
            input_index: 0,
            record_count: &mut record_count,
            candidates: &mut candidates,
            qnames: &mut qnames,
            barcodes: &mut barcodes,
        },
        &first_library_lookup,
        &mut library_registry,
        &mut summary,
        config,
    )?;
    for (input_index, input) in config.inputs.iter().enumerate().skip(1) {
        let mut reader = open_markdup_reader(config, input)?;
        let input_library_lookup = library_lookup(reader.header(), &mut library_registry);
        read_bam_records(
            &mut reader,
            &mut BamRecordSink {
                records: if retain_records {
                    Some(&mut records)
                } else {
                    None
                },
                output_locs: if seekable_multi_bam_output {
                    Some(&mut output_locs)
                } else {
                    None
                },
                input_index: u32::try_from(input_index).map_err(|_| {
                    MarkDuplicatesError::Operation(
                        "MarkDuplicates input index exceeds u32".to_string(),
                    )
                })?,
                record_count: &mut record_count,
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

    let mut decisions = DuplicateDecisions::new(record_count);
    process_pair_duplicate_groups(
        &candidates,
        &mut decisions,
        &mut summary,
        &mut library_registry,
        config,
    )?;
    mark_fragment_duplicate_groups(
        &candidates,
        &mut decisions,
        &mut summary,
        &mut library_registry,
        config,
    )?;

    let mut writer = open_markdup_writer(config, &config.output, &header)?;
    if config.inputs.len() > 1 {
        if seekable_multi_bam_output {
            write_multi_bam_records_by_locator(
                output_locs,
                &qnames,
                &decisions,
                config,
                &mut writer,
            )?;
        } else {
            let mut marked_records = records
                .into_iter()
                .enumerate()
                .map(|(record_index, record)| {
                    let decision = decisions.decision(record_index).ok_or_else(|| {
                        MarkDuplicatesError::Operation("missing duplicate decision".into())
                    })?;
                    Ok((record, decision))
                })
                .collect::<Result<Vec<_>, MarkDuplicatesError>>()?;
            marked_records.sort_by(|(left, left_decision), (right, right_decision)| {
                compare_bam_output_order(left, *left_decision, right, *right_decision)
            });
            write_bam_records(marked_records, config, &mut writer)?;
        }
    } else {
        let mut reader = open_markdup_reader(config, first_input)?;
        read_bam_records_for_output(&mut reader, &decisions, config, &mut writer)?;
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

fn all_inputs_are_bam(config: &MarkDuplicatesConfig) -> bool {
    config
        .inputs
        .iter()
        .all(|input| hts_io::path_format(input) == Some(bam::Format::Bam))
}

struct BamRecordSink<'a> {
    records: Option<&'a mut Vec<bam::Record>>,
    output_locs: Option<&'a mut Vec<OutputRecordLocator>>,
    input_index: u32,
    record_count: &'a mut usize,
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
    let mut record = bam::Record::new();
    let parse_optical_location = parse_optical_locations(config);
    loop {
        let offset = sink.output_locs.as_ref().map(|_| reader.tell());
        let Some(result) = reader.read(&mut record) else {
            break;
        };
        result?;
        let flag = record.flags() & !DUPLICATE_FLAG;
        if record.flags() != flag {
            record.set_flags(flag);
        }
        let library_id = record_library_id(&record, library_lookup);
        let record_index = *sink.record_count;
        *sink.record_count += 1;
        let locator_qname_id = if sink.output_locs.is_some() {
            Some(sink.qnames.intern(record.qname()))
        } else {
            None
        };
        if let (Some(output_locs), Some(offset)) = (sink.output_locs.as_deref_mut(), offset) {
            output_locs.push(OutputRecordLocator {
                input_index: sink.input_index,
                record_index,
                offset,
                reference_id: record.tid(),
                position: record.pos(),
                qname_id: locator_qname_id.expect("qname id recorded for output locator"),
                flags: flag,
            });
        }

        if flag & UNMAPPED_FLAG != 0 {
            summary.unmapped_records += 1;
            library_registry.summary_mut(library_id).unmapped_records += 1;
            if let Some(records) = sink.records.as_deref_mut() {
                records.push(std::mem::take(&mut record));
            }
            continue;
        }
        if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
            summary.secondary_or_supplementary_records += 1;
            library_registry
                .summary_mut(library_id)
                .secondary_or_supplementary_records += 1;
            if let Some(records) = sink.records.as_deref_mut() {
                records.push(std::mem::take(&mut record));
            }
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
        let qname_id = locator_qname_id.unwrap_or_else(|| sink.qnames.intern(record.qname()));
        let barcode_id = bam_barcode_id(&record, config, sink.barcodes);
        sink.candidates.push(DuplicateCandidate::from_record(
            record_index,
            &record,
            library_id,
            qname_id,
            barcode_id,
            parse_optical_location,
        ));
        if let Some(records) = sink.records.as_deref_mut() {
            records.push(std::mem::take(&mut record));
        }
    }

    Ok(())
}

fn read_bam_records_for_output<R: bam::Read>(
    reader: &mut R,
    decisions: &DuplicateDecisions,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    let mut seen_records = 0usize;
    let mut record = bam::Record::new();
    for record_index in 0usize.. {
        let Some(result) = reader.read(&mut record) else {
            break;
        };
        result?;
        let flags = record.flags() & !DUPLICATE_FLAG;
        if flags != record.flags() {
            record.set_flags(flags);
        }
        let decision = decisions
            .decision(record_index)
            .ok_or_else(|| MarkDuplicatesError::Operation("missing duplicate decision".into()))?;
        write_bam_record(&mut record, decision, config, writer)?;
        seen_records = record_index + 1;
    }
    if seen_records != decisions.len() {
        return Err(MarkDuplicatesError::Operation(format!(
            "duplicate decision count {} does not match reread record count {seen_records}",
            decisions.len()
        )));
    }
    Ok(())
}

fn write_multi_bam_records_by_locator(
    output_locs: Vec<OutputRecordLocator>,
    qnames: &ByteInterner,
    decisions: &DuplicateDecisions,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    if output_locs.len() != decisions.len() {
        return Err(MarkDuplicatesError::Operation(format!(
            "MarkDuplicates output locator count {} does not match decision count {}",
            output_locs.len(),
            decisions.len()
        )));
    }
    let mut sorter =
        ExternalSorter::new(markdup_sort_config(config, "turbo-picard-markdup-output"))
            .map_err(MarkDuplicatesError::Operation)?;
    for locator in &output_locs {
        let decision = decisions
            .decision(locator.record_index)
            .ok_or_else(|| MarkDuplicatesError::Operation("missing duplicate decision".into()))?;
        sorter
            .push(
                output_sort_key(locator, qnames, decision)?,
                locator_payload(locator),
            )
            .map_err(MarkDuplicatesError::Operation)?;
    }

    let mut readers = config
        .inputs
        .iter()
        .map(|input| open_markdup_reader(config, input))
        .collect::<Result<Vec<_>, _>>()?;
    let mut record = bam::Record::new();
    sorter
        .finish_into(|item| {
            let locator =
                decode_locator_payload(&item.payload).map_err(|error| error.to_string())?;
            let reader = readers
                .get_mut(
                    usize::try_from(locator.input_index).map_err(|_| {
                        "MarkDuplicates output input index exceeds usize".to_string()
                    })?,
                )
                .ok_or_else(|| "MarkDuplicates output input index is out of range".to_string())?;
            reader
                .seek(locator.offset)
                .map_err(|error| error.to_string())?;
            match reader.read(&mut record) {
                Some(Ok(())) => {
                    let decision = decisions.decision(locator.record_index).ok_or_else(|| {
                        "missing duplicate decision for sorted output record".to_string()
                    })?;
                    write_bam_record(&mut record, decision, config, writer)
                        .map_err(|error| error.to_string())
                }
                Some(Err(error)) => Err(error.to_string()),
                None => Err("MarkDuplicates output seek did not read a record".to_string()),
            }
        })
        .map_err(MarkDuplicatesError::Operation)?;
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
    if decision.duplicate() {
        record.flags() | DUPLICATE_FLAG
    } else {
        record.flags()
    }
}

fn output_sort_key(
    locator: &OutputRecordLocator,
    qnames: &ByteInterner,
    decision: RecordDecision,
) -> Result<Vec<u8>, MarkDuplicatesError> {
    let qname = qnames.get(locator.qname_id)?;
    let mut bytes = Vec::with_capacity(15 + qname.len());
    bytes.extend_from_slice(&sortable_i32(locator.reference_id));
    bytes.extend_from_slice(&sortable_i64(locator.position));
    bytes.extend_from_slice(qname);
    bytes.push(0);
    bytes.extend_from_slice(&effective_locator_flags(locator, decision).to_be_bytes());
    Ok(bytes)
}

fn effective_locator_flags(locator: &OutputRecordLocator, decision: RecordDecision) -> u16 {
    if decision.duplicate() {
        locator.flags | DUPLICATE_FLAG
    } else {
        locator.flags
    }
}

#[derive(Debug, Clone, Copy)]
struct OutputLocatorPayload {
    input_index: u32,
    record_index: usize,
    offset: i64,
}

fn locator_payload(locator: &OutputRecordLocator) -> Vec<u8> {
    let mut payload = Vec::with_capacity(20);
    payload.extend_from_slice(&locator.input_index.to_le_bytes());
    payload.extend_from_slice(
        &u64::try_from(locator.record_index)
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    payload.extend_from_slice(&locator.offset.to_le_bytes());
    payload
}

fn decode_locator_payload(payload: &[u8]) -> Result<OutputLocatorPayload, MarkDuplicatesError> {
    if payload.len() != 20 {
        return Err(MarkDuplicatesError::Operation(format!(
            "invalid MarkDuplicates output locator payload length: {}",
            payload.len()
        )));
    }
    let input_index = u32::from_le_bytes(payload[0..4].try_into().expect("slice length checked"));
    let record_index = decode_payload_index(&payload[4..12])?;
    let offset = i64::from_le_bytes(payload[12..20].try_into().expect("slice length checked"));
    Ok(OutputLocatorPayload {
        input_index,
        record_index,
        offset,
    })
}

fn write_bam_records(
    records: impl IntoIterator<Item = (bam::Record, RecordDecision)>,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    for (mut record, decision) in records {
        write_bam_record(&mut record, decision, config, writer)?;
    }
    Ok(())
}

fn write_bam_record(
    record: &mut bam::Record,
    decision: RecordDecision,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    let flags = effective_record_flags(record, decision);
    if (config.remove_duplicates && decision.duplicate())
        || (config.remove_sequencing_duplicates && decision.optical_duplicate())
    {
        return Ok(());
    }
    if flags != record.flags() {
        record.set_flags(flags);
    }
    if config.clear_dt {
        clear_duplicate_type_tag(record)?;
    }
    if let Some(duplicate_type) =
        duplicate_type_tag(config, record.flags(), decision.optical_duplicate())
    {
        add_duplicate_type_tag(record, duplicate_type)?;
    }
    if let Some(duplicate_set) = decision.duplicate_set {
        replace_i32_aux(record, b"DS", duplicate_set.size)?;
        replace_i32_aux(record, b"DI", duplicate_set.index)?;
    }
    if config.add_pg_tag_to_reads {
        add_program_group_to_bam_record(record)?;
    }
    writer.write(record)?;
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

fn parse_optical_locations(config: &MarkDuplicatesConfig) -> bool {
    config.read_name_regex.as_deref() != Some("null")
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

fn apply_pair_duplicate_group_members(
    group: &[usize],
    candidates: &[DuplicateCandidate],
    decisions: &mut DuplicateDecisions,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    stats: DuplicateGroupStats,
    config: &MarkDuplicatesConfig,
) -> usize {
    let duplicate_set_tag = (config.tag_duplicate_set_members
        && !config.remove_duplicates
        && stats.has_pair
        && stats.has_multiple_read_names)
        .then(|| DuplicateSetTag {
            size: i32::try_from(stats.unique_read_names).unwrap_or(i32::MAX),
            index: i32::try_from(stats.representative_record_index).unwrap_or(i32::MAX),
        });
    let representative_location = (config.read_name_regex.as_deref() != Some("null"))
        .then_some(candidates[stats.representative_candidate_index].optical_location);
    let pixel_distance = i64::from(config.optical_duplicate_pixel_distance.unwrap_or(100));
    let mut optical_names = HashSet::<InternedBytesId>::default();

    for index in group.iter().copied() {
        let candidate = &candidates[index];
        if let Some(tag) = duplicate_set_tag {
            decisions.set_duplicate_set(candidate.record_index, tag.size, tag.index);
        }
        if candidate.qname_id == stats.representative_qname_id {
            continue;
        }

        if let Some(Some(representative_location)) = representative_location {
            if optical_names.contains(&candidate.qname_id) {
                decisions.mark_optical_duplicate(candidate.record_index);
            } else if let Some(location) = candidate.optical_location
                && representative_location.is_within(&location, pixel_distance)
            {
                optical_names.insert(candidate.qname_id);
                decisions.mark_optical_duplicate(candidate.record_index);
            }
        }

        if candidate.is_pair() {
            summary.duplicate_pair_records += 1;
            library_registry
                .summary_mut(candidate.library_id())
                .duplicate_pair_records += 1;
        } else {
            summary.unpaired_duplicate_records += 1;
            library_registry
                .summary_mut(candidate.library_id())
                .unpaired_duplicate_records += 1;
        }
        decisions.mark_duplicate(candidate.record_index);
    }

    optical_names.len()
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

struct AtomicSamTextOutput {
    final_path: PathBuf,
    temp_path: PathBuf,
    writer: Option<BufWriter<File>>,
    persisted: bool,
}

impl AtomicSamTextOutput {
    fn create(output: &str) -> Result<Self, MarkDuplicatesError> {
        let final_path = PathBuf::from(output);
        let (temp_path, temp_file) = create_atomic_temp_file(&final_path, "markduplicates-sam")?;
        let writer = BufWriter::with_capacity(1024 * 1024, temp_file);
        Ok(Self {
            final_path,
            temp_path,
            writer: Some(writer),
            persisted: false,
        })
    }

    fn write_header_lines(
        &mut self,
        header_lines: &[String],
        add_pg_tag: bool,
    ) -> Result<(), MarkDuplicatesError> {
        let has_markdup_pg = header_lines
            .iter()
            .any(|line| line.starts_with("@PG") && line.contains("ID:MarkDuplicates"));
        for line in header_lines {
            self.write_line(line)?;
        }
        if add_pg_tag && !has_markdup_pg {
            self.write_line("@PG\tID:MarkDuplicates\tPN:MarkDuplicates")?;
        }
        Ok(())
    }

    fn write_line(&mut self, line: &str) -> Result<(), MarkDuplicatesError> {
        let writer = self.writer.as_mut().ok_or_else(|| {
            MarkDuplicatesError::Operation("MarkDuplicates output is closed".into())
        })?;
        writer.write_all(line.as_bytes())?;
        writer.write_all(b"\n")?;
        Ok(())
    }

    fn persist(mut self) -> Result<(), MarkDuplicatesError> {
        if let Some(mut writer) = self.writer.take() {
            writer.flush()?;
        }
        fs::rename(&self.temp_path, &self.final_path)?;
        self.persisted = true;
        Ok(())
    }
}

impl Drop for AtomicSamTextOutput {
    fn drop(&mut self) {
        if !self.persisted {
            let _ = fs::remove_file(&self.temp_path);
        }
    }
}

fn create_atomic_temp_file(
    final_path: &Path,
    prefix: &str,
) -> Result<(PathBuf, File), MarkDuplicatesError> {
    let parent = final_path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("output");
    for attempt in 0..1000u32 {
        let candidate = parent.join(format!(
            ".{file_name}.{prefix}.{}.{}.tmp",
            std::process::id(),
            attempt
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => return Ok((candidate, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(MarkDuplicatesError::Operation(format!(
        "could not create temporary output for {}",
        final_path.display()
    )))
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
    let mut reader = BufReader::with_capacity(64 * 1024, fs::File::open(output)?);
    let mut context = md5::Context::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        context.consume(&buffer[..bytes_read]);
    }
    let digest = context.finalize();
    fs::write(format!("{output}.md5"), format!("{digest:x}"))?;
    Ok(())
}

#[cfg(test)]
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

fn sam_text_duplicate_key(
    fields: &[String],
    flag: u16,
    line_number: usize,
    config: &MarkDuplicatesConfig,
    interner: &mut ByteInterner,
) -> Result<SamTextDuplicateKey, MarkDuplicatesError> {
    let reverse_strand = flag & 0x10 != 0;
    let position = parse_sam_integer(&fields[3], "POS", line_number)? - 1;
    let mate_position = parse_sam_integer(&fields[7], "MATE_POS", line_number)?;
    let template_length = parse_sam_integer(&fields[8], "TLEN", line_number)?;
    let five_prime_position =
        unclipped_five_prime_position(position, &fields[5], reverse_strand, line_number)?;
    let barcode_id = sam_barcode(fields, config).map(|barcode| interner.intern(&barcode));

    Ok(SamTextDuplicateKey {
        reference_id: interner.intern(fields[2].as_bytes()),
        position: five_prime_position,
        mate_reference_id: interner.intern(fields[6].as_bytes()),
        mate_position,
        template_length,
        reverse_strand,
        barcode_id,
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

#[cfg(test)]
fn duplicate_groups(
    candidates: &[DuplicateCandidate],
    config: &MarkDuplicatesConfig,
) -> Result<Vec<Vec<usize>>, MarkDuplicatesError> {
    let keyed_pairs = collate_pair_key_rows(candidates, config)?;
    let mut groups = Vec::<Vec<usize>>::new();
    scan_pair_key_rows(keyed_pairs, config, |group| groups.push(group.to_vec()))?;
    Ok(groups)
}

fn process_pair_duplicate_groups(
    candidates: &[DuplicateCandidate],
    decisions: &mut DuplicateDecisions,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    config: &MarkDuplicatesConfig,
) -> Result<(), MarkDuplicatesError> {
    scan_collated_pair_key_rows(candidates, config, |group| {
        apply_pair_duplicate_group(
            group,
            candidates,
            decisions,
            summary,
            library_registry,
            config,
        );
    })
}

fn apply_pair_duplicate_group(
    group: &[usize],
    candidates: &[DuplicateCandidate],
    decisions: &mut DuplicateDecisions,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    config: &MarkDuplicatesConfig,
) {
    if group.len() < 2 {
        return;
    }
    let stats = DuplicateGroupStats::from_group(group, candidates);
    if !stats.has_multiple_read_names {
        if let Some(set_size) = stats.paired_set_size {
            add_duplicate_set(summary, set_size, Some(set_size));
            if let Some(candidate_index) = group.first() {
                add_duplicate_set(
                    library_registry.summary_mut(candidates[*candidate_index].library_id()),
                    set_size,
                    Some(set_size),
                );
            }
        }
        return;
    }

    let optical_duplicate_read_names = apply_pair_duplicate_group_members(
        group,
        candidates,
        decisions,
        summary,
        library_registry,
        stats,
        config,
    );
    if let Some(set_size) = stats.paired_set_size {
        let optical_names = u64::try_from(optical_duplicate_read_names).unwrap_or(u64::MAX);
        let non_optical_size = (optical_names < set_size).then_some(set_size - optical_names);
        add_duplicate_set(summary, set_size, non_optical_size);
        if let Some(candidate_index) = group.first() {
            let library_summary =
                library_registry.summary_mut(candidates[*candidate_index].library_id());
            add_duplicate_set(library_summary, set_size, non_optical_size);
        }
    }
    summary.read_pair_optical_duplicates += optical_duplicate_read_names as u64;
    if let Some(candidate_index) = group.first() {
        library_registry
            .summary_mut(candidates[*candidate_index].library_id())
            .read_pair_optical_duplicates += optical_duplicate_read_names as u64;
    }
}

#[cfg(test)]
fn collate_pair_key_rows(
    candidates: &[DuplicateCandidate],
    config: &MarkDuplicatesConfig,
) -> Result<Vec<(BamDuplicateKey, [usize; 2])>, MarkDuplicatesError> {
    let mut keyed_pairs = Vec::<(BamDuplicateKey, [usize; 2])>::new();
    collate_pair_key_rows_into(candidates, config, |key, pair_indices| {
        keyed_pairs.push((key, pair_indices));
        Ok(())
    })?;
    Ok(keyed_pairs)
}

fn collate_pair_key_rows_into(
    candidates: &[DuplicateCandidate],
    config: &MarkDuplicatesConfig,
    mut emit_pair: impl FnMut(BamDuplicateKey, [usize; 2]) -> Result<(), MarkDuplicatesError>,
) -> Result<(), MarkDuplicatesError> {
    let mut paired_by_name = HashMap::<InternedBytesId, usize>::default();
    let mut candidate_index = 0usize;
    let max_displaced_pair_records = config.mate_cache_records.max(1);

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
                collate_adjacent_pair_run(candidate_index..run_end, candidates, &mut emit_pair)?
                && !insert_pending_pair_candidate(
                    qname_id,
                    unpaired_index,
                    &mut paired_by_name,
                    max_displaced_pair_records,
                )
            {
                return collate_pair_key_rows_by_qname_into(candidates, config, emit_pair);
            }
        } else {
            for index in candidate_index..run_end {
                if candidates[index].is_pair()
                    && !collate_displaced_pair_candidate(
                        index,
                        candidates,
                        &mut paired_by_name,
                        &mut emit_pair,
                        max_displaced_pair_records,
                    )?
                {
                    return collate_pair_key_rows_by_qname_into(candidates, config, emit_pair);
                }
            }
        }
        candidate_index = run_end;
    }

    Ok(())
}

fn collate_adjacent_pair_run(
    candidate_indices: std::ops::Range<usize>,
    candidates: &[DuplicateCandidate],
    emit_pair: &mut impl FnMut(BamDuplicateKey, [usize; 2]) -> Result<(), MarkDuplicatesError>,
) -> Result<Option<usize>, MarkDuplicatesError> {
    let mut first_index = None::<usize>;
    for candidate_index in candidate_indices {
        if !candidates[candidate_index].is_pair() {
            continue;
        }
        if let Some(first) = first_index.take() {
            emit_pair_key_row(first, candidate_index, candidates, emit_pair)?;
        } else {
            first_index = Some(candidate_index);
        }
    }
    Ok(first_index)
}

fn collate_pair_key_rows_by_qname_into(
    candidates: &[DuplicateCandidate],
    config: &MarkDuplicatesConfig,
    mut emit_pair: impl FnMut(BamDuplicateKey, [usize; 2]) -> Result<(), MarkDuplicatesError>,
) -> Result<(), MarkDuplicatesError> {
    let mut sorter =
        ExternalSorter::new(markdup_sort_config(config, "turbo-picard-markdup-qnames"))
            .map_err(MarkDuplicatesError::Operation)?;
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        if candidate.is_pair() {
            sorter
                .push(
                    qname_sort_key(candidate.qname_id),
                    index_payload(candidate_index),
                )
                .map_err(MarkDuplicatesError::Operation)?;
        }
    }

    let mut pair_order_sorter = ExternalSorter::new(markdup_sort_config(
        config,
        "turbo-picard-markdup-qname-pairs",
    ))
    .map_err(MarkDuplicatesError::Operation)?;
    let mut first_index = None::<usize>;
    let mut current_qname_id = None::<InternedBytesId>;
    sorter
        .finish_into(|item| {
            let qname_id = decode_qname_sort_key(&item.key).map_err(|error| error.to_string())?;
            let candidate_index =
                decode_index_payload(&item.payload).map_err(|error| error.to_string())?;
            if current_qname_id != Some(qname_id) {
                current_qname_id = Some(qname_id);
                first_index = Some(candidate_index);
                return Ok(());
            }
            if let Some(first) = first_index.take() {
                let (key, pair_indices) = pair_key_row(first, candidate_index, candidates);
                pair_order_sorter
                    .push(
                        pair_order_sort_key(pair_indices),
                        pair_order_payload(&key, pair_indices),
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())?;
            } else {
                first_index = Some(candidate_index);
            }
            Ok(())
        })
        .map_err(MarkDuplicatesError::Operation)?;

    pair_order_sorter
        .finish_into(|item| {
            let (key, pair_indices) =
                decode_pair_order_payload(&item.payload).map_err(|error| error.to_string())?;
            emit_pair(key, pair_indices).map_err(|error| error.to_string())
        })
        .map_err(MarkDuplicatesError::Operation)?;
    Ok(())
}

fn collate_displaced_pair_candidate(
    candidate_index: usize,
    candidates: &[DuplicateCandidate],
    paired_by_name: &mut HashMap<InternedBytesId, usize>,
    emit_pair: &mut impl FnMut(BamDuplicateKey, [usize; 2]) -> Result<(), MarkDuplicatesError>,
    max_displaced_pair_records: usize,
) -> Result<bool, MarkDuplicatesError> {
    let candidate = &candidates[candidate_index];
    if let Some(first_index) = paired_by_name.remove(&candidate.qname_id) {
        emit_pair_key_row(first_index, candidate_index, candidates, emit_pair)?;
        Ok(true)
    } else {
        Ok(insert_pending_pair_candidate(
            candidate.qname_id,
            candidate_index,
            paired_by_name,
            max_displaced_pair_records,
        ))
    }
}

fn insert_pending_pair_candidate(
    qname_id: InternedBytesId,
    candidate_index: usize,
    paired_by_name: &mut HashMap<InternedBytesId, usize>,
    max_displaced_pair_records: usize,
) -> bool {
    if paired_by_name.len() >= max_displaced_pair_records {
        return false;
    }
    paired_by_name.insert(qname_id, candidate_index);
    true
}

#[cfg(test)]
fn push_pair_key_row(
    first_index: usize,
    second_index: usize,
    candidates: &[DuplicateCandidate],
    keyed_pairs: &mut Vec<(BamDuplicateKey, [usize; 2])>,
) {
    keyed_pairs.push(pair_key_row(first_index, second_index, candidates));
}

fn emit_pair_key_row(
    first_index: usize,
    second_index: usize,
    candidates: &[DuplicateCandidate],
    emit_pair: &mut impl FnMut(BamDuplicateKey, [usize; 2]) -> Result<(), MarkDuplicatesError>,
) -> Result<(), MarkDuplicatesError> {
    let (key, candidate_indices) = pair_key_row(first_index, second_index, candidates);
    emit_pair(key, candidate_indices)
}

fn pair_key_row(
    first_index: usize,
    second_index: usize,
    candidates: &[DuplicateCandidate],
) -> (BamDuplicateKey, [usize; 2]) {
    let candidate_indices = [first_index, second_index];
    let barcode = first_barcode(candidates, &candidate_indices);
    let key = pair_duplicate_key_bam(
        &candidates[first_index],
        &candidates[second_index],
        candidates[first_index].library_id(),
        barcode,
    );
    (key, candidate_indices)
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

fn emit_completed_pair_group(
    emit_group: &mut impl FnMut(&[usize]),
    current_key: &mut Option<BamDuplicateKey>,
    current_group: &mut Vec<usize>,
    key: BamDuplicateKey,
    pair_indices: [usize; 2],
) {
    if current_key.as_ref() == Some(&key) {
        current_group.extend(pair_indices);
    } else {
        if !current_group.is_empty() {
            emit_group(current_group);
        }
        current_group.clear();
        current_group.extend(pair_indices);
        *current_key = Some(key);
    }
}

#[cfg(test)]
fn scan_pair_key_rows(
    keyed_pairs: impl IntoIterator<Item = (BamDuplicateKey, [usize; 2])>,
    config: &MarkDuplicatesConfig,
    mut emit_group: impl FnMut(&[usize]),
) -> Result<(), MarkDuplicatesError> {
    let mut sorter = ExternalSorter::new(markdup_sort_config(config, "turbo-picard-markdup-pairs"))
        .map_err(MarkDuplicatesError::Operation)?;
    for (key, pair_indices) in keyed_pairs {
        sorter
            .push(duplicate_sort_key(&key), pair_payload(pair_indices))
            .map_err(MarkDuplicatesError::Operation)?;
    }
    finish_pair_key_sorter(sorter, &mut emit_group)
}

fn scan_collated_pair_key_rows(
    candidates: &[DuplicateCandidate],
    config: &MarkDuplicatesConfig,
    mut emit_group: impl FnMut(&[usize]),
) -> Result<(), MarkDuplicatesError> {
    let mut sorter = ExternalSorter::new(markdup_sort_config(config, "turbo-picard-markdup-pairs"))
        .map_err(MarkDuplicatesError::Operation)?;
    collate_pair_key_rows_into(candidates, config, |key, pair_indices| {
        sorter
            .push(duplicate_sort_key(&key), pair_payload(pair_indices))
            .map(|_| ())
            .map_err(MarkDuplicatesError::Operation)
    })?;
    finish_pair_key_sorter(sorter, &mut emit_group)
}

fn finish_pair_key_sorter(
    sorter: ExternalSorter,
    emit_group: &mut impl FnMut(&[usize]),
) -> Result<(), MarkDuplicatesError> {
    let mut current_key = None::<BamDuplicateKey>;
    let mut current_group = Vec::<usize>::new();
    sorter
        .finish_into(|item| {
            let key = decode_duplicate_sort_key(&item.key).map_err(|error| error.to_string())?;
            let pair = decode_pair_payload(&item.payload).map_err(|error| error.to_string())?;
            emit_completed_pair_group(emit_group, &mut current_key, &mut current_group, key, pair);
            Ok(())
        })
        .map_err(MarkDuplicatesError::Operation)?;
    if !current_group.is_empty() {
        emit_group(&current_group);
    }
    Ok(())
}

#[cfg(test)]
fn fragment_duplicate_groups(
    candidates: &[DuplicateCandidate],
    config: &MarkDuplicatesConfig,
) -> Result<Vec<Vec<usize>>, MarkDuplicatesError> {
    let mut groups = Vec::<Vec<usize>>::new();
    scan_fragment_key_rows(fragment_key_rows(candidates), config, |group| {
        groups.push(group.to_vec());
    })?;
    Ok(groups)
}

fn fragment_key_rows(
    candidates: &[DuplicateCandidate],
) -> impl Iterator<Item = (BamDuplicateKey, usize)> + '_ {
    candidates
        .iter()
        .enumerate()
        .map(|(candidate_index, candidate)| (candidate.fragment_duplicate_key(), candidate_index))
}

fn scan_fragment_key_rows(
    keyed_fragments: impl IntoIterator<Item = (BamDuplicateKey, usize)>,
    config: &MarkDuplicatesConfig,
    mut emit_group: impl FnMut(&[usize]),
) -> Result<(), MarkDuplicatesError> {
    let mut current_key = None::<BamDuplicateKey>;
    let mut current_group = Vec::<usize>::new();
    let mut sorter = ExternalSorter::new(markdup_sort_config(
        config,
        "turbo-picard-markdup-fragments",
    ))
    .map_err(MarkDuplicatesError::Operation)?;
    for (key, candidate_index) in keyed_fragments {
        sorter
            .push(duplicate_sort_key(&key), index_payload(candidate_index))
            .map_err(MarkDuplicatesError::Operation)?;
    }
    sorter
        .finish_into(|item| {
            let key = decode_duplicate_sort_key(&item.key).map_err(|error| error.to_string())?;
            let index = decode_index_payload(&item.payload).map_err(|error| error.to_string())?;
            emit_completed_fragment_group(
                &mut emit_group,
                &mut current_key,
                &mut current_group,
                key,
                index,
            );
            Ok(())
        })
        .map_err(MarkDuplicatesError::Operation)?;
    if !current_group.is_empty() {
        emit_group(&current_group);
    }
    Ok(())
}

fn markdup_sort_config(config: &MarkDuplicatesConfig, prefix: &str) -> ExternalSortConfig {
    let tmp_dir = config
        .tmp_dirs
        .first()
        .map(Path::new)
        .map(Path::to_path_buf)
        .unwrap_or_else(env::temp_dir);
    let mut sort_config = ExternalSortConfig::new(tmp_dir);
    sort_config.max_records_in_ram = config.max_records_in_ram.max(1);
    sort_config.prefix = prefix.to_string();
    sort_config
}

fn duplicate_sort_key(key: &BamDuplicateKey) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(42);
    bytes.extend_from_slice(&key.library_id.to_be_bytes());
    bytes.extend_from_slice(&sortable_i32(key.reference_id));
    bytes.extend_from_slice(&sortable_i64(key.position));
    bytes.extend_from_slice(&sortable_i32(key.mate_reference_id));
    bytes.extend_from_slice(&sortable_i64(key.mate_position));
    bytes.extend_from_slice(&sortable_i64(key.template_length));
    bytes.push(u8::from(key.reverse_strand));
    match key.barcode_id {
        Some(barcode_id) => {
            bytes.push(1);
            bytes.extend_from_slice(&barcode_id.to_be_bytes());
        }
        None => {
            bytes.push(0);
            bytes.extend_from_slice(&0_u32.to_be_bytes());
        }
    }
    bytes
}

fn qname_sort_key(qname_id: InternedBytesId) -> Vec<u8> {
    qname_id.to_be_bytes().to_vec()
}

fn decode_qname_sort_key(bytes: &[u8]) -> Result<InternedBytesId, MarkDuplicatesError> {
    if bytes.len() != 4 {
        return Err(MarkDuplicatesError::Operation(format!(
            "invalid MarkDuplicates qname sort key length: {}",
            bytes.len()
        )));
    }
    Ok(u32::from_be_bytes(
        bytes.try_into().expect("slice length checked"),
    ))
}

fn decode_duplicate_sort_key(bytes: &[u8]) -> Result<BamDuplicateKey, MarkDuplicatesError> {
    if bytes.len() != 42 {
        return Err(MarkDuplicatesError::Operation(format!(
            "invalid MarkDuplicates duplicate-key sort payload length: {}",
            bytes.len()
        )));
    }
    let library_id = u32::from_be_bytes(bytes[0..4].try_into().expect("slice length checked"));
    let reference_id = unsortable_i32(bytes[4..8].try_into().expect("slice length checked"));
    let position = unsortable_i64(bytes[8..16].try_into().expect("slice length checked"));
    let mate_reference_id = unsortable_i32(bytes[16..20].try_into().expect("slice length checked"));
    let mate_position = unsortable_i64(bytes[20..28].try_into().expect("slice length checked"));
    let template_length = unsortable_i64(bytes[28..36].try_into().expect("slice length checked"));
    let reverse_strand = match bytes[36] {
        0 => false,
        1 => true,
        value => {
            return Err(MarkDuplicatesError::Operation(format!(
                "invalid MarkDuplicates duplicate-key strand byte: {value}"
            )));
        }
    };
    let barcode_id = match bytes[37] {
        0 => None,
        1 => Some(u32::from_be_bytes(
            bytes[38..42].try_into().expect("slice length checked"),
        )),
        value => {
            return Err(MarkDuplicatesError::Operation(format!(
                "invalid MarkDuplicates duplicate-key barcode tag: {value}"
            )));
        }
    };
    Ok(BamDuplicateKey {
        library_id,
        reference_id,
        position,
        mate_reference_id,
        mate_position,
        template_length,
        reverse_strand,
        barcode_id,
    })
}

fn sortable_i32(value: i32) -> [u8; 4] {
    ((value as u32) ^ 0x8000_0000).to_be_bytes()
}

fn sortable_i64(value: i64) -> [u8; 8] {
    ((value as u64) ^ 0x8000_0000_0000_0000).to_be_bytes()
}

fn unsortable_i32(bytes: [u8; 4]) -> i32 {
    (u32::from_be_bytes(bytes) ^ 0x8000_0000) as i32
}

fn unsortable_i64(bytes: [u8; 8]) -> i64 {
    (u64::from_be_bytes(bytes) ^ 0x8000_0000_0000_0000) as i64
}

fn pair_payload(pair_indices: [usize; 2]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(16);
    payload.extend_from_slice(
        &u64::try_from(pair_indices[0])
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    payload.extend_from_slice(
        &u64::try_from(pair_indices[1])
            .unwrap_or(u64::MAX)
            .to_le_bytes(),
    );
    payload
}

fn pair_order_sort_key(pair_indices: [usize; 2]) -> Vec<u8> {
    u64::try_from(pair_indices[1])
        .unwrap_or(u64::MAX)
        .to_be_bytes()
        .to_vec()
}

fn pair_order_payload(key: &BamDuplicateKey, pair_indices: [usize; 2]) -> Vec<u8> {
    let mut payload = Vec::with_capacity(58);
    payload.extend_from_slice(&duplicate_sort_key(key));
    payload.extend_from_slice(&pair_payload(pair_indices));
    payload
}

fn decode_pair_order_payload(
    payload: &[u8],
) -> Result<(BamDuplicateKey, [usize; 2]), MarkDuplicatesError> {
    if payload.len() != 58 {
        return Err(MarkDuplicatesError::Operation(format!(
            "invalid MarkDuplicates qname-pair sort payload length: {}",
            payload.len()
        )));
    }
    Ok((
        decode_duplicate_sort_key(&payload[..42])?,
        decode_pair_payload(&payload[42..])?,
    ))
}

fn decode_pair_payload(payload: &[u8]) -> Result<[usize; 2], MarkDuplicatesError> {
    if payload.len() != 16 {
        return Err(MarkDuplicatesError::Operation(format!(
            "invalid MarkDuplicates pair sort payload length: {}",
            payload.len()
        )));
    }
    Ok([
        decode_payload_index(&payload[0..8])?,
        decode_payload_index(&payload[8..16])?,
    ])
}

fn index_payload(index: usize) -> Vec<u8> {
    u64::try_from(index)
        .unwrap_or(u64::MAX)
        .to_le_bytes()
        .to_vec()
}

fn decode_index_payload(payload: &[u8]) -> Result<usize, MarkDuplicatesError> {
    if payload.len() != 8 {
        return Err(MarkDuplicatesError::Operation(format!(
            "invalid MarkDuplicates fragment sort payload length: {}",
            payload.len()
        )));
    }
    decode_payload_index(payload)
}

fn decode_payload_index(payload: &[u8]) -> Result<usize, MarkDuplicatesError> {
    let value = u64::from_le_bytes(payload.try_into().expect("slice length checked"));
    usize::try_from(value).map_err(|_| {
        MarkDuplicatesError::Operation("MarkDuplicates candidate index exceeds usize".to_string())
    })
}

fn emit_completed_fragment_group(
    emit_group: &mut impl FnMut(&[usize]),
    current_key: &mut Option<BamDuplicateKey>,
    current_group: &mut Vec<usize>,
    key: BamDuplicateKey,
    candidate_index: usize,
) {
    if current_key.as_ref() == Some(&key) {
        current_group.push(candidate_index);
    } else {
        if !current_group.is_empty() {
            emit_group(current_group);
        }
        current_group.clear();
        current_group.push(candidate_index);
        *current_key = Some(key);
    }
}

fn apply_fragment_duplicate_group(
    group: &[usize],
    candidates: &[DuplicateCandidate],
    decisions: &mut DuplicateDecisions,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
) {
    if group.len() < 2 {
        return;
    }
    let stats = DuplicateGroupStats::from_group(group, candidates);
    if !stats.has_multiple_read_names {
        return;
    }

    if stats.has_pair {
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
        return;
    }

    for candidate_index in group.iter().copied() {
        if candidates[candidate_index].qname_id == stats.representative_qname_id {
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

fn mark_fragment_duplicate_groups(
    candidates: &[DuplicateCandidate],
    decisions: &mut DuplicateDecisions,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    config: &MarkDuplicatesConfig,
) -> Result<(), MarkDuplicatesError> {
    scan_fragment_key_rows(fragment_key_rows(candidates), config, |group| {
        apply_fragment_duplicate_group(group, candidates, decisions, summary, library_registry);
    })
}

fn mark_unpaired_duplicate_record(
    candidate_index: usize,
    candidates: &[DuplicateCandidate],
    decisions: &mut DuplicateDecisions,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
) {
    let candidate = &candidates[candidate_index];
    let record_index = candidate.record_index;
    if decisions.duplicate(record_index) {
        return;
    }
    summary.unpaired_duplicate_records += 1;
    library_registry
        .summary_mut(candidate.library_id())
        .unpaired_duplicate_records += 1;
    decisions.mark_duplicate(record_index);
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
        .find_map(|index| candidates[*index].fragment_key.barcode_id)
}

fn bam_barcode_id(
    record: &bam::Record,
    config: &MarkDuplicatesConfig,
    interner: &mut ByteInterner,
) -> Option<InternedBytesId> {
    if let Some(tag) = config.barcode_tag.as_deref() {
        return bam_tag_value_id(record, tag, interner);
    }

    let barcode = combined_barcode(
        config
            .read_one_barcode_tag
            .as_deref()
            .and_then(|tag| bam_tag_value(record, tag)),
        config
            .read_two_barcode_tag
            .as_deref()
            .and_then(|tag| bam_tag_value(record, tag)),
    )?;
    Some(interner.intern(&barcode))
}

fn bam_tag_value_id(
    record: &bam::Record,
    tag: &str,
    interner: &mut ByteInterner,
) -> Option<InternedBytesId> {
    match record.aux(tag.as_bytes()) {
        Ok(Aux::String(value)) => Some(interner.intern(value.as_bytes())),
        Ok(Aux::Char(value)) => Some(interner.intern(&[value])),
        _ => None,
    }
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

fn pair_duplicate_key_bam(
    first: &DuplicateCandidate,
    second: &DuplicateCandidate,
    library_id: LibraryId,
    barcode_id: Option<InternedBytesId>,
) -> BamDuplicateKey {
    let (left, right) = if (first.fragment_key.reference_id, first.fragment_key.position)
        <= (
            second.fragment_key.reference_id,
            second.fragment_key.position,
        ) {
        (first, second)
    } else {
        (second, first)
    };

    BamDuplicateKey {
        library_id,
        reference_id: left.fragment_key.reference_id,
        position: left.fragment_key.position,
        mate_reference_id: right.fragment_key.reference_id,
        mate_position: right.fragment_key.position,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DuplicateGroupStats {
    has_multiple_read_names: bool,
    has_pair: bool,
    unique_read_names: usize,
    paired_set_size: Option<u64>,
    representative_candidate_index: usize,
    representative_qname_id: InternedBytesId,
    representative_record_index: usize,
}

impl DuplicateGroupStats {
    fn from_group(group: &[usize], candidates: &[DuplicateCandidate]) -> Self {
        const SMALL_GROUP_NAME_CAPACITY: usize = 4;

        if group.len() <= SMALL_GROUP_NAME_CAPACITY {
            return Self::from_small_group::<SMALL_GROUP_NAME_CAPACITY>(group, candidates);
        }

        let mut scores_by_name = HashMap::<InternedBytesId, DuplicateNameStats>::default();
        let mut has_pair = false;

        for index in group.iter().copied() {
            let candidate = &candidates[index];
            has_pair |= candidate.is_pair();
            let entry =
                scores_by_name
                    .entry(candidate.qname_id)
                    .or_insert_with(|| DuplicateNameStats {
                        qname_id: candidate.qname_id,
                        first_candidate_index: index,
                        duplicate_score: 0,
                        min_record_index: candidate.record_index,
                    });
            entry.duplicate_score += candidate.duplicate_score;
            entry.min_record_index = entry.min_record_index.min(candidate.record_index);
        }

        Self::from_name_stats(scores_by_name.into_values(), has_pair, candidates)
    }

    fn from_small_group<const CAPACITY: usize>(
        group: &[usize],
        candidates: &[DuplicateCandidate],
    ) -> Self {
        let mut name_stats = [None::<DuplicateNameStats>; CAPACITY];
        let mut unique_name_count = 0usize;
        let mut has_pair = false;

        for index in group.iter().copied() {
            let candidate = &candidates[index];
            has_pair |= candidate.is_pair();
            let existing_position = name_stats[..unique_name_count]
                .iter()
                .position(|stats| stats.is_some_and(|stats| stats.qname_id == candidate.qname_id));
            if let Some(position) = existing_position {
                let stats = name_stats[position]
                    .as_mut()
                    .expect("occupied prefix contains name stats");
                stats.duplicate_score += candidate.duplicate_score;
                stats.min_record_index = stats.min_record_index.min(candidate.record_index);
            } else {
                name_stats[unique_name_count] = Some(DuplicateNameStats {
                    qname_id: candidate.qname_id,
                    first_candidate_index: index,
                    duplicate_score: candidate.duplicate_score,
                    min_record_index: candidate.record_index,
                });
                unique_name_count += 1;
            }
        }

        Self::from_name_stats(
            name_stats.into_iter().take(unique_name_count).flatten(),
            has_pair,
            candidates,
        )
    }

    fn from_name_stats(
        name_stats: impl IntoIterator<Item = DuplicateNameStats>,
        has_pair: bool,
        candidates: &[DuplicateCandidate],
    ) -> Self {
        let mut unique_name_count = 0usize;
        let mut representative = None::<DuplicateNameStats>;

        for stats in name_stats {
            unique_name_count += 1;
            let should_replace = representative.is_none_or(|current| {
                stats.duplicate_score > current.duplicate_score
                    || (stats.duplicate_score == current.duplicate_score
                        && candidates[stats.first_candidate_index].record_index
                            < candidates[current.first_candidate_index].record_index)
            });
            if should_replace {
                representative = Some(stats);
            }
        }

        let representative = representative.expect("non-empty duplicate group");
        let paired_set_size = has_pair
            .then(|| u64::try_from(unique_name_count).ok())
            .flatten()
            .filter(|size| *size > 0);

        Self {
            has_multiple_read_names: unique_name_count > 1,
            has_pair,
            unique_read_names: unique_name_count,
            paired_set_size,
            representative_candidate_index: representative.first_candidate_index,
            representative_qname_id: candidates[representative.first_candidate_index].qname_id,
            representative_record_index: representative.min_record_index,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct DuplicateNameStats {
    qname_id: InternedBytesId,
    first_candidate_index: usize,
    duplicate_score: u64,
    min_record_index: usize,
}

#[cfg(test)]
fn paired_duplicate_set_size(group: &[usize], candidates: &[DuplicateCandidate]) -> Option<u64> {
    DuplicateGroupStats::from_group(group, candidates).paired_set_size
}

#[cfg(test)]
fn best_duplicate_representative_index(
    group: &[usize],
    candidates: &[DuplicateCandidate],
) -> usize {
    DuplicateGroupStats::from_group(group, candidates).representative_candidate_index
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
            mate_cache_records: 500_000,
            tmp_dirs: Vec::new(),
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

    fn empty_summary() -> MarkDuplicatesSummary {
        MarkDuplicatesSummary {
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
        }
    }

    fn summary_and_library_registry() -> (MarkDuplicatesSummary, LibraryRegistry) {
        let mut registry = LibraryRegistry::new();
        registry.intern("Unknown Library");
        (empty_summary(), registry)
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

    fn assert_malformed_sam_text_key_err(
        result: Result<SamTextDuplicateKey, MarkDuplicatesError>,
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
        assert!(
            reason.contains(field),
            "expected reason {reason:?} to contain {field:?}"
        );
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
                DuplicateCandidate::from_record(index, record, 0, qname_id, None, true)
            })
            .collect();
        (candidates, qnames)
    }

    #[test]
    fn duplicate_candidate_caches_fragment_key() {
        let cigar = bam::record::CigarString(vec![
            bam::record::Cigar::Match(5),
            bam::record::Cigar::SoftClip(3),
        ]);
        let mut record = bam::Record::new();
        record.set(b"frag-a", Some(&cigar), b"AAAACCCC", b"FFFFFFFF");
        record.set_flags(0x10);
        record.set_tid(2);
        record.set_pos(100);
        let candidate = DuplicateCandidate::from_record(4, &record, 11, 7, Some(13), true);

        assert!(!candidate.is_pair());
        assert!(candidate.reverse_strand());
        assert_eq!(
            candidate.fragment_key,
            FragmentDuplicateKey {
                library_id: 11,
                reference_id: 2,
                position: 107,
                barcode_id: Some(13),
            }
        );
        assert_eq!(
            candidate.fragment_duplicate_key(),
            BamDuplicateKey {
                library_id: 11,
                reference_id: 2,
                position: 107,
                mate_reference_id: -1,
                mate_position: -1,
                template_length: 0,
                reverse_strand: true,
                barcode_id: Some(13),
            }
        );
    }

    #[test]
    fn duplicate_candidate_caches_pair_eligibility() {
        let mut paired = record_with_name_and_flags(b"paired", 0x1 | 0x2);
        paired.set_tid(0);
        paired.set_pos(10);
        let mut mate_unmapped = record_with_name_and_flags(b"mate-unmapped", 0x1 | 0x8);
        mate_unmapped.set_tid(0);
        mate_unmapped.set_pos(20);

        let paired_candidate = DuplicateCandidate::from_record(0, &paired, 0, 1, None, true);
        let mate_unmapped_candidate =
            DuplicateCandidate::from_record(1, &mate_unmapped, 0, 2, None, true);

        assert!(paired_candidate.is_pair());
        assert!(!mate_unmapped_candidate.is_pair());
    }

    #[test]
    fn duplicate_candidate_skips_optical_location_when_disabled() {
        let record = record_with_name_and_flags(b"INST:1:2:3:4", 0);

        let parsed = DuplicateCandidate::from_record(0, &record, 0, 1, None, true);
        let skipped = DuplicateCandidate::from_record(0, &record, 0, 1, None, false);

        assert_eq!(
            parsed.optical_location,
            Some(ReadLocation {
                tile: 2,
                x: 3,
                y: 4,
            })
        );
        assert_eq!(skipped.optical_location, None);
    }

    #[test]
    fn byte_interner_shares_key_and_value_storage() {
        let mut interner = ByteInterner::default();

        let id = interner.intern(b"read-a");
        assert_eq!(id, interner.intern(b"read-a"));

        let key = interner.ids.keys().next().expect("interned key");
        assert!(Rc::ptr_eq(
            key,
            &interner.values[usize::try_from(id).unwrap()]
        ));
    }

    #[test]
    fn record_decision_packs_duplicate_flags() {
        let empty = RecordDecision::default();
        assert!(!empty.duplicate());
        assert!(!empty.optical_duplicate());
        assert!(empty.duplicate_set.is_none());

        let decision = RecordDecision {
            flags: RecordDecision::DUPLICATE | RecordDecision::OPTICAL_DUPLICATE,
            duplicate_set: Some(DuplicateSetTag { size: 7, index: 3 }),
        };
        assert!(decision.duplicate());
        assert!(decision.optical_duplicate());
        assert_eq!(decision.flags, 0b0000_0011);
        assert_eq!(
            decision.duplicate_set,
            Some(DuplicateSetTag { size: 7, index: 3 })
        );
    }

    #[test]
    fn duplicate_decisions_store_flags_in_bitsets_and_tags_sparsely() {
        let mut decisions = DuplicateDecisions::new(65);
        decisions.mark_duplicate(1);
        decisions.mark_optical_duplicate(64);
        decisions.set_duplicate_set(3, 9, 4);
        decisions.mark_duplicate(65);
        decisions.mark_optical_duplicate(65);
        decisions.set_duplicate_set(65, 7, 2);

        assert_eq!(decisions.len(), 65);
        assert_eq!(decisions.duplicate_flags.len(), 2);
        assert_eq!(decisions.optical_duplicate_flags.len(), 2);
        assert!(decisions.decision(1).expect("decision").duplicate());
        assert!(
            decisions
                .decision(64)
                .expect("decision")
                .optical_duplicate()
        );
        assert_eq!(
            decisions.decision(3).expect("decision").duplicate_set,
            Some(DuplicateSetTag { size: 9, index: 4 })
        );
        assert!(decisions.decision(65).is_none());
        assert_eq!(decisions.duplicate_sets.len(), 1);
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
    fn duplicate_group_stats_computes_name_metrics_in_one_pass() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[10], 0x1),
            record_with_name_and_qualities(b"dup-a", &[20], 0x1),
            record_with_name_and_qualities(b"dup-b", &[35], 0x1),
            record_with_name_and_qualities(b"dup-c", &[5], 0x0),
        ];
        let candidates = candidates_for_records(&records);

        let stats = DuplicateGroupStats::from_group(&[0, 1, 2, 3], &candidates);

        assert!(stats.has_multiple_read_names);
        assert!(stats.has_pair);
        assert_eq!(stats.unique_read_names, 3);
        assert_eq!(stats.paired_set_size, Some(3));
        assert_eq!(stats.representative_candidate_index, 2);
        assert_eq!(stats.representative_qname_id, candidates[2].qname_id);
        assert_eq!(
            stats.representative_record_index,
            candidates[2].record_index
        );
    }

    #[test]
    fn duplicate_group_stats_handles_large_name_sets() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[10], 0x1),
            record_with_name_and_qualities(b"dup-b", &[15], 0x1),
            record_with_name_and_qualities(b"dup-c", &[20], 0x1),
            record_with_name_and_qualities(b"dup-d", &[25], 0x0),
            record_with_name_and_qualities(b"dup-e", &[20], 0x1),
        ];
        let candidates = candidates_for_records(&records);

        let stats = DuplicateGroupStats::from_group(&[0, 1, 2, 3, 4], &candidates);

        assert!(stats.has_multiple_read_names);
        assert!(stats.has_pair);
        assert_eq!(stats.unique_read_names, 5);
        assert_eq!(stats.paired_set_size, Some(5));
        assert_eq!(stats.representative_candidate_index, 3);
        assert_eq!(stats.representative_qname_id, candidates[3].qname_id);
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

        let groups = duplicate_groups(&candidates, &sam_markdup_config())
            .expect("pair duplicate-key sorting succeeds");

        assert_eq!(groups, vec![vec![0, 1, 2, 3], vec![4, 5]]);
    }

    #[test]
    fn duplicate_groups_uses_external_sorter_for_forced_pair_runs() {
        let records = [
            record_with_name_flags_and_position(b"z-pair", 0x1, 10),
            record_with_name_flags_and_position(b"z-pair", 0x1, 20),
            record_with_name_flags_and_position(b"a-pair", 0x1, 10),
            record_with_name_flags_and_position(b"a-pair", 0x1, 20),
            record_with_name_flags_and_position(b"later-pair", 0x1, 30),
            record_with_name_flags_and_position(b"later-pair", 0x1, 40),
        ];
        let candidates = candidates_for_records(&records);
        let tmp = tempfile::tempdir().expect("tempdir exists");
        let config = MarkDuplicatesConfig {
            max_records_in_ram: 1,
            tmp_dirs: vec![tmp.path().display().to_string()],
            ..sam_markdup_config()
        };

        let groups = duplicate_groups(&candidates, &config).expect("pair sorting succeeds");

        assert_eq!(groups, vec![vec![0, 1, 2, 3], vec![4, 5]]);
        assert!(
            fs::read_dir(tmp.path())
                .expect("tmp dir remains readable")
                .next()
                .is_none(),
            "external sorter cleans MarkDuplicates pair runs"
        );
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
        let config = sam_markdup_config();

        assert_eq!(
            collate_pair_key_rows(&candidates, &config).expect("pair collation succeeds"),
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
        let config = sam_markdup_config();

        assert_eq!(
            collate_pair_key_rows(&candidates, &config).expect("pair collation succeeds"),
            collate_pair_key_rows_legacy(&candidates)
        );
    }

    #[test]
    fn pair_collation_falls_back_to_compact_qname_sort_when_cache_limit_is_reached() {
        let records = [
            record_with_name_flags_and_position(b"pending-a", 0x1, 10),
            record_with_name_flags_and_position(b"pending-b", 0x1, 20),
            record_with_name_flags_and_position(b"pending-c", 0x1, 50),
            record_with_name_flags_and_position(b"pending-b", 0x1, 30),
            record_with_name_flags_and_position(b"pending-a", 0x1, 40),
        ];
        let candidates = candidates_for_records(&records);
        let tmp = tempfile::tempdir().expect("tempdir exists");
        let config = MarkDuplicatesConfig {
            max_records_in_ram: 2,
            mate_cache_records: 2,
            tmp_dirs: vec![tmp.path().display().to_string()],
            ..sam_markdup_config()
        };

        assert_eq!(
            collate_pair_key_rows(&candidates, &config).expect("pair collation succeeds"),
            collate_pair_key_rows_legacy(&candidates)
        );
        assert!(
            fs::read_dir(tmp.path())
                .expect("tmp dir remains readable")
                .next()
                .is_none(),
            "external sorter cleans MarkDuplicates qname fallback runs"
        );
    }

    #[test]
    fn pair_collation_uses_mate_cache_limit_independent_of_sort_run_limit() {
        let records = [
            record_with_name_flags_and_position(b"pending-a", 0x1, 10),
            record_with_name_flags_and_position(b"pending-b", 0x1, 20),
            record_with_name_flags_and_position(b"pending-c", 0x1, 50),
            record_with_name_flags_and_position(b"pending-b", 0x1, 30),
            record_with_name_flags_and_position(b"pending-a", 0x1, 40),
        ];
        let candidates = candidates_for_records(&records);
        let tmp = tempfile::tempdir().expect("tempdir exists");
        let config = MarkDuplicatesConfig {
            max_records_in_ram: 64,
            mate_cache_records: 2,
            tmp_dirs: vec![tmp.path().display().to_string()],
            ..sam_markdup_config()
        };

        assert_eq!(
            collate_pair_key_rows(&candidates, &config).expect("pair collation succeeds"),
            collate_pair_key_rows_legacy(&candidates)
        );
    }

    #[test]
    fn duplicate_sort_key_preserves_bam_duplicate_key_order() {
        let mut keys = vec![
            BamDuplicateKey {
                library_id: 1,
                reference_id: -1,
                position: 0,
                mate_reference_id: 2,
                mate_position: -10,
                template_length: 10,
                reverse_strand: false,
                barcode_id: None,
            },
            BamDuplicateKey {
                library_id: 1,
                reference_id: 0,
                position: -5,
                mate_reference_id: 2,
                mate_position: -10,
                template_length: 10,
                reverse_strand: false,
                barcode_id: None,
            },
            BamDuplicateKey {
                library_id: 1,
                reference_id: 0,
                position: -5,
                mate_reference_id: 2,
                mate_position: -10,
                template_length: 10,
                reverse_strand: true,
                barcode_id: None,
            },
            BamDuplicateKey {
                library_id: 1,
                reference_id: 0,
                position: -5,
                mate_reference_id: 2,
                mate_position: -10,
                template_length: 10,
                reverse_strand: true,
                barcode_id: Some(1),
            },
            BamDuplicateKey {
                library_id: 0,
                reference_id: i32::MAX,
                position: i64::MAX,
                mate_reference_id: i32::MIN,
                mate_position: i64::MIN,
                template_length: -1,
                reverse_strand: true,
                barcode_id: Some(2),
            },
        ];
        let mut encoded = keys.clone();

        keys.sort();
        encoded.sort_by_key(duplicate_sort_key);

        assert_eq!(encoded, keys);
        for key in keys {
            let decoded =
                decode_duplicate_sort_key(&duplicate_sort_key(&key)).expect("key decodes");
            assert_eq!(decoded, key);
        }
    }

    #[test]
    fn pair_order_payload_round_trips_duplicate_key_and_pair_indices() {
        let key = BamDuplicateKey {
            library_id: 7,
            reference_id: 3,
            position: 11,
            mate_reference_id: 4,
            mate_position: 17,
            template_length: -29,
            reverse_strand: true,
            barcode_id: Some(5),
        };
        let pair_indices = [13, 19];

        let decoded = decode_pair_order_payload(&pair_order_payload(&key, pair_indices))
            .expect("qname-pair payload decodes");

        assert_eq!(decoded, (key, pair_indices));
        assert_eq!(pair_order_sort_key([1, 2]), pair_order_sort_key([0, 2]));
        assert!(pair_order_sort_key([0, 2]) < pair_order_sort_key([0, 3]));
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

        let groups = fragment_duplicate_groups(&candidates, &sam_markdup_config())
            .expect("fragment duplicate-key sorting succeeds");

        assert_eq!(groups, vec![vec![1, 2], vec![0, 3]]);
    }

    #[test]
    fn fragment_duplicate_groups_uses_external_sorter_for_forced_runs() {
        let records = [
            record_with_name_flags_and_position(b"later-a", 0x0, 30),
            record_with_name_flags_and_position(b"dup-a", 0x0, 10),
            record_with_name_flags_and_position(b"dup-b", 0x0, 10),
            record_with_name_flags_and_position(b"later-b", 0x0, 30),
        ];
        let candidates = candidates_for_records(&records);
        let tmp = tempfile::tempdir().expect("tempdir exists");
        let config = MarkDuplicatesConfig {
            max_records_in_ram: 1,
            tmp_dirs: vec![tmp.path().display().to_string()],
            ..sam_markdup_config()
        };

        let groups =
            fragment_duplicate_groups(&candidates, &config).expect("fragment sorting succeeds");

        assert_eq!(groups, vec![vec![1, 2], vec![0, 3]]);
        assert!(
            fs::read_dir(tmp.path())
                .expect("tmp dir remains readable")
                .next()
                .is_none(),
            "external sorter cleans MarkDuplicates fragment runs"
        );
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
        let mut decisions = DuplicateDecisions::new(records.len());
        let stats = DuplicateGroupStats::from_group(&[0, 1, 2, 3], &candidates);
        let config = MarkDuplicatesConfig {
            tag_duplicate_set_members: true,
            ..sam_markdup_config()
        };
        let (mut summary, mut library_registry) = summary_and_library_registry();

        apply_pair_duplicate_group_members(
            &[0, 1, 2, 3],
            &candidates,
            &mut decisions,
            &mut summary,
            &mut library_registry,
            stats,
            &config,
        );

        for index in [0usize, 1, 2, 3] {
            assert_eq!(
                decisions
                    .decision(index)
                    .and_then(|decision| decision.duplicate_set),
                Some(DuplicateSetTag { size: 3, index: 0 })
            );
        }
    }

    #[test]
    fn add_duplicate_set_member_tags_skips_groups_without_paired_record() {
        let records = [
            record_with_name_and_flags(b"dup-a", 0x0),
            record_with_name_and_flags(b"dup-a", 0x0),
        ];
        let candidates = candidates_for_records(&records);
        let mut decisions = DuplicateDecisions::new(records.len());
        let stats = DuplicateGroupStats::from_group(&[0, 1], &candidates);
        let config = MarkDuplicatesConfig {
            tag_duplicate_set_members: true,
            ..sam_markdup_config()
        };
        let (mut summary, mut library_registry) = summary_and_library_registry();

        apply_pair_duplicate_group_members(
            &[0, 1],
            &candidates,
            &mut decisions,
            &mut summary,
            &mut library_registry,
            stats,
            &config,
        );

        assert!(
            decisions
                .decision(0)
                .and_then(|decision| decision.duplicate_set)
                .is_none()
        );
        assert!(
            decisions
                .decision(1)
                .and_then(|decision| decision.duplicate_set)
                .is_none()
        );
    }

    #[test]
    fn pair_group_member_actions_count_unique_optical_read_names() {
        let records = [
            record_with_name_and_qualities(b"INST:1:FC:1:1101:100:100", &[40], 0x1),
            record_with_name_and_qualities(b"INST:1:FC:1:1101:105:105", &[20], 0x1),
            record_with_name_and_qualities(b"INST:1:FC:1:1101:105:105", &[20], 0x1),
        ];
        let candidates = candidates_for_records(&records);
        let stats = DuplicateGroupStats::from_group(&[0, 1, 2], &candidates);
        let config = MarkDuplicatesConfig {
            read_name_regex: None,
            optical_duplicate_pixel_distance: Some(100),
            ..sam_markdup_config()
        };
        let mut decisions = DuplicateDecisions::new(records.len());
        let (mut summary, mut library_registry) = summary_and_library_registry();

        assert_eq!(
            candidates[0].optical_location,
            Some(ReadLocation {
                tile: 1101,
                x: 100,
                y: 100
            })
        );
        let optical_read_names = apply_pair_duplicate_group_members(
            &[0, 1, 2],
            &candidates,
            &mut decisions,
            &mut summary,
            &mut library_registry,
            stats,
            &config,
        );

        assert_eq!(optical_read_names, 1);
        assert!(!decisions.decision(0).expect("decision").optical_duplicate());
        assert!(decisions.decision(1).expect("decision").optical_duplicate());
        assert!(decisions.decision(2).expect("decision").optical_duplicate());
        assert_eq!(summary.duplicate_pair_records, 2);
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

    #[test]
    fn sam_text_duplicate_key_interns_repeated_text_fields() {
        let mut config = sam_markdup_config();
        config.barcode_tag = Some("BC".to_string());
        let mut fields = valid_sam_fields();
        fields[6] = "chr1".to_string();
        fields.push("BC:Z:ACGT".to_string());
        let mut interner = ByteInterner::default();

        let first =
            sam_text_duplicate_key(&fields, 0, 37, &config, &mut interner).expect("first key");
        let second =
            sam_text_duplicate_key(&fields, 0, 37, &config, &mut interner).expect("second key");

        assert_eq!(first, second);
        assert_eq!(first.reference_id, first.mate_reference_id);
        assert_eq!(interner.values.len(), 2);
        assert_eq!(
            interner.values[usize::try_from(first.reference_id).unwrap()].as_ref(),
            b"chr1"
        );
        assert_eq!(
            interner.values[usize::try_from(first.barcode_id.unwrap()).unwrap()].as_ref(),
            b"ACGT"
        );
    }

    #[test]
    fn bam_barcode_id_interns_single_tag_without_combining() {
        let mut config = sam_markdup_config();
        config.barcode_tag = Some("BC".to_string());
        let mut record = record_with_name_and_flags(b"read", 0);
        record
            .push_aux(b"BC", Aux::String("ACGT"))
            .expect("push barcode");
        let mut interner = ByteInterner::default();

        let first = bam_barcode_id(&record, &config, &mut interner).expect("barcode id");
        let second = bam_barcode_id(&record, &config, &mut interner).expect("barcode id");

        assert_eq!(first, second);
        assert_eq!(interner.values.len(), 1);
        assert_eq!(
            interner.values[usize::try_from(first).unwrap()].as_ref(),
            b"ACGT"
        );
    }

    #[test]
    fn bam_barcode_id_interns_char_tags() {
        let mut config = sam_markdup_config();
        config.barcode_tag = Some("BC".to_string());
        let mut record = record_with_name_and_flags(b"read", 0);
        record
            .push_aux(b"BC", Aux::Char(b'A'))
            .expect("push barcode");
        let mut interner = ByteInterner::default();

        let barcode_id = bam_barcode_id(&record, &config, &mut interner).expect("barcode id");

        assert_eq!(
            interner.values[usize::try_from(barcode_id).unwrap()].as_ref(),
            b"A"
        );
    }

    #[test]
    fn sam_text_duplicate_key_preserves_malformed_cigar_errors() {
        let mut fields = valid_sam_fields();
        fields[5] = "0M".to_string();
        let mut interner = ByteInterner::default();

        let key = sam_text_duplicate_key(&fields, 0, 41, &sam_markdup_config(), &mut interner);

        assert_malformed_sam_text_key_err(key, 41, "CIGAR");
        assert!(interner.values.is_empty());
    }

    #[test]
    fn output_sort_key_uses_interned_qname_bytes() {
        let mut qnames = ByteInterner::default();
        let qname_id = qnames.intern(b"read-a");
        assert_eq!(qname_id, qnames.intern(b"read-a"));
        assert_eq!(qnames.values.len(), 1);
        let locator = OutputRecordLocator {
            input_index: 0,
            record_index: 7,
            offset: 123,
            reference_id: 1,
            position: 99,
            qname_id,
            flags: 0,
        };

        let decision = RecordDecision {
            flags: RecordDecision::DUPLICATE,
            duplicate_set: None,
        };
        let key = output_sort_key(&locator, &qnames, decision).expect("sort key");

        assert_eq!(&key[12..18], b"read-a");
        assert_eq!(&key[19..21], &DUPLICATE_FLAG.to_be_bytes());
    }

    #[test]
    fn output_sort_key_rejects_unknown_qname_id() {
        let qnames = ByteInterner::default();
        let locator = OutputRecordLocator {
            input_index: 0,
            record_index: 7,
            offset: 123,
            reference_id: 1,
            position: 99,
            qname_id: 42,
            flags: 0,
        };

        let error =
            output_sort_key(&locator, &qnames, RecordDecision::default()).expect_err("error");

        assert!(
            error
                .to_string()
                .contains("interned MarkDuplicates byte id 42 is out of range"),
            "{error}"
        );
    }

    #[test]
    fn output_record_locator_layout_stays_compact() {
        assert_eq!(std::mem::size_of::<OutputRecordLocator>(), 40);
    }

    #[test]
    fn duplicate_candidate_layout_stays_compact() {
        assert_eq!(std::mem::size_of::<FragmentDuplicateKey>(), 24);
        assert_eq!(std::mem::size_of::<DuplicateCandidate>(), 80);
    }
}
