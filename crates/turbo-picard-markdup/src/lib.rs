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
        summary.unpaired_reads_e