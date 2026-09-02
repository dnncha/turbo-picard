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
    encode_external_barcode(&mut key, key_fields.barcode.read_one.as_deref());
    encode_external_barcode(&mut key, key_fields.barcode.read_two.as_deref());
    key
}

fn external_pair_key(first: &ExternalPlanRecord, second: &ExternalPlanRecord) -> Vec<u8> {
    let (left, right) = if (first.reference_id, first.unclipped_position)
        <= (second.reference_id, second.unclipped_position)
    {
        (first, second)
    } else {
        (second, first)
    };
    let left_reverse = u8::from(left.flags & 0x10 != 0);
    let right_reverse = u8::from(right.flags & 0x10 != 0);
    let barcode = paired_external_barcode_values(first, second);
    external_key(ExternalKey {
        library_id: first.library_id,
        reference_id: left.reference_id,
        position: left.unclipped_position,
        mate_reference_id: right.reference_id,
        mate_position: right.unclipped_position,
        orientation: (left_reverse << 1) | right_reverse,
        reverse_strand: false,
        barcode: &barcode,
    })
}

fn external_fragment_key(record: &ExternalPlanRecord) -> Vec<u8> {
    external_key(ExternalKey {
        library_id: record.library_id,
        reference_id: record.reference_id,
        position: record.unclipped_position,
        mate_reference_id: -1,
        mate_position: -1,
        orientation: 0,
        reverse_strand: record.flags & 0x10 != 0,
        barcode: &record.barcode,
    })
}

fn process_external_duplicate_groups(
    sorter: ExternalSorter,
    decisions: &mut ExternalSorter,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    paired_groups: bool,
    track_duplicate_set_histogram: bool,
    processing_config: &ExternalDuplicateProcessingConfig<'_>,
) -> Result<(), MarkDuplicatesError> {
    let mut current_key = None::<Vec<u8>>;
    let mut group = Vec::<ExternalPlanRecord>::new();
    sorter
        .finish_into(|item: SortItem| {
            if current_key.as_deref() != Some(item.key.as_slice()) {
                process_external_duplicate_group(
                    &group,
                    decisions,
                    summary,
                    library_registry,
                    paired_groups,
                    track_duplicate_set_histogram,
                    processing_config,
                )?;
                group.clear();
                current_key = Some(item.key.clone());
            }
            group.push(decode_external_plan_record(&item.key, &item.payload, true)?);
            Ok(())
        })
        .map_err(MarkDuplicatesError::Operation)?;
    process_external_duplicate_group(
        &group,
        decisions,
        summary,
        library_registry,
        paired_groups,
        track_duplicate_set_histogram,
        processing_config,
    )
    .map_err(MarkDuplicatesError::Operation)
}

fn process_external_duplicate_group(
    group: &[ExternalPlanRecord],
    decisions: &mut ExternalSorter,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    paired_groups: bool,
    track_duplicate_set_histogram: bool,
    processing_config: &ExternalDuplicateProcessingConfig<'_>,
) -> Result<(), String> {
    if group.len() < 2 {
        return Ok(());
    }

    let mut names = HashMap::<Vec<u8>, (u64, u64)>::default();
    for member in group {
        let entry = names
            .entry(member.qname.clone())
            .or_insert((0, member.ordinal));
        entry.0 = entry.0.saturating_add(member.quality_score);
        entry.1 = entry.1.min(member.ordinal);
    }
    if !paired_groups {
        if names.len() < 2 {
            return Ok(());
        }
        return process_external_fragment_group(
            group,
            &names,
            decisions,
            summary,
            library_registry,
        );
    }
    let set_size = u64::try_from(names.len()).unwrap_or(u64::MAX);

    if names.len() < 2 {
        add_duplicate_set(summary, set_size, Some(set_size));
        if let Some(first) = group.first() {
            add_duplicate_set(
                library_registry.summary_mut(first.library_id),
                set_size,
                Some(set_size),
            );
        }
        return Ok(());
    }

    let representative_name = representative_external_name(&names);
    let mut reads = names
        .keys()
        .map(|name| OpticalRead {
            name: name.clone(),
            location: group
                .iter()
                .find(|member| member.qname == *name)
                .and_then(|member| {
                    processing_config
                        .read_name_parser
                        .coordinates(member.qname.as_slice())
                        .map(|(tile, x, y)| ReadLocation {
                            read_group: member.read_group.clone(),
                            tile,
                            x,
                            y,
                        })
                }),
        })
        .collect::<Vec<_>>();
    reads.sort_by_key(|read| {
        names
            .get(&read.name)
            .map(|(_, ordinal)| *ordinal)
            .unwrap_or(u64::MAX)
    });
    let optical_names = find_optical_duplicate_names(
        &reads,
        representative_name,
        i64::from(
            processing_config
                .config
                .optical_duplicate_pixel_distance
                .unwrap_or(100),
        ),
    );
    if track_duplicate_set_histogram {
        let optical_names_count = u64::try_from(optical_names.len()).unwrap_or(u64::MAX);
        let non_optical_size =
            (optical_names_count < set_size).then_some(set_size - optical_names_count);
        add_duplicate_set(summary, set_size, non_optical_size);
        if let Some(first) = group.first() {
            add_duplicate_set(
                library_registry.summary_mut(first.library_id),
                set_size,
                non_optical_size,
            );
        }
    }
    let optical_names_count = u64::try_from(optical_names.len()).unwrap_or(u64::MAX);
    summary.read_pair_optical_duplicates = summary
        .read_pair_optical_duplicates
        .saturating_add(optical_names_count);
    if let Some(first) = group.first() {
        let library_summary = library_registry.summary_mut(first.library_id);
        library_summary.read_pair_optical_duplicates = library_summary
            .read_pair_optical_duplicates
            .saturating_add(optical_names_count);
    }
    let duplicate_set_tags = (processing_config.config.tag_duplicate_set_members
        && !processing_config.config.remove_duplicates)
        .then(|| {
            let duplicate_set_index = group
                .iter()
                .filter(|member| member.qname.as_slice() == representative_name)
                .map(|member| i32::try_from(member.ordinal).unwrap_or(i32::MAX))
                .min()
                .unwrap_or(i32::MAX);
            (
                i32::try_from(set_size).unwrap_or(i32::MAX),
                duplicate_set_index,
            )
        });
    for member in group {
        let is_representative = member.qname.as_slice() == representative_name;
        if is_representative && duplicate_set_tags.is_none() {
            continue;
        }
        mark_external_decision(
            member,
            decisions,
            summary,
            library_registry,
            !is_representative,
            !is_representative && optical_names.contains(member.qname.as_slice()),
            duplicate_set_tags,
        )?;
    }
    Ok(())
}

fn process_external_fragment_group(
    group: &[ExternalPlanRecord],
    names: &HashMap<Vec<u8>, (u64, u64)>,
    decisions: &mut ExternalSorter,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
) -> Result<(), String> {
    if group
        .iter()
        .any(|member| duplicate_candidate_is_pair(member.flags))
    {
        for member in group {
            if !duplicate_candidate_is_pair(member.flags) {
                mark_external_duplicate(member, decisions, summary, library_registry, false)?;
            }
        }
        return Ok(());
    }

    let representative_name = representative_external_name(names);
    for member in group {
        if member.qname.as_slice() != representative_name {
            mark_external_duplicate(member, decisions, summary, library_registry, false)?;
        }
    }
    Ok(())
}

fn representative_external_name(names: &HashMap<Vec<u8>, (u64, u64)>) -> &[u8] {
    names
        .iter()
        .max_by(|left, right| {
            left.1
                .0
                .cmp(&right.1.0)
                .then_with(|| right.1.1.cmp(&left.1.1))
        })
        .map(|(name, _)| name.as_slice())
        .expect("external duplicate group has a name")
}

fn mark_external_duplicate(
    member: &ExternalPlanRecord,
    decisions: &mut ExternalSorter,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    is_optical_duplicate: bool,
) -> Result<(), String> {
    mark_external_decision(
        member,
        decisions,
        summary,
        library_registry,
        true,
        is_optical_duplicate,
        None,
    )
}

fn mark_external_decision(
    member: &ExternalPlanRecord,
    decisions: &mut ExternalSorter,
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    is_duplicate: bool,
    is_optical_duplicate: bool,
    duplicate_set_tags: Option<(i32, i32)>,
) -> Result<(), String> {
    let mut flags = 0_u8;
    if is_duplicate {
        flags |= EXTERNAL_DECISION_DUPLICATE;
    }
    if is_optical_duplicate {
        flags |= EXTERNAL_DECISION_OPTICAL;
    }
    if duplicate_set_tags.is_some() {
        flags |= EXTERNAL_DECISION_SET_MEMBERS;
    }
    decisions
        .push(
            member.ordinal.to_be_bytes().to_vec(),
            encode_external_decision_payload(&ExternalDecision {
                flags,
                duplicate_set_size: duplicate_set_tags.map(|(size, _)| size),
                duplicate_set_index: duplicate_set_tags.map(|(_, index)| index),
            }),
        )
        .map_err(|error| error.to_string())?;
    if !is_duplicate {
        return Ok(());
    }
    if duplicate_candidate_is_pair(member.flags) {
        summary.duplicate_pair_records += 1;
        library_registry
            .summary_mut(member.library_id)
            .duplicate_pair_records += 1;
    } else {
        summary.unpaired_duplicate_records += 1;
        library_registry
            .summary_mut(member.library_id)
            .unpaired_duplicate_records += 1;
    }
    Ok(())
}

fn encode_external_decision_payload(decision: &ExternalDecision) -> Vec<u8> {
    let mut payload = Vec::with_capacity(EXTERNAL_DECISION_PAYLOAD_BYTES);
    payload.push(decision.flags);
    payload.extend_from_slice(
        &decision
            .duplicate_set_size
            .unwrap_or(EXTERNAL_DECISION_ABSENT_TAG)
            .to_le_bytes(),
    );
    payload.extend_from_slice(
        &decision
            .duplicate_set_index
            .unwrap_or(EXTERNAL_DECISION_ABSENT_TAG)
            .to_le_bytes(),
    );
    payload
}

fn decode_external_decision_payload(payload: &[u8]) -> Result<ExternalDecision, String> {
    if payload.len() != EXTERNAL_DECISION_PAYLOAD_BYTES {
        return Err("external MarkDuplicates decision payload has invalid length".to_string());
    }
    let flags = payload[0];
    let known_flags =
        EXTERNAL_DECISION_OPTICAL | EXTERNAL_DECISION_DUPLICATE | EXTERNAL_DECISION_SET_MEMBERS;
    if flags & !known_flags != 0 {
        return Err("external MarkDuplicates decision flags are invalid".to_string());
    }
    let mut size_bytes = [0_u8; std::mem::size_of::<i32>()];
    size_bytes.copy_from_slice(&payload[1..1 + std::mem::size_of::<i32>()]);
    let mut index_bytes = [0_u8; std::mem::size_of::<i32>()];
    index_bytes.copy_from_slice(&payload[1 + std::mem::size_of::<i32>()..]);
    let size = i32::from_le_bytes(size_bytes);
    let index = i32::from_le_bytes(index_bytes);
    let has_set_members = flags & EXTERNAL_DECISION_SET_MEMBERS != 0;
    if has_set_members
        == (size == EXTERNAL_DECISION_ABSENT_TAG || index == EXTERNAL_DECISION_ABSENT_TAG)
    {
        return Err("external MarkDuplicates duplicate-set metadata is incomplete".to_string());
    }
    Ok(ExternalDecision {
        flags,
        duplicate_set_size: (size != EXTERNAL_DECISION_ABSENT_TAG).then_some(size),
        duplicate_set_index: (index != EXTERNAL_DECISION_ABSENT_TAG).then_some(index),
    })
}

fn merge_external_decisions(
    left: ExternalDecision,
    right: ExternalDecision,
) -> Result<ExternalDecision, String> {
    let duplicate_set_size = match (left.duplicate_set_size, right.duplicate_set_size) {
        (Some(left), Some(right)) if left != right => {
            return Err("external MarkDuplicates duplicate-set sizes conflict".to_string());
        }
        (Some(value), _) | (_, Some(value)) => Some(value),
        (None, None) => None,
    };
    let duplicate_set_index = match (left.duplicate_set_index, right.duplicate_set_index) {
        (Some(left), Some(right)) if left != right => {
            return Err("external MarkDuplicates duplicate-set indices conflict".to_string());
        }
        (Some(value), _) | (_, Some(value)) => Some(value),
        (None, None) => None,
    };
    Ok(ExternalDecision {
        flags: left.flags | right.flags,
        duplicate_set_size,
        duplicate_set_index,
    })
}

fn write_external_duplicate_ordinals(
    sorter: ExternalSorter,
    path: &Path,
) -> Result<(), MarkDuplicatesError> {
    // Duplicate groups arrive in fragment/pair sort order. Normalize their
    // ordinals once so BAM replay can consume a sequential stream instead of
    // performing one random seek for every duplicate record. The fixed-width
    // decision payload preserves optical status and optional DS/DI metadata.
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    let mut previous = None::<(u64, ExternalDecision)>;
    sorter
        .finish_into(|item| {
            if item.key.len() != std::mem::size_of::<u64>() {
                return Err("external MarkDuplicates decision key has invalid length".to_string());
            }
            let decision = decode_external_decision_payload(&item.payload)?;
            let mut bytes = [0_u8; 8];
            bytes.copy_from_slice(&item.key);
            let ordinal = u64::from_be_bytes(bytes);
            match previous {
                Some((previous_ordinal, previous_decision)) if previous_ordinal == ordinal => {
                    previous = Some((
                        previous_ordinal,
                        merge_external_decisions(previous_decision, decision)?,
                    ));
                }
                Some((previous_ordinal, previous_decision)) => {
                    write_external_decision(&mut writer, previous_ordinal, previous_decision)?;
                    previous = Some((ordinal, decision));
                }
                None => previous = Some((ordinal, decision)),
            }
            Ok(())
        })
        .map_err(MarkDuplicatesError::Operation)?;
    if let Some((ordinal, decision)) = previous {
        write_external_decision(&mut writer, ordinal, decision)
            .map_err(MarkDuplicatesError::Operation)?;
    }
    writer.flush()?;
    Ok(())
}

fn write_external_decision(
    writer: &mut impl Write,
    ordinal: u64,
    decision: ExternalDecision,
) -> Result<(), String> {
    writer
        .write_all(&ordinal.to_be_bytes())
        .map_err(|error| error.to_string())?;
    writer
        .write_all(&encode_external_decision_payload(&decision))
        .map_err(|error| error.to_string())
}

fn read_external_duplicate_decision(
    reader: &mut impl IoRead,
) -> Result<Option<(u64, ExternalDecision)>, std::io::Error> {
    let mut ordinal_bytes = [0_u8; 8];
    if reader.read(&mut ordinal_bytes[..1])? == 0 {
        return Ok(None);
    }
    reader.read_exact(&mut ordinal_bytes[1..])?;
    let mut payload = [0_u8; EXTERNAL_DECISION_PAYLOAD_BYTES];
    reader.read_exact(&mut payload)?;
    let decision = decode_external_decision_payload(&payload)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    Ok(Some((u64::from_be_bytes(ordinal_bytes), decision)))
}

fn write_external_plan_records(
    reader: &mut bam::Reader,
    decisions: &mut impl IoRead,
    ordinal: &mut u64,
    next_duplicate: &mut Option<(u64, ExternalDecision)>,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    for result in reader.records() {
        let mut record = result?;
        let mut flags = record.flags() & !DUPLICATE_FLAG;
        let mut is_optical_duplicate = false;
        if let Some((duplicate_ordinal, decision)) = *next_duplicate {
            if duplicate_ordinal < *ordinal {
                return Err(MarkDuplicatesError::Operation(
                    "external MarkDuplicates decision ordinals are not monotonic".to_string(),
                ));
            }
            if duplicate_ordinal == *ordinal {
                if decision.flags & EXTERNAL_DECISION_DUPLICATE != 0 {
                    flags |= DUPLICATE_FLAG;
                    is_optical_duplicate = decision.flags & EXTERNAL_DECISION_OPTICAL != 0;
                }
                if config.tag_duplicate_set_members
                    && !config.remove_duplicates
                    && let (Some(size), Some(index)) =
                        (decision.duplicate_set_size, decision.duplicate_set_index)
                {
                    replace_i32_aux(&mut record, b"DS", size)?;
                    replace_i32_aux(&mut record, b"DI", index)?;
                }
                *next_duplicate = read_external_duplicate_decision(decisions)?;
            }
        }
        record.set_flags(flags);
        write_bam_record(record, is_optical_duplicate, config, writer)?;
        *ordinal = (*ordinal)
            .checked_add(1)
            .ok_or_else(|| MarkDuplicatesError::Operation("too many BAM records".to_string()))?;
    }
    Ok(())
}

fn try_run_single_bam_compact_plan(
    config: &MarkDuplicatesConfig,
) -> Result<Option<MarkDuplicatesSummary>, MarkDuplicatesError> {
    if config.inputs.len() != 1 {
        return Ok(None);
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
    if config.add_pg_tag_to_reads {
        push_markdup_pg_header_if_needed(&mut header);
    }
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
            compact_plan: true,
        };
        read_state.read(&mut reader, &first_library_lookup, &mut library_registry)?;
    }

    let optical_duplicate_records = mark_duplicate_plan(
        &mut records,
        &mut read_ends,
        &eligible_indices,
        &mut summary,
        &mut library_registry,
        config,
    )?;
    drop(reader);
    drop(read_ends);
    drop(eligible_indices);

    let mut decision_bytes = Vec::new();
    write_compact_plan_duplicate_ordinals(
        &records,
        &optical_duplicate_records,
        &mut decision_bytes,
        config,
    )?;
    drop(records);
    drop(optical_duplicate_records);

    let mut decisions = Cursor::new(decision_bytes);
    let mut writer = open_markdup_writer(config, &config.output, &header)?;
    let mut replay_ordinal = 0_u64;
    let mut next_duplicate = read_external_duplicate_decision(&mut decisions)?;
    let mut replay_reader = open_markdup_reader(config, first_input)?;
    write_external_plan_records(
        &mut replay_reader,
        &mut decisions,
        &mut replay_ordinal,
        &mut next_duplicate,
        config,
        &mut writer,
    )?;
    if next_duplicate.is_some() {
        return Err(MarkDuplicatesError::Operation(
            "compact MarkDuplicates decision ordinal exceeds replay record count".to_string(),
        ));
    }
    drop(writer);

    finish_markdup_output(config, &library_registry)?;
    Ok(Some(summary))
}

fn mark_duplicate_plan(
    records: &mut [bam::Record],
    read_ends: &mut [ReadEndMetadata],
    eligible_indices: &[usize],
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
    config: &MarkDuplicatesConfig,
) -> Result<Vec<bool>, MarkDuplicatesError> {
    let duplicate_groups = duplicate_groups(records, read_ends, eligible_indices);
    let mut optical_duplicate_records = vec![false; records.len()];
    let track_duplicate_set_histogram = tracks_duplicate_set_histogram(config);
    let read_name_parser = ReadNameLocationParser::from_config(config)?;

    for group in duplicate_groups.values() {
        if group.len() < 2 {
            continue;
        }
        let paired_set_size = paired_duplicate_set_size(group, records);
        if !has_multiple_read_names(group, records) {
            if let Some(set_size) = paired_set_size {
                add_duplicate_set(summary, set_size, Some(set_size));
                if let Some(index) = group.first() {
                    add_duplicate_set(
                        library_registry.summary_mut(read_ends[*index].library_id),
                        set_size,
                        Some(set_size),
                    );
                }
            }
            continue;
        }

        let representative_index = best_duplicate_representative_index(group, records, read_ends);
        let representative_name = records[representative_index].qname().to_vec();
        let optical_duplicates = optical_duplicate_record_indices(
            group,
            records,
            representative_name.as_slice(),
            &read_name_parser,
            config.optical_duplicate_pixel_distance,
        );
        if let Some(set_size) = paired_set_size {
            let optical_names = u64::try_from(optical_duplicates.read_names).unwrap_or(u64::MAX);
            let non_optical_size = (optical_names < set_size).then_some(set_size - optical_names);
            if track_duplicate_set_histogram {
                add_duplicate_set(summary, set_size, non_optical_size);
                if let Some(index) = group.first() {
                    let library_summary =
                        library_registry.summary_mut(read_ends[*index].library_id);
                    add_duplicate_set(library_summary, set_size, non_optical_size);
                }
            }
        }
        summary.read_pair_optical_duplicates += optical_duplicates.read_names as u64;
        if let Some(index) = group.first() {
            library_registry
                .summary_mut(read_ends[*index].library_id)
                .read_pair_optical_duplicates += optical_duplicates.read_names as u64;
        }
        for index in optical_duplicates.record_indices {
            optical_duplicate_records[index] = true;
        }
        if config.tag_duplicate_set_members && !config.remove_duplicates {
            add_duplicate_set_member_tags(group, records, representative_name.as_slice())?;
        }

        for index in group.iter().copied() {
            if records[index].qname() == representative_name.as_slice() {
                continue;
            }
            let flag = records[index].flags();
            if duplicate_candidate_is_pair(flag) {
                summary.duplicate_pair_records += 1;
                library_registry
                    .summary_mut(read_ends[index].library_id)
                    .duplicate_pair_records += 1;
            } else {
                summary.unpaired_duplicate_records += 1;
                library_registry
                    .summary_mut(read_ends[index].library_id)
                    .unpaired_duplicate_records += 1;
            }
            records[index].set_flags(flag | DUPLICATE_FLAG);
        }
    }
    mark_fragment_duplicate_groups(
        records,
        read_ends,
        eligible_indices,
        summary,
        library_registry,
    );
    Ok(optical_duplicate_records)
}

fn compact_plan_external_decision(
    record: &bam::Record,
    optical_duplicate: bool,
    config: &MarkDuplicatesConfig,
) -> Option<ExternalDecision> {
    let mut flags = 0_u8;
    if record.flags() & DUPLICATE_FLAG != 0 {
        flags |= EXTERNAL_DECISION_DUPLICATE;
    }
    if optical_duplicate {
        flags |= EXTERNAL_DECISION_OPTICAL;
    }
    let mut duplicate_set_size = None;
    let mut duplicate_set_index = None;
    if config.tag_duplicate_set_members && !config.remove_duplicates {
        if let Ok(Aux::I32(size)) = record.aux(b"DS") {
            duplicate_set_size = Some(size);
        }
        if let Ok(Aux::I32(index)) = record.aux(b"DI") {
            duplicate_set_index = Some(index);
        }
        if duplicate_set_size.is_some() && duplicate_set_index.is_some() {
            flags |= EXTERNAL_DECISION_SET_MEMBERS;
        } else {
            duplicate_set_size = None;
            duplicate_set_index = None;
        }
    }
    if flags == 0 {
        return None;
    }
    Some(ExternalDecision {
        flags,
        duplicate_set_size,
        duplicate_set_index,
    })
}

fn write_compact_plan_duplicate_ordinals(
    plan_records: &[bam::Record],
    optical_duplicate_records: &[bool],
    writer: &mut Vec<u8>,
    config: &MarkDuplicatesConfig,
) -> Result<(), MarkDuplicatesError> {
    for (ordinal, record) in plan_records.iter().enumerate() {
        let optical_duplicate =
            optical_duplicate_records
                .get(ordinal)
                .copied()
                .ok_or_else(|| {
                    MarkDuplicatesError::Operation(
                        "MarkDuplicates compact plan is missing an optical-duplicate decision"
                            .to_string(),
                    )
                })?;
        if let Some(decision) = compact_plan_external_decision(record, optical_duplicate, config) {
            write_external_decision(writer, ordinal as u64, decision)
                .map_err(MarkDuplicatesError::Operation)?;
        }
    }
    Ok(())
}

fn try_run_small_single_bam_compact_plan(
    config: &MarkDuplicatesConfig,
) -> Result<Option<MarkDuplicatesSummary>, MarkDuplicatesError> {
    if config.inputs.len() != 1 {
        return Ok(None);
    }

    let mut reader = open_markdup_reader(config, &config.inputs[0])?;
    for (record_index, result) in reader.records().enumerate() {
        result?;
        if record_index >= COMPACT_MARKDUP_MAX_RECORDS {
            return Ok(None);
        }
    }

    try_run_single_bam_compact_plan(config)
}

fn finish_markdup_output(
    config: &MarkDuplicatesConfig,
    library_registry: &LibraryRegistry,
) -> Result<(), MarkDuplicatesError> {
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
    Ok(())
}

#[derive(Debug)]
struct FastPairRecord {
    library_id: LibraryId,
    reference_id: i32,
    position: i64,
    mate_reference_id: i32,
    mate_position: i64,
    template_length: i64,
    barcode: Option<Vec<u8>>,
}

fn try_run_single_bam_no_duplicate_fast_path(
    config: &MarkDuplicatesConfig,
) -> Result<Option<MarkDuplicatesSummary>, MarkDuplicatesError> {
    if config.inputs.len() != 1
        || config.tag_duplicate_set_members
        || config.barcode_tag.is_some()
        || config.read_one_barcode_tag.is_some()
        || config.read_two_barcode_tag.is_some()
    {
        return Ok(None);
    }

    let mut reader = open_markdup_reader(config, &config.input)?;
    let mut library_registry = LibraryRegistry::new();
    let library_lookup = library_lookup(reader.header(), &mut library_registry);
    let library = library_registry
        .summary(library_lookup.first_library_id)
        .library
        .clone();
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
    let mut seen_single_keys = HashMap::<BamDuplicateKey, Vec<u8>>::default();
    let mut last_pair_key = None::<(BamDuplicateKey, Vec<u8>)>;
    let mut adjacent_pending_pair = None::<(Vec<u8>, FastPairRecord)>;
    let mut pending_pairs = HashMap::<Vec<u8>, FastPairRecord>::default();
    let copy_input_on_success = can_copy_input_without_rewrite(config);
    let temp_output = (!copy_input_on_success).then(|| temp_hts_output_path(&config.output));
    let mut writer = if let Some(temp_output) = temp_output.as_deref() {
        let mut header = bam::Header::from_template(reader.header());
        if config.add_pg_tag_to_reads {
            push_markdup_pg_header_if_needed(&mut header);
        }
        let writer = open_markdup_writer(config, temp_output, &header)?;
        Some(writer)
    } else {
        None
    };
    let mut should_fallback = false;

    for result in reader.records() {
        let record = result?;
        let flag = record.flags();
        if flag & DUPLICATE_FLAG != 0 {
            should_fallback = true;
            break;
        }
        let library_id = record_library_id(&record, &library_lookup);

        if flag & UNMAPPED_FLAG != 0 {
            summary.unmapped_records += 1;
            library_registry.summary_mut(library_id).unmapped_records += 1;
            if let Some(writer) = writer.as_mut() {
                write_bam_record(record, false, config, writer)?;
            }
            continue;
        }
        if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
            summary.secondary_or_supplementary_records += 1;
            library_registry
                .summary_mut(library_id)
                .secondary_or_supplementary_records += 1;
            if let Some(writer) = writer.as_mut() {
                write_bam_record(record, false, config, writer)?;
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
            let qname = record.qname();
            let current = fast_pair_record(&record, library_id, config);
            if let Some((pending_qname, first)) = adjacent_pending_pair.take() {
                if pending_qname.as_slice() == qname {
                    if fast_pair_key_requires_fallback(&mut last_pair_key, &first, &current, qname)
                    {
                        should_fallback = true;
                        break;
                    }
                    add_duplicate_set(&mut summary, 1, Some(1));
                    add_duplicate_set(library_registry.summary_mut(library_id), 1, Some(1));
                } else {
                    pending_pairs.insert(pending_qname, first);
                    if let Some(first) = pending_pairs.remove(qname) {
                        if fast_pair_key_requires_fallback(
                            &mut last_pair_key,
                            &first,
                            &current,
                            qname,
                        ) {
                            should_fallback = true;
                            break;
                        }
                        add_duplicate_set(&mut summary, 1, Some(1));
                        add_duplicate_set(library_registry.summary_mut(library_id), 1, Some(1));
                    } else {
                        adjacent_pending_pair = Some((qname.to_vec(), current));
                    }
                }
            } else if let Some(first) = pending_pairs.remove(qname) {
                if fast_pair_key_requires_fallback(&mut last_pair_key, &first, &current, qname) {
                    should_fallback = true;
                    break;
                }
                add_duplicate_set(&mut summary, 1, Some(1));
                add_duplicate_set(library_registry.summary_mut(library_id), 1, Some(1));
            } else {
                adjacent_pending_pair = Some((qname.to_vec(), current));
            }
        } else {
            summary.unpaired_reads_examined += 1;
            library_registry
                .summary_mut(library_id)
                .unpaired_reads_examined += 1;
            let key = single_duplicate_key_bam(&record, library_id, &bam_barcode(&record, config));
            if duplicate_key_seen_with_different_name(&mut seen_single_keys, key, record.qname())? {
                should_fallback = true;
                break;
            }
        }
        if let Some(writer) = writer.as_mut() {
            write_bam_record(record, false, config, writer)?;
        }
    }

    if !should_fallback && let Some((qname, record)) = adjacent_pending_pair {
        let key = fast_single_duplicate_key(&record);
        if duplicate_key_seen_with_different_name(&mut seen_single_keys, key, &qname)? {
            should_fallback = true;
        }
    }

    if !should_fallback {
        for (qname, record) in pending_pairs {
            let key = fast_single_duplicate_key(&record);
            if duplicate_key_seen_with_different_name(&mut seen_single_keys, key, &qname)? {
                should_fallback = true;
                break;
            }
        }
    }

    drop(writer);
    if should_fallback {
        if let Some(temp_output) = temp_output.as_deref() {
            let _ = fs::remove_file(temp_output);
        }
        return Ok(None);
    }
    if Path::new(&config.output).exists() {
        fs::remove_file(&config.output)?;
    }
    if copy_input_on_success {
        fs::copy(&config.input, &config.output)?;
    } else if let Some(temp_output) = temp_output.as_deref() {
        fs::rename(temp_output, &config.output)?;
    }
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
    Ok(Some(summary))
}

fn fast_pair_key_requires_fallback(
    last_pair_key: &mut Option<(BamDuplicateKey, Vec<u8>)>,
    first: &FastPairRecord,
    current: &FastPairRecord,
    qname: &[u8],
) -> bool {
    let barcode = first.barcode.clone().or_else(|| current.barcode.clone());
    let key = fast_pair_duplicate_key(first, current, barcode);
    !matches!(
        ordered_duplicate_key_state(last_pair_key, key, qname),
        DuplicateKeyState::Unique
    )
}

fn temp_hts_output_path(output: &str) -> String {
    let extension = hts_io::path_extension_lower(output).unwrap_or_else(|| "bam".to_string());
    format!("{}.tmp.{}.{}", output, std::process::id(), extension)
}

fn can_copy_input_without_rewrite(config: &MarkDuplicatesConfig) -> bool {
    config.inputs.len() == 1
        && !config.add_pg_tag_to_reads
        && !config.clear_dt
        && config.compression_level.is_none()
        && hts_io::path_format(&config.input) == hts_io::path_format(&config.output)
        && hts_io::path_format(&config.input) == Some(bam::Format::Bam)
        && Path::new(&config.input) != Path::new(&config.output)
}

enum DuplicateKeyState {
    Unique,
    Duplicate,
    OutOfOrder,
}

fn ordered_duplicate_key_state(
    last_key_and_name: &mut Option<(BamDuplicateKey, Vec<u8>)>,
    key: BamDuplicateKey,
    qname: &[u8],
) -> DuplicateKeyState {
    let Some((last_key, last_qname)) = last_key_and_name else {
        *last_key_and_name = Some((key, qname.to_vec()));
        return DuplicateKeyState::Unique;
    };

    match key.cmp(last_key) {
        Ordering::Greater => {
            *last_key = key;
            last_qname.clear();
            last_qname.extend_from_slice(qname);
            DuplicateKeyState::Unique
        }
        Ordering::Equal if last_qname.as_slice() != qname => DuplicateKeyState::Duplicate,
        Ordering::Equal => DuplicateKeyState::OutOfOrder,
        Ordering::Less => DuplicateKeyState::OutOfOrder,
    }
}

fn duplicate_key_seen_with_different_name(
    seen_keys: &mut HashMap<BamDuplicateKey, Vec<u8>>,
    key: BamDuplicateKey,
    qname: &[u8],
) -> Result<bool, MarkDuplicatesError> {
    if let Some(existing_qname) = seen_keys.get(&key) {
        return Ok(existing_qname.as_slice() != qname);
    }
    seen_keys.insert(key, qname.to_vec());
    Ok(false)
}

fn fast_pair_record(
    record: &bam::Record,
    library_id: LibraryId,
    config: &MarkDuplicatesConfig,
) -> FastPairRecord {
    FastPairRecord {
        library_id,
        reference_id: record.tid(),
        position: unclipped_record_position(record),
        mate_reference_id: record.mtid(),
        mate_position: record.mpos(),
        template_length: record.insert_size(),
        barcode: bam_barcode(record, config),
    }
}

fn fast_pair_duplicate_key(
    first: &FastPairRecord,
    second: &FastPairRecord,
    barcode: Option<Vec<u8>>,
) -> BamDuplicateKey {
    let (left, right) =
        if (first.reference_id, first.position) <= (second.reference_id, second.position) {
            (first, second)
        } else {
            (second, first)
        };

    BamDuplicateKey {
        library_id: first.library_id,
        reference_id: left.reference_id,
        position: left.position,
        mate_reference_id: right.reference_id,
        mate_position: right.position,
        template_length: first
            .template_length
            .abs()
            .max(second.template_length.abs()),
        reverse_strand: false,
        barcode,
    }
}

fn fast_single_duplicate_key(record: &FastPairRecord) -> BamDuplicateKey {
    BamDuplicateKey {
        library_id: record.library_id,
        reference_id: record.reference_id,
        position: record.position,
        mate_reference_id: record.mate_reference_id,
        mate_position: record.mate_position,
        template_length: record.template_length,
        reverse_strand: false,
        barcode: record.barcode.clone(),
    }
}

struct BamRecordReadState<'a> {
    records: &'a mut Vec<bam::Record>,
    read_ends: &'a mut Vec<ReadEndMetadata>,
    eligible_indices: &'a mut Vec<usize>,
    barcode_registry: &'a mut BarcodeRegistry,
    config: &'a MarkDuplicatesConfig,
    summary: &'a mut MarkDuplicatesSummary,
    compact_plan: bool,
}

impl BamRecordReadState<'_> {
    fn read<R: bam::Read>(
        &mut self,
        reader: &mut R,
        library_lookup: &LibraryLookup,
        library_registry: &mut LibraryRegistry,
    ) -> Result<(), MarkDuplicatesError> {
        for result in reader.records() {
            let mut record = result?;
            let flag = record.flags() & !DUPLICATE_FLAG;
            if record.flags() != flag {
                record.set_flags(flag);
            }
            let library_id = record_library_id(&record, library_lookup);
            let record_index = self.records.len();

            if flag & UNMAPPED_FLAG != 0 {
                self.summary.unmapped_records += 1;
                library_registry.summary_mut(library_id).unmapped_records += 1;
                self.read_ends.push(ReadEndMetadata {
                    library_id,
                    unclipped_position: 0,
                    quality_score: UNCOMPUTED_QUALITY_SCORE,
                    barcode_id: None,
                });
                self.store_record(record);
                continue;
            }
            if flag & SECONDARY_OR_SUPPLEMENTARY_FLAGS != 0 {
                self.summary.secondary_or_supplementary_records += 1;
                library_registry
                    .summary_mut(library_id)
                    .secondary_or_supplementary_records += 1;
                self.read_ends.push(ReadEndMetadata {
                    library_id,
                    unclipped_position: 0,
                    quality_score: UNCOMPUTED_QUALITY_SCORE,
                    barcode_id: None,
                });
                self.store_record(record);
                continue;
            }

            if duplicate_candidate_is_pair(flag) {
                self.summary.paired_records_examined += 1;
                library_registry
                    .summary_mut(library_id)
                    .paired_records_examined += 1;
                if flag & FIRST_IN_PAIR_FLAG != 0 {
                    self.summary.read_pairs_examined += 1;
                    library_registry.summary_mut(library_id).read_pairs_examined += 1;
                }
            } else {
                self.summary.unpaired_reads_examined += 1;
                library_registry
                    .summary_mut(library_id)
                    .unpaired_reads_examined += 1;
            }
            let read_end = ReadEndMetadata {
                library_id,
                unclipped_position: unclipped_record_position(&record),
                quality_score: quality_score(&record),
                barcode_id: self
                    .barcode_registry
                    .intern(bam_barcode(&record, self.config))?,
            };
            self.eligible_indices.push(record_index);
            self.read_ends.push(read_end);
            self.store_record(record);
        }

        Ok(())
    }

    fn store_record(&mut self, record: bam::Record) {
        let stored_record = if self.compact_plan {
            compact_plan_record(&record)
        } else {
            record
        };
        self.records.push(stored_record);
    }
}

fn compact_plan_record(record: &bam::Record) -> bam::Record {
    let mut compact = bam::Record::new();
    compact.set(record.qname(), None, &[], &[]);
    compact.set_tid(record.tid());
    compact.set_pos(record.pos());
    compact.set_mapq(record.mapq());
    compact.set_flags(record.flags());
    compact.set_mtid(record.mtid());
    compact.set_mpos(record.mpos());
    compact.set_insert_size(record.insert_size());
    compact
}

fn compare_bam_output_order(left: &bam::Record, right: &bam::Record) -> Ordering {
    left.tid()
        .cmp(&right.tid())
        .then_with(|| left.pos().cmp(&right.pos()))
        .then_with(|| left.qname().cmp(right.qname()))
        .then_with(|| left.flags().cmp(&right.flags()))
}

fn write_bam_records(
    records: impl IntoIterator<Item = (bam::Record, bool)>,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    for (record, is_optical_duplicate) in records {
        write_bam_record(record, is_optical_duplicate, config, writer)?;
    }
    Ok(())
}

fn write_bam_record(
    mut record: bam::Record,
    is_optical_duplicate: bool,
    config: &MarkDuplicatesConfig,
    writer: &mut bam::Writer,
) -> Result<(), MarkDuplicatesError> {
    let is_duplicate = record.flags() & DUPLICATE_FLAG != 0;
    if (config.remove_duplicates && is_duplicate)
        || (config.remove_sequencing_duplicates && is_optical_duplicate)
    {
        return Ok(());
    }
    if config.clear_dt {
        clear_duplicate_type_tag(&mut record)?;
    }
    if let Some(duplicate_type) = duplicate_type_tag(config, record.flags(), is_optical_duplicate) {
        add_duplicate_type_tag(&mut record, duplicate_type)?;
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

#[derive(Debug)]
enum ReadNameLocationParser {
    Disabled,
    Default,
    Custom(Regex),
}

impl ReadNameLocationParser {
    fn from_config(config: &MarkDuplicatesConfig) -> Result<Self, MarkDuplicatesError> {
        match config.read_name_regex.as_deref() {
            Some("null") => Ok(Self::Disabled),
            Some(pattern) => Regex::new(pattern).map(Self::Custom).map_err(|error| {
                MarkDuplicatesError::Operation(format!("invalid READ_NAME_REGEX: {error}"))
            }),
            None => Ok(Self::Default),
        }
    }

    fn coordinates(&self, name: &[u8]) -> Option<(i64, i64, i64)> {
        let text = std::str::from_utf8(name).ok()?;
        match self {
            Self::Disabled => None,
            Self::Default => parse_default_read_coordinates(text),
            Self::Custom(regex) => {
                let captures = regex.captures(text)?;
                let whole = captures.get(0)?;
                if whole.start() != 0 || whole.end() != text.len() {
                    return None;
                }
                Some((
                    captures.get(1)?.as_str().parse().ok()?,
                    captures.get(2)?.as_str().parse().ok()?,
                    captures.get(3)?.as_str().parse().ok()?,
                ))
            }
        }
    }
}

fn optical_duplicate_record_indices(
    group: &[usize],
    records: &[bam::Record],
    representative_name: &[u8],
    read_name_parser: &ReadNameLocationParser,
    optical_duplicate_pixel_distance: Option<u32>,
) -> OpticalDuplicateRecords {
    let mut seen_names = HashSet::<Vec<u8>>::default();
    let mut reads = Vec::new();
    for index in group.iter().copied() {
        let name = records[index].qname().to_vec();
        if seen_names.insert(name.clone()) {
            reads.push(OpticalRead {
                name,
                location: read_location_for_record(&records[index], read_name_parser),
            });
        }
    }

    let optical_names = find_optical_duplicate_names(
        &reads,
        representative_name,
        i64::from(optical_duplicate_pixel_distance.unwrap_or(100)),
    );
    let record_indices = group
        .iter()
        .copied()
        .filter(|index| optical_names.contains(records[*index].qname()))
        .collect();
    OpticalDuplicateRecords {
        read_names: optical_names.len(),
        record_indices,
    }
}

fn tracks_duplicate_set_histogram(config: &MarkDuplicatesConfig) -> bool {
    // Picard only populates duplicate-set histograms while discovering optical
    // duplicates. Its explicit READ_NAME_REGEX=null mode still records
    // singleton pairs, but it does not record duplicate-pair set sizes.
    config.read_name_regex.as_deref() != Some("null")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReadLocation {
    read_group: Option<Vec<u8>>,
    tile: i64,
    x: i64,
    y: i64,
}

impl ReadLocation {
    fn is_within(&self, other: &Self, pixel_distance: i64) -> bool {
        self.read_group == other.read_group
            && self.tile == other.tile
            && (self.x - other.x).abs() <= pixel_distance
            && (self.y - other.y).abs() <= pixel_distance
    }
}

fn read_location_for_record(
    record: &bam::Record,
    read_name_parser: &ReadNameLocationParser,
) -> Option<ReadLocation> {
    let (tile, x, y) = read_name_parser.coordinates(record.qname())?;
    Some(ReadLocation {
        read_group: record_read_group(record),
        tile,
        x,
        y,
    })
}

fn parse_default_read_coordinates(text: &str) -> Option<(i64, i64, i64)> {
    let fields = text.split(':').collect::<Vec<_>>();
    if fields.len() != 5 && fields.len() != 7 {
        return None;
    }
    let start = fields.len() - 3;
    Some((
        parse_numeric_prefix(fields[start])?,
        parse_numeric_prefix(fields[start + 1])?,
        parse_numeric_prefix(fields[start + 2])?,
    ))
}

fn parse_numeric_prefix(value: &str) -> Option<i64> {
    let bytes = value.as_bytes();
    let mut end = usize::from(bytes.first() == Some(&b'-'));
    let first_digit = end;
    while bytes.get(end).is_some_and(u8::is_ascii_digit) {
        end += 1;
    }
    if end == first_digit {
        return None;
    }
    value[..end].parse().ok()
}

#[derive(Debug, Clone)]
struct OpticalRead {
    name: Vec<u8>,
    location: Option<ReadLocation>,
}

fn find_optical_duplicate_names(
    reads: &[OpticalRead],
    representative_name: &[u8],
    pixel_distance: i64,
) -> HashSet<Vec<u8>> {
    if reads.len() < 2 || reads.len() > DEFAULT_MAX_OPTICAL_DUPLICATE_SET_SIZE {
        return HashSet::default();
    }

    let keeper_index = reads
        .iter()
        .position(|read| read.name.as_slice() == representative_name && read.location.is_some());
    let mut flags = vec![false; reads.len()];
    let graph_mode_threshold = if keeper_index.is_some() { 4 } else { 3 };

    if reads.len() < graph_mode_threshold {
        if let Some(keeper_index) = keeper_index {
            for (index, read) in reads.iter().enumerate() {
                flags[index] = reads[keeper_index]
                    .location
                    .as_ref()
                    .zip(read.location.as_ref())
                    .is_some_and(|(keeper, other)| {
                        keeper.is_within(other, pixel_distance) && index != keeper_index
                    });
            }
        }
        for index in 0..reads.len() {
            if Some(index) == keeper_index {
                continue;
            }
            for other_index in (index + 1)..reads.len() {
                if Some(other_index) == keeper_index || (flags[index] && flags[other_index]) {
                    continue;
                }
                let close = reads[index]
                    .location
                    .as_ref()
                    .zip(reads[other_index].location.as_ref())
                    .is_some_and(|(left, right)| left.is_within(right, pixel_distance));
                if close {
                    let optical_index = if flags[other_index] {
                        index
                    } else {
                        other_index
                    };
                    flags[optical_index] = true;
                }
            }
        }
    } else {
        let mut union_find = UnionFind::new(reads.len());
        let bucket_size = pixel_distance.max(1);
        let mut buckets = HashMap::<OpticalBucket, Vec<usize>>::default();
        for (index, read) in reads.iter().enumerate() {
            let Some(location) = read.location.as_ref() else {
                continue;
            };
            let bucket = optical_bucket(location, bucket_size);
            for delta_x in -1..=1 {
                for delta_y in -1..=1 {
                    let neighbour = OpticalBucket {
                        read_group: bucket.read_group.clone(),
                        tile: bucket.tile,
                        x: bucket.x + delta_x,
                        y: bucket.y + delta_y,
                    };
                    if let Some(indices) = buckets.get(&neighbour) {
                        for other_index in indices {
                            let close = reads[*other_index]
                                .location
                                .as_ref()
                                .is_some_and(|other| location.is_within(other, pixel_distance));
                            if close {
                                union_find.union(index, *other_index);
                            }
                        }
                    }
                }
            }
            buckets.entry(bucket).or_default().push(index);
        }

        let keeper_root = keeper_index.map(|index| union_find.find(index));
        let mut components = HashMap::<usize, Vec<usize>>::default();
        for (index, read) in reads.iter().enumerate() {
            if read.location.is_some() {
                let root = union_find.find(index);
                components.entry(root).or_default().push(index);
            }
        }
        for (root, indices) in components {
            let representative = if keeper_root == Some(root) {
                keeper_index.expect("keeper root has a keeper index")
            } else {
                indices
                    .iter()
                    .copied()
                    .min_by(|left, right| {
                        let left = reads[*left]
                            .location
                            .as_ref()
                            .expect("component members have locations");
                        let right = reads[*right]
                            .location
                            .as_ref()
                            .expect("component members have locations");
                        left.x.cmp(&right.x).then_with(|| left.y.cmp(&right.y))
                    })
                    .expect("optical component is non-empty")
            };
            for index in indices {
                if index != representative {
                    flags[index] = true;
                }
            }
        }
    }

    reads
        .iter()
        .enumerate()
        .filter(|(index, _)| flags[*index])
        .map(|(_, read)| read.name.clone())
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct OpticalBucket {
    read_group: Option<Vec<u8>>,
    tile: i64,
    x: i64,
    y: i64,
}

fn optical_bucket(location: &ReadLocation, bucket_size: i64) -> OpticalBucket {
    OpticalBucket {
        read_group: location.read_group.clone(),
        tile: location.tile,
        x: location.x.div_euclid(bucket_size),
        y: location.y.div_euclid(bucket_size),
    }
}

#[derive(Debug)]
struct UnionFind {
    parents: Vec<usize>,
    ranks: Vec<u8>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parents: (0..size).collect(),
            ranks: vec![0; size],
        }
    }

    fn find(&mut self, index: usize) -> usize {
        if self.parents[index] != index {
            let root = self.find(self.parents[index]);
            self.parents[index] = root;
        }
        self.parents[index]
    }

    fn union(&mut self, left: usize, right: usize) {
        let mut left_root = self.find(left);
        let mut right_root = self.find(right);
        if left_root == right_root {
            return;
        }
        if self.ranks[left_root] < self.ranks[right_root] {
            std::mem::swap(&mut left_root, &mut right_root);
        }
        self.parents[right_root] = left_root;
        if self.ranks[left_root] == self.ranks[right_root] {
            self.ranks[left_root] = self.ranks[left_root].saturating_add(1);
        }
    }
}

fn add_duplicate_set_member_tags(
    group: &[usize],
    records: &mut [bam::Record],
    representative_name: &[u8],
) -> Result<(), MarkDuplicatesError> {
    if !group
        .iter()
        .any(|index| duplicate_candidate_is_pair(records[*index].flags()))
    {
        return Ok(());
    }

    let mut member_names = HashSet::<&[u8]>::default();
    for index in group.iter().copied() {
        member_names.insert(records[index].qname());
    }
    if member_names.len() < 2 {
        return Ok(());
    }

    let duplicate_set_index = group
        .iter()
        .copied()
        .filter(|index| records[*index].qname() == representative_name)
        .min()
        .unwrap_or(group[0]);
    let duplicate_set_size = i32::try_from(member_names.len()).unwrap_or(i32::MAX);
    let duplicate_set_index = i32::try_from(duplicate_set_index).unwrap_or(i32::MAX);

    for index in group.iter().copied() {
        replace_i32_aux(&mut records[index], b"DS", duplicate_set_size)?;
        replace_i32_aux(&mut records[index], b"DI", duplicate_set_index)?;
    }

    Ok(())
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
    records: &[bam::Record],
    read_ends: &[ReadEndMetadata],
    eligible_indices: &[usize],
) -> HashMap<ReadEndDuplicateKey, Vec<usize>> {
    // Qnames live in the immutable record buffer for the duration of grouping.
    // Borrow them instead of allocating a Vec<u8> for every pending mate.
    let mut paired_by_name = HashMap::<&[u8], usize>::default();
    let mut duplicate_groups = HashMap::<ReadEndDuplicateKey, Vec<usize>>::default();

    for index in eligible_indices.iter().copied() {
        let record = &records[index];
        if duplicate_candidate_is_pair(record.flags()) {
            if let Some(first_index) = paired_by_name.remove(record.qname()) {
                let indices = [first_index, index];
                let barcode_id = indices
                    .iter()
                    .find_map(|index| read_ends[*index].barcode_id);
                let key = pair_duplicate_key(
                    &records[first_index],
                    record,
                    &read_ends[first_index],
                    &read_ends[index],
                    barcode_id,
                );
                duplicate_groups.entry(key).or_default().extend(indices);
            } else {
                paired_by_name.insert(record.qname(), index);
            }
        }
    }

    duplicate_groups
}

fn mark_fragment_duplicate_groups(
    records: &mut [bam::Record],
    read_ends: &mut [ReadEndMetadata],
    eligible_indices: &[usize],
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
) {
    let mut fragment_groups = HashMap::<ReadEndDuplicateKey, Vec<usize>>::default();
    for index in eligible_indices.iter().copied() {
        let key = fragment_duplicate_key(&records[index], &read_ends[index]);
        fragment_groups.entry(key).or_default().push(index);
    }

    for group in fragment_groups.values() {
        if group.len() < 2 || !has_multiple_read_names(group, records) {
            continue;
        }

        let contains_complete_pair = group
            .iter()
            .any(|index| duplicate_candidate_is_pair(records[*index].flags()));
        if contains_complete_pair {
            for index in group.iter().copied() {
                if duplicate_candidate_is_pair(records[index].flags()) {
                    continue;
                }
                mark_unpaired_duplicate_record(
                    index,
                    records,
                    read_ends,
                    summary,
                    library_registry,
                );
            }
            continue;
        }

        let representative_index = best_duplicate_representative_index(group, records, read_ends);
        let representative_name = records[representative_index].qname().to_vec();
        for index in group.iter().copied() {
            if records[index].qname() == representative_name.as_slice() {
                continue;
            }
            mark_unpaired_duplicate_record(index, records, read_ends, summary, library_registry);
        }
    }
}

fn mark_unpaired_duplicate_record(
    index: usize,
    records: &mut [bam::Record],
    read_ends: &[ReadEndMetadata],
    summary: &mut MarkDuplicatesSummary,
    library_registry: &mut LibraryRegistry,
) {
    let flag = records[index].flags();
    if flag & DUPLICATE_FLAG != 0 {
        return;
    }
    summary.unpaired_duplicate_records += 1;
    library_registry
        .summary_mut(read_ends[index].library_id)
        .unpaired_duplicate_records += 1;
    records[index].set_flags(flag | DUPLICATE_FLAG);
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

fn single_duplicate_key_bam(
    record: &bam::Record,
    library_id: LibraryId,
    barcode: &Option<Vec<u8>>,
) -> BamDuplicateKey {
    let reverse_strand = record.flags() & 0x10 != 0;
    let position = unclipped_record_position(record);
    BamDuplicateKey {
        library_id,
        reference_id: record.tid(),
        position,
        mate_reference_id: record.mtid(),
        mate_position: record.mpos(),
        template_length: record.insert_size(),
        reverse_strand,
        barcode: barcode.clone(),
    }
}

fn fragment_duplicate_key(record: &bam::Record, read_end: &ReadEndMetadata) -> ReadEndDuplicateKey {
    let reverse_strand = record.flags() & 0x10 != 0;
    ReadEndDuplicateKey {
        library_id: read_end.library_id,
        reference_id: record.tid(),
        position: read_end.unclipped_position,
        mate_reference_id: -1,
        mate_position: -1,
        orientation: 0,
        reverse_strand,
        barcode_id: read_end.barcode_id,
    }
}

fn pair_duplicate_key(
    first: &bam::Record,
    second: &bam::Record,
    first_read_end: &ReadEndMetadata,
    second_read_end: &ReadEndMetadata,
    barcode_id: Option<BarcodeId>,
) -> ReadEndDuplicateKey {
    let first_position = first_read_end.unclipped_position;
    let second_position = second_read_end.unclipped_position;
    let (left, right) = if (first.tid(), first_position) <= (second.tid(), second_position) {
        ((first, first_read_end), (second, second_read_end))
    } else {
        ((second, second_read_end), (first, first_read_end))
    };

    ReadEndDuplicateKey {
        library_id: first_read_end.library_id,
        reference_id: left.0.tid(),
        position: left.1.unclipped_position,
        mate_reference_id: right.0.tid(),
        mate_position: right.1.unclipped_position,
        orientation: pair_orientation_code(left.0, right.0),
        reverse_strand: false,
        barcode_id,
    }
}

fn pair_orientation_code(left: &bam::Record, right: &bam::Record) -> u8 {
    let left_reverse = u8::from(left.flags() & 0x10 != 0);
    let right_reverse = u8::from(right.flags() & 0x10 != 0);
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

fn has_multiple_read_names(group: &[usize], records: &[bam::Record]) -> bool {
    let Some(first_index) = group.first() else {
        return false;
    };
    let first_name = records[*first_index].qname();
    group
        .iter()
        .skip(1)
        .any(|index| records[*index].qname() != first_name)
}

fn paired_duplicate_set_size(group: &[usize], records: &[bam::Record]) -> Option<u64> {
    if !group
        .iter()
        .any(|index| duplicate_candidate_is_pair(records[*index].flags()))
    {
        return None;
    }
    let mut names = HashSet::<&[u8]>::default();
    for index in group.iter().copied() {
        names.insert(records[index].qname());
    }
    u64::try_from(names.len()).ok().filter(|size| *size > 0)
}

fn best_duplicate_representative_index(
    group: &[usize],
    records: &[bam::Record],
    read_ends: &mut [ReadEndMetadata],
) -> usize {
    // The records outlive this map, so use borrowed qname slices. This keeps
    // representative selection allocation-free while preserving Picard's
    // stable first-record tie break.
    let mut scores_by_name = HashMap::<&[u8], (usize, u64)>::default();

    for index in group.iter().copied() {
        let score = if read_ends[index].quality_score == UNCOMPUTED_QUALITY_SCORE {
            let score = quality_score(&records[index]);
            read_ends[index].quality_score = score;
            score
        } else {
            read_ends[index].quality_score
        };
        let name = records[index].qname();
        let entry = scores_by_name.entry(name).or_insert((index, 0));
        entry.1 += score;
    }

    scores_by_name
        .into_values()
        .max_by(|left, right| left.1.cmp(&right.1).then_with(|| right.0.cmp(&left.0)))
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
    let optical_set_size = non_optical_set_size
        .and_then(|non_optical| all_set_size.checked_sub(non_optical))
        .filter(|optical| *optical > 0)
        .and_then(|optical| optical.checked_add(1));
    if let Some(optical_set_size) = optical_set_size {
        summary
            .duplicate_set_histogram
            .entry(optical_set_size)
            .or_default()
            .optical_sets += 1;
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
        let has_optical_sets = histogram.values().any(|counts| counts.optical_sets > 0);
        if has_optical_sets {
            output.push_str("set_size\tall_sets\toptical_sets\tnon_optical_sets\n");
            for (set_size, counts) in histogram {
                if counts.all_sets == 0 && counts.optical_sets == 0 && counts.non_optical_sets == 0
                {
                    continue;
                }
                output.push_str(&format!(
                    "{:.1}\t{}\t{}\t{}\n",
                    set_size as f64, counts.all_sets, counts.optical_sets, counts.non_optical_sets
                ));
            }
        } else {
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
            let estimated_library_size =
                estimate_library_size(read_pairs_examined, unique_read_pairs);
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
            target.optical_sets += counts.optical_sets;
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

fn record_read_group(record: &bam::Record) -> Option<Vec<u8>> {
    match record.aux(b"RG") {
        Ok(Aux::String(read_group)) => Some(read_group.as_bytes().to_vec()),
        Ok(Aux::Char(read_group)) => Some(vec![read_group]),
        _ => None,
    }
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
            tmp_dir: None,
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

    fn read_ends_for_records(records: &[bam::Record]) -> Vec<ReadEndMetadata> {
        records
            .iter()
            .map(|record| ReadEndMetadata {
                library_id: 0,
                unclipped_position: unclipped_record_position(record),
                quality_score: quality_score(record),
                barcode_id: None,
            })
            .collect()
    }

    #[test]
    fn best_duplicate_representative_index_aggregates_score_by_read_name() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[20], 0),
            record_with_name_and_qualities(b"dup-a", &[15], 0),
            record_with_name_and_qualities(b"dup-b", &[35], 0),
            record_with_name_and_qualities(b"dup-c", &[5], 0),
        ];

        let mut read_ends = read_ends_for_records(&records);
        let representative_index =
            best_duplicate_representative_index(&[0, 1, 2, 3], &records, &mut read_ends);
        assert_eq!(representative_index, 0);
    }

    #[test]
    fn best_duplicate_representative_index_tie_keeps_first_seen_index() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[20], 0),
            record_with_name_and_qualities(b"dup-b", &[10], 0),
            record_with_name_and_qualities(b"dup-c", &[10], 0),
        ];

        let mut read_ends = read_ends_for_records(&records);
        let representative_index =
            best_duplicate_representative_index(&[0, 1, 2], &records, &mut read_ends);
        assert_eq!(representative_index, 0);
    }

    #[test]
    fn barcode_registry_interns_equal_values_without_collisions() {
        let mut registry = BarcodeRegistry::default();
        let first = registry
            .intern(Some(b"ACGT".to_vec()))
            .expect("first barcode");
        let equal = registry
            .intern(Some(b"ACGT".to_vec()))
            .expect("equal barcode");
        let distinct = registry
            .intern(Some(b"TGCA".to_vec()))
            .expect("distinct barcode");

        assert_eq!(first, equal);
        assert_ne!(first, distinct);
        assert_eq!(registry.by_value.len(), 2);
        assert_eq!(registry.intern(None).expect("missing barcode"), None);
    }

    #[test]
    fn read_end_metadata_stays_compact() {
        assert!(std::mem::size_of::<ReadEndMetadata>() <= 24);
        assert!(std::mem::size_of::<ReadEndDuplicateKey>() <= 40);
    }

    #[test]
    fn external_plan_payload_round_trips_qname_and_duplicate_metadata() {
        let record = record_with_name_and_qualities(b"read-17", &[20, 30], 0x41);
        let config = sam_markdup_config();
        let plan = external_plan_record(17, &record, 3, &config);

        let qname_payload = encode_external_plan_record(&plan, false);
        let qname_round_trip =
            decode_external_plan_record(plan.qname.as_slice(), &qname_payload, false)
                .expect("qname-sort payload decodes");
        assert_eq!(qname_round_trip.ordinal, 17);
        assert_eq!(qname_round_trip.library_id, 3);
        assert_eq!(qname_round_trip.flags, 0x41);
        assert_eq!(qname_round_trip.qname, b"read-17");

        let member_payload = encode_external_plan_record(&plan, true);
        let member_round_trip =
            decode_external_plan_record(b"duplicate-key", &member_payload, true)
                .expect("duplicate-member payload decodes");
        assert_eq!(member_round_trip.qname, b"read-17");
        assert_eq!(
            member_round_trip.unclipped_position,
            plan.unclipped_position
        );
        assert_eq!(member_round_trip.quality_score, plan.quality_score);
    }

    #[test]
    fn external_decision_payload_round_trips_duplicate_set_metadata() {
        let decision = ExternalDecision {
            flags: EXTERNAL_DECISION_DUPLICATE
                | EXTERNAL_DECISION_OPTICAL
                | EXTERNAL_DECISION_SET_MEMBERS,
            duplicate_set_size: Some(7),
            duplicate_set_index: Some(42),
        };

        let encoded = encode_external_decision_payload(&decision);

        assert_eq!(decode_external_decision_payload(&encoded), Ok(decision));
    }

    #[test]
    fn external_decisions_merge_duplicate_flags_and_set_metadata() {
        let duplicate = ExternalDecision {
            flags: EXTERNAL_DECISION_DUPLICATE,
            duplicate_set_size: None,
            duplicate_set_index: None,
        };
        let tags = ExternalDecision {
            flags: EXTERNAL_DECISION_SET_MEMBERS,
            duplicate_set_size: Some(2),
            duplicate_set_index: Some(0),
        };

        assert_eq!(
            merge_external_decisions(duplicate, tags),
            Ok(ExternalDecision {
                flags: EXTERNAL_DECISION_DUPLICATE | EXTERNAL_DECISION_SET_MEMBERS,
                duplicate_set_size: Some(2),
                duplicate_set_index: Some(0),
            })
        );
    }

    #[test]
    fn external_duplicate_ordinals_are_sorted_and_deduplicated() {
        let temporary = tempdir().expect("temporary directory");
        let mut sorter = external_sorter(temporary.path(), "test-decisions").expect("sorter");
        for ordinal in [9_u64, 2, 9, 4] {
            sorter
                .push(
                    ordinal.to_be_bytes().to_vec(),
                    encode_external_decision_payload(&ExternalDecision {
                        flags: EXTERNAL_DECISION_DUPLICATE,
                        duplicate_set_size: None,
                        duplicate_set_index: None,
                    }),
                )
                .expect("decision ordinal");
        }
        let path = temporary.path().join("duplicate-ordinals.bin");
        write_external_duplicate_ordinals(sorter, &path).expect("ordinal stream");

        let mut reader = File::open(path).expect("ordinal stream file");
        let mut ordinals = Vec::new();
        while let Some((ordinal, _)) =
            read_external_duplicate_decision(&mut reader).expect("ordinal")
        {
            ordinals.push(ordinal);
        }
        assert_eq!(ordinals, vec![2, 4, 9]);
    }

    #[test]
    fn external_plan_requires_an_explicit_reference_for_cram() {
        let mut config = sam_markdup_config();
        config.input = "input.cram".to_string();
        config.inputs = vec![config.input.clone()];
        config.read_name_regex = Some("null".to_string());

        assert!(!supports_external_markdup_plan(&config));

        config.reference_sequence = Some("fixtures/reference/chrM.fa".to_string());
        assert!(supports_external_markdup_plan(&config));
    }

    #[test]
    fn external_plan_accepts_multiple_supported_alignment_inputs() {
        let mut config = sam_markdup_config();
        config.inputs = vec!["lane-1.bam".to_string(), "lane-2.bam".to_string()];
        config.read_name_regex = Some("null".to_string());

        assert!(supports_external_markdup_plan(&config));

        config.tag_duplicate_set_members = true;
        assert!(supports_external_markdup_plan(&config));

        config.inputs.push("unsupported.sam".to_string());
        assert!(!supports_external_markdup_plan(&config));
    }

    #[test]
    fn representative_selection_uses_cached_quality_score() {
        let records = [
            record_with_name_and_qualities(b"first", &[40], 0),
            record_with_name_and_qualities(b"second", &[10], 0),
        ];
        let mut read_ends = read_ends_for_records(&records);
        // The mark-plan consumes cached metadata; it must not rescan the BAM
        // quality payload while grouping duplicate families.
        read_ends[0].quality_score = 1;
        read_ends[1].quality_score = 100;

        assert_eq!(
            best_duplicate_representative_index(&[0, 1], &records, &mut read_ends),
            1
        );
    }

    #[test]
    fn paired_duplicate_set_size_uses_unique_read_names_for_pairs() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[10], 0x1),
            record_with_name_and_qualities(b"dup-a", &[20], 0x1),
            record_with_name_and_qualities(b"dup-b", &[30], 0x0),
            record_with_name_and_qualities(b"dup-b", &[40], 0x1),
        ];

        assert_eq!(paired_duplicate_set_size(&[0, 1, 2, 3], &records), Some(2));
    }

    #[test]
    fn paired_duplicate_set_size_none_without_paired_candidate() {
        let records = [
            record_with_name_and_qualities(b"dup-a", &[10], 0x0),
            record_with_name_and_qualities(b"dup-a", &[20], 0x0),
        ];

        assert_eq!(paired_duplicate_set_size(&[0, 1], &records), None);
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

    #[test]
    fn optical_duplicate_indices_count_each_read_name_once() {
        let records = [
            record_with_name_and_flags(b"INST:RUN:FLOW:1:1:100:100", 0x1),
            record_with_name_and_flags(b"INST:RUN:FLOW:1:1:110:110", 0x1),
            record_with_name_and_flags(b"INST:RUN:FLOW:1:1:110:110", 0x1),
            record_with_name_and_flags(b"INST:RUN:FLOW:1:1:1000:1000", 0x1),
        ];
        let mut config = sam_markdup_config();
        config.optical_duplicate_pixel_distance = Some(100);
        let parser = ReadNameLocationParser::from_config(&config).expect("parser");

        let optical = optical_duplicate_record_indices(
            &[0, 1, 2, 3],
            &records,
            records[0].qname(),
            &parser,
            config.optical_duplicate_pixel_distance,
        );

        assert_eq!(optical.read_names, 1);
        assert_eq!(optical.record_indices, vec![1, 2]);
    }

    #[test]
    fn metrics_preserve_optical_histogram_counts_when_combining_libraries() {
        let mut summary = MarkDuplicatesSummary {
            library: "lib".to_string(),
            unpaired_reads_examined: 0,
            read_pairs_examined: 2,
            paired_records_examined: 4,
            secondary_or_supplementary_records: 0,
            unpaired_duplicate_records: 0,
            duplicate_pair_records: 1,
            read_pair_optical_duplicates: 1,
            unmapped_records: 0,
            duplicate_set_histogram: BTreeMap::new(),
        };
        add_duplicate_set(&mut summary, 2, Some(1));

        let metrics = metrics_text_for_libraries([&summary]);

        assert!(metrics.contains("set_size\tall_sets\toptical_sets\tnon_optical_sets"));
        assert!(metrics.contains("1.0\t0\t0\t1"));
        assert!(metrics.contains("2.0\t1\t1\t0"));
    }

    #[test]
    fn add_duplicate_set_member_tags_uses_unique_read_names_for_duplicate_set_size() {
        let mut records = [
            record_with_name_and_flags(b"dup-a", 0x1),
            record_with_name_and_flags(b"dup-a", 0x1),
            record_with_name_and_flags(b"dup-b", 0x1),
            record_with_name_and_flags(b"dup-c", 0x1),
        ];

        add_duplicate_set_member_tags(&[0, 1, 2, 3], &mut records, b"dup-a").expect("tags applied");

        for index in [0usize, 1, 2, 3] {
            let di = records[index].aux(b"DI").expect("DI tag exists");
            let ds = records[index].aux(b"DS").expect("DS tag exists");
            assert!(matches!(di, Aux::I32(0)));
            assert!(matches!(ds, Aux::I32(3)));
        }
    }

    #[test]
    fn add_duplicate_set_member_tags_skips_groups_without_paired_record() {
        let mut records = [
            record_with_name_and_flags(b"dup-a", 0x0),
            record_with_name_and_flags(b"dup-a", 0x0),
        ];

        add_duplicate_set_member_tags(&[0, 1], &mut records, b"dup-a").expect("returns");

        assert!(records[0].aux(b"DI").is_err());
        assert!(records[1].aux(b"DI").is_err());
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
