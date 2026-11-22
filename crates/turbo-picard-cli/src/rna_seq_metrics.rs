use super::{
    CommandError, PicardArgs, bgzf_threads_for, format_float, has_sam_extension,
    open_bam_reader_for_args, optional_bool, optional_f64, optional_scalar, optional_u32,
    reject_unsupported_compression_level, required_scalar_for, unsupported,
    write_summary_chart_pdf,
};
use flate2::read::GzDecoder;
use rust_htslib::bam::record::{Aux, Cigar, CigarStringView};
use rust_htslib::bam::{self, FetchDefinition, Read};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use turbo_picard_core::bgzf_threads::HtsThreadRole;
use turbo_picard_core::picard_args::normalize_picard_args_for_command;

const HEADER_COLUMNS: &str = "PF_BASES\tPF_ALIGNED_BASES\tRIBOSOMAL_BASES\tCODING_BASES\tUTR_BASES\tINTRONIC_BASES\tINTERGENIC_BASES\tIGNORED_READS\tCORRECT_STRAND_READS\tINCORRECT_STRAND_READS\tNUM_R1_TRANSCRIPT_STRAND_READS\tNUM_R2_TRANSCRIPT_STRAND_READS\tNUM_UNEXPLAINED_READS\tPCT_R1_TRANSCRIPT_STRAND_READS\tPCT_R2_TRANSCRIPT_STRAND_READS\tPCT_RIBOSOMAL_BASES\tPCT_CODING_BASES\tPCT_UTR_BASES\tPCT_INTRONIC_BASES\tPCT_INTERGENIC_BASES\tPCT_MRNA_BASES\tPCT_USABLE_BASES\tPCT_CORRECT_STRAND_READS\tMEDIAN_CV_COVERAGE\tMEDIAN_5PRIME_BIAS\tMEDIAN_3PRIME_BIAS\tMEDIAN_5PRIME_TO_3PRIME_BIAS\tSAMPLE\tLIBRARY\tREAD_GROUP";

const SUPPORTED_ARGS: &[&str] = &[
    "INPUT",
    "OUTPUT",
    "REF_FLAT",
    "RIBOSOMAL_INTERVALS",
    "STRAND_SPECIFICITY",
    "MINIMUM_LENGTH",
    "RRNA_FRAGMENT_PERCENTAGE",
    "END_BIAS_BASES",
    "IGNORE_SEQUENCE",
    "METRIC_ACCUMULATION_LEVEL",
    "CHART_OUTPUT",
    "ASSUME_SORTED",
    "STOP_AFTER",
    "REFERENCE_SEQUENCE",
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StrandSpecificity {
    None,
    FirstReadTranscriptionStrand,
    SecondReadTranscriptionStrand,
}

impl StrandSpecificity {
    fn parse(raw: &str) -> Result<Self, CommandError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "NONE" => Ok(Self::None),
            "FIRST_READ_TRANSCRIPTION_STRAND" => Ok(Self::FirstReadTranscriptionStrand),
            "SECOND_READ_TRANSCRIPTION_STRAND" => Ok(Self::SecondReadTranscriptionStrand),
            _ => Err(unsupported(format!(
                "unsupported CollectRnaSeqMetrics STRAND_SPECIFICITY={raw}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum AccumulationLevel {
    AllReads,
    Sample,
    Library,
    ReadGroup,
}

impl AccumulationLevel {
    fn parse(raw: &str) -> Result<Self, CommandError> {
        match raw.trim().to_ascii_uppercase().as_str() {
            "ALL_READS" => Ok(Self::AllReads),
            "SAMPLE" => Ok(Self::Sample),
            "LIBRARY" => Ok(Self::Library),
            "READ_GROUP" => Ok(Self::ReadGroup),
            _ => Err(unsupported(format!(
                "unsupported CollectRnaSeqMetrics METRIC_ACCUMULATION_LEVEL={raw}"
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
enum LocusFn {
    #[default]
    Intergenic = 0,
    Intronic = 1,
    Utr = 2,
    Coding = 3,
}

#[derive(Clone, Copy, Debug)]
struct Exon {
    start: i64,     // 1-based inclusive
    end: i64,       // 1-based inclusive
    tx_offset: u32, // 0-based offset in concatenated transcript sequence
}

#[derive(Clone, Debug)]
struct Transcript {
    gene_id: u32,
    tx_start: i64,
    tx_end: i64,
    coding_start: i64,
    coding_end: i64,
    exons: Vec<Exon>,
    length: usize,
    is_negative: bool,
}

#[derive(Clone, Debug)]
struct GeneEntry {
    start: i64,
    end: i64,
    max_end: i64,
    is_negative: bool,
    tx_start_idx: usize,
    tx_end_idx: usize,
}

#[derive(Clone, Debug, Default)]
struct ContigGenes {
    genes: Vec<GeneEntry>,
}

impl ContigGenes {
    fn finalize(&mut self) {
        self.genes.sort_by_key(|gene| (gene.start, gene.end));
        let mut running_max = 0_i64;
        for gene in &mut self.genes {
            if gene.end > running_max {
                running_max = gene.end;
            }
            gene.max_end = running_max;
        }
    }

    fn overlapping(&self, start: i64, end: i64, out: &mut Vec<usize>) {
        out.clear();
        if self.genes.is_empty() {
            return;
        }
        let upper = self.genes.partition_point(|gene| gene.start <= end);
        let mut idx = upper;
        while idx > 0 {
            idx -= 1;
            let gene = &self.genes[idx];
            if gene.max_end < start {
                break;
            }
            if gene.end >= start {
                out.push(idx);
            }
        }
        out.reverse();
    }
}

#[derive(Clone, Debug, Default)]
struct GeneAnnotation {
    transcripts: Vec<Transcript>,
    by_contig: FxHashMap<String, ContigGenes>,
}

#[derive(Clone, Debug, Default)]
struct RibosomalIntervals {
    by_contig: FxHashMap<String, Vec<(i64, i64)>>,
}

impl RibosomalIntervals {
    fn max_intersection(&self, contig: &str, frag_start: i64, frag_end: i64) -> i64 {
        let Some(intervals) = self.by_contig.get(contig) else {
            return 0;
        };
        if intervals.is_empty() {
            return 0;
        }
        let upper = intervals.partition_point(|&(start, _)| start <= frag_end);
        let mut best = 0_i64;
        for &(start, end) in &intervals[..upper] {
            if end < frag_start {
                continue;
            }
            let intersection = frag_end.min(end) - frag_start.max(start) + 1;
            if intersection > best {
                best = intersection;
            }
        }
        best
    }
}

#[derive(Clone, Debug)]
struct RgInfo {
    sample: Option<String>,
    library: Option<String>,
    platform_unit: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct UnitAccumulator {
    sample: Option<String>,
    library: Option<String>,
    read_group: Option<String>,
    pf_bases: u64,
    pf_aligned_bases: u64,
    ribosomal_bases: Option<u64>,
    coding_bases: u64,
    utr_bases: u64,
    intronic_bases: u64,
    intergenic_bases: u64,
    ignored_reads: u64,
    correct_strand_reads: u64,
    incorrect_strand_reads: u64,
    num_r1_transcript_strand_reads: u64,
    num_r2_transcript_strand_reads: u64,
    num_unexplained_reads: u64,
    coverage_by_tx: FxHashMap<u32, Vec<u32>>,
}

impl UnitAccumulator {
    fn new(
        sample: Option<String>,
        library: Option<String>,
        read_group: Option<String>,
        has_ribo: bool,
    ) -> Self {
        Self {
            sample,
            library,
            read_group,
            ribosomal_bases: has_ribo.then_some(0),
            ..Self::default()
        }
    }

    fn merge(&mut self, other: Self) {
        self.pf_bases += other.pf_bases;
        self.pf_aligned_bases += other.pf_aligned_bases;
        if let (Some(dst), Some(src)) = (&mut self.ribosomal_bases, other.ribosomal_bases) {
            *dst += src;
        }
        self.coding_bases += other.coding_bases;
        self.utr_bases += other.utr_bases;
        self.intronic_bases += other.intronic_bases;
        self.intergenic_bases += other.intergenic_bases;
        self.ignored_reads += other.ignored_reads;
        self.correct_strand_reads += other.correct_strand_reads;
        self.incorrect_strand_reads += other.incorrect_strand_reads;
        self.num_r1_transcript_strand_reads += other.num_r1_transcript_strand_reads;
        self.num_r2_transcript_strand_reads += other.num_r2_transcript_strand_reads;
        self.num_unexplained_reads += other.num_unexplained_reads;
        for (tx_id, counts) in other.coverage_by_tx {
            self.coverage_by_tx
                .entry(tx_id)
                .and_modify(|existing| {
                    for (dst, src) in existing.iter_mut().zip(&counts) {
                        *dst = dst.saturating_add(*src);
                    }
                })
                .or_insert(counts);
        }
    }

    fn histogram_label(&self) -> String {
        if let Some(read_group) = &self.read_group
            && !read_group.is_empty()
        {
            return read_group.clone();
        }
        if let Some(library) = &self.library
            && !library.is_empty()
        {
            return library.clone();
        }
        if let Some(sample) = &self.sample
            && !sample.is_empty()
        {
            return sample.clone();
        }
        "All_Reads".to_string()
    }

    fn finish(
        &self,
        annotation: &GeneAnnotation,
        min_length: usize,
        end_bias_bases: usize,
    ) -> FinishedUnit {
        let length_cutoff = min_length.max(end_bias_bases);
        let mut best_by_gene: FxHashMap<u32, (u32, f64)> = FxHashMap::default();
        for (&tx_id, cov) in &self.coverage_by_tx {
            let tx = &annotation.transcripts[tx_id as usize];
            if tx.length < length_cutoff || tx.length == 0 {
                continue;
            }
            let sum: u64 = cov.iter().map(|&value| u64::from(value)).sum();
            let mean = (sum as f64) / (tx.length as f64);
            if mean < 1.0 {
                continue;
            }
            best_by_gene
                .entry(tx.gene_id)
                .and_modify(|existing| {
                    if mean > existing.1 {
                        *existing = (tx_id, mean);
                    }
                })
                .or_insert((tx_id, mean));
        }

        let mut cvs = Vec::with_capacity(best_by_gene.len());
        let mut five_biases = Vec::with_capacity(best_by_gene.len());
        let mut three_biases = Vec::with_capacity(best_by_gene.len());
        let mut five_to_three_biases = Vec::with_capacity(best_by_gene.len());
        let num_picked = best_by_gene.len();
        let mut histogram = if num_picked > 0 {
            Some([0.0_f64; 101])
        } else {
            None
        };

        for (tx_id, tx_mean) in best_by_gene.into_values() {
            let tx = &annotation.transcripts[tx_id as usize];
            let Some(raw_cov) = self.coverage_by_tx.get(&tx_id) else {
                continue;
            };
            let len = raw_cov.len();
            let cov_5p_to_3p = |idx: usize| -> f64 {
                if tx.is_negative {
                    f64::from(raw_cov[len - 1 - idx])
                } else {
                    f64::from(raw_cov[idx])
                }
            };

            let mut sq_diff_sum = 0.0_f64;
            for idx in 0..len {
                let diff = cov_5p_to_3p(idx) - tx_mean;
                sq_diff_sum += diff * diff;
            }
            let pop_sd = (sq_diff_sum / (len as f64)).sqrt();
            cvs.push(pop_sd / tx_mean);

            let mut sum_5p = 0.0_f64;
            let mut sum_3p = 0.0_f64;
            for idx in 0..end_bias_bases {
                sum_5p += cov_5p_to_3p(idx);
                sum_3p += cov_5p_to_3p(len - end_bias_bases + idx);
            }
            let end5 = sum_5p / (end_bias_bases as f64);
            let end3 = sum_3p / (end_bias_bases as f64);
            five_biases.push(end5 / tx_mean);
            three_biases.push(end3 / tx_mean);
            five_to_three_biases.push(if end3 == 0.0 { 0.0 } else { end5 / end3 });

            if let Some(hist) = &mut histogram {
                let last = (len - 1) as f64;
                for (bin, slot) in hist.iter_mut().enumerate() {
                    let pct = (bin as f64) / 100.0;
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "matches Picard integer bin bounds clamped to [0, last]"
                    )]
                    let start = (last * (pct - 0.005)).max(0.0) as usize;
                    #[expect(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "matches Picard integer bin bounds clamped to [0, last]"
                    )]
                    let end = (last * (pct + 0.005)).min(last) as usize;
                    let mut bin_sum = 0.0_f64;
                    let count = (end - start + 1) as f64;
                    for idx in start..=end {
                        bin_sum += cov_5p_to_3p(idx);
                    }
                    let bin_avg = bin_sum / count;
                    *slot += (bin_avg / tx_mean) / (num_picked as f64);
                }
            }
        }

        let median_cv = picard_median(&mut cvs);
        let median_5p = picard_median(&mut five_biases);
        let median_3p = picard_median(&mut three_biases);
        let median_5p_3p = picard_median(&mut five_to_three_biases);

        let aligned = self.pf_aligned_bases as f64;
        let pct_ribo = self.ribosomal_bases.and_then(|ribo| {
            if self.pf_aligned_bases > 0 {
                Some((ribo as f64) / aligned)
            } else {
                None
            }
        });
        let pct_coding = ratio_f64(self.coding_bases, self.pf_aligned_bases);
        let pct_utr = ratio_f64(self.utr_bases, self.pf_aligned_bases);
        let pct_intronic = ratio_f64(self.intronic_bases, self.pf_aligned_bases);
        let pct_intergenic = ratio_f64(self.intergenic_bases, self.pf_aligned_bases);
        let pct_mrna = pct_coding + pct_utr;
        let pct_usable = ratio_f64(self.coding_bases + self.utr_bases, self.pf_bases);
        let pct_correct_strand = ratio_f64(
            self.correct_strand_reads,
            self.correct_strand_reads + self.incorrect_strand_reads,
        );
        let r1_plus_r2 = self.num_r1_transcript_strand_reads + self.num_r2_transcript_strand_reads;
        let pct_r1 = ratio_f64(self.num_r1_transcript_strand_reads, r1_plus_r2);
        let pct_r2 = ratio_f64(self.num_r2_transcript_strand_reads, r1_plus_r2);

        FinishedUnit {
            label: self.histogram_label(),
            sample: self.sample.clone().unwrap_or_default(),
            library: self.library.clone().unwrap_or_default(),
            read_group: self.read_group.clone().unwrap_or_default(),
            pf_bases: self.pf_bases,
            pf_aligned_bases: self.pf_aligned_bases,
            ribosomal_bases: self.ribosomal_bases,
            coding_bases: self.coding_bases,
            utr_bases: self.utr_bases,
            intronic_bases: self.intronic_bases,
            intergenic_bases: self.intergenic_bases,
            ignored_reads: self.ignored_reads,
            correct_strand_reads: self.correct_strand_reads,
            incorrect_strand_reads: self.incorrect_strand_reads,
            num_r1: self.num_r1_transcript_strand_reads,
            num_r2: self.num_r2_transcript_strand_reads,
            num_unexplained: self.num_unexplained_reads,
            pct_r1,
            pct_r2,
            pct_ribo,
            pct_coding,
            pct_utr,
            pct_intronic,
            pct_intergenic,
            pct_mrna,
            pct_usable,
            pct_correct_strand,
            median_cv,
            median_5p,
            median_3p,
            median_5p_3p,
            histogram,
        }
    }
}

struct FinishedUnit {
    label: String,
    sample: String,
    library: String,
    read_group: String,
    pf_bases: u64,
    pf_aligned_bases: u64,
    ribosomal_bases: Option<u64>,
    coding_bases: u64,
    utr_bases: u64,
    intronic_bases: u64,
    intergenic_bases: u64,
    ignored_reads: u64,
    correct_strand_reads: u64,
    incorrect_strand_reads: u64,
    num_r1: u64,
    num_r2: u64,
    num_unexplained: u64,
    pct_r1: f64,
    pct_r2: f64,
    pct_ribo: Option<f64>,
    pct_coding: f64,
    pct_utr: f64,
    pct_intronic: f64,
    pct_intergenic: f64,
    pct_mrna: f64,
    pct_usable: f64,
    pct_correct_strand: f64,
    median_cv: f64,
    median_5p: f64,
    median_3p: f64,
    median_5p_3p: f64,
    histogram: Option<[f64; 101]>,
}

fn ratio_f64(num: u64, den: u64) -> f64 {
    if den == 0 {
        0.0
    } else {
        (num as f64) / (den as f64)
    }
}

fn picard_median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len() % 2 == 1 {
        values[mid]
    } else {
        0.5 * (values[mid - 1] + values[mid])
    }
}

#[derive(Clone, Debug)]
struct LevelDistributor {
    has_ribo: bool,
    // Preserve Picard LinkedHashMap insertion order cleanly with key -> index
    order: Vec<Option<String>>,
    indices: FxHashMap<Option<String>, usize>,
    units: Vec<UnitAccumulator>,
}

impl LevelDistributor {
    fn new(level: AccumulationLevel, header_rgs: &[RgInfo], has_ribo: bool) -> Self {
        let mut dist = Self {
            has_ribo,
            order: Vec::new(),
            indices: FxHashMap::default(),
            units: Vec::new(),
        };
        match level {
            AccumulationLevel::AllReads => {
                dist.insert_unit(None, UnitAccumulator::new(None, None, None, has_ribo));
            }
            AccumulationLevel::Sample => {
                for rg in header_rgs {
                    let key = rg.sample.clone();
                    if !dist.indices.contains_key(&key) {
                        dist.insert_unit(
                            key.clone(),
                            UnitAccumulator::new(key, None, None, has_ribo),
                        );
                    }
                }
            }
            AccumulationLevel::Library => {
                for rg in header_rgs {
                    let key = rg.library.clone();
                    if !dist.indices.contains_key(&key) {
                        dist.insert_unit(
                            key.clone(),
                            UnitAccumulator::new(rg.sample.clone(), key, None, has_ribo),
                        );
                    }
                }
            }
            AccumulationLevel::ReadGroup => {
                for rg in header_rgs {
                    let key = rg.platform_unit.clone();
                    if !dist.indices.contains_key(&key) {
                        dist.insert_unit(
                            key.clone(),
                            UnitAccumulator::new(
                                rg.sample.clone(),
                                rg.library.clone(),
                                key,
                                has_ribo,
                            ),
                        );
                    }
                }
            }
        }
        dist
    }

    fn fresh_worker_clone(&self) -> Self {
        let units = self
            .units
            .iter()
            .map(|unit| {
                UnitAccumulator::new(
                    unit.sample.clone(),
                    unit.library.clone(),
                    unit.read_group.clone(),
                    self.has_ribo,
                )
            })
            .collect();
        Self {
            has_ribo: self.has_ribo,
            order: self.order.clone(),
            indices: self.indices.clone(),
            units,
        }
    }

    fn insert_unit(&mut self, key: Option<String>, unit: UnitAccumulator) -> usize {
        let idx = self.units.len();
        self.order.push(key.clone());
        self.indices.insert(key, idx);
        self.units.push(unit);
        idx
    }

    fn unit_for_mut(
        &mut self,
        level: AccumulationLevel,
        rg: Option<&RgInfo>,
    ) -> &mut UnitAccumulator {
        if level == AccumulationLevel::AllReads {
            return &mut self.units[0];
        }
        let lookup_key = rg
            .and_then(|info| match level {
                AccumulationLevel::Sample => info.sample.as_deref(),
                AccumulationLevel::Library => info.library.as_deref(),
                AccumulationLevel::ReadGroup => info.platform_unit.as_deref(),
                AccumulationLevel::AllReads => None,
            })
            .unwrap_or("unknown");

        if let Some(&idx) = self.indices.get(&Some(lookup_key.to_string())) {
            return &mut self.units[idx];
        }
        // Only creates "unknown" (or unmapped key) dynamically when hit
        let owned_key = lookup_key.to_string();
        let unit = match level {
            AccumulationLevel::Sample => {
                UnitAccumulator::new(Some(owned_key.clone()), None, None, self.has_ribo)
            }
            AccumulationLevel::Library => UnitAccumulator::new(
                Some(owned_key.clone()),
                Some(owned_key.clone()),
                None,
                self.has_ribo,
            ),
            AccumulationLevel::ReadGroup => UnitAccumulator::new(
                Some(owned_key.clone()),
                Some(owned_key.clone()),
                Some(owned_key.clone()),
                self.has_ribo,
            ),
            AccumulationLevel::AllReads => unreachable!(),
        };
        let idx = self.insert_unit(Some(owned_key), unit);
        &mut self.units[idx]
    }

    fn merge(&mut self, other: Self) {
        for (key, unit) in other.order.into_iter().zip(other.units) {
            if let Some(&idx) = self.indices.get(&key) {
                self.units[idx].merge(unit);
            } else {
                self.insert_unit(key, unit);
            }
        }
    }
}

struct MultiUnitCollector {
    levels: Vec<(AccumulationLevel, LevelDistributor)>,
}

impl MultiUnitCollector {
    fn new(levels: &[AccumulationLevel], header_rgs: &[RgInfo], has_ribo: bool) -> Self {
        let mut sorted_unique = BTreeSet::new();
        for &level in levels {
            sorted_unique.insert(level);
        }
        let distributors = sorted_unique
            .into_iter()
            .map(|level| (level, LevelDistributor::new(level, header_rgs, has_ribo)))
            .collect();
        Self {
            levels: distributors,
        }
    }

    fn fresh_worker_clone(&self) -> Self {
        Self {
            levels: self
                .levels
                .iter()
                .map(|(level, dist)| (*level, dist.fresh_worker_clone()))
                .collect(),
        }
    }

    fn merge(&mut self, other: Self) {
        for ((_, dst), (_, src)) in self.levels.iter_mut().zip(other.levels) {
            dst.merge(src);
        }
    }
}

struct RnaSeqConfig {
    input: String,
    output: String,
    ref_flat: String,
    ribosomal_intervals: Option<String>,
    strand: StrandSpecificity,
    min_length: usize,
    rrna_fragment_pct: f64,
    end_bias_bases: usize,
    ignore_sequences: Vec<String>,
    levels: Vec<AccumulationLevel>,
    chart_output: Option<String>,
    stop_after: u32,
    reference: Option<String>,
}

struct ReadClassification {
    read_len: u64,
    is_pf_counted: bool, // !secondary (QCFAIL already filtered before)
    ignored_mapped: bool,
    aligned_bases: u64,
    ribosomal_only: bool,
    coding_bases: u64,
    utr_bases: u64,
    intronic_bases: u64,
    intergenic_bases: u64,
    strand_vote: Option<bool>, // Some(true) = correct, Some(false) = incorrect
    transcript_strand: Option<TranscriptStrandVote>,
}

#[derive(Clone, Copy)]
enum TranscriptStrandVote {
    R1,
    R2,
    Unexplained,
}

#[derive(Default)]
struct ScratchBuffers {
    gene_indices: Vec<usize>,
    locus_fns: Vec<LocusFn>,
    blocks: Vec<(i64, i64)>,
    tx_increments: Vec<(u32, i64, i64)>,
}

pub(crate) fn run_collectrnaseqmetrics(args: &[String]) -> Result<(), CommandError> {
    let args = normalize_picard_args_for_command("CollectRnaSeqMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_args(&args)?;

    let input = required_scalar_for(&args, "INPUT", "CollectRnaSeqMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectRnaSeqMetrics")?;
    let ref_flat_path = required_scalar_for(&args, "REF_FLAT", "CollectRnaSeqMetrics")?;
    let strand_raw = required_scalar_for(&args, "STRAND_SPECIFICITY", "CollectRnaSeqMetrics")?;
    let strand = StrandSpecificity::parse(&strand_raw)?;

    let ribosomal_intervals_path = optional_scalar(&args, "RIBOSOMAL_INTERVALS")?;
    let min_length = usize::try_from(optional_u32(&args, "MINIMUM_LENGTH")?.unwrap_or(500))
        .unwrap_or(usize::MAX);
    let rrna_fragment_pct = optional_f64(&args, "RRNA_FRAGMENT_PERCENTAGE")?.unwrap_or(0.8);
    let end_bias_bases = usize::try_from(optional_u32(&args, "END_BIAS_BASES")?.unwrap_or(100))
        .unwrap_or(usize::MAX);
    let ignore_sequences = args.get("IGNORE_SEQUENCE").cloned().unwrap_or_default();
    let levels = parse_accumulation_levels(&args)?;
    let chart_output = optional_scalar(&args, "CHART_OUTPUT")?;
    let stop_after = optional_u32(&args, "STOP_AFTER")?.unwrap_or(0);
    let reference = optional_scalar(&args, "REFERENCE_SEQUENCE")?;

    let config = RnaSeqConfig {
        input,
        output,
        ref_flat: ref_flat_path,
        ribosomal_intervals: ribosomal_intervals_path,
        strand,
        min_length,
        rrna_fragment_pct,
        end_bias_bases,
        ignore_sequences,
        levels,
        chart_output,
        stop_after,
        reference,
    };

    let annotation = load_ref_flat(&config.ref_flat)?;
    let ribo = match &config.ribosomal_intervals {
        Some(path) => Some(load_ribosomal_intervals(path)?),
        None => None,
    };

    if has_sam_extension(&config.input) {
        return Err(unsupported(
            "CollectRnaSeqMetrics text SAM input falls back to upstream Picard".to_string(),
        ));
    }

    let mut reader = open_bam_reader_for_args(&config.input, &args).map_err(|e| e.clone())?;
    let header_bytes = reader.header().as_bytes().to_vec();
    let header_text = String::from_utf8_lossy(&header_bytes);
    let (header_rgs, rg_id_map) = parse_read_groups_from_header(&header_text);
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let ignored_tids: FxHashSet<i32> = config
        .ignore_sequences
        .iter()
        .filter_map(|name| {
            target_names
                .iter()
                .position(|target| target == name)
                .and_then(|idx| i32::try_from(idx).ok())
        })
        .collect();

    let mut collector = MultiUnitCollector::new(&config.levels, &header_rgs, ribo.is_some());

    let workers = bgzf_threads_for(HtsThreadRole::RegionWorker).unwrap_or(1);
    if config.stop_after == 0
        && workers > 1
        && let Some(index_path) = find_bam_index(&config.input)
    {
        drop(reader);
        collect_parallel_indexed(
            &config,
            &index_path,
            &annotation,
            ribo.as_ref(),
            &target_names,
            &ignored_tids,
            &header_rgs,
            &rg_id_map,
            &mut collector,
            workers,
        )?;
    } else {
        let mut scratch = ScratchBuffers::default();
        let mut record = bam::Record::new();
        let mut seen = 0_u32;
        while let Some(result) = reader.read(&mut record) {
            result.map_err(|error| error.to_string())?;
            seen = seen.saturating_add(1);
            if config.stop_after > 0 && seen > config.stop_after {
                break;
            }
            process_record(
                &record,
                &annotation,
                ribo.as_ref(),
                &target_names,
                &ignored_tids,
                config.strand,
                config.rrna_fragment_pct,
                &header_rgs,
                &rg_id_map,
                &mut collector,
                &mut scratch,
            );
        }
    }

    let mut finished_units = Vec::new();
    for (_, dist) in &collector.levels {
        for unit in &dist.units {
            finished_units.push(unit.finish(&annotation, config.min_length, config.end_bias_bases));
        }
    }

    let text = format_metrics_output(&finished_units);
    fs::write(&config.output, text).map_err(|error| error.to_string())?;

    if let Some(chart_path) = &config.chart_output {
        write_summary_chart_pdf(chart_path, "CollectRnaSeqMetrics")?;
    }

    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "indexed contig-parallel worker shares immutable annotation and config references"
)]
fn collect_parallel_indexed(
    config: &RnaSeqConfig,
    index_path: &Path,
    annotation: &GeneAnnotation,
    ribo: Option<&RibosomalIntervals>,
    target_names: &[String],
    ignored_tids: &FxHashSet<i32>,
    header_rgs: &[RgInfo],
    rg_id_map: &FxHashMap<Vec<u8>, usize>,
    collector: &mut MultiUnitCollector,
    workers: usize,
) -> Result<(), CommandError> {
    let total_tasks = target_names.len() + 1;
    let next_task = AtomicUsize::new(0);
    let num_workers = workers.min(total_tasks);
    let worker_templates: Vec<MultiUnitCollector> = (0..num_workers)
        .map(|_| collector.fresh_worker_clone())
        .collect();

    let outcomes = thread::scope(|scope| {
        let mut handles = Vec::with_capacity(num_workers);
        for mut local_collector in worker_templates {
            let next_task = &next_task;
            handles.push(
                scope.spawn(move || -> Result<MultiUnitCollector, CommandError> {
                    let mut reader = bam::IndexedReader::from_path_and_index(
                        Path::new(&config.input),
                        index_path,
                    )
                    .map_err(|error| error.to_string())?;
                    if let Some(ref_path) = config.reference.as_deref() {
                        reader
                            .set_reference(Path::new(ref_path))
                            .map_err(|error| error.to_string())?;
                    }
                    let mut scratch = ScratchBuffers::default();
                    let mut record = bam::Record::new();

                    loop {
                        let task_id = next_task.fetch_add(1, Ordering::Relaxed);
                        if task_id >= total_tasks {
                            break;
                        }
                        let fetch_def = if task_id == target_names.len() {
                            FetchDefinition::Unmapped
                        } else {
                            let Ok(tid) = i32::try_from(task_id) else {
                                continue;
                            };
                            FetchDefinition::Region(tid, 0, i64::MAX)
                        };
                        reader.fetch(fetch_def).map_err(|error| error.to_string())?;
                        while let Some(result) = reader.read(&mut record) {
                            result.map_err(|error| error.to_string())?;
                            if task_id == target_names.len() && record.tid() >= 0 {
                                continue;
                            }
                            process_record(
                                &record,
                                annotation,
                                ribo,
                                target_names,
                                ignored_tids,
                                config.strand,
                                config.rrna_fragment_pct,
                                header_rgs,
                                rg_id_map,
                                &mut local_collector,
                                &mut scratch,
                            );
                        }
                    }
                    Ok(local_collector)
                }),
            );
        }
        handles
            .into_iter()
            .map(|handle| {
                handle
                    .join()
                    .unwrap_or_else(|_| Err("CollectRnaSeqMetrics worker panicked".into()))
            })
            .collect::<Vec<_>>()
    });

    for outcome in outcomes {
        collector.merge(outcome?);
    }
    Ok(())
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

#[expect(
    clippy::too_many_arguments,
    reason = "per-record classifier takes immutable annotation tables and mutable accumulators"
)]
fn process_record(
    record: &bam::Record,
    annotation: &GeneAnnotation,
    ribo: Option<&RibosomalIntervals>,
    target_names: &[String],
    ignored_tids: &FxHashSet<i32>,
    strand: StrandSpecificity,
    rrna_fragment_pct: f64,
    header_rgs: &[RgInfo],
    rg_id_map: &FxHashMap<Vec<u8>, usize>,
    collector: &mut MultiUnitCollector,
    scratch: &mut ScratchBuffers,
) {
    if record.is_quality_check_failed() {
        return;
    }

    let classified = classify_record(
        record,
        annotation,
        ribo,
        target_names,
        ignored_tids,
        strand,
        rrna_fragment_pct,
        scratch,
    );

    let rg_info = if collector
        .levels
        .iter()
        .any(|(level, _)| *level != AccumulationLevel::AllReads)
    {
        record
            .aux(b"RG")
            .ok()
            .and_then(|aux| match aux {
                Aux::String(value) => rg_id_map.get(value.as_bytes()).copied(),
                _ => None,
            })
            .map(|idx| &header_rgs[idx])
    } else {
        None
    };

    for (level, dist) in &mut collector.levels {
        let unit = dist.unit_for_mut(*level, rg_info);
        if classified.is_pf_counted {
            unit.pf_bases += classified.read_len;
        }
        if classified.ignored_mapped {
            unit.ignored_reads += 1;
            continue;
        }
        if classified.ribosomal_only {
            if let Some(ribo_bases) = &mut unit.ribosomal_bases {
                *ribo_bases += classified.aligned_bases;
            }
            unit.pf_aligned_bases += classified.aligned_bases;
            continue;
        }
        if classified.aligned_bases == 0 {
            continue;
        }
        unit.pf_aligned_bases += classified.aligned_bases;
        unit.coding_bases += classified.coding_bases;
        unit.utr_bases += classified.utr_bases;
        unit.intronic_bases += classified.intronic_bases;
        unit.intergenic_bases += classified.intergenic_bases;

        if let Some(correct) = classified.strand_vote {
            if correct {
                unit.correct_strand_reads += 1;
            } else {
                unit.incorrect_strand_reads += 1;
            }
        }
        if let Some(tx_strand) = classified.transcript_strand {
            match tx_strand {
                TranscriptStrandVote::R1 => unit.num_r1_transcript_strand_reads += 1,
                TranscriptStrandVote::R2 => unit.num_r2_transcript_strand_reads += 1,
                TranscriptStrandVote::Unexplained => unit.num_unexplained_reads += 1,
            }
        }

        for &(tx_id, b_start, b_end) in &scratch.tx_increments {
            if b_start >= b_end {
                continue;
            }
            let tx = &annotation.transcripts[tx_id as usize];
            let cov = unit
                .coverage_by_tx
                .entry(tx_id)
                .or_insert_with(|| vec![0_u32; tx.length]);
            add_tx_coverage(tx, cov, b_start, b_end);
        }
    }
}

fn add_tx_coverage(tx: &Transcript, cov: &mut [u32], b_start: i64, b_end: i64) {
    // Matches Picard Gene$Transcript.addCoverageCounts: loops strict `< b_end`.
    let limit = b_end - 1;
    if b_start > limit {
        return;
    }
    let first_exon = tx.exons.partition_point(|exon| exon.end < b_start);
    for exon in &tx.exons[first_exon..] {
        if exon.start > limit {
            break;
        }
        let ov_s = b_start.max(exon.start);
        let ov_e = limit.min(exon.end);
        if ov_s <= ov_e {
            let offset = exon.tx_offset as usize + usize::try_from(ov_s - exon.start).unwrap_or(0);
            let len = usize::try_from(ov_e - ov_s + 1).unwrap_or(0);
            if let Some(slice) = cov.get_mut(offset..offset + len) {
                for count in slice {
                    *count = count.saturating_add(1);
                }
            }
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "classification reads alignment blocks and gene indices without extra wrappers"
)]
fn classify_record(
    record: &bam::Record,
    annotation: &GeneAnnotation,
    ribo: Option<&RibosomalIntervals>,
    target_names: &[String],
    ignored_tids: &FxHashSet<i32>,
    strand: StrandSpecificity,
    rrna_fragment_pct: f64,
    scratch: &mut ScratchBuffers,
) -> ReadClassification {
    scratch.tx_increments.clear();
    let is_pf_counted = !record.is_secondary();
    let read_len = u64::try_from(record.seq_len()).unwrap_or(0);

    let tid = record.tid();
    let unmapped = record.is_unmapped() || tid < 0;
    if !unmapped && is_pf_counted && ignored_tids.contains(&tid) {
        return ReadClassification {
            read_len,
            is_pf_counted,
            ignored_mapped: true,
            aligned_bases: 0,
            ribosomal_only: false,
            coding_bases: 0,
            utr_bases: 0,
            intronic_bases: 0,
            intergenic_bases: 0,
            strand_vote: None,
            transcript_strand: None,
        };
    }

    if record.is_secondary() || unmapped {
        return ReadClassification {
            read_len,
            is_pf_counted,
            ignored_mapped: false,
            aligned_bases: 0,
            ribosomal_only: false,
            coding_bases: 0,
            utr_bases: 0,
            intronic_bases: 0,
            intergenic_bases: 0,
            strand_vote: None,
            transcript_strand: None,
        };
    }

    let Some(contig) = usize::try_from(tid)
        .ok()
        .and_then(|idx| target_names.get(idx))
    else {
        return ReadClassification {
            read_len,
            is_pf_counted,
            ignored_mapped: false,
            aligned_bases: 0,
            ribosomal_only: false,
            coding_bases: 0,
            utr_bases: 0,
            intronic_bases: 0,
            intergenic_bases: 0,
            strand_vote: None,
            transcript_strand: None,
        };
    };

    let cigar = record.cigar();
    extract_alignment_blocks(record.pos() + 1, &cigar, &mut scratch.blocks);
    let aligned_bases: u64 = scratch
        .blocks
        .iter()
        .map(|&(start, end)| u64::try_from(end - start + 1).unwrap_or(0))
        .sum();
    if aligned_bases == 0 || scratch.blocks.is_empty() {
        return ReadClassification {
            read_len,
            is_pf_counted,
            ignored_mapped: false,
            aligned_bases: 0,
            ribosomal_only: false,
            coding_bases: 0,
            utr_bases: 0,
            intronic_bases: 0,
            intergenic_bases: 0,
            strand_vote: None,
            transcript_strand: None,
        };
    }

    let aln_start = scratch.blocks[0].0;
    let aln_end = scratch.blocks[scratch.blocks.len() - 1].1;

    if let Some(ribo_intervals) = ribo {
        let frag = if !record.is_paired() {
            Some((aln_start, aln_end))
        } else if !record.is_mate_unmapped() && record.mtid() == tid {
            let mate_start = record.mpos() + 1;
            let frag_start = aln_start.min(mate_start);
            let abs_tlen = record.insert_size().unsigned_abs().cast_signed();
            if abs_tlen > 0 {
                Some((frag_start, frag_start + abs_tlen - 1))
            } else {
                None
            }
        } else {
            None
        };

        if let Some((frag_start, frag_end)) = frag {
            let frag_len = frag_end - frag_start + 1;
            if frag_len > 0 {
                let max_ov = ribo_intervals.max_intersection(contig, frag_start, frag_end);
                if (max_ov as f64) / (frag_len as f64) >= rrna_fragment_pct {
                    return ReadClassification {
                        read_len,
                        is_pf_counted,
                        ignored_mapped: false,
                        aligned_bases,
                        ribosomal_only: true,
                        coding_bases: 0,
                        utr_bases: 0,
                        intronic_bases: 0,
                        intergenic_bases: 0,
                        strand_vote: None,
                        transcript_strand: None,
                    };
                }
            }
        }
    }

    let contig_genes = annotation.by_contig.get(contig);
    if let Some(cg) = contig_genes {
        cg.overlapping(aln_start, aln_end, &mut scratch.gene_indices);
    } else {
        scratch.gene_indices.clear();
    }

    if scratch.gene_indices.is_empty() {
        return ReadClassification {
            read_len,
            is_pf_counted,
            ignored_mapped: false,
            aligned_bases,
            ribosomal_only: false,
            coding_bases: 0,
            utr_bases: 0,
            intronic_bases: 0,
            intergenic_bases: aligned_bases,
            strand_vote: None,
            transcript_strand: None,
        };
    }

    let Some(cg) = contig_genes else {
        unreachable!();
    };

    let mut coding_bases = 0_u64;
    let mut utr_bases = 0_u64;
    let mut intronic_bases = 0_u64;
    let mut intergenic_bases = 0_u64;

    for &(b_start, b_end) in &scratch.blocks {
        let b_len = usize::try_from(b_end - b_start + 1).unwrap_or(0);
        scratch.locus_fns.clear();
        scratch.locus_fns.resize(b_len, LocusFn::Intergenic);

        for &gene_idx in &scratch.gene_indices {
            let gene = &cg.genes[gene_idx];
            if gene.start > b_end || gene.end < b_start {
                continue;
            }
            for tx_idx in gene.tx_start_idx..gene.tx_end_idx {
                let tx = &annotation.transcripts[tx_idx];
                if tx.tx_start <= b_end && tx.tx_end >= b_start {
                    assign_locus_function_for_transcript(
                        b_start,
                        b_end,
                        tx,
                        &mut scratch.locus_fns,
                    );
                    if b_start < b_end
                        && let Ok(tx_id) = u32::try_from(tx_idx)
                    {
                        scratch.tx_increments.push((tx_id, b_start, b_end));
                    }
                }
            }
        }

        for &fn_val in &scratch.locus_fns {
            match fn_val {
                LocusFn::Intergenic => intergenic_bases += 1,
                LocusFn::Intronic => intronic_bases += 1,
                LocusFn::Utr => utr_bases += 1,
                LocusFn::Coding => coding_bases += 1,
            }
        }
    }

    let overlaps_exon = (coding_bases + utr_bases) > 0;
    let mut strand_vote = None;
    let mut transcript_strand = None;

    if !record.is_supplementary() && overlaps_exon && scratch.gene_indices.len() == 1 {
        let gene = &cg.genes[scratch.gene_indices[0]];
        let is_r1 = !record.is_paired() || record.is_first_in_template();
        let read_neg = record.is_reverse();
        let gene_neg = gene.is_negative;

        if strand != StrandSpecificity::None {
            let same_strand = read_neg == gene_neg;
            let expect_same = is_r1 == (strand == StrandSpecificity::FirstReadTranscriptionStrand);
            strand_vote = Some(same_strand == expect_same);
        }

        if is_r1 {
            let vote = if record.is_paired() {
                if record.is_mate_unmapped() {
                    TranscriptStrandVote::Unexplained
                } else {
                    let mate_tid = record.mtid();
                    if mate_tid == tid {
                        let mate_start = record.mpos() + 1;
                        let mate_neg = record.is_mate_reverse();
                        let mate_end = compute_mate_end(record, mate_start, read_len);
                        let is_fr = is_fr_pair(
                            read_neg, aln_start, aln_end, mate_neg, mate_start, mate_end,
                        );
                        let frag_start = aln_start.min(mate_start);
                        let frag_end = aln_end.max(mate_end);
                        if is_fr && gene.start <= frag_start && gene.end >= frag_end {
                            if read_neg == gene_neg {
                                TranscriptStrandVote::R1
                            } else {
                                TranscriptStrandVote::R2
                            }
                        } else {
                            TranscriptStrandVote::Unexplained
                        }
                    } else {
                        TranscriptStrandVote::Unexplained
                    }
                }
            } else if gene.start <= aln_start && gene.end >= aln_end {
                if read_neg == gene_neg {
                    TranscriptStrandVote::R1
                } else {
                    TranscriptStrandVote::R2
                }
            } else {
                TranscriptStrandVote::Unexplained
            };
            transcript_strand = Some(vote);
        }
    }

    ReadClassification {
        read_len,
        is_pf_counted,
        ignored_mapped: false,
        aligned_bases,
        ribosomal_only: false,
        coding_bases,
        utr_bases,
        intronic_bases,
        intergenic_bases,
        strand_vote,
        transcript_strand,
    }
}

fn assign_locus_function_for_transcript(
    b_start: i64,
    b_end: i64,
    tx: &Transcript,
    locus_fns: &mut [LocusFn],
) {
    let ov_start = b_start.max(tx.tx_start);
    let ov_end = b_end.min(tx.tx_end);
    if ov_start > ov_end {
        return;
    }

    let intronic_s = usize::try_from(ov_start - b_start).unwrap_or(0);
    let intronic_e = usize::try_from(ov_end - b_start).unwrap_or(0);
    for slot in &mut locus_fns[intronic_s..=intronic_e] {
        if *slot < LocusFn::Intronic {
            *slot = LocusFn::Intronic;
        }
    }

    let first_exon = tx.exons.partition_point(|exon| exon.end < ov_start);
    for exon in &tx.exons[first_exon..] {
        if exon.start > ov_end {
            break;
        }
        let ex_s = ov_start.max(exon.start);
        let ex_e = ov_end.min(exon.end);
        if ex_s > ex_e {
            continue;
        }

        // Non-coding transcripts have coding_start > coding_end, or all positions outside [coding_start, coding_end].
        let code_s = ex_s.max(tx.coding_start);
        let code_e = ex_e.min(tx.coding_end);

        if code_s <= code_e {
            // Left UTR portion: ex_s .. code_s - 1
            if ex_s < code_s {
                mark_locus_range(b_start, ex_s, code_s - 1, LocusFn::Utr, locus_fns);
            }
            // Coding portion: code_s .. code_e
            mark_locus_range(b_start, code_s, code_e, LocusFn::Coding, locus_fns);
            // Right UTR portion: code_e + 1 .. ex_e
            if code_e < ex_e {
                mark_locus_range(b_start, code_e + 1, ex_e, LocusFn::Utr, locus_fns);
            }
        } else {
            mark_locus_range(b_start, ex_s, ex_e, LocusFn::Utr, locus_fns);
        }
    }
}

fn mark_locus_range(b_start: i64, start: i64, end: i64, value: LocusFn, locus_fns: &mut [LocusFn]) {
    let idx_s = usize::try_from(start - b_start).unwrap_or(0);
    let idx_e = usize::try_from(end - b_start).unwrap_or(0);
    for slot in &mut locus_fns[idx_s..=idx_e] {
        if *slot < value {
            *slot = value;
        }
    }
}

fn extract_alignment_blocks(pos_1based: i64, cigar: &CigarStringView, out: &mut Vec<(i64, i64)>) {
    out.clear();
    let mut ref_pos = pos_1based;
    for op in cigar {
        match op {
            Cigar::Match(len) | Cigar::Equal(len) | Cigar::Diff(len) => {
                let len = i64::from(*len);
                if len > 0 {
                    out.push((ref_pos, ref_pos + len - 1));
                    ref_pos += len;
                }
            }
            Cigar::Del(len) | Cigar::RefSkip(len) => {
                ref_pos += i64::from(*len);
            }
            Cigar::Ins(_) | Cigar::SoftClip(_) | Cigar::HardClip(_) | Cigar::Pad(_) => {}
        }
    }
}

fn compute_mate_end(record: &bam::Record, mate_start: i64, fallback_read_len: u64) -> i64 {
    if let Ok(Aux::String(mc)) = record.aux(b"MC") {
        let mut ref_len = 0_i64;
        let mut num = 0_i64;
        for byte in mc.bytes() {
            if byte.is_ascii_digit() {
                num = num * 10 + i64::from(byte - b'0');
            } else {
                if matches!(byte, b'M' | b'=' | b'X' | b'D' | b'N') {
                    ref_len += num;
                }
                num = 0;
            }
        }
        if ref_len > 0 {
            return mate_start + ref_len - 1;
        }
    }
    mate_start + fallback_read_len.cast_signed().saturating_sub(1)
}

fn is_fr_pair(
    read_neg: bool,
    read_start: i64,
    read_end: i64,
    mate_neg: bool,
    mate_start: i64,
    mate_end: i64,
) -> bool {
    if read_neg == mate_neg {
        return false;
    }
    if read_neg {
        mate_start <= read_end
    } else {
        read_start <= mate_end
    }
}

fn open_maybe_gz(path: &str) -> Result<Box<dyn BufRead>, CommandError> {
    let file = File::open(path).map_err(|error| error.to_string())?;
    if path.to_ascii_lowercase().ends_with(".gz") {
        Ok(Box::new(BufReader::with_capacity(
            128 * 1024,
            GzDecoder::new(file),
        )))
    } else {
        Ok(Box::new(BufReader::with_capacity(128 * 1024, file)))
    }
}

fn load_ref_flat(path: &str) -> Result<GeneAnnotation, CommandError> {
    struct GeneGroup {
        start: i64,
        end: i64,
        tx_indices: Vec<usize>,
    }

    let reader = open_maybe_gz(path)?;
    // Picard groups rows by (gene_name, chrom, strand) in deterministic order.
    let mut gene_groups: BTreeMap<(String, String, bool), GeneGroup> = BTreeMap::new();
    let mut raw_transcripts: Vec<Transcript> = Vec::new();

    for (line_idx, line_res) in reader.lines().enumerate() {
        let line = line_res.map_err(|error| error.to_string())?;
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < 11 {
            return Err(format!(
                "CollectRnaSeqMetrics refFlat line {} has {} columns (expected 11)",
                line_idx + 1,
                cols.len()
            )
            .into());
        }
        let gene_name = cols[0].to_string();
        let chrom = cols[2].to_string();
        let is_negative = match cols[3] {
            "-" => true,
            "+" => false,
            other => {
                return Err(format!(
                    "CollectRnaSeqMetrics invalid refFlat strand '{other}' on line {}",
                    line_idx + 1
                )
                .into());
            }
        };

        let tx_start = cols[4]
            .parse::<i64>()
            .map_err(|_| format!("invalid txStart on line {}", line_idx + 1))?
            + 1;
        let tx_end = cols[5]
            .parse::<i64>()
            .map_err(|_| format!("invalid txEnd on line {}", line_idx + 1))?;
        let coding_start = cols[6]
            .parse::<i64>()
            .map_err(|_| format!("invalid cdsStart on line {}", line_idx + 1))?
            + 1;
        let coding_end = cols[7]
            .parse::<i64>()
            .map_err(|_| format!("invalid cdsEnd on line {}", line_idx + 1))?;
        let exon_count = cols[8]
            .parse::<usize>()
            .map_err(|_| format!("invalid exonCount on line {}", line_idx + 1))?;

        let exon_starts: Vec<i64> = cols[9]
            .split(',')
            .filter(|part| !part.is_empty())
            .map(|part| part.parse::<i64>().map(|v| v + 1))
            .collect::<Result<_, _>>()
            .map_err(|_| format!("invalid exonStarts on line {}", line_idx + 1))?;
        let exon_ends: Vec<i64> = cols[10]
            .split(',')
            .filter(|part| !part.is_empty())
            .map(str::parse::<i64>)
            .collect::<Result<_, _>>()
            .map_err(|_| format!("invalid exonEnds on line {}", line_idx + 1))?;

        if exon_starts.len() != exon_ends.len() || exon_starts.len() != exon_count {
            return Err(format!(
                "CollectRnaSeqMetrics exon count mismatch on line {}",
                line_idx + 1
            )
            .into());
        }

        let mut exons = Vec::with_capacity(exon_count);
        let mut cumulative_len = 0_usize;
        for (start, end) in exon_starts.into_iter().zip(exon_ends) {
            let ex_len = usize::try_from(end - start + 1).unwrap_or(0);
            exons.push(Exon {
                start,
                end,
                tx_offset: u32::try_from(cumulative_len).unwrap_or(u32::MAX),
            });
            cumulative_len += ex_len;
        }

        let tx_idx = raw_transcripts.len();
        raw_transcripts.push(Transcript {
            gene_id: 0,
            tx_start,
            tx_end,
            coding_start,
            coding_end,
            exons,
            length: cumulative_len,
            is_negative,
        });

        gene_groups
            .entry((gene_name, chrom, is_negative))
            .and_modify(|entry| {
                entry.start = entry.start.min(tx_start);
                entry.end = entry.end.max(tx_end);
                entry.tx_indices.push(tx_idx);
            })
            .or_insert_with(|| GeneGroup {
                start: tx_start,
                end: tx_end,
                tx_indices: vec![tx_idx],
            });
    }

    let mut annotation = GeneAnnotation::default();
    for (gene_idx, ((_, chrom, is_negative), group)) in gene_groups.into_iter().enumerate() {
        let gene_id = u32::try_from(gene_idx).unwrap_or(u32::MAX);
        let tx_start_idx = annotation.transcripts.len();
        for raw_idx in group.tx_indices {
            let mut tx = raw_transcripts[raw_idx].clone();
            tx.gene_id = gene_id;
            annotation.transcripts.push(tx);
        }
        let tx_end_idx = annotation.transcripts.len();
        annotation
            .by_contig
            .entry(chrom)
            .or_default()
            .genes
            .push(GeneEntry {
                start: group.start,
                end: group.end,
                max_end: group.end,
                is_negative,
                tx_start_idx,
                tx_end_idx,
            });
    }

    for cg in annotation.by_contig.values_mut() {
        cg.finalize();
    }

    Ok(annotation)
}

fn load_ribosomal_intervals(path: &str) -> Result<RibosomalIntervals, CommandError> {
    let reader = open_maybe_gz(path)?;
    let mut intervals = RibosomalIntervals::default();
    for (line_idx, line_res) in reader.lines().enumerate() {
        let line = line_res.map_err(|error| error.to_string())?;
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('@') || trimmed.starts_with('#') {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 3 {
            return Err(format!(
                "CollectRnaSeqMetrics invalid ribosomal interval line {}",
                line_idx + 1
            )
            .into());
        }
        let contig = cols[0].to_string();
        let start = cols[1]
            .parse::<i64>()
            .map_err(|_| format!("invalid interval start on line {}", line_idx + 1))?;
        let end = cols[2]
            .parse::<i64>()
            .map_err(|_| format!("invalid interval end on line {}", line_idx + 1))?;
        intervals
            .by_contig
            .entry(contig)
            .or_default()
            .push((start, end));
    }
    for list in intervals.by_contig.values_mut() {
        list.sort_unstable();
    }
    Ok(intervals)
}

fn parse_read_groups_from_header(header_text: &str) -> (Vec<RgInfo>, FxHashMap<Vec<u8>, usize>) {
    let mut infos = Vec::new();
    let mut id_map = FxHashMap::default();
    for line in header_text.lines() {
        if !line.starts_with("@RG\t") {
            continue;
        }
        let mut id = None;
        let mut sample = None;
        let mut library = None;
        let mut platform_unit = None;
        for field in line.split('\t').skip(1) {
            if let Some(value) = field.strip_prefix("ID:") {
                id = Some(value.as_bytes().to_vec());
            } else if let Some(value) = field.strip_prefix("SM:") {
                sample = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("LB:") {
                library = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("PU:") {
                platform_unit = Some(value.to_string());
            }
        }
        if let Some(id_bytes) = id {
            let idx = infos.len();
            infos.push(RgInfo {
                sample,
                library,
                platform_unit,
            });
            id_map.insert(id_bytes, idx);
        }
    }
    (infos, id_map)
}

fn parse_accumulation_levels(args: &PicardArgs) -> Result<Vec<AccumulationLevel>, CommandError> {
    let Some(values) = args.get("METRIC_ACCUMULATION_LEVEL") else {
        return Ok(vec![AccumulationLevel::AllReads]);
    };
    let mut levels = Vec::new();
    for value in values {
        let stripped = value.trim().trim_start_matches('[').trim_end_matches(']');
        for item in stripped.split(',') {
            let trimmed = item.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("null") {
                continue;
            }
            levels.push(AccumulationLevel::parse(trimmed)?);
        }
    }
    if levels.is_empty() {
        levels.push(AccumulationLevel::AllReads);
    }
    Ok(levels)
}

fn reject_unsupported_args(args: &PicardArgs) -> Result<(), CommandError> {
    for key in args.keys() {
        if !SUPPORTED_ARGS.contains(&key.as_str()) {
            return Err(unsupported(format!(
                "unsupported CollectRnaSeqMetrics argument: {key}"
            )));
        }
    }
    optional_scalar(args, "RIBOSOMAL_INTERVALS")?;
    optional_u32(args, "MINIMUM_LENGTH")?;
    optional_f64(args, "RRNA_FRAGMENT_PERCENTAGE")?;
    optional_u32(args, "END_BIAS_BASES")?;
    optional_scalar(args, "CHART_OUTPUT")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_u32(args, "STOP_AFTER")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    let _ = args.get("TMP_DIR");
    reject_unsupported_compression_level(args, "CollectRnaSeqMetrics")?;
    Ok(())
}

fn format_metrics_output(units: &[FinishedUnit]) -> String {
    let mut out = String::from("## METRICS CLASS\tpicard.analysis.RnaSeqMetrics\n");
    out.push_str(HEADER_COLUMNS);
    out.push('\n');

    for unit in units {
        let ribo_str = unit
            .ribosomal_bases
            .map_or_else(String::new, |value| value.to_string());
        let pct_ribo_str = unit.pct_ribo.map_or_else(String::new, format_float);
        let _ = writeln!(
            out,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            unit.pf_bases,
            unit.pf_aligned_bases,
            ribo_str,
            unit.coding_bases,
            unit.utr_bases,
            unit.intronic_bases,
            unit.intergenic_bases,
            unit.ignored_reads,
            unit.correct_strand_reads,
            unit.incorrect_strand_reads,
            unit.num_r1,
            unit.num_r2,
            unit.num_unexplained,
            format_float(unit.pct_r1),
            format_float(unit.pct_r2),
            pct_ribo_str,
            format_float(unit.pct_coding),
            format_float(unit.pct_utr),
            format_float(unit.pct_intronic),
            format_float(unit.pct_intergenic),
            format_float(unit.pct_mrna),
            format_float(unit.pct_usable),
            format_float(unit.pct_correct_strand),
            format_float(unit.median_cv),
            format_float(unit.median_5p),
            format_float(unit.median_3p),
            format_float(unit.median_5p_3p),
            unit.sample,
            unit.library,
            unit.read_group,
        );
    }

    let hist_units: Vec<(&str, &[f64; 101])> = units
        .iter()
        .filter_map(|unit| {
            unit.histogram
                .as_ref()
                .map(|hist| (unit.label.as_str(), hist))
        })
        .collect();

    if !hist_units.is_empty() {
        out.push_str("\n## HISTOGRAM\tjava.lang.Integer\n");
        out.push_str("normalized_position");
        for (label, _) in &hist_units {
            let _ = write!(out, "\t{label}.normalized_coverage");
        }
        out.push('\n');
        for bin in 0..=100 {
            let _ = write!(out, "{bin}");
            for (_, hist) in &hist_units {
                let _ = write!(out, "\t{}", format_float(hist[bin]));
            }
            out.push('\n');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strand_specificity_parses_all_picard_variants() {
        assert_eq!(
            StrandSpecificity::parse("NONE").ok(),
            Some(StrandSpecificity::None)
        );
        assert_eq!(
            StrandSpecificity::parse("first_read_transcription_strand").ok(),
            Some(StrandSpecificity::FirstReadTranscriptionStrand)
        );
        assert_eq!(
            StrandSpecificity::parse("SECOND_READ_TRANSCRIPTION_STRAND").ok(),
            Some(StrandSpecificity::SecondReadTranscriptionStrand)
        );
        assert!(StrandSpecificity::parse("UNKNOWN").is_err());
    }

    #[test]
    fn picard_median_matches_odd_and_even_lists() {
        let mut odd = [3.0, 1.0, 9.0];
        assert!((picard_median(&mut odd) - 3.0).abs() < 1e-12);
        let mut even = [4.0, 1.0, 2.0, 8.0];
        assert!((picard_median(&mut even) - 3.0).abs() < 1e-12);
        let mut empty: [f64; 0] = [];
        assert!(picard_median(&mut empty) == 0.0);
    }

    #[test]
    fn ribosomal_intervals_find_max_single_overlap() {
        let mut ribo = RibosomalIntervals::default();
        ribo.by_contig
            .insert("chr1".to_string(), vec![(100, 150), (140, 220)]);
        assert_eq!(ribo.max_intersection("chr1", 120, 200), 61);
        assert_eq!(ribo.max_intersection("chr2", 120, 200), 0);
    }

    #[test]
    fn fr_pair_orientation_check_matches_sam_pair_util() {
        assert!(is_fr_pair(false, 100, 150, true, 120, 170));
        assert!(is_fr_pair(true, 120, 170, false, 100, 150));
        assert!(!is_fr_pair(false, 200, 250, true, 100, 150));
        assert!(!is_fr_pair(false, 100, 150, false, 200, 250));
    }
}
