#![forbid(unsafe_code)]

use regex::Regex;
use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::record::Aux;
use rust_htslib::bam::{self, Read, index};
use rustc_hash::{FxHashMap as HashMap, FxHashSet as HashSet};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::fs::File;
use std::io::{BufWriter, Cursor, Read as IoRead, Write};
use std::num::NonZeroU32;
use std::path::Path;
use tempfile::{Builder as TempDirBuilder, TempDir, tempdir};
use thiserror::Error;
use turbo_picard_core::external_sort::{ExternalSortConfig, ExternalSorter, SortItem};
use turbo_picard_core::hts_io;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;

const DUPLICATE_FLAG: u16 = 0x400;
const UNMAPPED_FLAG: u16 = 0x4;
const UNCOMPUTED_QUALITY_SCORE: u64 = u64::MAX;
const COMPACT_MARKDUP_MAX_RECORDS: usize = 100_000;
type LibraryId = u32;
type BarcodeId = NonZeroU32;

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
    optical_sets: u64,
    non_optical_sets: u64,
}

#[derive(Debug, Error)]
pub enum MarkDuplicatesError {
    #[error(
        "unsupported MarkDuplicates input format for {0}; this engine supports BAM inputs and single SAM text input"
    )]
    UnsupportedInputFormat(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Htslib(#[from] rust_htslib::errors::Error),
    #[error("{0}")]
    Operation(String),
    #[error("malformed SAM at line {line_number}: {reason}")]
    MalformedSam { line_number: usize, reason: String },
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
    barcode: Option<Vec<u8>>,
}

/// The immutable subset of a BAM record needed by duplicate decisions.
///
/// Keeping this separate from `bam::Record` is the foundation for the bounded
/// two-pass engine: the first pass can eventually retain these compact read
/// ends while the second pass streams records through the resulting mark plan.
/// It also ensures that CIGARs, qualities and barcode tags are decoded only
/// once in the current in-memory implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ReadEndMetadata {
    unclipped_position: i64,
    quality_score: u64,
    library_id: LibraryId,
    barcode_id: Option<BarcodeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ReadEndDuplicateKey {
    position: i64,
    mate_position: i64,
    library_id: LibraryId,
    reference_id: i32,
    mate_reference_id: i32,
    barcode_id: Option<BarcodeId>,
    orientation: u8,
    reverse_strand: bool,
}

#[derive(Debug, Default)]
struct BarcodeRegistry {
    by_value: HashMap<Vec<u8>, BarcodeId>,
}

impl BarcodeRegistry {
    fn intern(
        &mut self,
        barcode: Option<Vec<u8>>,
    ) -> Result<Option<BarcodeId>, MarkDuplicatesError> {
        let Some(barcode) = barcode else {
            return Ok(None);
        };
        if let Some(id) = self.by_value.get(barcode.as_slice()) {
            return Ok(Some(*id));
        }
        let id = u32::try_from(self.by_value.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .and_then(BarcodeId::new)
            .ok_or_else(|| {
                MarkDuplicatesError::Operation(
                    "more than 4,294,967,294 distinct molecular barcodes are not supported"
                        .to_string(),
                )
            })?;
        self.by_value.insert(barcode, id);
        Ok(Some(id))
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

        if try_append_fast_sam_markduplicate_line(
            line,
            line_number,
            &mut seen,
            &mut summary,
            &mut output,
            config,
        )? {
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

fn try_append_fast_sam_markduplicate_line(
    line: &str,
    line_number: usize,
    seen: &mut HashMap<DuplicateKey, usize>,
    summary: &mut MarkDuplicatesSummary,
    output: &mut String,
    config: &MarkDuplicatesConfig,
) -> Result<bool, MarkDuplicatesError> {
    if config.barcode_tag.is_some()
        || config.read_one_barcode_tag.is_some()
        || config.read_two_barcode_tag.is_some()
    {
        return Ok(false);
    }
    let Some(fields) = split_exact_11_sam_fields(line) else {
        return Ok(false);
    };
    let flag = fields[1]
        .parse::<u16>()
        .map_err(|_| MarkDuplicatesError::MalformedSam {
            line_number,
            reason: format!("invalid FLAG value: {}", fields[1]),
        })?;
    if flag & UNMAPPED_FLAG != 0 {
        summary.unmapped_records += 1;
        output.push_str(line);
        output.push('\n');
        return Ok(true);
    }
    if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
        summary.secondary_or_supplementary_records += 1;
        output.push_str(line);
        output.push('\n');
        return Ok(true);
    }
    let Some(position) = simple_sam_unclipped_position(fields[3], fields[5], flag, line_number)?
    else {
        return Ok(false);
    };

    let mate_position = parse_sam_integer(fields[7], "MATE_POS", line_number)?;
    let template_length = parse_sam_integer(fields[8], "TLEN", line_number)?;
    if duplicate_candidate_is_pair(flag) {
        summary.paired_records_examined += 1;
        if flag & FIRST_IN_PAIR_FLAG != 0 {
            summary.read_pairs_examined += 1;
        }
    } else {
        summary.unpaired_reads_examined += 1;
    }

    let key = DuplicateKey {
        reference_name: fields[2].to_string(),
        position,
        mate_reference_name: fields[6].to_string(),
        mate_position,
        template_length,
        reverse_strand: flag & 0x10 != 0,
        barcode: None,
    };
    let seen_count = seen.entry(key).or_insert(0);
    let duplicate = *seen_count > 0;
    *seen_count += 1;

    let mut output_flag = flag;
    if duplicate {
        if duplicate_candidate_is_pair(flag) {
            summary.duplicate_pair_records += 1;
        } else {
            summary.unpaired_duplicate_records += 1;
        }
        output_flag |= DUPLICATE_FLAG;
    }

    if duplicate && config.remove_duplicates {
        return Ok(true);
    }

    let append_duplicate_type =
        config.tagging_policy.as_deref() == Some("All") && output_flag & DUPLICATE_FLAG != 0;
    append_sam_line_with_replaced_flag(
        output,
        line,
        output_flag,
        append_duplicate_type,
        config.add_pg_tag_to_reads,
    );
    Ok(true)
}

fn split_exact_11_sam_fields(line: &str) -> Option<[&str; 11]> {
    let mut fields = [""; 11];
    let mut parts = line.split('\t');
    for field in &mut fields {
        *field = parts.next()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(fields)
}

fn simple_sam_unclipped_position(
    position: &str,
    cigar: &str,
    flag: u16,
    line_number: usize,
) -> Result<Option<i64>, MarkDuplicatesError> {
    let position = parse_sam_integer(position, "POS", line_number)? - 1;
    let mut bytes = cigar.bytes();
    let Some(first) = bytes.next() else {
        return Ok(None);
    };
    if !first.is_ascii_digit() {
        return Ok(None);
    }
    let mut length = i64::from(first - b'0');
    for byte in bytes.by_ref() {
        if byte.is_ascii_digit() {
            length = length
                .checked_mul(10)
                .and_then(|value| value.checked_add(i64::from(byte - b'0')))
                .filter(|value| *value >= 0)
                .ok_or_else(|| MarkDuplicatesError::MalformedSam {
                    line_number,
                    reason: format!("invalid CIGAR value: {cigar}"),
                })?;
            continue;
        }
        if length == 0 || !matches!(byte as char, 'M' | 'D' | 'N' | '=' | 'X') {
            return Ok(None);
        }
        if bytes.next().is_some() {
            return Ok(None);
        }
        return if flag & 0x10 != 0 {
            Ok(Some(position + length - 1))
        } else {
            Ok(Some(position))
        };
    }
    Ok(None)
}

fn append_sam_line_with_replaced_flag(
    output: &mut String,
    line: &str,
    flag: u16,
    append_duplicate_type: bool,
    append_program_group: bool,
) {
    let Some(first_tab) = line.find('\t') else {
        output.push_str(line);
        output.push('\n');
        return;
    };
    let flag_start = first_tab + 1;
    let Some(flag_width) = line[flag_start..].find('\t') else {
        output.push_str(line);
        output.push('\n');
        return;
    };
    let flag_end = flag_start + flag_width;
    output.push_str(&line[..flag_start]);
    output.push_str(&flag.to_string());
    output.push_str(&line[flag_end..]);
    if append_duplicate_type {
        output.push_str("\tDT:Z:LB");
    }
    if append_program_group {
        output.push_str("\tPG:Z:MarkDuplicates");
    }
    output.push('\n');
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
    if let Some(summary) = try_run_single_bam_no_duplicate_fast_path(config)? {
        return Ok(summary);
    }
    if let Some(summary) = try_run_small_single_bam_compact_plan(config)? {
        return Ok(summary);
    }
    if let Some(summary) = try_run_external_plan(config)? {
        return Ok(summary);
    }
    if let Some(summary) = try_run_single_bam_compact_plan(config)? {
        return Ok(summary);
    }

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
    let mut read_ends = Vec::new();
    let mut eligible_indices = Vec::new();
    let mut barcode_registry = BarcodeRegistry::default();
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

    {
        let mut read_state = BamRecordReadState {
            records: &mut records,
            read_ends: &mut read_ends,
            eligible_indices: &mut eligible_indices,
            barcode_registry: &mut barcode_registry,
            config,
            summary: &mut summary,
            compact_plan: false,
        };
        read_state.read(&mut reader, &first_library_lookup, &mut library_registry)?;
        for input in config.inputs.iter().skip(1) {
            let mut reader = open_markdup_reader(config, input)?;
            let input_library_lookup = library_lookup(reader.header(), &mut library_registry);
            read_state.read(&mut reader, &input_library_lookup, &mut library_registry)?;
        }
    }

    let optical_duplicate_records = mark_duplicate_plan(
        &mut records,
        &mut read_ends,
        &eligible_indices,
        &mut summary,
        &mut library_registry,
        config,
    )?;

    {
        if config.inputs.len() > 1 {
            let mut marked_records = records
                .into_iter()
                .zip(optical_duplicate_records)
                .collect::<Vec<_>>();
            marked_records.sort_by(|(left, _), (right, _)| compare_bam_output_order(left, right));
            write_bam_records(marked_records, config, &mut writer)?;
        } else {
            write_bam_records(
                records.into_iter().zip(optical_duplicate_records),
                config,
                &mut writer,
            )?;
        }
    }
    drop(writer);

    finish_markdup_output(config, &library_registry)?;
    Ok(summary)
}

// Keep each bounded sort window large enough to avoid excessive temporary-file
// churn on ordinary shards while retaining a hard per-sorter memory ceiling.
// The external path owns two windows during its first pass, so this remains a
// small fraction of the production-scale memory budget rather than becoming a
// whole-file retention strategy.
const EXTERNAL_MARKDUP_MAX_RECORDS_IN_RAM: usize = 500_000;
const EXTERNAL_MARKDUP_MAX_BYTES_IN_RAM: usize = 256 * 1024 * 1024;
const DEFAULT_MAX_OPTICAL_DUPLICATE_SET_SIZE: usize = 300_000;
const EXTERNAL_DECISION_OPTICAL: u8 = 0x01;
const EXTERNAL_DECISION_DUPLICATE: u8 = 0x02;
const EXTERNAL_DECISION_SET_MEMBERS: u8 = 0x04;
const EXTERNAL_DECISION_PAYLOAD_BYTES: usize = 1 + (2 * std::mem::size_of::<i32>());
const EXTERNAL_DECISION_ABSENT_TAG: i32 = i32::MIN;

struct ExternalDuplicateProcessingConfig<'a> {
    config: &'a MarkDuplicatesConfig,
    read_name_parser: &'a ReadNameLocationParser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ExternalDecision {
    flags: u8,
    duplicate_set_size: Option<i32>,
    duplicate_set_index: Option<i32>,
}

/// Barcode dimensions retained by the bounded external duplicate-group sort.
/// The three fields mirror Picard's primary, read-one, and read-two barcode
/// comparisons while keeping their presence independent for mate-specific
/// tags.
#[derive(Debug, Clone)]
struct ExternalBarcodeValues {
    primary: Option<Vec<u8>>,
    read_one: Option<Vec<u8>>,
    read_two: Option<Vec<u8>>,
}

/// The fixed-width part of a record plan.  The original alignment payload is
/// deliberately absent: the second pass reopens the input and only needs the
/// ordinal plus the duplicate decision.  QNAMEs and barcode values are
/// retained only in the external sort payloads where they are needed to form
/// duplicate groups.
#[derive(Debug, Clone)]
struct ExternalPlanRecord {
    ordinal: u64,
    library_id: LibraryId,
    read_group: Option<Vec<u8>>,
    flags: u16,
    reference_id: i32,
    position: i64,
    mate_reference_id: i32,
    mate_position: i64,
    template_length: i64,
    unclipped_position: i64,
    quality_score: u64,
    qname: Vec<u8>,
    barcode: ExternalBarcodeValues,
}

fn external_plan_record(
    ordinal: u64,
    record: &bam::Record,
    library_id: LibraryId,
    config: &MarkDuplicatesConfig,
) -> ExternalPlanRecord {
    ExternalPlanRecord {
        ordinal,
        library_id,
        read_group: record_read_group(record),
        flags: record.flags(),
        reference_id: record.tid(),
        position: record.pos(),
        mate_reference_id: record.mtid(),
        mate_position: record.mpos(),
        template_length: record.insert_size(),
        unclipped_position: unclipped_record_position(record),
        quality_score: quality_score(record),
        qname: record.qname().to_vec(),
        barcode: external_barcode_values(record, config),
    }
}

fn external_barcode_values(
    record: &bam::Record,
    config: &MarkDuplicatesConfig,
) -> ExternalBarcodeValues {
    if let Some(tag) = config.barcode_tag.as_deref() {
        return ExternalBarcodeValues {
            primary: bam_tag_value(record, tag),
            read_one: None,
            read_two: None,
        };
    }

    let paired = record.flags() & PAIRED_FLAG != 0;
    let first_in_pair = record.flags() & FIRST_IN_PAIR_FLAG != 0;
    ExternalBarcodeValues {
        primary: None,
        read_one: (!paired || first_in_pair)
            .then_some(config.read_one_barcode_tag.as_deref())
            .flatten()
            .and_then(|tag| bam_tag_value(record, tag)),
        read_two: (paired && !first_in_pair)
            .then_some(config.read_two_barcode_tag.as_deref())
            .flatten()
            .and_then(|tag| bam_tag_value(record, tag)),
    }
}

fn paired_external_barcode_values(
    first: &ExternalPlanRecord,
    second: &ExternalPlanRecord,
) -> ExternalBarcodeValues {
    let primary = if first.flags & FIRST_IN_PAIR_FLAG != 0 {
        first.barcode.primary.clone()
    } else if second.flags & FIRST_IN_PAIR_FLAG != 0 {
        second.barcode.primary.clone()
    } else {
        first
            .barcode
            .primary
            .clone()
            .or_else(|| second.barcode.primary.clone())
    };
    ExternalBarcodeValues {
        primary,
        read_one: first
            .barcode
            .read_one
            .clone()
            .or_else(|| second.barcode.read_one.clone()),
        read_two: first
            .barcode
            .read_two
            .clone()
            .or_else(|| second.barcode.read_two.clone()),
    }
}

fn external_sorter(tmp_dir: &Path, prefix: &str) -> Result<ExternalSorter, MarkDuplicatesError> {
    let mut sort_config = ExternalSortConfig::new(tmp_dir);
    sort_config.max_records_in_ram = EXTERNAL_MARKDUP_MAX_RECORDS_IN_RAM;
    sort_config.max_bytes_in_ram = EXTERNAL_MARKDUP_MAX_BYTES_IN_RAM;
    sort_config.prefix = prefix.to_string();
    ExternalSorter::new(sort_config).map_err(MarkDuplicatesError::Operation)
}

fn external_plan_tempdir(config: &MarkDuplicatesConfig) -> Result<TempDir, MarkDuplicatesError> {
    let Some(tmp_dir) = config.tmp_dir.as_deref() else {
        return Ok(tempdir()?);
    };
    let tmp_dir = Path::new(tmp_dir);
    fs::create_dir_all(tmp_dir)?;
    TempDirBuilder::new()
        .prefix("turbo-picard-markdup-")
        .tempdir_in(tmp_dir)
        .map_err(MarkDuplicatesError::Io)
}

fn supports_external_markdup_plan(config: &MarkDuplicatesConfig) -> bool {
    if config.inputs.is_empty() {
        return false;
    }
    let has_explicit_reference = config
        .reference_sequence
        .as_deref()
        .is_some_and(|reference| !reference.trim().is_empty());
    config.inputs.iter().all(|input| {
        let input_path = Path::new(input);
        let is_bam = input_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("bam"));
        let is_reference_backed_cram = input_path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("cram"))
            && has_explicit_reference;
        is_bam || is_reference_backed_cram
    })
}

fn try_run_external_plan(
    config: &MarkDuplicatesConfig,
) -> Result<Option<MarkDuplicatesSummary>, MarkDuplicatesError> {
    if !supports_external_markdup_plan(config) {
        return Ok(None);
    }
    let read_name_parser = ReadNameLocationParser::from_config(config)?;
    let processing_config = ExternalDuplicateProcessingConfig {
        config,
        read_name_parser: &read_name_parser,
    };

    let first_input = &config.inputs[0];
    let reader = open_markdup_reader(config, first_input)?;
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
    let temporary = external_plan_tempdir(config)?;
    let mut qname_sorter = external_sorter(temporary.path(), "turbo-picard-markdup-qname")?;
    let mut fragment_sorter = external_sorter(temporary.path(), "turbo-picard-markdup-fragment")?;
    let mut record_count = 0_u64;
    let mut last_output_order = None::<(i32, i64, Vec<u8>, u16)>;

    for input in &config.inputs {
        let mut reader = open_markdup_reader(config, input)?;
        let input_library_lookup = library_lookup(reader.header(), &mut library_registry);
        for result in reader.records() {
            let mut record = result?;
            let flag = record.flags() & !DUPLICATE_FLAG;
            if record.flags() != flag {
                record.set_flags(flag);
            }
            let output_order = (record.tid(), record.pos(), record.qname().to_vec(), flag);
            if last_output_order
                .as_ref()
                .is_some_and(|previous| output_order.cmp(previous) == Ordering::Less)
            {
                // The compact multi-input path sorts the final records by
                // coordinate.  Do not silently change that contract here:
                // fall back if the input streams are not already globally
                // ordered, so the bounded path remains deterministic.
                return Ok(None);
            }
            last_output_order = Some(output_order);

            let library_id = record_library_id(&record, &input_library_lookup);
            let ordinal = record_count;
            record_count = record_count.checked_add(1).ok_or_else(|| {
                MarkDuplicatesError::Operation("too many BAM records".to_string())
            })?;

            if flag & UNMAPPED_FLAG != 0 {
                summary.unmapped_records += 1;
                library_registry.summary_mut(library_id).unmapped_records += 1;
                continue;
            }
            if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
                summary.secondary_or_supplementary_records += 1;
                library_registry
                    .summary_mut(library_id)
                    .secondary_or_supplementary_records += 1;
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

            let plan = external_plan_record(ordinal, &record, library_id, config);
            // The QNAME pass only pairs records.  Do not send unpaired
            // records through a second external sort: the fragment pass
            // below already owns their duplicate decisions, and single-end
            // inputs are common in real pipelines.
            if duplicate_candidate_is_pair(flag) {
                qname_sorter
                    .push(
                        plan.qname.clone(),
                        encode_external_plan_record(&plan, false),
                    )
                    .map_err(MarkDuplicatesError::Operation)?;
            }
            fragment_sorter
                .push(
                    external_fragment_key(&plan),
                    encode_external_plan_record(&plan, true),
                )
                .map_err(MarkDuplicatesError::Operation)?;
        }
    }

    let mut pair_sorter = external_sorter(temporary.path(), "turbo-picard-markdup-pair")?;
    let mut decision_sorter = external_sorter(temporary.path(), "turbo-picard-markdup-decisions")?;
    let mut pending_pair = None::<ExternalPlanRecord>;
    qname_sorter
        .finish_into(|item| {
            if pending_pair
                .as_ref()
                .is_some_and(|pending| pending.qname.as_slice() != item.key.as_slice())
            {
                pending_pair = None;
            }
            let plan = decode_external_plan_record(&item.key, &item.payload, false)?;
            if !duplicate_candidate_is_pair(plan.flags) {
                return Ok(());
            }
            if let Some(first) = pending_pair.take() {
                if first.qname == plan.qname {
                    let key = external_pair_key(&first, &plan);
                    pair_sorter.push(key.clone(), encode_external_plan_record(&first, true))?;
                    pair_sorter.push(key, encode_external_plan_record(&plan, true))?;
                } else {
                    pending_pair = Some(plan);
                }
            } else {
                pending_pair = Some(plan);
            }
            Ok(())
        })
        .map_err(MarkDuplicatesError::Operation)?;

    process_external_duplicate_groups(
        pair_sorter,
        &mut decision_sorter,
        &mut summary,
        &mut library_registry,
        true,
        tracks_duplicate_set_histogram(config),
        &processing_config,
    )?;
    process_external_duplicate_groups(
        fragment_sorter,
        &mut decision_sorter,
        &mut summary,
        &mut library_registry,
        false,
        tracks_duplicate_set_histogram(config),
        &processing_config,
    )?;
    let decision_path = temporary.path().join("duplicate-ordinals.bin");
    write_external_duplicate_ordinals(decision_sorter, &decision_path)?;

    let mut writer = open_markdup_writer(config, &config.output, &header)?;
    let mut decisions = File::open(&decision_path)?;
    let mut replay_ordinal = 0_u64;
    let mut next_duplicate = read_external_duplicate_decision(&mut decisions)?;
    for input in &config.inputs {
        let mut replay_reader = open_markdup_reader(config, input)?;
        write_external_plan_records(
            &mut replay_reader,
            &mut decisions,
            &mut replay_ordinal,
            &mut next_duplicate,
            config,
            &mut writer,
        )?;
    }
    if replay_ordinal != record_count {
        return Err(MarkDuplicatesError::Operation(
            "MarkDuplicates replay record count differs from external plan".to_string(),
        ));
    }
    if next_duplicate.is_some() {
        return Err(MarkDuplicatesError::Operation(
            "external MarkDuplicates decision ordinal exceeds replay record count".to_string(),
        ));
    }
    drop(writer);

    finish_markdup_output(config, &library_registry)?;
    Ok(Some(summary))
}

fn encode_external_plan_record(record: &ExternalPlanRecord, include_qname: bool) -> Vec<u8> {
    let mut payload = Vec::with_capacity(80 + record.qname.len());
    payload.extend_from_slice(&record.ordinal.to_le_bytes());
    payload.extend_from_slice(&record.library_id.to_le_bytes());
    payload.extend_from_slice(&record.flags.to_le_bytes());
    payload.extend_from_slice(&record.reference_id.to_le_bytes());
    payload.extend_from_slice(&record.position.to_le_bytes());
    payload.extend_from_slice(&record.mate_reference_id.to_le_bytes());
    payload.extend_from_slice(&record.mate_position.to_le_bytes());
    payload.extend_from_slice(&record.template_length.to_le_bytes());
    payload.extend_from_slice(&record.unclipped_position.to_le_bytes());
    payload.extend_from_slice(&record.quality_score.to_le_bytes());
    if include_qname {
        payload.extend_from_slice(
            &u32::try_from(record.qname.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        payload.extend_from_slice(&record.qname);
    }
    encode_external_barcode_values(&mut payload, &record.barcode);
    encode_external_barcode(&mut payload, record.read_group.as_deref());
    payload
}

fn encode_external_barcode_values(payload: &mut Vec<u8>, barcode: &ExternalBarcodeValues) {
    encode_external_barcode(payload, barcode.primary.as_deref());
    encode_external_barcode(payload, barcode.read_one.as_deref());
    encode_external_barcode(payload, barcode.read_two.as_deref());
}

fn encode_external_barcode(payload: &mut Vec<u8>, barcode: Option<&[u8]>) {
    match barcode {
        None => payload.push(0),
        Some(barcode) => {
            payload.push(1);
            payload.extend_from_slice(
                &u64::try_from(barcode.len())
                    .unwrap_or(u64::MAX)
                    .to_le_bytes(),
            );
            payload.extend_from_slice(barcode);
        }
    }
}

fn decode_external_plan_record(
    key: &[u8],
    payload: &[u8],
    qname_in_payload: bool,
) -> Result<ExternalPlanRecord, String> {
    let mut offset = 0usize;
    let ordinal = read_external_u64(payload, &mut offset)?;
    let library_id = read_external_u32(payload, &mut offset)?;
    let flags = read_external_u16(payload, &mut offset)?;
    let reference_id = read_external_i32(payload, &mut offset)?;
    let position = read_external_i64(payload, &mut offset)?;
    let mate_reference_id = read_external_i32(payload, &mut offset)?;
    let mate_position = read_external_i64(payload, &mut offset)?;
    let template_length = read_external_i64(payload, &mut offset)?;
    let unclipped_position = read_external_i64(payload, &mut offset)?;
    let quality_score = read_external_u64(payload, &mut offset)?;
    let qname = if qname_in_payload {
        let length = usize::try_from(read_external_u32(payload, &mut offset)?)
            .map_err(|_| "external MarkDuplicates QNAME length is too large".to_string())?;
        read_external_bytes(payload, &mut offset, length)?.to_vec()
    } else {
        key.to_vec()
    };
    let barcode = ExternalBarcodeValues {
        primary: decode_external_barcode(payload, &mut offset)?,
        read_one: decode_external_barcode(payload, &mut offset)?,
        read_two: decode_external_barcode(payload, &mut offset)?,
    };
    let read_group = decode_external_barcode(payload, &mut offset)?;
    if offset != payload.len() {
        return Err("external MarkDuplicates record payload has trailing bytes".to_string());
    }
    Ok(ExternalPlanRecord {
        ordinal,
        library_id,
        read_group,
        flags,
        reference_id,
        position,
        mate_reference_id,
        mate_position,
        template_length,
        unclipped_position,
        quality_score,
        qname,
        barcode,
    })
}

fn decode_external_barcode(payload: &[u8], offset: &mut usize) -> Result<Option<Vec<u8>>, String> {
    match read_external_u8(payload, offset)? {
        0 => Ok(None),
        1 => {
            let length = usize::try_from(read_external_u64(payload, offset)?)
                .map_err(|_| "external MarkDuplicates barcode length is too large".to_string())?;
            Ok(Some(read_external_bytes(payload, offset, length)?.to_vec()))
        }
        _ => Err("external MarkDuplicates barcode marker is invalid".to_string()),
    }
}

fn read_external_bytes<'a>(
    payload: &'a [u8],
    offset: &mut usize,
    length: usize,
) -> Result<&'a [u8], String> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| "external MarkDuplicates payload length overflow".to_string())?;
    let bytes = payload
        .get(*offset..end)
        .ok_or_else(|| "external MarkDuplicates record payload is truncated".to_string())?;
    *offset = end;
    Ok(bytes)
}

fn read_external_u8(payload: &[u8], offset: &mut usize) -> Result<u8, String> {
    Ok(*read_external_bytes(payload, offset, 1)?
        .first()
        .expect("one byte was requested"))
}

fn read_external_u16(payload: &[u8], offset: &mut usize) -> Result<u16, String> {
    let mut bytes = [0_u8; 2];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(u16::from_le_bytes(bytes))
}

fn read_external_u32(payload: &[u8], offset: &mut usize) -> Result<u32, String> {
    let mut bytes = [0_u8; 4];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(u32::from_le_bytes(bytes))
}

fn read_external_u64(payload: &[u8], offset: &mut usize) -> Result<u64, String> {
    let mut bytes = [0_u8; 8];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(u64::from_le_bytes(bytes))
}

fn read_external_i32(payload: &[u8], offset: &mut usize) -> Result<i32, String> {
    let mut bytes = [0_u8; 4];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(i32::from_le_bytes(bytes))
}

fn read_external_i64(payload: &[u8], offset: &mut usize) -> Result<i64, String> {
    let mut bytes = [0_u8; 8];
    let length = bytes.len();
    bytes.copy_from_slice(read_external_bytes(payload, offset, length)?);
    Ok(i64::from_le_bytes(bytes))
}

struct ExternalKey<'a> {
    library_id: LibraryId,
    reference_id: i32,
    position: i64,
    mate_reference_id: i32,
    mate_position: i64,
    orientation: u8,
    reverse_strand: bool,
    barcode: &'a ExternalBarcodeValues,
}

fn external_key(key_fields: ExternalKey<'_>) -> Vec<u8> {
    let mut key = Vec::with_capacity(4 + 4 + 8 + 4 + 8 + 2);
    key.extend_from_slice(&key_fields.library_id.to_le_bytes());
    key.extend_from_slice(&key_fields.reference_id.to_le_bytes());
    key.extend_from_slice(&key_fields.position.to_le_bytes());
    key.extend_from_slice(&key_fields.mate_reference_id.to_le_bytes());
    key.extend_from_slice(&key_fields.mate_position.to_le_bytes());
    key.push(key_fields.orientation);
    key.push(u8::from(key_fields.reverse_strand));
    encode_external_barcode(&mut key, key_fields.barcode.primary.as_deref());
    encode_external_barcode(&m