use super::bam_parallel::BAM_ENC_TO_ASCII;
use super::{
    CommandError, PicardArgs, bgzf_threads_for, format_float, has_sam_extension,
    open_bam_reader_for_args, optional_bool, optional_f64, optional_i64, optional_scalar,
    optional_u32, reject_unsupported_compression_level, required_scalar_for, unsupported,
};
use flate2::read::GzDecoder;
use rust_htslib::bam::record::Aux;
use rust_htslib::bam::{self, FetchDefinition, Read};
use rust_htslib::faidx;
use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use turbo_picard_core::bgzf_threads::HtsThreadRole;
use turbo_picard_core::picard_args::normalize_picard_args_for_command;

const DEFAULT_ERROR_METRICS: &[&str] = &[
    "ERROR",
    "ERROR:BASE_QUALITY",
    "ERROR:INSERT_LENGTH",
    "ERROR:GC_CONTENT",
    "ERROR:READ_DIRECTION",
    "ERROR:PAIR_ORIENTATION",
    "ERROR:HOMOPOLYMER",
    "ERROR:BINNED_HOMOPOLYMER",
    "ERROR:CYCLE",
    "ERROR:READ_ORDINALITY",
    "ERROR:READ_ORDINALITY:CYCLE",
    "ERROR:READ_ORDINALITY:HOMOPOLYMER",
    "ERROR:READ_ORDINALITY:GC_CONTENT",
    "ERROR:READ_ORDINALITY:PRE_DINUC",
    "ERROR:MAPPING_QUALITY",
    "ERROR:READ_GROUP",
    "ERROR:MISMATCHES_IN_READ",
    "ERROR:ONE_BASE_PADDED_CONTEXT",
    "OVERLAPPING_ERROR",
    "OVERLAPPING_ERROR:BASE_QUALITY",
    "OVERLAPPING_ERROR:INSERT_LENGTH",
    "OVERLAPPING_ERROR:READ_ORDINALITY",
    "OVERLAPPING_ERROR:READ_ORDINALITY:CYCLE",
    "OVERLAPPING_ERROR:READ_ORDINALITY:HOMOPOLYMER",
    "OVERLAPPING_ERROR:READ_ORDINALITY:GC_CONTENT",
    "INDEL_ERROR",
];

const SUPPORTED_ARGS: &[&str] = &[
    "INPUT",
    "OUTPUT",
    "REFERENCE_SEQUENCE",
    "ERROR_METRICS",
    "MIN_MAPPING_Q",
    "MIN_BASE_Q",
    "PRIOR_Q",
    "PROBABILITY",
    "MAX_LOCI",
    "LONG_HOMOPOLYMER",
    "LOCATION_BIN_SIZE",
    "INTERVALS",
    "FILE_EXTENSION",
    "VCF",
    "INTERVAL_ITERATOR",
    "PROGRESS_STEP_INTERVAL",
    "VALIDATION_STRINGENCY",
    "QUIET",
    "VERBOSITY",
    "COMPRESSION_LEVEL",
    "TMP_DIR",
    "MAX_RECORDS_IN_RAM",
    "CREATE_INDEX",
    "CREATE_MD5_FILE",
    "USE_JDK_DEFLATER",
    "USE_JDK_INFLATER",
];

const CHUNK_SPAN: i64 = 2_000_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ErrorKind {
    Simple,
    Indel,
    Overlapping,
}

impl ErrorKind {
    fn parse(raw: &str) -> Result<Self, CommandError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "ERROR" => Ok(Self::Simple),
            "INDEL_ERROR" => Ok(Self::Indel),
            "OVERLAPPING_ERROR" => Ok(Self::Overlapping),
            _ => Err(unsupported(format!(
                "unsupported CollectSamErrorMetrics ERROR_METRICS calculator: {raw}"
            ))),
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::Simple => "error",
            Self::Indel => "indel_error",
            Self::Overlapping => "overlapping_error",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StratifierKind {
    All,
    BaseQuality,
    InsertLength,
    GcContent,
    ReadDirection,
    PairOrientation,
    Homopolymer,
    BinnedHomopolymer,
    Cycle,
    ReadOrdinality,
    MappingQuality,
    ReadGroup,
    MismatchesInRead,
    OneBasePaddedContext,
    PreDinuc,
    PostDinuc,
    ReadBase,
    PrevBase,
    NextBase,
    ReferenceBase,
    HomopolymerLength,
}

impl StratifierKind {
    fn parse(raw: &str) -> Result<Self, CommandError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "ALL" => Ok(Self::All),
            "BASE_QUALITY" => Ok(Self::BaseQuality),
            "INSERT_LENGTH" => Ok(Self::InsertLength),
            "GC_CONTENT" => Ok(Self::GcContent),
            "READ_DIRECTION" => Ok(Self::ReadDirection),
            "PAIR_ORIENTATION" => Ok(Self::PairOrientation),
            "HOMOPOLYMER" => Ok(Self::Homopolymer),
            "BINNED_HOMOPOLYMER" => Ok(Self::BinnedHomopolymer),
            "CYCLE" => Ok(Self::Cycle),
            "READ_ORDINALITY" => Ok(Self::ReadOrdinality),
            "MAPPING_QUALITY" => Ok(Self::MappingQuality),
            "READ_GROUP" => Ok(Self::ReadGroup),
            "MISMATCHES_IN_READ" => Ok(Self::MismatchesInRead),
            "ONE_BASE_PADDED_CONTEXT" => Ok(Self::OneBasePaddedContext),
            "PRE_DINUC" => Ok(Self::PreDinuc),
            "POST_DINUC" => Ok(Self::PostDinuc),
            "READ_BASE" => Ok(Self::ReadBase),
            "PREV_BASE" => Ok(Self::PrevBase),
            "NEXT_BASE" => Ok(Self::NextBase),
            "REFERENCE_BASE" => Ok(Self::ReferenceBase),
            "HOMOPOLYMER_LENGTH" => Ok(Self::HomopolymerLength),
            _ => Err(unsupported(format!(
                "unsupported CollectSamErrorMetrics stratifier: {raw}"
            ))),
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::BaseQuality => "base_quality",
            Self::InsertLength => "insert_length",
            Self::GcContent => "gc",
            Self::ReadDirection => "read_direction",
            Self::PairOrientation => "pair_orientation",
            Self::Homopolymer => "homopolymer_and_following_ref_base",
            Self::BinnedHomopolymer => "binned_length_homopolymer_and_following_ref_base",
            Self::Cycle => "cycle",
            Self::ReadOrdinality => "read_ordinality",
            Self::MappingQuality => "mapping_quality",
            Self::ReadGroup => "read_group",
            Self::MismatchesInRead => "mismatches_in_read",
            Self::OneBasePaddedContext => "one_base_padded_context",
            Self::PreDinuc => "pre_dinuc",
            Self::PostDinuc => "post_dinuc",
            Self::ReadBase => "read_base",
            Self::PrevBase => "prev_base",
            Self::NextBase => "next_base",
            Self::ReferenceBase => "ref_base",
            Self::HomopolymerLength => "homopolymer_length",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
enum PairOrientation {
    F1R2 = 0,
    F2R1 = 1,
    F1F2 = 2,
    R1R2 = 3,
}

impl PairOrientation {
    fn as_str(self) -> &'static str {
        match self {
            Self::F1R2 => "F1R2",
            Self::F2R1 => "F2R1",
            Self::F1F2 => "F1F2",
            Self::R1R2 => "R1R2",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
enum ReadOrdinality {
    First = 0,
    Second = 1,
}

impl ReadOrdinality {
    fn as_str(self) -> &'static str {
        match self {
            Self::First => "FIRST",
            Self::Second => "SECOND",
        }
    }
}

// Structured key matching Java Comparable ordering inside TreeSet before toString().
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum CovariateKey {
    Str(String),
    Int(i32),
    GcPct(u8),
    ReadDir(bool), // false = '+' < true = '-'
    PairOrient(PairOrientation),
    ReadOrd(ReadOrdinality),
    LongShortHp(bool), // false = SHORT < true = LONG
    Char(char),
    Tuple(Vec<CovariateKey>),
}

impl CovariateKey {
    fn format_picard(&self) -> String {
        match self {
            Self::Str(s) => s.clone(),
            Self::Int(v) => v.to_string(),
            Self::GcPct(pct) => format_float(f64::from(*pct) / 100.0),
            Self::ReadDir(neg) => {
                if *neg {
                    "-".to_string()
                } else {
                    "+".to_string()
                }
            }
            Self::PairOrient(po) => po.as_str().to_string(),
            Self::ReadOrd(ro) => ro.as_str().to_string(),
            Self::LongShortHp(is_long) => {
                if *is_long {
                    "LONG_HOMOPOLYMER".to_string()
                } else {
                    "SHORT_HOMOPOLYMER".to_string()
                }
            }
            Self::Char(ch) => ch.to_string(),
            Self::Tuple(items) => items
                .iter()
                .map(Self::format_picard)
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SimpleCounters {
    total_bases: u64,
    error_bases: u64,
}

impl SimpleCounters {
    fn merge(&mut self, other: Self) {
        self.total_bases += other.total_bases;
        self.error_bases += other.error_bases;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct IndelCounters {
    total_bases: u64,
    num_insertions: u64,
    num_inserted_bases: u64,
    num_deletions: u64,
    num_deleted_bases: u64,
}

impl IndelCounters {
    fn merge(&mut self, other: Self) {
        self.total_bases += other.total_bases;
        self.num_insertions += other.num_insertions;
        self.num_inserted_bases += other.num_inserted_bases;
        self.num_deletions += other.num_deletions;
        self.num_deleted_bases += other.num_deleted_bases;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct OverlappingCounters {
    total_bases: u64,
    num_overlapping: u64,
    disagrees_ref_only: u64,
    disagrees_ref_and_mate: u64,
    three_ways: u64,
}

impl OverlappingCounters {
    fn merge(&mut self, other: Self) {
        self.total_bases += other.total_bases;
        self.num_overlapping += other.num_overlapping;
        self.disagrees_ref_only += other.disagrees_ref_only;
        self.disagrees_ref_and_mate += other.disagrees_ref_and_mate;
        self.three_ways += other.three_ways;
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct MetricCounters {
    active: bool,
    simple: SimpleCounters,
    indel: IndelCounters,
    overlapping: OverlappingCounters,
}

impl MetricCounters {
    fn merge(&mut self, other: Self) {
        if !other.active {
            return;
        }
        self.active = true;
        self.simple.merge(other.simple);
        self.indel.merge(other.indel);
        self.overlapping.merge(other.overlapping);
    }

    fn apply_event(&mut self, kind: ErrorKind, ev: BaseEvent) {
        self.active = true;
        match kind {
            ErrorKind::Simple => match ev.op {
                EventOp::Match {
                    read_base,
                    ref_base,
                } => {
                    if !is_no_call(read_base) {
                        self.simple.total_bases += 1;
                        if read_base != ref_base {
                            self.simple.error_bases += 1;
                        }
                    }
                }
                EventOp::Insertion {
                    read_base: _,
                    ins_len,
                } => {
                    self.simple.total_bases += u64::from(ins_len);
                }
                EventOp::Deletion {
                    del_len: _,
                    read_base: _,
                } => {}
            },
            ErrorKind::Indel => match ev.op {
                EventOp::Match {
                    read_base,
                    ref_base: _,
                } => {
                    if !is_no_call(read_base) {
                        self.indel.total_bases += 1;
                    }
                }
                EventOp::Insertion {
                    read_base: _,
                    ins_len,
                } => {
                    self.indel.total_bases += u64::from(ins_len);
                    self.indel.num_insertions += 1;
                    self.indel.num_inserted_bases += u64::from(ins_len);
                }
                EventOp::Deletion {
                    del_len,
                    read_base: _,
                } => {
                    self.indel.num_deletions += 1;
                    self.indel.num_deleted_bases += u64::from(del_len);
                }
            },
            ErrorKind::Overlapping => match ev.op {
                EventOp::Match {
                    read_base,
                    ref_base,
                } => {
                    if !is_no_call(read_base) {
                        self.overlapping.total_bases += 1;
                    }
                    if let Some(mate_base) = ev.mate_base
                        && !is_no_call(read_base)
                        && !is_no_call(mate_base)
                    {
                        self.overlapping.num_overlapping += 1;
                        if read_base != ref_base {
                            if read_base == mate_base {
                                self.overlapping.disagrees_ref_only += 1;
                            } else if mate_base == ref_base {
                                self.overlapping.disagrees_ref_and_mate += 1;
                            } else {
                                self.overlapping.three_ways += 1;
                            }
                        }
                    }
                }
                EventOp::Insertion { read_base, ins_len } => {
                    self.overlapping.total_bases += u64::from(ins_len);
                    if let Some(mate_base) = ev.mate_base
                        && !is_no_call(read_base)
                        && !is_no_call(mate_base)
                    {
                        self.overlapping.num_overlapping += 1;
                        if read_base != ev.ref_base_at_locus {
                            if read_base == mate_base {
                                self.overlapping.disagrees_ref_only += 1;
                            } else if mate_base == ev.ref_base_at_locus {
                                self.overlapping.disagrees_ref_and_mate += 1;
                            } else {
                                self.overlapping.three_ways += 1;
                            }
                        }
                    }
                }
                EventOp::Deletion {
                    del_len: _,
                    read_base,
                } => {
                    if let Some(mate_base) = ev.mate_base
                        && !is_no_call(read_base)
                        && !is_no_call(mate_base)
                    {
                        self.overlapping.num_overlapping += 1;
                        if read_base != ev.ref_base_at_locus {
                            if read_base == mate_base {
                                self.overlapping.disagrees_ref_only += 1;
                            } else if mate_base == ev.ref_base_at_locus {
                                self.overlapping.disagrees_ref_and_mate += 1;
                            } else {
                                self.overlapping.three_ways += 1;
                            }
                        }
                    }
                }
            },
        }
    }
}

#[derive(Clone, Copy)]
enum EventOp {
    Match { read_base: u8, ref_base: u8 },
    Insertion { read_base: u8, ins_len: u32 },
    Deletion { del_len: u32, read_base: u8 },
}

#[derive(Clone, Copy)]
struct BaseEvent {
    op: EventOp,
    ref_base_at_locus: u8,
    mate_base: Option<u8>,
}

#[derive(Clone, Debug)]
struct MetricSpec {
    kind: ErrorKind,
    stratifiers: Vec<StratifierKind>,
    suffix: String,
}

#[derive(Clone, Debug)]
enum AggregatorStore {
    Single(MetricCounters),
    BaseQuality(Vec<MetricCounters>, BTreeMap<i32, MetricCounters>),
    ReadDir([MetricCounters; 2]),
    ReadOrd([MetricCounters; 2]),
    PairOrient(Box<[MetricCounters; 4]>),
    Gc(Vec<MetricCounters>),
    Mapq(Vec<MetricCounters>),
    Cycle(Vec<MetricCounters>),
    Mismatches(Vec<MetricCounters>, BTreeMap<i32, MetricCounters>),
    InsertLen(Vec<MetricCounters>, BTreeMap<i32, MetricCounters>),
    // Fixed lookup tables for 5-letter ACGTN contexts: 5*5*5 = 125
    OneBaseContext(Vec<MetricCounters>, BTreeMap<String, MetricCounters>),
    // Homopolymer length (0..32) * p_base (5) * c_base (5) = 800
    Homopolymer(Vec<MetricCounters>, BTreeMap<(i32, u8, u8), MetricCounters>),
    // Binned homopolymer (2) * p_base (5) * c_base (5) = 50
    BinnedHomopolymer(
        Vec<MetricCounters>,
        BTreeMap<(bool, u8, u8), MetricCounters>,
    ),
    // Interned read group ID -> MetricCounters, finalized to BTreeMap<String, MetricCounters>
    ReadGroup(Vec<MetricCounters>, BTreeMap<String, MetricCounters>),
    // Ord (2) * Cycle (dense)
    OrdCycle(Vec<[MetricCounters; 2]>),
    // Ord (2) * GC (101)
    OrdGc([Vec<MetricCounters>; 2]),
    // Ord (2) * Homopolymer (32 * 25 = 800)
    OrdHomopolymer(
        Vec<MetricCounters>,
        BTreeMap<(ReadOrdinality, i32, u8, u8), MetricCounters>,
    ),
    // Ord (2) * PreDinuc (25)
    OrdPreDinuc(
        Vec<MetricCounters>,
        BTreeMap<(ReadOrdinality, u8, u8), MetricCounters>,
    ),
    General(BTreeMap<CovariateKey, MetricCounters>),
}

const HP_DENSE_LIMIT: usize = 32;
const HOMOPOLYMER_ALPHABET: [u8; 5] = *b"ACGTN";

impl AggregatorStore {
    fn new_for_spec(spec: &MetricSpec) -> Self {
        match spec.stratifiers.as_slice() {
            [StratifierKind::All] => Self::Single(MetricCounters::default()),
            [StratifierKind::BaseQuality] => {
                Self::BaseQuality(vec![MetricCounters::default(); 96], BTreeMap::new())
            }
            [StratifierKind::ReadDirection] => Self::ReadDir([MetricCounters::default(); 2]),
            [StratifierKind::ReadOrdinality] => Self::ReadOrd([MetricCounters::default(); 2]),
            [StratifierKind::PairOrientation] => {
                Self::PairOrient(Box::new([MetricCounters::default(); 4]))
            }
            [StratifierKind::GcContent] => Self::Gc(vec![MetricCounters::default(); 101]),
            [StratifierKind::MappingQuality] => Self::Mapq(vec![MetricCounters::default(); 256]),
            [StratifierKind::Cycle] => Self::Cycle(Vec::new()),
            [StratifierKind::MismatchesInRead] => {
                Self::Mismatches(vec![MetricCounters::default(); 64], BTreeMap::new())
            }
            [StratifierKind::InsertLength] => {
                Self::InsertLen(vec![MetricCounters::default(); 1024], BTreeMap::new())
            }
            [StratifierKind::OneBasePaddedContext] => {
                Self::OneBaseContext(vec![MetricCounters::default(); 125], BTreeMap::new())
            }
            [StratifierKind::Homopolymer] => Self::Homopolymer(
                vec![MetricCounters::default(); HP_DENSE_LIMIT * 25],
                BTreeMap::new(),
            ),
            [StratifierKind::BinnedHomopolymer] => {
                Self::BinnedHomopolymer(vec![MetricCounters::default(); 50], BTreeMap::new())
            }
            [StratifierKind::ReadGroup] => Self::ReadGroup(Vec::new(), BTreeMap::new()),
            [StratifierKind::ReadOrdinality, StratifierKind::Cycle] => Self::OrdCycle(Vec::new()),
            [StratifierKind::ReadOrdinality, StratifierKind::GcContent] => Self::OrdGc([
                vec![MetricCounters::default(); 101],
                vec![MetricCounters::default(); 101],
            ]),
            [StratifierKind::ReadOrdinality, StratifierKind::Homopolymer] => Self::OrdHomopolymer(
                vec![MetricCounters::default(); 2 * HP_DENSE_LIMIT * 25],
                BTreeMap::new(),
            ),
            [StratifierKind::ReadOrdinality, StratifierKind::PreDinuc] => {
                Self::OrdPreDinuc(vec![MetricCounters::default(); 50], BTreeMap::new())
            }
            _ => Self::General(BTreeMap::new()),
        }
    }

    fn finalize_read_groups(&mut self, rg_names: &[String]) {
        if let Self::ReadGroup(vec, map) = self {
            for (idx, c) in vec.drain(..).enumerate() {
                if c.active
                    && let Some(name) = rg_names.get(idx)
                {
                    map.entry(name.clone()).or_default().merge(c);
                }
            }
        }
    }

    fn merge(&mut self, other: Self) {
        match (self, other) {
            (Self::Single(a), Self::Single(b)) => a.merge(b),
            (Self::BaseQuality(a_arr, a_map), Self::BaseQuality(b_arr, b_map))
            | (Self::Mismatches(a_arr, a_map), Self::Mismatches(b_arr, b_map))
            | (Self::InsertLen(a_arr, a_map), Self::InsertLen(b_arr, b_map)) => {
                for (dst, src) in a_arr.iter_mut().zip(b_arr) {
                    dst.merge(src);
                }
                for (k, v) in b_map {
                    a_map.entry(k).or_default().merge(v);
                }
            }
            (Self::ReadDir(a), Self::ReadDir(b)) | (Self::ReadOrd(a), Self::ReadOrd(b)) => {
                a[0].merge(b[0]);
                a[1].merge(b[1]);
            }
            (Self::PairOrient(a), Self::PairOrient(b)) => {
                for (dst, src) in a.iter_mut().zip(b.iter()) {
                    dst.merge(*src);
                }
            }
            (Self::Gc(a), Self::Gc(b)) | (Self::Mapq(a), Self::Mapq(b)) => {
                for (dst, src) in a.iter_mut().zip(b) {
                    dst.merge(src);
                }
            }
            (Self::Cycle(a), Self::Cycle(b)) => {
                if b.len() > a.len() {
                    a.resize(b.len(), MetricCounters::default());
                }
                for (dst, src) in a.iter_mut().zip(b) {
                    dst.merge(src);
                }
            }
            (Self::OneBaseContext(a_arr, a_map), Self::OneBaseContext(b_arr, b_map)) => {
                for (dst, src) in a_arr.iter_mut().zip(b_arr) {
                    dst.merge(src);
                }
                for (k, v) in b_map {
                    a_map.entry(k).or_default().merge(v);
                }
            }
            (Self::Homopolymer(a_arr, a_map), Self::Homopolymer(b_arr, b_map)) => {
                for (dst, src) in a_arr.iter_mut().zip(b_arr) {
                    dst.merge(src);
                }
                for (k, v) in b_map {
                    a_map.entry(k).or_default().merge(v);
                }
            }
            (Self::BinnedHomopolymer(a_arr, a_map), Self::BinnedHomopolymer(b_arr, b_map)) => {
                for (dst, src) in a_arr.iter_mut().zip(b_arr) {
                    dst.merge(src);
                }
                for (k, v) in b_map {
                    a_map.entry(k).or_default().merge(v);
                }
            }
            (Self::ReadGroup(a_vec, a_map), Self::ReadGroup(b_vec, b_map)) => {
                if b_vec.len() > a_vec.len() {
                    a_vec.resize(b_vec.len(), MetricCounters::default());
                }
                for (dst, src) in a_vec.iter_mut().zip(b_vec) {
                    dst.merge(src);
                }
                for (k, v) in b_map {
                    a_map.entry(k).or_default().merge(v);
                }
            }
            (Self::OrdCycle(a), Self::OrdCycle(b)) => {
                if b.len() > a.len() {
                    a.resize(b.len(), [MetricCounters::default(); 2]);
                }
                for (dst, src) in a.iter_mut().zip(b) {
                    dst[0].merge(src[0]);
                    dst[1].merge(src[1]);
                }
            }
            (Self::OrdGc(a), Self::OrdGc(b)) => {
                for (dst_vec, src_vec) in a.iter_mut().zip(b) {
                    for (dst, src) in dst_vec.iter_mut().zip(src_vec) {
                        dst.merge(src);
                    }
                }
            }
            (Self::OrdHomopolymer(a_arr, a_map), Self::OrdHomopolymer(b_arr, b_map)) => {
                for (dst, src) in a_arr.iter_mut().zip(b_arr) {
                    dst.merge(src);
                }
                for (k, v) in b_map {
                    a_map.entry(k).or_default().merge(v);
                }
            }
            (Self::OrdPreDinuc(a_arr, a_map), Self::OrdPreDinuc(b_arr, b_map)) => {
                for (dst, src) in a_arr.iter_mut().zip(b_arr) {
                    dst.merge(src);
                }
                for (k, v) in b_map {
                    a_map.entry(k).or_default().merge(v);
                }
            }
            (Self::General(a), Self::General(b)) => {
                for (k, v) in b {
                    a.entry(k).or_default().merge(v);
                }
            }
            _ => {}
        }
    }

    fn into_sorted_rows(self) -> Vec<(CovariateKey, MetricCounters)> {
        match self {
            Self::Single(c) => {
                if c.active {
                    vec![(CovariateKey::Str("all".to_string()), c)]
                } else {
                    Vec::new()
                }
            }
            Self::BaseQuality(arr, extra) => {
                let mut rows = Vec::new();
                for (q, c) in arr.into_iter().enumerate() {
                    if c.active {
                        rows.push((CovariateKey::Int(i32::try_from(q).unwrap_or(0)), c));
                    }
                }
                for (q, c) in extra {
                    if c.active {
                        rows.push((CovariateKey::Int(q), c));
                    }
                }
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            }
            Self::ReadDir(arr) => {
                let mut rows = Vec::new();
                if arr[0].active {
                    rows.push((CovariateKey::ReadDir(false), arr[0]));
                }
                if arr[1].active {
                    rows.push((CovariateKey::ReadDir(true), arr[1]));
                }
                rows
            }
            Self::ReadOrd(arr) => {
                let mut rows = Vec::new();
                if arr[0].active {
                    rows.push((CovariateKey::ReadOrd(ReadOrdinality::First), arr[0]));
                }
                if arr[1].active {
                    rows.push((CovariateKey::ReadOrd(ReadOrdinality::Second), arr[1]));
                }
                rows
            }
            Self::PairOrient(arr) => {
                let variants = [
                    PairOrientation::F1R2,
                    PairOrientation::F2R1,
                    PairOrientation::F1F2,
                    PairOrientation::R1R2,
                ];
                let mut rows = Vec::new();
                for (po, c) in variants.into_iter().zip(*arr) {
                    if c.active {
                        rows.push((CovariateKey::PairOrient(po), c));
                    }
                }
                rows
            }
            Self::Gc(arr) => {
                let mut rows = Vec::new();
                for (pct, c) in arr.into_iter().enumerate() {
                    if c.active {
                        rows.push((CovariateKey::GcPct(u8::try_from(pct).unwrap_or(0)), c));
                    }
                }
                rows
            }
            Self::Mapq(arr) => {
                let mut rows = Vec::new();
                for (mq, c) in arr.into_iter().enumerate() {
                    if c.active {
                        rows.push((CovariateKey::Int(i32::try_from(mq).unwrap_or(0)), c));
                    }
                }
                rows
            }
            Self::Cycle(vec) => {
                let mut rows = Vec::new();
                for (cycle, c) in vec.into_iter().enumerate() {
                    if c.active {
                        rows.push((CovariateKey::Int(i32::try_from(cycle).unwrap_or(0)), c));
                    }
                }
                rows
            }
            Self::Mismatches(arr, extra) => {
                let mut rows = Vec::new();
                for (m, c) in arr.into_iter().enumerate() {
                    if c.active {
                        rows.push((CovariateKey::Int(i32::try_from(m).unwrap_or(0)), c));
                    }
                }
                for (m, c) in extra {
                    if c.active {
                        rows.push((CovariateKey::Int(m), c));
                    }
                }
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            }
            Self::InsertLen(vec, extra) => {
                let mut rows = Vec::new();
                for (ilen, c) in vec.into_iter().enumerate() {
                    if c.active {
                        rows.push((CovariateKey::Int(i32::try_from(ilen).unwrap_or(0)), c));
                    }
                }
                for (ilen, c) in extra {
                    if c.active {
                        rows.push((CovariateKey::Int(ilen), c));
                    }
                }
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            }
            Self::OneBaseContext(arr, extra) => {
                const ALPHABET: [u8; 5] = *b"ACGTN";
                let mut rows = Vec::new();
                for (idx, c) in arr.into_iter().enumerate() {
                    if c.active {
                        let b0 = ALPHABET[idx / 25] as char;
                        let b1 = ALPHABET[(idx / 5) % 5] as char;
                        let b2 = ALPHABET[idx % 5] as char;
                        rows.push((CovariateKey::Str(format!("{b0}{b1}{b2}")), c));
                    }
                }
                for (s, c) in extra {
                    if c.active {
                        rows.push((CovariateKey::Str(s), c));
                    }
                }
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            }
            Self::Homopolymer(arr, extra) => {
                let mut rows = Vec::new();
                for (idx, c) in arr.into_iter().enumerate() {
                    if c.active {
                        let hp = i32::try_from(idx / 25).unwrap_or(0);
                        let p = HOMOPOLYMER_ALPHABET[(idx / 5) % 5] as char;
                        let cur = HOMOPOLYMER_ALPHABET[idx % 5] as char;
                        rows.push((
                            CovariateKey::Tuple(vec![
                                CovariateKey::Int(hp),
                                CovariateKey::Char(p),
                                CovariateKey::Char(cur),
                            ]),
                            c,
                        ));
                    }
                }
                for ((hp, p, cur), c) in extra {
                    if c.active {
                        rows.push((
                            CovariateKey::Tuple(vec![
                                CovariateKey::Int(hp),
                                CovariateKey::Char(p as char),
                                CovariateKey::Char(cur as char),
                            ]),
                            c,
                        ));
                    }
                }
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            }
            Self::BinnedHomopolymer(arr, extra) => {
                let mut rows = Vec::new();
                for (idx, c) in arr.into_iter().enumerate() {
                    if c.active {
                        let is_long = idx >= 25;
                        let rem = idx % 25;
                        let p = HOMOPOLYMER_ALPHABET[rem / 5] as char;
                        let cur = HOMOPOLYMER_ALPHABET[rem % 5] as char;
                        rows.push((
                            CovariateKey::Tuple(vec![
                                CovariateKey::LongShortHp(is_long),
                                CovariateKey::Char(p),
                                CovariateKey::Char(cur),
                            ]),
                            c,
                        ));
                    }
                }
                for ((is_long, p, cur), c) in extra {
                    if c.active {
                        rows.push((
                            CovariateKey::Tuple(vec![
                                CovariateKey::LongShortHp(is_long),
                                CovariateKey::Char(p as char),
                                CovariateKey::Char(cur as char),
                            ]),
                            c,
                        ));
                    }
                }
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            }
            Self::ReadGroup(_, map) => map
                .into_iter()
                .filter(|(_, c)| c.active)
                .map(|(name, c)| (CovariateKey::Str(name), c))
                .collect(),
            Self::OrdCycle(vec) => {
                let mut rows = Vec::new();
                for (ord_idx, ord) in [ReadOrdinality::First, ReadOrdinality::Second]
                    .into_iter()
                    .enumerate()
                {
                    for (cycle, pair) in vec.iter().enumerate() {
                        let c = pair[ord_idx];
                        if c.active {
                            rows.push((
                                CovariateKey::Tuple(vec![
                                    CovariateKey::ReadOrd(ord),
                                    CovariateKey::Int(i32::try_from(cycle).unwrap_or(0)),
                                ]),
                                c,
                            ));
                        }
                    }
                }
                rows
            }
            Self::OrdGc(arr) => {
                let mut rows = Vec::new();
                for (ord, ord_vec) in [ReadOrdinality::First, ReadOrdinality::Second]
                    .into_iter()
                    .zip(arr)
                {
                    for (pct, c) in ord_vec.into_iter().enumerate() {
                        if c.active {
                            rows.push((
                                CovariateKey::Tuple(vec![
                                    CovariateKey::ReadOrd(ord),
                                    CovariateKey::GcPct(u8::try_from(pct).unwrap_or(0)),
                                ]),
                                c,
                            ));
                        }
                    }
                }
                rows
            }
            Self::OrdHomopolymer(arr, extra) => {
                let mut rows = Vec::new();
                let span = HP_DENSE_LIMIT * 25;
                for (idx, c) in arr.into_iter().enumerate() {
                    if c.active {
                        let ord = if idx < span {
                            ReadOrdinality::First
                        } else {
                            ReadOrdinality::Second
                        };
                        let rem = idx % span;
                        let hp = i32::try_from(rem / 25).unwrap_or(0);
                        let p = HOMOPOLYMER_ALPHABET[(rem / 5) % 5] as char;
                        let cur = HOMOPOLYMER_ALPHABET[rem % 5] as char;
                        rows.push((
                            CovariateKey::Tuple(vec![
                                CovariateKey::ReadOrd(ord),
                                CovariateKey::Int(hp),
                                CovariateKey::Char(p),
                                CovariateKey::Char(cur),
                            ]),
                            c,
                        ));
                    }
                }
                for ((ord, hp, p, cur), c) in extra {
                    if c.active {
                        rows.push((
                            CovariateKey::Tuple(vec![
                                CovariateKey::ReadOrd(ord),
                                CovariateKey::Int(hp),
                                CovariateKey::Char(p as char),
                                CovariateKey::Char(cur as char),
                            ]),
                            c,
                        ));
                    }
                }
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            }
            Self::OrdPreDinuc(arr, extra) => {
                let mut rows = Vec::new();
                for (idx, c) in arr.into_iter().enumerate() {
                    if c.active {
                        let ord = if idx < 25 {
                            ReadOrdinality::First
                        } else {
                            ReadOrdinality::Second
                        };
                        let rem = idx % 25;
                        let p = HOMOPOLYMER_ALPHABET[rem / 5] as char;
                        let cur = HOMOPOLYMER_ALPHABET[rem % 5] as char;
                        rows.push((
                            CovariateKey::Tuple(vec![
                                CovariateKey::ReadOrd(ord),
                                CovariateKey::Char(p),
                                CovariateKey::Char(cur),
                            ]),
                            c,
                        ));
                    }
                }
                for ((ord, p, cur), c) in extra {
                    if c.active {
                        rows.push((
                            CovariateKey::Tuple(vec![
                                CovariateKey::ReadOrd(ord),
                                CovariateKey::Char(p as char),
                                CovariateKey::Char(cur as char),
                            ]),
                            c,
                        ));
                    }
                }
                rows.sort_by(|a, b| a.0.cmp(&b.0));
                rows
            }
            Self::General(map) => map.into_iter().collect(),
        }
    }
}

#[derive(Clone, Debug)]
struct ActiveRead {
    qname: Vec<u8>, // populated only if overlapping metrics enabled on paired primary
    qname_hash: u64,
    pos0: i64,
    tid: i32,
    mpos0: i64,
    mtid: i32,
    flags: u16,
    mapq: u8,
    seq: Vec<u8>,
    qual: Vec<u8>,
    gc_pct: u8,
    insert_len: i32,
    pair_orientation: Option<PairOrientation>,
    read_ordinality: Option<ReadOrdinality>,
    read_group_id: Option<u32>,
    nm_tag: Option<i32>,
}

impl ActiveRead {
    #[inline]
    fn is_paired(&self) -> bool {
        self.flags & 0x1 != 0
    }
    #[inline]
    fn is_unmapped(&self) -> bool {
        self.flags & 0x4 != 0
    }
    #[inline]
    fn is_neg(&self) -> bool {
        self.flags & 0x10 != 0
    }
    #[inline]
    fn is_first(&self) -> bool {
        self.flags & 0x40 != 0
    }
    #[inline]
    fn is_second(&self) -> bool {
        self.flags & 0x80 != 0
    }
    #[inline]
    fn is_secondary(&self) -> bool {
        self.flags & 0x100 != 0
    }
}

#[derive(Clone, Copy, Debug)]
enum LocusOp {
    Match {
        read_slot: u32,
        offset: u32,
    },
    Insertion {
        read_slot: u32,
        offset: u32,
        ins_len: u32,
    },
    Deletion {
        read_slot: u32,
        offset: i32,
        del_len: u32,
    },
}

#[derive(Clone)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "precomputed feature flags avoid repeated BAM aux and context scans in inner loop"
)]
struct Config {
    input: String,
    output: String,
    reference: String,
    specs: Vec<MetricSpec>,
    has_overlapping: bool,
    needs_rg: bool,
    needs_nm: bool,
    needs_hp: bool,
    needs_p_c: bool,
    needs_n: bool,
    needs_gc: bool,
    min_mapping_q: i32,
    min_base_q: i32,
    prior_q: i32,
    max_loci: u64,
    long_homopolymer: i32,
    file_extension: String,
    intervals_path: Option<String>,
}

pub(crate) fn run_collectsamerrormetrics(args: &[String]) -> Result<(), CommandError> {
    let args = normalize_picard_args_for_command("CollectSamErrorMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_args(&args)?;

    let input = required_scalar_for(&args, "INPUT", "CollectSamErrorMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectSamErrorMetrics")?;
    let reference = required_scalar_for(&args, "REFERENCE_SEQUENCE", "CollectSamErrorMetrics")?;

    if optional_scalar(&args, "VCF")?.is_some() {
        return Err(unsupported(
            "CollectSamErrorMetrics VCF known-variant filtering delegates to upstream Picard"
                .to_string(),
        ));
    }
    if let Some(prob) = optional_f64(&args, "PROBABILITY")?
        && (prob - 1.0).abs() > 1e-12
    {
        return Err(unsupported(
            "CollectSamErrorMetrics downsampling PROBABILITY < 1.0 delegates to upstream Picard"
                .to_string(),
        ));
    }

    let intervals_values = args.get("INTERVALS").cloned().unwrap_or_default();
    if intervals_values.len() > 1
        || intervals_values.first().is_some_and(|p| {
            let trimmed = p
                .strip_suffix(".gz")
                .or_else(|| p.strip_suffix(".GZ"))
                .unwrap_or(p);
            Path::new(trimmed)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("vcf"))
        })
    {
        return Err(unsupported(
            "CollectSamErrorMetrics multi-file or VCF INTERVALS delegates to upstream Picard"
                .to_string(),
        ));
    }
    let intervals_path = intervals_values.into_iter().next();

    if has_sam_extension(&input) {
        return Err(unsupported(
            "CollectSamErrorMetrics text SAM input delegates to upstream Picard".to_string(),
        ));
    }

    let min_mapping_q =
        i32::try_from(optional_u32(&args, "MIN_MAPPING_Q")?.unwrap_or(20)).unwrap_or(i32::MAX);
    let min_base_q =
        i32::try_from(optional_u32(&args, "MIN_BASE_Q")?.unwrap_or(20)).unwrap_or(i32::MAX);
    let prior_q = i32::try_from(optional_u32(&args, "PRIOR_Q")?.unwrap_or(30)).unwrap_or(i32::MAX);
    let max_loci_raw = optional_i64(&args, "MAX_LOCI")?.unwrap_or(0);
    if max_loci_raw < 0 {
        return Err("CollectSamErrorMetrics MAX_LOCI cannot be negative".into());
    }
    let max_loci = u64::try_from(max_loci_raw).unwrap_or(0);
    let long_homopolymer =
        i32::try_from(optional_u32(&args, "LONG_HOMOPOLYMER")?.unwrap_or(6)).unwrap_or(6);
    let file_extension = optional_scalar(&args, "FILE_EXTENSION")?.unwrap_or_default();

    let raw_directives = parse_error_metrics_directives(&args);
    let specs = compile_metric_specs(&raw_directives)?;
    let has_overlapping = specs.iter().any(|s| s.kind == ErrorKind::Overlapping);
    let needs_rg = specs
        .iter()
        .any(|s| s.stratifiers.contains(&StratifierKind::ReadGroup));
    let needs_nm = specs
        .iter()
        .any(|s| s.stratifiers.contains(&StratifierKind::MismatchesInRead));
    let needs_hp = specs.iter().any(|s| {
        s.stratifiers.iter().any(|k| {
            matches!(
                k,
                StratifierKind::Homopolymer
                    | StratifierKind::BinnedHomopolymer
                    | StratifierKind::HomopolymerLength
            )
        })
    });
    let needs_p_c = needs_hp
        || specs.iter().any(|s| {
            s.stratifiers.iter().any(|k| {
                matches!(
                    k,
                    StratifierKind::PreDinuc | StratifierKind::OneBasePaddedContext
                )
            })
        });
    let needs_n = specs.iter().any(|s| {
        s.stratifiers.iter().any(|k| {
            matches!(
                k,
                StratifierKind::PostDinuc | StratifierKind::OneBasePaddedContext
            )
        })
    });
    let needs_gc = specs
        .iter()
        .any(|s| s.stratifiers.contains(&StratifierKind::GcContent));

    let config = Config {
        input,
        output,
        reference,
        specs,
        has_overlapping,
        needs_rg,
        needs_nm,
        needs_hp,
        needs_p_c,
        needs_n,
        needs_gc,
        min_mapping_q,
        min_base_q,
        prior_q,
        max_loci,
        long_homopolymer,
        file_extension,
        intervals_path,
    };

    let mut reader = open_bam_reader_for_args(&config.input, &args).map_err(|e| e.clone())?;
    let header_bytes = reader.header().as_bytes().to_vec();
    let header_text = String::from_utf8_lossy(&header_bytes);
    if !header_is_coordinate_sorted(&header_text) {
        return Err("Input BAM must be sorted by coordinate".into());
    }

    let bam_contigs: Vec<(String, i64)> = reader
        .header()
        .target_names()
        .iter()
        .enumerate()
        .map(|(tid, name)| {
            let len = i64::try_from(
                reader
                    .header()
                    .target_len(u32::try_from(tid).unwrap_or(0))
                    .unwrap_or(0),
            )
            .unwrap_or(0);
            (String::from_utf8_lossy(name).to_string(), len)
        })
        .collect();

    let intervals_by_tid = if let Some(path) = &config.intervals_path {
        Some(load_intervals(path, &bam_contigs)?)
    } else {
        None
    };

    let faidx_reader =
        faidx::Reader::from_path(Path::new(&config.reference)).map_err(|e| e.to_string())?;
    validate_fasta_matches_bam(&faidx_reader, &bam_contigs)?;

    let mut stores: Vec<AggregatorStore> = config
        .specs
        .iter()
        .map(AggregatorStore::new_for_spec)
        .collect();

    let workers = bgzf_threads_for(HtsThreadRole::RegionWorker).unwrap_or(1);
    if config.max_loci == 0
        && workers > 1
        && let Some(index_path) = find_bam_index(&config.input)
    {
        drop(reader);
        run_parallel_indexed(
            &config,
            &index_path,
            &bam_contigs,
            intervals_by_tid.as_ref(),
            &mut stores,
            workers,
        )?;
    } else {
        drop(faidx_reader);
        run_sequential(
            &config,
            &mut reader,
            &bam_contigs,
            intervals_by_tid.as_ref(),
            &mut stores,
        )?;
    }

    let prior_error = 10.0_f64.powf(-(f64::from(config.prior_q)) / 10.0);
    for (spec, store) in config.specs.iter().zip(stores) {
        let out_path = format!("{}.{}{}", config.output, spec.suffix, config.file_extension);
        let content = format_metric_file(spec, store, prior_error);
        fs::write(&out_path, content).map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn run_sequential(
    config: &Config,
    reader: &mut bam::Reader,
    bam_contigs: &[(String, i64)],
    intervals_by_tid: Option<&BTreeMap<i32, Vec<(i64, i64)>>>,
    stores: &mut [AggregatorStore],
) -> Result<(), CommandError> {
    let faidx_reader =
        faidx::Reader::from_path(Path::new(&config.reference)).map_err(|e| e.to_string())?;
    let mut record = bam::Record::new();
    let mut active_tid: Option<i32> = None;
    let mut contig_seq: Vec<u8> = Vec::new();
    let mut state = PileupState::default();
    let mut processed_loci = 0_u64;

    while let Some(res) = reader.read(&mut record) {
        res.map_err(|e| e.to_string())?;
        if record.is_unmapped()
            || record.is_secondary()
            || record.is_supplementary()
            || record.is_quality_check_failed()
            || record.is_duplicate()
            || i32::from(record.mapq()) < config.min_mapping_q
        {
            continue;
        }
        let tid = record.tid();
        if tid < 0 {
            continue;
        }
        if let Some(iv_map) = intervals_by_tid
            && !iv_map.contains_key(&tid)
        {
            continue;
        }
        if active_tid != Some(tid) {
            if let Some(prev_tid) = active_tid {
                let prev_intervals = intervals_by_tid.and_then(|m| m.get(&prev_tid));
                let hit_limit = state.flush_until(
                    i64::MAX,
                    0,
                    i64::MAX,
                    &contig_seq,
                    prev_intervals,
                    config,
                    stores,
                    &mut processed_loci,
                );
                if hit_limit {
                    return Ok(());
                }
            }
            active_tid = Some(tid);
            state.reset_for_contig();
            let (name, len) = &bam_contigs[usize::try_from(tid).unwrap_or(0)];
            contig_seq = load_contig_upper(&faidx_reader, name, *len)?;
        }

        let pos0 = record.pos();
        let cur_intervals = intervals_by_tid.and_then(|m| m.get(&tid));
        if state.flush_until(
            pos0,
            0,
            i64::MAX,
            &contig_seq,
            cur_intervals,
            config,
            stores,
            &mut processed_loci,
        ) {
            return Ok(());
        }
        state.push_record(&record, config, cur_intervals);
    }

    if let Some(prev_tid) = active_tid {
        let prev_intervals = intervals_by_tid.and_then(|m| m.get(&prev_tid));
        let _ = state.flush_until(
            i64::MAX,
            0,
            i64::MAX,
            &contig_seq,
            prev_intervals,
            config,
            stores,
            &mut processed_loci,
        );
    }
    for store in stores.iter_mut() {
        store.finalize_read_groups(&state.rg_names);
    }

    Ok(())
}

fn run_parallel_indexed(
    config: &Config,
    index_path: &Path,
    bam_contigs: &[(String, i64)],
    intervals_by_tid: Option<&BTreeMap<i32, Vec<(i64, i64)>>>,
    stores: &mut [AggregatorStore],
    workers: usize,
) -> Result<(), CommandError> {
    let total_len: i64 = bam_contigs.iter().map(|&(_, len)| len.max(0)).sum();
    let target_chunks = i64::try_from(workers.saturating_mul(8)).unwrap_or(8).max(1);
    let chunk_span = (total_len / target_chunks).clamp(100_000, CHUNK_SPAN);

    let mut chunks: Vec<(i32, i64, i64)> = Vec::new();
    for (tid_idx, (_, len)) in bam_contigs.iter().enumerate() {
        let Ok(tid) = i32::try_from(tid_idx) else {
            continue;
        };
        if *len <= 0 {
            continue;
        }
        if let Some(iv_map) = intervals_by_tid {
            let Some(ivs) = iv_map.get(&tid) else {
                continue;
            };
            // Split covered range per contig cleanly on adaptive chunk boundaries
            let mut start = 0_i64;
            while start < *len {
                let end = (start + chunk_span).min(*len);
                if ivs.iter().any(|&(s, e)| s < end && e >= start) {
                    chunks.push((tid, start, end));
                }
                start = end;
            }
        } else {
            let mut start = 0_i64;
            while start < *len {
                let end = (start + chunk_span).min(*len);
                chunks.push((tid, start, end));
                start = end;
            }
        }
    }

    if chunks.is_empty() {
        return Ok(());
    }

    let next_chunk = AtomicUsize::new(0);
    let num_workers = workers.min(chunks.len());
    let outcomes = thread::scope(|scope| {
        let mut handles = Vec::with_capacity(num_workers);
        for _ in 0..num_workers {
            let next_chunk = &next_chunk;
            let chunks = &chunks;
            handles.push(
                scope.spawn(move || -> Result<Vec<AggregatorStore>, CommandError> {
                    let mut reader = bam::IndexedReader::from_path_and_index(
                        Path::new(&config.input),
                        index_path,
                    )
                    .map_err(|e| e.to_string())?;
                    if config.reference.to_ascii_lowercase().ends_with(".fa")
                        || config.reference.to_ascii_lowercase().ends_with(".fasta")
                        || config.reference.to_ascii_lowercase().ends_with(".fna")
                        || config.reference.to_ascii_lowercase().ends_with(".gz")
                    {
                        let _ = reader.set_reference(Path::new(&config.reference));
                    }
                    let faidx_reader = faidx::Reader::from_path(Path::new(&config.reference))
                        .map_err(|e| e.to_string())?;

                    let mut local_stores: Vec<AggregatorStore> = config
                        .specs
                        .iter()
                        .map(AggregatorStore::new_for_spec)
                        .collect();
                    let mut record = bam::Record::new();
                    let mut cached_tid: Option<i32> = None;
                    let mut contig_seq: Vec<u8> = Vec::new();
                    let mut state = PileupState::default();
                    let mut dummy_loci = 0_u64;

                    loop {
                        let idx = next_chunk.fetch_add(1, Ordering::Relaxed);
                        if idx >= chunks.len() {
                            break;
                        }
                        let (tid, chunk_start, chunk_end) = chunks[idx];
                        if cached_tid != Some(tid) {
                            let (name, len) = &bam_contigs[usize::try_from(tid).unwrap_or(0)];
                            contig_seq = load_contig_upper(&faidx_reader, name, *len)?;
                            cached_tid = Some(tid);
                        }
                        state.reset_for_contig();
                        reader
                            .fetch(FetchDefinition::Region(tid, chunk_start, chunk_end))
                            .map_err(|e| e.to_string())?;
                        let cur_intervals = intervals_by_tid.and_then(|m| m.get(&tid));

                        while let Some(res) = reader.read(&mut record) {
                            res.map_err(|e| e.to_string())?;
                            if record.is_unmapped()
                                || record.is_secondary()
                                || record.is_supplementary()
                                || record.is_quality_check_failed()
                                || record.is_duplicate()
                                || i32::from(record.mapq()) < config.min_mapping_q
                            {
                                continue;
                            }
                            let pos0 = record.pos();
                            if pos0 >= chunk_end {
                                break;
                            }
                            let _ = state.flush_until(
                                pos0,
                                chunk_start,
                                chunk_end,
                                &contig_seq,
                                cur_intervals,
                                config,
                                &mut local_stores,
                                &mut dummy_loci,
                            );
                            state.push_record(&record, config, cur_intervals);
                        }
                        let _ = state.flush_until(
                            chunk_end,
                            chunk_start,
                            chunk_end,
                            &contig_seq,
                            cur_intervals,
                            config,
                            &mut local_stores,
                            &mut dummy_loci,
                        );
                    }

                    for store in &mut local_stores {
                        store.finalize_read_groups(&state.rg_names);
                    }
                    Ok(local_stores)
                }),
            );
        }
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|_| Err("CollectSamErrorMetrics worker panicked".into()))
            })
            .collect::<Vec<_>>()
    });

    for outcome in outcomes {
        let local_stores = outcome?;
        for (dst, src) in stores.iter_mut().zip(local_stores) {
            dst.merge(src);
        }
    }
    Ok(())
}

#[derive(Default)]
struct PileupState {
    reads: Vec<ActiveRead>,
    free_slots: Vec<u32>,
    expiring: BinaryHeap<Reverse<(i64, u32)>>,
    head_pos: i64,
    ring: VecDeque<Vec<LocusOp>>,
    sparse: BTreeMap<i64, Vec<LocusOp>>,
    op_pool: Vec<Vec<LocusOp>>,
    rg_names: Vec<String>,
    rg_map: FxHashMap<Vec<u8>, u32>,
    scratch_locus_ops: Vec<LocusOp>,
    scratch_mate_map: FxHashMap<(i64, u64, bool), (u32, u32)>,
}

#[inline]
fn hash_bytes_fnv1a(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

const MAX_RING_OFFSET: i64 = 4096;

fn first_interval_pos_for_deletion(
    del_start: i64,
    del_len: i64,
    intervals: Option<&Vec<(i64, i64)>>,
) -> Option<i64> {
    let Some(ivs) = intervals else {
        return Some(del_start);
    };
    let del_end = del_start + del_len - 1;
    let idx = ivs.partition_point(|&(s, _)| s <= del_end);
    for &(s, e) in &ivs[..idx] {
        if e >= del_start {
            return Some(del_start.max(s));
        }
    }
    None
}

impl PileupState {
    fn reset_for_contig(&mut self) {
        self.free_slots.clear();
        for idx in 0..self.reads.len() {
            self.reads[idx].qname.clear();
            self.reads[idx].seq.clear();
            self.reads[idx].qual.clear();
            if let Ok(slot) = u32::try_from(idx) {
                self.free_slots.push(slot);
            }
        }
        self.expiring.clear();
        self.head_pos = 0;
        while let Some(mut v) = self.ring.pop_front() {
            v.clear();
            self.op_pool.push(v);
        }
        self.sparse.clear();
    }

    fn intern_rg(&mut self, bytes: &[u8]) -> u32 {
        if let Some(&id) = self.rg_map.get(bytes) {
            return id;
        }
        let id = u32::try_from(self.rg_names.len()).unwrap_or(u32::MAX);
        self.rg_names
            .push(String::from_utf8_lossy(bytes).into_owned());
        self.rg_map.insert(bytes.to_vec(), id);
        id
    }

    fn alloc_vec(&mut self) -> Vec<LocusOp> {
        self.op_pool.pop().unwrap_or_default()
    }

    fn alloc_read_slot(&mut self) -> u32 {
        if let Some(slot) = self.free_slots.pop() {
            slot
        } else {
            let slot = u32::try_from(self.reads.len()).unwrap_or(u32::MAX);
            self.reads.push(ActiveRead {
                qname: Vec::new(),
                qname_hash: 0,
                pos0: 0,
                tid: -1,
                mpos0: -1,
                mtid: -1,
                flags: 0x4,
                mapq: 0,
                seq: Vec::new(),
                qual: Vec::new(),
                gc_pct: 0,
                insert_len: 0,
                pair_orientation: None,
                read_ordinality: None,
                read_group_id: None,
                nm_tag: None,
            });
            slot
        }
    }

    fn enqueue_op(&mut self, pos: i64, op: LocusOp) {
        if pos < self.head_pos {
            return;
        }
        let delta = pos - self.head_pos;
        if delta < MAX_RING_OFFSET {
            let idx = usize::try_from(delta).unwrap_or(0);
            while self.ring.len() <= idx {
                let v = self.alloc_vec();
                self.ring.push_back(v);
            }
            self.ring[idx].push(op);
        } else {
            self.sparse.entry(pos).or_default().push(op);
        }
    }

    fn push_record(
        &mut self,
        record: &bam::Record,
        config: &Config,
        intervals: Option<&Vec<(i64, i64)>>,
    ) {
        let read_group_id = if config.needs_rg {
            match record.aux(b"RG") {
                Ok(Aux::String(v)) => Some(self.intern_rg(v.as_bytes())),
                _ => None,
            }
        } else {
            None
        };
        let nm_tag = if config.needs_nm {
            record.aux(b"NM").ok().and_then(|aux| match aux {
                Aux::I8(v) => Some(i32::from(v)),
                Aux::U8(v) => Some(i32::from(v)),
                Aux::I16(v) => Some(i32::from(v)),
                Aux::U16(v) => Some(i32::from(v)),
                Aux::I32(v) => Some(v),
                Aux::U32(v) => i32::try_from(v).ok(),
                _ => None,
            })
        } else {
            None
        };

        let read_slot = self.alloc_read_slot();
        let slot_ref = &mut self.reads[read_slot as usize];

        let encoded = record.seq().encoded;
        let seq_len = record.seq_len();
        slot_ref.seq.clear();
        slot_ref.seq.resize(seq_len, 0);
        let full_pairs = seq_len / 2;
        let mut gc_count = 0_usize;
        if config.needs_gc {
            for (pair_idx, &byte) in encoded.iter().enumerate().take(full_pairs) {
                let b0 = BAM_ENC_TO_ASCII[usize::from((byte >> 4) & 0xf)];
                let b1 = BAM_ENC_TO_ASCII[usize::from(byte & 0xf)];
                slot_ref.seq[pair_idx * 2] = b0;
                slot_ref.seq[pair_idx * 2 + 1] = b1;
                gc_count +=
                    usize::from(b0 == b'G' || b0 == b'C') + usize::from(b1 == b'G' || b1 == b'C');
            }
            if seq_len & 1 != 0 {
                let byte = encoded[full_pairs];
                let b0 = BAM_ENC_TO_ASCII[usize::from((byte >> 4) & 0xf)];
                slot_ref.seq[seq_len - 1] = b0;
                gc_count += usize::from(b0 == b'G' || b0 == b'C');
            }
        } else {
            for (pair_idx, &byte) in encoded.iter().enumerate().take(full_pairs) {
                slot_ref.seq[pair_idx * 2] = BAM_ENC_TO_ASCII[usize::from((byte >> 4) & 0xf)];
                slot_ref.seq[pair_idx * 2 + 1] = BAM_ENC_TO_ASCII[usize::from(byte & 0xf)];
            }
            if seq_len & 1 != 0 {
                let byte = encoded[full_pairs];
                slot_ref.seq[seq_len - 1] = BAM_ENC_TO_ASCII[usize::from((byte >> 4) & 0xf)];
            }
        }

        slot_ref.qual.clear();
        slot_ref.qual.extend_from_slice(record.qual());
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "GC percentage rounded to 0..=100 matching SequenceUtil.calculateGc * 100"
        )]
        let gc_pct = if !config.needs_gc || seq_len == 0 {
            0
        } else {
            (((gc_count as f64 / seq_len as f64) * 100.0).round() as usize).min(100) as u8
        };

        let is_paired = record.is_paired();
        let is_first = record.is_first_in_template();
        let is_second = record.is_last_in_template();
        let is_unmapped = record.is_unmapped();
        let is_neg = record.is_reverse();
        let mate_neg = record.is_mate_reverse();
        let mate_unmapped = record.is_mate_unmapped();

        let read_ordinality = if is_paired && is_first != is_second {
            if is_first {
                Some(ReadOrdinality::First)
            } else {
                Some(ReadOrdinality::Second)
            }
        } else {
            None
        };

        let pair_orientation = if is_paired
            && !is_unmapped
            && !mate_unmapped
            && let Some(ord) = read_ordinality
        {
            let read_pos = !is_neg;
            let mate_pos = !mate_neg;
            let (first_pos, second_pos) = match ord {
                ReadOrdinality::First => (read_pos, mate_pos),
                ReadOrdinality::Second => (mate_pos, read_pos),
            };
            // Matches Picard ReadBaseStratification$PairOrientation.ofExplicit bytecode
            Some(match (first_pos, second_pos) {
                (true, true) => PairOrientation::R1R2,
                (false, false) => PairOrientation::F1F2,
                (true, false) => PairOrientation::F1R2,
                (false, true) => PairOrientation::F2R1,
            })
        } else {
            None
        };

        let max_ins = i32::try_from(seq_len.saturating_mul(10)).unwrap_or(i32::MAX);
        let abs_tlen = i32::try_from(record.insert_size().unsigned_abs()).unwrap_or(i32::MAX);
        let insert_len = max_ins.min(abs_tlen);

        let pos0 = record.pos();
        slot_ref.qname.clear();
        slot_ref.qname_hash = if config.has_overlapping && is_paired && !is_unmapped {
            let qn = record.qname();
            slot_ref.qname.extend_from_slice(qn);
            hash_bytes_fnv1a(qn)
        } else {
            0
        };
        slot_ref.pos0 = pos0;
        slot_ref.tid = record.tid();
        slot_ref.mpos0 = record.mpos();
        slot_ref.mtid = record.mtid();
        slot_ref.flags = record.flags();
        slot_ref.mapq = record.mapq();
        slot_ref.gc_pct = gc_pct;
        slot_ref.insert_len = insert_len;
        slot_ref.pair_orientation = pair_orientation;
        slot_ref.read_ordinality = read_ordinality;
        slot_ref.read_group_id = read_group_id;
        slot_ref.nm_tag = nm_tag;

        if self.ring.is_empty() && self.sparse.is_empty() {
            self.head_pos = pos0;
        }

        // Enqueue raw CIGAR ops into ring/sparse buffer without heap allocation
        let mut rpos = pos0;
        let mut qpos = 0_i64;
        let mut max_enqueued_pos = pos0;
        for &raw_op in record.raw_cigar() {
            let op_code = raw_op & 0xf;
            let op_len = raw_op >> 4;
            let op_len_i64 = i64::from(op_len);
            match op_code {
                // M (0), = (7), X (8)
                0 | 7 | 8 => {
                    if op_len_i64 > 0
                        && rpos >= self.head_pos
                        && (rpos - self.head_pos) + op_len_i64 <= MAX_RING_OFFSET
                    {
                        let start_delta = usize::try_from(rpos - self.head_pos).unwrap_or(0);
                        let len_usize = usize::try_from(op_len).unwrap_or(0);
                        let end_delta = start_delta + len_usize;
                        while self.ring.len() < end_delta {
                            let v = self.alloc_vec();
                            self.ring.push_back(v);
                        }
                        let base_qo = u32::try_from(qpos).unwrap_or(u32::MAX);
                        for i in 0..len_usize {
                            let qo = base_qo.saturating_add(u32::try_from(i).unwrap_or(u32::MAX));
                            self.ring[start_delta + i].push(LocusOp::Match {
                                read_slot,
                                offset: qo,
                            });
                        }
                        let last_p = rpos + op_len_i64 - 1;
                        if last_p > max_enqueued_pos {
                            max_enqueued_pos = last_p;
                        }
                    } else {
                        for i in 0..op_len_i64 {
                            let qo = u32::try_from(qpos + i).unwrap_or(u32::MAX);
                            let p = rpos + i;
                            if p > max_enqueued_pos {
                                max_enqueued_pos = p;
                            }
                            self.enqueue_op(
                                p,
                                LocusOp::Match {
                                    read_slot,
                                    offset: qo,
                                },
                            );
                        }
                    }
                    rpos += op_len_i64;
                    qpos += op_len_i64;
                }
                // I (1)
                1 => {
                    if rpos > pos0 {
                        let qo = u32::try_from(qpos).unwrap_or(0);
                        let p = rpos - 1;
                        if p > max_enqueued_pos {
                            max_enqueued_pos = p;
                        }
                        self.enqueue_op(
                            p,
                            LocusOp::Insertion {
                                read_slot,
                                offset: qo,
                                ins_len: op_len,
                            },
                        );
                    }
                    qpos += op_len_i64;
                }
                // D (2)
                2 => {
                    if let Some(del_pos) =
                        first_interval_pos_for_deletion(rpos, op_len_i64, intervals)
                    {
                        let del_offset = i32::try_from(qpos - 1).unwrap_or(-1);
                        if del_pos > max_enqueued_pos {
                            max_enqueued_pos = del_pos;
                        }
                        self.enqueue_op(
                            del_pos,
                            LocusOp::Deletion {
                                read_slot,
                                offset: del_offset,
                                del_len: op_len,
                            },
                        );
                    }
                    rpos += op_len_i64;
                }
                // N (3)
                3 => {
                    rpos += op_len_i64;
                }
                // S (4)
                4 => {
                    qpos += op_len_i64;
                }
                // H (5), P (6)
                _ => {}
            }
        }

        self.expiring.push(Reverse((max_enqueued_pos, read_slot)));
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "sliding-window locus flusher processes queued ops in exact position order"
    )]
    fn flush_until(
        &mut self,
        target_pos: i64,
        chunk_start: i64,
        chunk_end: i64,
        contig_seq: &[u8],
        intervals: Option<&Vec<(i64, i64)>>,
        config: &Config,
        stores: &mut [AggregatorStore],
        processed_loci: &mut u64,
    ) -> bool {
        let mut locus_ops = std::mem::take(&mut self.scratch_locus_ops);
        let mut hit_max_loci = false;
        while self.head_pos < target_pos {
            while let Some(&Reverse((end_pos, slot))) = self.expiring.peek() {
                if end_pos < self.head_pos {
                    self.expiring.pop();
                    self.free_slots.push(slot);
                } else {
                    break;
                }
            }

            if self.ring.is_empty() {
                if let Some(&next_sparse) = self.sparse.keys().next() {
                    if next_sparse < target_pos {
                        self.head_pos = next_sparse;
                    } else {
                        self.head_pos = target_pos;
                        break;
                    }
                } else {
                    self.head_pos = target_pos;
                    break;
                }
            }

            let pos = self.head_pos;
            locus_ops.clear();
            if let Some(mut ring_vec) = self.ring.pop_front() {
                locus_ops.append(&mut ring_vec);
                self.op_pool.push(ring_vec);
            }
            if let Some(sparse_vec) = self.sparse.remove(&pos) {
                locus_ops.extend(sparse_vec);
            }
            self.head_pos += 1;

            if locus_ops.is_empty() {
                continue;
            }
            if pos < chunk_start || pos >= chunk_end {
                continue;
            }
            if let Some(ivs) = intervals
                && !position_in_intervals(pos, ivs)
            {
                continue;
            }
            let Ok(pos_idx) = usize::try_from(pos) else {
                continue;
            };
            let Some(&ref_base) = contig_seq.get(pos_idx) else {
                continue;
            };

            let any_emitted = evaluate_locus(
                pos,
                ref_base,
                &locus_ops,
                &self.reads,
                &self.rg_names,
                &mut self.scratch_mate_map,
                config,
                stores,
            );
            if any_emitted {
                *processed_loci += 1;
                if config.max_loci > 0 && *processed_loci >= config.max_loci {
                    hit_max_loci = true;
                    break;
                }
            }
        }
        locus_ops.clear();
        self.scratch_locus_ops = locus_ops;
        hit_max_loci
    }
}

fn position_in_intervals(pos: i64, intervals: &[(i64, i64)]) -> bool {
    let idx = intervals.partition_point(|&(start, _)| start <= pos);
    idx > 0 && pos <= intervals[idx - 1].1
}

#[expect(
    clippy::too_many_arguments,
    reason = "reuses locus scratch buffers across loci without heap allocation"
)]
fn evaluate_locus(
    pos: i64,
    ref_base: u8,
    ops: &[LocusOp],
    reads: &[ActiveRead],
    rg_names: &[String],
    mate_map: &mut FxHashMap<(i64, u64, bool), (u32, u32)>,
    config: &Config,
    stores: &mut [AggregatorStore],
) -> bool {
    let mut processed_any = false;
    let use_mate_map = config.has_overlapping && ops.len() > 12;
    let mut mate_map_built = false;

    // Helper closure to find overlapping mate base among valid Match ops at this locus
    let mut find_mate_base = |r: &ActiveRead| -> Option<u8> {
        if !config.has_overlapping
            || !r.is_paired()
            || r.is_unmapped()
            || r.is_secondary()
            || r.mtid != r.tid
            || r.mpos0 > pos
        {
            return None;
        }
        if use_mate_map {
            if !mate_map_built {
                mate_map.clear();
                for &op in ops {
                    if let LocusOp::Match { read_slot, offset } = op {
                        let m = &reads[read_slot as usize];
                        if m.is_paired()
                            && !m.is_unmapped()
                            && !m.is_secondary()
                            && m.qual
                                .get(offset as usize)
                                .is_some_and(|&q| i32::from(q) >= config.min_base_q)
                        {
                            mate_map
                                .entry((m.pos0, m.qname_hash, m.is_first()))
                                .or_insert((read_slot, offset));
                        }
                    }
                }
                mate_map_built = true;
            }
            if let Some(&(read_slot, offset)) =
                mate_map.get(&(r.mpos0, r.qname_hash, !r.is_first()))
            {
                let m = &reads[read_slot as usize];
                if m.tid == r.mtid
                    && r.is_second() != m.is_second()
                    && m.qname == r.qname
                    && let Some(&mate_base) = m.seq.get(offset as usize)
                {
                    return Some(mate_base);
                }
            }
        }
        for &op in ops {
            if let LocusOp::Match { read_slot, offset } = op {
                let m = &reads[read_slot as usize];
                if m.qname_hash == r.qname_hash
                    && r.mpos0 == m.pos0
                    && r.mtid == m.tid
                    && r.is_first() != m.is_first()
                    && r.is_second() != m.is_second()
                    && m.is_paired()
                    && !m.is_unmapped()
                    && !m.is_secondary()
                    && m.qual
                        .get(offset as usize)
                        .is_some_and(|&q| i32::from(q) >= config.min_base_q)
                    && m.qname == r.qname
                    && let Some(&mate_base) = m.seq.get(offset as usize)
                {
                    return Some(mate_base);
                }
            }
        }
        None
    };

    // Order in Picard CollectSamErrorMetrics.addLocusBases:
    // 1. All getRecordAndOffsets (Match, then Deletion if not previously seen - note SamLocusIterator puts Match and Deletion in recordAndOffsets, then InsertedInRecord second!)
    for &op in ops {
        match op {
            LocusOp::Match { read_slot, offset } => {
                let r = &reads[read_slot as usize];
                let off = offset as usize;
                let Some(&q_val) = r.qual.get(off) else {
                    continue;
                };
                if i32::from(q_val) < config.min_base_q {
                    continue;
                }
                let Some(&read_base) = r.seq.get(off) else {
                    continue;
                };
                processed_any = true;
                let mate_base = find_mate_base(r);
                let ev = BaseEvent {
                    op: EventOp::Match {
                        read_base,
                        ref_base,
                    },
                    ref_base_at_locus: ref_base,
                    mate_base,
                };
                dispatch_to_stores(
                    config,
                    stores,
                    r,
                    rg_names,
                    isize::try_from(off).unwrap_or(0),
                    Some(q_val),
                    read_base,
                    ref_base,
                    ev,
                );
            }
            LocusOp::Deletion {
                read_slot,
                offset,
                del_len,
            } => {
                let r = &reads[read_slot as usize];
                processed_any = true;
                let del_qual = usize::try_from(offset)
                    .ok()
                    .and_then(|off| r.qual.get(off).copied());
                let read_base = usize::try_from(offset)
                    .ok()
                    .and_then(|off| r.seq.get(off).copied())
                    .unwrap_or(b'N');
                let mate_base = find_mate_base(r);
                let ev = BaseEvent {
                    op: EventOp::Deletion { del_len, read_base },
                    ref_base_at_locus: ref_base,
                    mate_base,
                };
                dispatch_to_stores(
                    config,
                    stores,
                    r,
                    rg_names,
                    offset as isize,
                    del_qual,
                    read_base,
                    ref_base,
                    ev,
                );
            }
            LocusOp::Insertion { .. } => {}
        }
    }

    for &op in ops {
        if let LocusOp::Insertion {
            read_slot,
            offset,
            ins_len,
        } = op
        {
            let r = &reads[read_slot as usize];
            let off = offset as usize;
            let Some(&q_val) = r.qual.get(off) else {
                continue;
            };
            if i32::from(q_val) < config.min_base_q {
                continue;
            }
            let Some(&read_base) = r.seq.get(off) else {
                continue;
            };
            processed_any = true;
            let mate_base = find_mate_base(r);
            let ev = BaseEvent {
                op: EventOp::Insertion { read_base, ins_len },
                ref_base_at_locus: ref_base,
                mate_base,
            };
            dispatch_to_stores(
                config,
                stores,
                r,
                rg_names,
                isize::try_from(off).unwrap_or(0),
                Some(q_val),
                read_base,
                ref_base,
                ev,
            );
        }
    }

    processed_any
}

#[expect(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    reason = "hot inner dispatch updates all active aggregators once per base without heap allocations"
)]
fn dispatch_to_stores(
    config: &Config,
    stores: &mut [AggregatorStore],
    r: &ActiveRead,
    rg_names: &[String],
    offset: isize,
    qual_opt: Option<u8>,
    read_base: u8,
    ref_base: u8,
    ev: BaseEvent,
) {
    let r_len_i32 = i32::try_from(r.seq.len()).unwrap_or(0);
    let offset_i32 = i32::try_from(offset).unwrap_or(-1);
    let is_neg = r.is_neg();
    let cycle = if is_neg {
        (r_len_i32 - offset_i32 - 1) + 1
    } else {
        offset_i32 + 1
    };

    let (p_base, c_base) = if config.needs_p_c {
        (
            stratify_read_base(r, offset, -1),
            stratify_ref_base(is_neg, ref_base),
        )
    } else {
        (None, None)
    };
    let n_base = if config.needs_n {
        stratify_read_base(r, offset, 1)
    } else {
        None
    };
    let hp_len = if config.needs_hp && c_base.is_some() {
        stratify_homopolymer_length(r, offset, ref_base)
    } else {
        None
    };

    for (spec, store) in config.specs.iter().zip(stores.iter_mut()) {
        match store {
            AggregatorStore::Single(c) => c.apply_event(spec.kind, ev),
            AggregatorStore::BaseQuality(arr, extra) => {
                if let Some(q) = qual_opt {
                    let qi = q as usize;
                    if qi < arr.len() {
                        arr[qi].apply_event(spec.kind, ev);
                    } else {
                        extra
                            .entry(i32::from(q))
                            .or_default()
                            .apply_event(spec.kind, ev);
                    }
                }
            }
            AggregatorStore::ReadDir(arr) => {
                arr[usize::from(is_neg)].apply_event(spec.kind, ev);
            }
            AggregatorStore::ReadOrd(arr) => {
                if let Some(ord) = r.read_ordinality {
                    arr[ord as usize].apply_event(spec.kind, ev);
                }
            }
            AggregatorStore::PairOrient(arr) => {
                if let Some(po) = r.pair_orientation {
                    arr[po as usize].apply_event(spec.kind, ev);
                }
            }
            AggregatorStore::Gc(arr) => {
                arr[r.gc_pct as usize].apply_event(spec.kind, ev);
            }
            AggregatorStore::Mapq(arr) => {
                arr[r.mapq as usize].apply_event(spec.kind, ev);
            }
            AggregatorStore::Cycle(vec) => {
                if let Ok(ci) = usize::try_from(cycle) {
                    if vec.len() <= ci {
                        vec.resize(ci + 1, MetricCounters::default());
                    }
                    vec[ci].apply_event(spec.kind, ev);
                }
            }
            AggregatorStore::Mismatches(arr, extra) => {
                if let Some(nm) = r.nm_tag {
                    let val = if read_base == ref_base { nm } else { nm - 1 };
                    if let Ok(vi) = usize::try_from(val)
                        && vi < arr.len()
                    {
                        arr[vi].apply_event(spec.kind, ev);
                    } else {
                        extra.entry(val).or_default().apply_event(spec.kind, ev);
                    }
                }
            }
            AggregatorStore::InsertLen(vec, extra) => {
                if let Ok(ii) = usize::try_from(r.insert_len)
                    && ii < vec.len()
                {
                    vec[ii].apply_event(spec.kind, ev);
                } else {
                    extra
                        .entry(r.insert_len)
                        .or_default()
                        .apply_event(spec.kind, ev);
                }
            }
            AggregatorStore::OneBaseContext(arr, extra) => {
                if let (Some(b0), Some(b1), Some(b2)) = (p_base, c_base, n_base) {
                    if let (Some(i0), Some(i1), Some(i2)) =
                        (base_idx5(b0), base_idx5(b1), base_idx5(b2))
                    {
                        arr[i0 * 25 + i1 * 5 + i2].apply_event(spec.kind, ev);
                    } else {
                        let s = format!("{}{}{}", b0 as char, b1 as char, b2 as char);
                        extra.entry(s).or_default().apply_event(spec.kind, ev);
                    }
                }
            }
            AggregatorStore::Homopolymer(arr, extra) => {
                if let (Some(hp), Some(p), Some(cur)) = (hp_len, p_base, c_base) {
                    if let (Ok(hpi), Some(ip), Some(ic)) =
                        (usize::try_from(hp), base_idx5(p), base_idx5(cur))
                        && hpi < HP_DENSE_LIMIT
                    {
                        arr[hpi * 25 + ip * 5 + ic].apply_event(spec.kind, ev);
                    } else {
                        extra
                            .entry((hp, p, cur))
                            .or_default()
                            .apply_event(spec.kind, ev);
                    }
                }
            }
            AggregatorStore::BinnedHomopolymer(arr, extra) => {
                if let (Some(hp), Some(p), Some(cur)) = (hp_len, p_base, c_base) {
                    let is_long = hp >= config.long_homopolymer;
                    if let (Some(ip), Some(ic)) = (base_idx5(p), base_idx5(cur)) {
                        arr[usize::from(is_long) * 25 + ip * 5 + ic].apply_event(spec.kind, ev);
                    } else {
                        extra
                            .entry((is_long, p, cur))
                            .or_default()
                            .apply_event(spec.kind, ev);
                    }
                }
            }
            AggregatorStore::ReadGroup(vec, _) => {
                if let Some(rg_id) = r.read_group_id {
                    let idx = rg_id as usize;
                    if vec.len() <= idx {
                        vec.resize(idx + 1, MetricCounters::default());
                    }
                    vec[idx].apply_event(spec.kind, ev);
                }
            }
            AggregatorStore::OrdCycle(vec) => {
                if let Some(ord) = r.read_ordinality
                    && let Ok(ci) = usize::try_from(cycle)
                {
                    if vec.len() <= ci {
                        vec.resize(ci + 1, [MetricCounters::default(); 2]);
                    }
                    vec[ci][ord as usize].apply_event(spec.kind, ev);
                }
            }
            AggregatorStore::OrdGc(arr) => {
                if let Some(ord) = r.read_ordinality {
                    arr[ord as usize][r.gc_pct as usize].apply_event(spec.kind, ev);
                }
            }
            AggregatorStore::OrdHomopolymer(arr, extra) => {
                if let (Some(ord), Some(hp), Some(p), Some(cur)) =
                    (r.read_ordinality, hp_len, p_base, c_base)
                {
                    if let (Ok(hpi), Some(ip), Some(ic)) =
                        (usize::try_from(hp), base_idx5(p), base_idx5(cur))
                        && hpi < HP_DENSE_LIMIT
                    {
                        arr[(ord as usize) * HP_DENSE_LIMIT * 25 + hpi * 25 + ip * 5 + ic]
                            .apply_event(spec.kind, ev);
                    } else {
                        extra
                            .entry((ord, hp, p, cur))
                            .or_default()
                            .apply_event(spec.kind, ev);
                    }
                }
            }
            AggregatorStore::OrdPreDinuc(arr, extra) => {
                if let (Some(ord), Some(p), Some(cur)) = (r.read_ordinality, p_base, c_base) {
                    if let (Some(ip), Some(ic)) = (base_idx5(p), base_idx5(cur)) {
                        arr[(ord as usize) * 25 + ip * 5 + ic].apply_event(spec.kind, ev);
                    } else {
                        extra
                            .entry((ord, p, cur))
                            .or_default()
                            .apply_event(spec.kind, ev);
                    }
                }
            }
            AggregatorStore::General(map) => {
                if let Some(key) = compute_covariate_key(
                    &spec.stratifiers,
                    r,
                    rg_names,
                    offset,
                    qual_opt,
                    read_base,
                    ref_base,
                    cycle,
                    config.long_homopolymer,
                ) {
                    map.entry(key).or_default().apply_event(spec.kind, ev);
                }
            }
        }
    }
}

fn base_idx5(b: u8) -> Option<usize> {
    match b {
        b'A' => Some(0),
        b'C' => Some(1),
        b'G' => Some(2),
        b'T' => Some(3),
        b'N' => Some(4),
        _ => None,
    }
}

fn orient_base(b: u8, is_neg: bool) -> u8 {
    let ub = b.to_ascii_uppercase();
    if !is_neg {
        return ub;
    }
    match ub {
        b'A' => b'T',
        b'C' => b'G',
        b'G' => b'C',
        b'T' => b'A',
        other => other,
    }
}

fn is_no_call(b: u8) -> bool {
    let ub = b.to_ascii_uppercase();
    ub == b'N' || ub == b'.'
}

fn stratify_read_base(r: &ActiveRead, offset: isize, rel_dir: isize) -> Option<u8> {
    let is_neg = r.is_neg();
    let step = if is_neg { -rel_dir } else { rel_dir };
    let idx = offset + step;
    if idx < 0 {
        return None;
    }
    let u = usize::try_from(idx).ok()?;
    let &b = r.seq.get(u)?;
    Some(orient_base(b, is_neg))
}

fn stratify_ref_base(is_neg: bool, ref_base: u8) -> Option<u8> {
    if is_no_call(ref_base) {
        return None;
    }
    Some(orient_base(ref_base, is_neg))
}

fn stratify_homopolymer_length(r: &ActiveRead, offset: isize, ref_base: u8) -> Option<i32> {
    if is_no_call(ref_base) {
        return None;
    }
    let len = r.seq.len();
    let off = usize::try_from(offset).ok()?;
    if off >= len {
        return None;
    }
    if r.is_neg() {
        if off + 1 >= len {
            return Some(0);
        }
        let target = r.seq[off + 1];
        let mut cursor = off + 1;
        while cursor < len && r.seq[cursor] == target {
            cursor += 1;
        }
        Some(i32::try_from(cursor - off - 1).unwrap_or(0))
    } else {
        if off == 0 {
            return Some(0);
        }
        let target = r.seq[off - 1];
        let mut cursor = off.cast_signed() - 1;
        while cursor >= 0 && r.seq[cursor.cast_unsigned()] == target {
            cursor -= 1;
        }
        Some(i32::try_from(off.cast_signed() - cursor - 1).unwrap_or(0))
    }
}

fn one_base_context_bytes(r: &ActiveRead, offset: isize, ref_base: u8) -> Option<(u8, u8, u8)> {
    let b_prev = stratify_read_base(r, offset, -1)?;
    let b_cur = stratify_ref_base(r.is_neg(), ref_base)?;
    let b_next = stratify_read_base(r, offset, 1)?;
    Some((b_prev, b_cur, b_next))
}

#[expect(
    clippy::too_many_arguments,
    reason = "computes general and composite CovariateKey values matching Picard ReadBaseStratification"
)]
fn compute_single_covariate(
    strat: StratifierKind,
    r: &ActiveRead,
    rg_names: &[String],
    offset: isize,
    qual_opt: Option<u8>,
    read_base: u8,
    ref_base: u8,
    cycle: i32,
    long_hp: i32,
) -> Option<CovariateKey> {
    let is_neg = r.is_neg();
    match strat {
        StratifierKind::All => Some(CovariateKey::Str("all".to_string())),
        StratifierKind::BaseQuality => qual_opt.map(|q| CovariateKey::Int(i32::from(q))),
        StratifierKind::InsertLength => Some(CovariateKey::Int(r.insert_len)),
        StratifierKind::GcContent => Some(CovariateKey::GcPct(r.gc_pct)),
        StratifierKind::ReadDirection => Some(CovariateKey::ReadDir(is_neg)),
        StratifierKind::PairOrientation => r.pair_orientation.map(CovariateKey::PairOrient),
        StratifierKind::ReadOrdinality => r.read_ordinality.map(CovariateKey::ReadOrd),
        StratifierKind::Cycle => Some(CovariateKey::Int(cycle)),
        StratifierKind::MappingQuality => Some(CovariateKey::Int(i32::from(r.mapq))),
        StratifierKind::ReadGroup => r
            .read_group_id
            .and_then(|id| rg_names.get(id as usize))
            .map(|rg| CovariateKey::Str(rg.clone())),
        StratifierKind::MismatchesInRead => r.nm_tag.map(|nm| {
            let v = if read_base == ref_base { nm } else { nm - 1 };
            CovariateKey::Int(v)
        }),
        StratifierKind::PrevBase => {
            stratify_read_base(r, offset, -1).map(|b| CovariateKey::Char(b as char))
        }
        StratifierKind::NextBase => {
            stratify_read_base(r, offset, 1).map(|b| CovariateKey::Char(b as char))
        }
        StratifierKind::ReadBase => {
            stratify_read_base(r, offset, 0).map(|b| CovariateKey::Char(b as char))
        }
        StratifierKind::ReferenceBase => {
            stratify_ref_base(is_neg, ref_base).map(|b| CovariateKey::Char(b as char))
        }
        StratifierKind::PreDinuc => {
            let p = stratify_read_base(r, offset, -1)? as char;
            let c = stratify_ref_base(is_neg, ref_base)? as char;
            Some(CovariateKey::Tuple(vec![
                CovariateKey::Char(p),
                CovariateKey::Char(c),
            ]))
        }
        StratifierKind::PostDinuc => {
            let c = stratify_ref_base(is_neg, ref_base)? as char;
            let n = stratify_read_base(r, offset, 1)? as char;
            Some(CovariateKey::Tuple(vec![
                CovariateKey::Char(c),
                CovariateKey::Char(n),
            ]))
        }
        StratifierKind::HomopolymerLength => {
            stratify_homopolymer_length(r, offset, ref_base).map(CovariateKey::Int)
        }
        StratifierKind::Homopolymer => {
            let len = stratify_homopolymer_length(r, offset, ref_base)?;
            let p = stratify_read_base(r, offset, -1)? as char;
            let c = stratify_ref_base(is_neg, ref_base)? as char;
            Some(CovariateKey::Tuple(vec![
                CovariateKey::Int(len),
                CovariateKey::Char(p),
                CovariateKey::Char(c),
            ]))
        }
        StratifierKind::BinnedHomopolymer => {
            let len = stratify_homopolymer_length(r, offset, ref_base)?;
            let p = stratify_read_base(r, offset, -1)? as char;
            let c = stratify_ref_base(is_neg, ref_base)? as char;
            Some(CovariateKey::Tuple(vec![
                CovariateKey::LongShortHp(len >= long_hp),
                CovariateKey::Char(p),
                CovariateKey::Char(c),
            ]))
        }
        StratifierKind::OneBasePaddedContext => {
            let (b0, b1, b2) = one_base_context_bytes(r, offset, ref_base)?;
            Some(CovariateKey::Str(format!(
                "{}{}{}",
                b0 as char, b1 as char, b2 as char
            )))
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "computes flattened PairStratifier tuple matching Picard BaseErrorAggregation"
)]
fn compute_covariate_key(
    stratifiers: &[StratifierKind],
    r: &ActiveRead,
    rg_names: &[String],
    offset: isize,
    qual_opt: Option<u8>,
    read_base: u8,
    ref_base: u8,
    cycle: i32,
    long_hp: i32,
) -> Option<CovariateKey> {
    if stratifiers.len() == 1 {
        return compute_single_covariate(
            stratifiers[0],
            r,
            rg_names,
            offset,
            qual_opt,
            read_base,
            ref_base,
            cycle,
            long_hp,
        );
    }
    let mut parts = Vec::with_capacity(stratifiers.len());
    for &strat in stratifiers {
        parts.push(compute_single_covariate(
            strat, r, rg_names, offset, qual_opt, read_base, ref_base, cycle, long_hp,
        )?);
    }
    Some(CovariateKey::Tuple(parts))
}

fn phred_q_score(errors: u64, total: u64, prior_error: f64) -> i32 {
    let p = ((errors as f64) + prior_error) / ((total as f64) + 1.0);
    let q = (-10.0 * p.log10()).round();
    #[expect(
        clippy::cast_possible_truncation,
        reason = "Phred Q score fits cleanly in i32"
    )]
    {
        q as i32
    }
}

fn format_metric_file(spec: &MetricSpec, store: AggregatorStore, prior_error: f64) -> String {
    let rows = store.into_sorted_rows();
    let mut out = String::new();
    match spec.kind {
        ErrorKind::Simple => {
            out.push_str("## METRICS CLASS\tpicard.sam.SamErrorMetric.BaseErrorMetric\n");
            out.push_str("ERROR_BASES\tQ_SCORE\tCOVARIATE\tTOTAL_BASES\n");
            for (key, c) in rows {
                let s = c.simple;
                if s.total_bases == 0 {
                    continue;
                }
                let q = phred_q_score(s.error_bases, s.total_bases, prior_error);
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}\t{}",
                    s.error_bases,
                    q,
                    key.format_picard(),
                    s.total_bases
                );
            }
        }
        ErrorKind::Indel => {
            out.push_str("## METRICS CLASS\tpicard.sam.SamErrorMetric.IndelErrorMetric\n");
            out.push_str("NUM_INSERTIONS\tNUM_INSERTED_BASES\tINSERTIONS_Q\tNUM_DELETIONS\tNUM_DELETED_BASES\tDELETIONS_Q\tERROR_BASES\tQ_SCORE\tCOVARIATE\tTOTAL_BASES\n");
            for (key, c) in rows {
                let ind = c.indel;
                let ins_q = phred_q_score(ind.num_insertions, ind.total_bases, prior_error);
                let del_q = phred_q_score(ind.num_deletions, ind.total_bases, prior_error);
                let err_bases = ind.num_inserted_bases + ind.num_deleted_bases;
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t0\t{}\t{}",
                    ind.num_insertions,
                    ind.num_inserted_bases,
                    ins_q,
                    ind.num_deletions,
                    ind.num_deleted_bases,
                    del_q,
                    err_bases,
                    key.format_picard(),
                    ind.total_bases
                );
            }
        }
        ErrorKind::Overlapping => {
            out.push_str("## METRICS CLASS\tpicard.sam.SamErrorMetric.OverlappingErrorMetric\n");
            out.push_str("NUM_BASES_WITH_OVERLAPPING_READS\tNUM_DISAGREES_WITH_REFERENCE_ONLY\tDISAGREES_WITH_REFERENCE_ONLY_Q\tNUM_DISAGREES_WITH_REF_AND_MATE\tDISAGREES_WITH_REF_AND_MATE_ONLY_Q\tNUM_THREE_WAYS_DISAGREEMENT\tTHREE_WAYS_DISAGREEMENT_ONLY_Q\tCOVARIATE\tTOTAL_BASES\n");
            for (key, c) in rows {
                let ov = c.overlapping;
                let q_ref_only =
                    phred_q_score(ov.disagrees_ref_only, ov.num_overlapping, prior_error);
                let q_ref_and_mate =
                    phred_q_score(ov.disagrees_ref_and_mate, ov.num_overlapping, prior_error);
                let q_three = phred_q_score(ov.three_ways, ov.num_overlapping, prior_error);
                let _ = writeln!(
                    out,
                    "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                    ov.num_overlapping,
                    ov.disagrees_ref_only,
                    q_ref_only,
                    ov.disagrees_ref_and_mate,
                    q_ref_and_mate,
                    ov.three_ways,
                    q_three,
                    key.format_picard(),
                    ov.total_bases
                );
            }
        }
    }
    out
}

fn parse_error_metrics_directives(args: &PicardArgs) -> Vec<String> {
    let mut result: Vec<String> = DEFAULT_ERROR_METRICS
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    let Some(values) = args.get("ERROR_METRICS") else {
        return result;
    };
    for raw in values {
        let stripped = raw.trim().trim_start_matches('[').trim_end_matches(']');
        for item in stripped.split(',') {
            let trimmed = item.trim();
            if trimmed.eq_ignore_ascii_case("null") {
                result.clear();
            } else if !trimmed.is_empty() {
                result.push(trimmed.to_string());
            }
        }
    }
    result
}

fn compile_metric_specs(directives: &[String]) -> Result<Vec<MetricSpec>, CommandError> {
    let mut seen_suffixes = BTreeSet::new();
    let mut specs = Vec::with_capacity(directives.len());
    for dir in directives {
        let mut parts = dir.split(':');
        let err_raw = parts
            .next()
            .ok_or_else(|| format!("malformed ERROR_METRICS directive: {dir}"))?;
        let kind = ErrorKind::parse(err_raw)?;
        let mut stratifiers = Vec::new();
        for s_raw in parts {
            stratifiers.push(StratifierKind::parse(s_raw)?);
        }
        if stratifiers.is_empty() {
            stratifiers.push(StratifierKind::All);
        }
        let strat_suffix = stratifiers
            .iter()
            .map(|s| s.suffix())
            .collect::<Vec<_>>()
            .join("_and_");
        let suffix = format!("{}_by_{}", kind.suffix(), strat_suffix);
        if !seen_suffixes.insert(suffix.clone()) {
            return Err(format!("duplicate ERROR_METRICS output suffix: {suffix}").into());
        }
        specs.push(MetricSpec {
            kind,
            stratifiers,
            suffix,
        });
    }
    Ok(specs)
}

fn header_is_coordinate_sorted(header_text: &str) -> bool {
    header_text
        .lines()
        .find(|line| line.starts_with("@HD\t"))
        .is_some_and(|line| line.split('\t').any(|f| f == "SO:coordinate"))
}

fn validate_fasta_matches_bam(
    faidx: &faidx::Reader,
    bam_contigs: &[(String, i64)],
) -> Result<(), CommandError> {
    let n_seqs = usize::try_from(faidx.n_seqs()).unwrap_or(0);
    if n_seqs != bam_contigs.len() {
        return Err(format!(
            "SAM dictionaries are not the same length ({} in BAM vs {n_seqs} in FASTA)",
            bam_contigs.len()
        )
        .into());
    }
    for (idx, (name, len)) in bam_contigs.iter().enumerate() {
        let seq_len = i64::try_from(faidx.fetch_seq_len(name)).unwrap_or(0);
        if seq_len != *len {
            return Err(format!(
                "SAM dictionaries differ for contig {name} (BAM len {len} vs FASTA len {seq_len}) at index {idx}"
            )
            .into());
        }
    }
    Ok(())
}

fn load_contig_upper(faidx: &faidx::Reader, name: &str, len: i64) -> Result<Vec<u8>, CommandError> {
    if len <= 0 {
        return Ok(Vec::new());
    }
    let end = usize::try_from(len - 1).unwrap_or(0);
    let mut seq = faidx.fetch_seq(name, 0, end).map_err(|e| e.to_string())?;
    seq.make_ascii_uppercase();
    Ok(seq)
}

fn load_intervals(
    path: &str,
    bam_contigs: &[(String, i64)],
) -> Result<BTreeMap<i32, Vec<(i64, i64)>>, CommandError> {
    let name_to_tid: FxHashMap<&str, i32> = bam_contigs
        .iter()
        .enumerate()
        .filter_map(|(i, (name, _))| i32::try_from(i).ok().map(|tid| (name.as_str(), tid)))
        .collect();
    let file = File::open(path).map_err(|e| e.to_string())?;
    let reader: Box<dyn BufRead> = if path.to_ascii_lowercase().ends_with(".gz") {
        Box::new(BufReader::new(GzDecoder::new(file)))
    } else {
        Box::new(BufReader::new(file))
    };

    let mut per_tid: BTreeMap<i32, Vec<(i64, i64)>> = BTreeMap::new();
    for line_res in reader.lines() {
        let line = line_res.map_err(|e| e.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('@') || trimmed.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 3 {
            return Err(format!("malformed interval line: {trimmed}").into());
        }
        let Some(&tid) = name_to_tid.get(cols[0]) else {
            continue;
        };
        let start1 = cols[1]
            .parse::<i64>()
            .map_err(|_| format!("invalid interval start: {}", cols[1]))?;
        let end1 = cols[2]
            .parse::<i64>()
            .map_err(|_| format!("invalid interval end: {}", cols[2]))?;
        // Convert 1-based inclusive to 0-based inclusive
        let s0 = (start1 - 1).max(0);
        let e0 = (end1 - 1).max(s0);
        per_tid.entry(tid).or_default().push((s0, e0));
    }

    for ivs in per_tid.values_mut() {
        ivs.sort_unstable();
        let mut merged: Vec<(i64, i64)> = Vec::with_capacity(ivs.len());
        for (s, e) in ivs.drain(..) {
            if let Some(last) = merged.last_mut()
                && s <= last.1 + 1
            {
                last.1 = last.1.max(e);
                continue;
            }
            merged.push((s, e));
        }
        *ivs = merged;
    }

    Ok(per_tid)
}

fn find_bam_index(input: &str) -> Option<PathBuf> {
    if !input.to_ascii_lowercase().ends_with(".bam") {
        return None;
    }
    [
        PathBuf::from(format!("{input}.bai")),
        Path::new(input).with_extension("bai"),
        PathBuf::from(format!("{input}.csi")),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn reject_unsupported_args(args: &PicardArgs) -> Result<(), CommandError> {
    for key in args.keys() {
        if !SUPPORTED_ARGS.contains(&key.as_str()) {
            return Err(unsupported(format!(
                "unsupported CollectSamErrorMetrics argument: {key}"
            )));
        }
    }
    optional_u32(args, "MIN_MAPPING_Q")?;
    optional_u32(args, "MIN_BASE_Q")?;
    optional_u32(args, "PRIOR_Q")?;
    optional_f64(args, "PROBABILITY")?;
    optional_i64(args, "MAX_LOCI")?;
    optional_u32(args, "LONG_HOMOPOLYMER")?;
    optional_u32(args, "LOCATION_BIN_SIZE")?;
    optional_scalar(args, "FILE_EXTENSION")?;
    optional_scalar(args, "VCF")?;
    optional_bool(args, "INTERVAL_ITERATOR")?;
    optional_u32(args, "PROGRESS_STEP_INTERVAL")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    let _ = args.get("TMP_DIR");
    reject_unsupported_compression_level(args, "CollectSamErrorMetrics")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phred_q_score_matches_picard_formula() {
        // (0 + 0.001) / (0 + 1) = 0.001 -> Q = 30
        assert_eq!(phred_q_score(0, 0, 1e-3), 30);
        // (0 + 0.001) / (64 + 1) -> Q = 48
        assert_eq!(phred_q_score(0, 64, 1e-3), 48);
        // (3 + 0.001) / (64 + 1) -> Q = 13
        assert_eq!(phred_q_score(3, 64, 1e-3), 13);
        // (2 + 0.001) / (77 + 1) -> Q = 16
        assert_eq!(phred_q_score(2, 77, 1e-3), 16);
    }

    #[test]
    fn default_error_metrics_compile_to_26_unique_suffixes() {
        let directives: Vec<String> = DEFAULT_ERROR_METRICS
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        let specs = compile_metric_specs(&directives).unwrap_or_default();
        assert_eq!(specs.len(), 26);
        assert_eq!(specs[0].suffix, "error_by_all");
        assert_eq!(specs[13].suffix, "error_by_read_ordinality_and_pre_dinuc");
        assert_eq!(specs[25].suffix, "indel_error_by_all");
    }

    #[test]
    fn covariate_key_orders_integers_and_long_short_like_java_treeset() {
        let mut keys = [
            CovariateKey::Int(10),
            CovariateKey::Int(2),
            CovariateKey::Int(1),
        ];
        keys.sort();
        assert_eq!(keys[0], CovariateKey::Int(1));
        assert_eq!(keys[1], CovariateKey::Int(2));
        assert_eq!(keys[2], CovariateKey::Int(10));

        assert!(CovariateKey::LongShortHp(false) < CovariateKey::LongShortHp(true));
    }
}
