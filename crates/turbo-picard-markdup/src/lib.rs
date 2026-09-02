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

// PLACEHOLDER_FOR_CHUNKING_TEST