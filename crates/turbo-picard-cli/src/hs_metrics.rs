//! Native, bounded-scope implementation of Picard `CollectHsMetrics`.
//!
//! The collector intentionally keeps the scope explicit.  It implements the
//! core ALL_READS metrics, histogram, and optional per-target/per-base sidecar
//! reports that capture/QC workflows consume. Unsupported accumulation levels
//! and advanced options remain explicitly outside this native scope.

use rust_htslib::bam::record::Cigar;
use rust_htslib::bam::{self, Read};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::BTreeMap;
use std::fs;

const HS_METRICS_HEADER: &str = "BAIT_SET\tBAIT_TERRITORY\tBAIT_DESIGN_EFFICIENCY\tON_BAIT_BASES\tNEAR_BAIT_BASES\tOFF_BAIT_BASES\tPCT_SELECTED_BASES\tPCT_OFF_BAIT\tON_BAIT_VS_SELECTED\tMEAN_BAIT_COVERAGE\tPCT_USABLE_BASES_ON_BAIT\tPCT_USABLE_BASES_ON_TARGET\tFOLD_ENRICHMENT\tHS_LIBRARY_SIZE\tHS_PENALTY_10X\tHS_PENALTY_20X\tHS_PENALTY_30X\tHS_PENALTY_40X\tHS_PENALTY_50X\tHS_PENALTY_100X\tTARGET_TERRITORY\tGENOME_SIZE\tTOTAL_READS\tPF_READS\tPF_BASES\tPF_UNIQUE_READS\tPF_UQ_READS_ALIGNED\tPF_BASES_ALIGNED\tPF_UQ_BASES_ALIGNED\tON_TARGET_BASES\tPCT_PF_READS\tPCT_PF_UQ_READS\tPCT_PF_UQ_READS_ALIGNED\tMEAN_TARGET_COVERAGE\tMEDIAN_TARGET_COVERAGE\tMAX_TARGET_COVERAGE\tMIN_TARGET_COVERAGE\tZERO_CVG_TARGETS_PCT\tPCT_EXC_DUPE\tPCT_EXC_ADAPTER\tPCT_EXC_MAPQ\tPCT_EXC_BASEQ\tPCT_EXC_OVERLAP\tPCT_EXC_OFF_TARGET\tFOLD_80_BASE_PENALTY\tPCT_TARGET_BASES_1X\tPCT_TARGET_BASES_2X\tPCT_TARGET_BASES_10X\tPCT_TARGET_BASES_20X\tPCT_TARGET_BASES_30X\tPCT_TARGET_BASES_40X\tPCT_TARGET_BASES_50X\tPCT_TARGET_BASES_100X\tPCT_TARGET_BASES_250X\tPCT_TARGET_BASES_500X\tPCT_TARGET_BASES_1000X\tPCT_TARGET_BASES_2500X\tPCT_TARGET_BASES_5000X\tPCT_TARGET_BASES_10000X\tPCT_TARGET_BASES_25000X\tPCT_TARGET_BASES_50000X\tPCT_TARGET_BASES_100000X\tAT_DROPOUT\tGC_DROPOUT\tHET_SNP_SENSITIVITY\tHET_SNP_Q\tSAMPLE\tLIBRARY\tREAD_GROUP";

const TARGET_DEPTH_THRESHOLDS: [usize; 17] = [
    1, 2, 10, 20, 30, 40, 50, 100, 250, 500, 1000, 2500, 5000, 10000, 25000, 50000, 100000,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenomicInterval {
    /// Picard interval-list coordinates: one-based, inclusive.
    pub contig: String,
    pub start: u64,
    pub end: u64,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct HsMetricsConfig {
    pub bait_intervals: Vec<GenomicInterval>,
    pub target_intervals: Vec<GenomicInterval>,
    pub reference_lengths: BTreeMap<String, u64>,
    pub reference_sequences: BTreeMap<String, Vec<u8>>,
    pub per_target_coverage: Option<String>,
    pub per_base_coverage: Option<String>,
    pub genome_size: u64,
    pub bait_set_name: String,
    pub clip_overlapping_reads: bool,
    pub near_distance: u32,
    pub minimum_mapping_quality: u8,
    pub minimum_base_quality: u8,
    pub coverage_cap: u32,
    pub sample_size: u32,
    pub include_indels: bool,
    pub stop_after: u32,
}

#[derive(Debug, Clone)]
struct Span {
    contig: String,
    start: u64,
    end: u64,
    name: String,
}

#[derive(Debug, Clone)]
struct IndexedSpan {
    start: u64,
    end: u64,
    index: usize,
}

#[derive(Debug, Default, Clone)]
struct IntervalIndex {
    by_contig: BTreeMap<String, Vec<IndexedSpan>>,
}

impl IntervalIndex {
    fn from_spans(spans: &[Span]) -> Self {
        let mut by_contig = BTreeMap::<String, Vec<IndexedSpan>>::new();
        for (index, span) in spans.iter().enumerate() {
            by_contig
                .entry(span.contig.clone())
                .or_default()
                .push(IndexedSpan {
                    start: span.start,
                    end: span.end,
                    index,
                });
        }
        Self { by_contig }
    }

    fn candidates<'a>(
        &'a self,
        contig: &str,
        start: u64,
        end: u64,
    ) -> Box<dyn Iterator<Item = &'a IndexedSpan> + 'a> {
        let Some(spans) = self.by_contig.get(contig) else {
            return Box::new(std::iter::empty());
        };
        let first = first_span_with_end_after(spans, start);
        Box::new(
            spans[first..]
                .iter()
                .take_while(move |span| span.start < end)
                .filter(move |span| span.end > start),
        )
    }

    fn overlaps(&self, contig: &str, start: u64, end: u64) -> bool {
        self.candidates(contig, start, end).next().is_some()
    }

    fn overlap_bases(&self, contig: &str, start: u64, end: u64) -> u64 {
        self.candidates(contig, start, end)
            .map(|span| span.end.min(end).saturating_sub(span.start.max(start)))
            .sum()
    }

    fn index_at(&self, contig: &str, position: u64) -> Option<usize> {
        self.candidates(contig, position, position.saturating_add(1))
            .next()
            .map(|span| span.index)
    }
}

fn first_span_with_end_after(spans: &[IndexedSpan], position: u64) -> usize {
    let mut low = 0;
    let mut high = spans.len();
    while low < high {
        let middle = (low + high) / 2;
        if spans[middle].end <= position {
            low = middle + 1;
        } else {
            high = middle;
        }
    }
    low
}

#[derive(Debug)]
struct TargetCoverage {
    span: Span,
    high_quality_depths: Vec<u32>,
    unfiltered_depths: Vec<u32>,
    gc_fraction: f64,
    gc_percent: usize,
    read_count: u64,
}

#[derive(Debug, Default)]
struct OverlapBitmap {
    words: Vec<u64>,
}

impl OverlapBitmap {
    fn with_bit_len(bits: usize) -> Self {
        Self {
            words: vec![0; bits.div_ceil(64)],
        }
    }

    fn set(&mut self, index: usize) {
        if let Some(word) = self.words.get_mut(index / 64) {
            *word |= 1_u64 << (index % 64);
        }
    }

    fn get(&self, index: usize) -> bool {
        self.words
            .get(index / 64)
            .copied()
            .is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
    }
}

#[derive(Debug)]
struct CachedOverlap {
    overlap_start: u64,
    bitmap: OverlapBitmap,
}

impl CachedOverlap {
    fn covered_at(&self, position: u64) -> bool {
        position >= self.overlap_start && self.bitmap.get((position - self.overlap_start) as usize)
    }
}

#[derive(Debug, Default)]
struct MateBuffer {
    pending: FxHashMap<Vec<u8>, CachedOverlap>,
}

enum MateProbe {
    Alone,
    WouldBuffer {
        overlap_start: u64,
        overlap_len: u64,
    },
    PairWith(CachedOverlap),
}

impl MateBuffer {
    fn clear(&mut self) {
        self.pending.clear();
    }

    fn probe(&mut self, record: &bam::Record) -> MateProbe {
        if !record.is_paired() || record.is_unmapped() || record.is_mate_unmapped() {
            return MateProbe::Alone;
        }
        if record.tid() < 0 || record.mtid() < 0 || record.tid() != record.mtid() {
            return MateProbe::Alone;
        }
        if let Some(cached) = self.pending.remove(record.qname()) {
            return MateProbe::PairWith(cached);
        }

        let read_start = record.pos().max(0) as u64;
        let mate_start = record.mpos().max(0) as u64;
        let read_end = read_start.saturating_add(reference_consumed_len(record));
        if mate_start >= read_start && mate_start < read_end {
            return MateProbe::WouldBuffer {
                overlap_start: mate_start,
                overlap_len: read_end.saturating_sub(mate_start),
            };
        }
        MateProbe::Alone
    }

    fn insert(&mut self, qname: &[u8], cached: CachedOverlap) {
        self.pending.insert(qname.to_vec(), cached);
    }
}

#[derive(Debug)]
pub struct HsMetricsCollector {
    config: HsMetricsConfig,
    bait_index: IntervalIndex,
    target_index: IntervalIndex,
    targets: Vec<TargetCoverage>,
    target_territory: u64,
    bait_territory: u64,
    total_reads: u64,
    pf_reads: u64,
    pf_bases: u64,
    pf_unique_reads: u64,
    pf_uq_reads_aligned: u64,
    pf_bases_aligned: u64,
    pf_uq_bases_aligned: u64,
    on_target_bases: u64,
    on_target_from_pair_bases: u64,
    on_bait_bases: u64,
    near_bait_bases: u64,
    off_bait_bases: u64,
    selected_pairs: u64,
    selected_unique_pairs: u64,
    excluded_dupe: u64,
    excluded_adapter: u64,
    excluded_mapq: u64,
    excluded_baseq: u64,
    excluded_overlap: u64,
    excluded_off_target: u64,
    baseq_histogram: [u64; 256],
    uncapped_baseq_histogram: [u64; 256],
    mate_buffer: MateBuffer,
}

impl HsMetricsCollector {
    pub fn new(config: &HsMetricsConfig) -> Result<Self, String> {
        let bait_spans = normalize_intervals(&config.bait_intervals, &config.reference_lengths)?;
        let target_spans =
            normalize_intervals(&config.target_intervals, &config.reference_lengths)?;
        if bait_spans.is_empty() {
            return Err("CollectHsMetrics bait intervals are empty".to_string());
        }
        if target_spans.is_empty() {
            return Err("CollectHsMetrics target intervals are empty".to_string());
        }

        let targets = target_spans
            .iter()
            .cloned()
            .map(|span| {
                let sequence = config.reference_sequences.get(&span.contig).ok_or_else(|| {
                    format!(
                        "CollectHsMetrics target contig {} sequence is not loaded from reference",
                        span.contig
                    )
                })?;
                let gc_fraction = gc_fraction_for_slice(sequence, span.start, span.end);
                let gc_percent = (gc_fraction * 100.0).round() as usize;
                let length = span.end.saturating_sub(span.start) as usize;
                Ok(TargetCoverage {
                    span,
                    high_quality_depths: vec![0; length],
                    unfiltered_depths: vec![0; length],
                    gc_fraction,
                    gc_percent,
                    read_count: 0,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;

        let target_territory = targets
            .iter()
            .map(|target| target.span.end.saturating_sub(target.span.start))
            .sum();
        let bait_territory = bait_spans
            .iter()
            .map(|span| span.end.saturating_sub(span.start))
            .sum();

        Ok(Self {
            config: config.clone(),
            bait_index: IntervalIndex::from_spans(&bait_spans),
            target_index: IntervalIndex::from_spans(&target_spans),
            targets,
            target_territory,
            bait_territory,
            total_reads: 0,
            pf_reads: 0,
            pf_bases: 0,
            pf_unique_reads: 0,
            pf_uq_reads_aligned: 0,
            pf_bases_aligned: 0,
            pf_uq_bases_aligned: 0,
            on_target_bases: 0,
            on_target_from_pair_bases: 0,
            on_bait_bases: 0,
            near_bait_bases: 0,
            off_bait_bases: 0,
            selected_pairs: 0,
            selected_unique_pairs: 0,
            excluded_dupe: 0,
            excluded_adapter: 0,
            excluded_mapq: 0,
            excluded_baseq: 0,
            excluded_overlap: 0,
            excluded_off_target: 0,
            baseq_histogram: [0; 256],
            uncapped_baseq_histogram: [0; 256],
            mate_buffer: MateBuffer::default(),
        })
    }

    pub fn observe(&mut self, record: &bam::Record, target_names: &[String]) -> Result<(), String> {
        // Picard ignores secondary alignments altogether and does not include
        // supplementary records in read-based metrics.
        if record.is_secondary() {
            return Ok(());
        }

        let is_supplementary = record.is_supplementary();
        let is_pf = !record.is_quality_check_failed();
        if !is_supplementary {
            self.total_reads += 1;
            if is_pf {
                self.pf_reads += 1;
            }
        }
        if !is_pf {
            return Ok(());
        }

        if !is_supplementary {
            self.pf_bases += record.seq_len() as u64;
        }

        let aligned_bases = if record.is_unmapped() {
            0
        } else {
            aligned_block_len(record)
        };
        if !record.is_unmapped() {
            self.pf_bases_aligned += aligned_bases;
            if !record.is_duplicate() {
                self.pf_uq_bases_aligned += aligned_bases;
            }
        }
        if !record.is_duplicate() && !record.is_unmapped() && !is_supplementary {
            self.pf_uq_reads_aligned += 1;
        }
        if !record.is_duplicate() && !is_supplementary {
            self.pf_unique_reads += 1;
        }
        if record.is_unmapped() {
            return Ok(());
        }

        let tid = record.tid();
        let contig = target_names
            .get(
                usize::try_from(tid)
                    .map_err(|_| "CollectHsMetrics record has invalid target".to_string())?,
            )
            .ok_or_else(|| "CollectHsMetrics record references unknown target".to_string())?;
        let read_start = record.pos().max(0) as u64;
        let read_end = read_start.saturating_add(reference_consumed_len(record));

        // Bait metrics deliberately precede duplicate, adapter, mapQ, baseQ,
        // and overlap filters, matching Picard's assay-level contract.
        let near_bait = expanded_bait_overlaps(
            &self.bait_index,
            contig,
            read_start,
            read_end,
            self.config.near_distance as u64,
        );
        let on_bait = aligned_bases_in_baits(record, &self.bait_index, contig);
        if near_bait {
            self.on_bait_bases += on_bait;
            self.near_bait_bases += aligned_bases.saturating_sub(on_bait);
        } else {
            self.off_bait_bases += aligned_bases;
        }

        let mapped_in_pair = record.is_paired() && !record.is_mate_unmapped() && !is_supplementary;
        if !is_supplementary
            && record.is_paired()
            && record.is_first_in_template()
            && !record.is_mate_unmapped()
            && near_bait
        {
            self.selected_pairs += 1;
            if !record.is_duplicate() {
                self.selected_unique_pairs += 1;
            }
        }

        if record.is_duplicate() {
            self.excluded_dupe += aligned_bases;
            return Ok(());
        }
        if super::is_adapter_read(
            &record.seq().as_bytes(),
            false,
            record.mapq(),
            record.is_reverse(),
        ) {
            self.excluded_adapter += aligned_bases;
            return Ok(());
        }
        if record.mapq() < self.config.minimum_mapping_quality {
            self.excluded_mapq += aligned_bases;
            return Ok(());
        }

        if !self.config.clip_overlapping_reads {
            return self.observe_cigar_ops(record, contig, mapped_in_pair, None);
        }

        match self.mate_buffer.probe(record) {
            MateProbe::Alone => self.observe_cigar_ops(record, contig, mapped_in_pair, None),
            MateProbe::WouldBuffer {
                overlap_start,
                overlap_len,
            } => {
                let mut bitmap = OverlapBitmap::with_bit_len(overlap_len as usize);
                self.observe_cigar_ops(
                    record,
                    contig,
                    mapped_in_pair,
                    Some(OverlapMode::Buffer {
                        overlap_start,
                        bitmap: &mut bitmap,
                    }),
                )?;
                self.mate_buffer.insert(
                    record.qname(),
                    CachedOverlap {
                        overlap_start,
                        bitmap,
                    },
                );
                Ok(())
            }
            MateProbe::PairWith(cached) => self.observe_cigar_ops(
                record,
                contig,
                mapped_in_pair,
                Some(OverlapMode::Pair(&cached)),
            ),
        }
    }

    fn observe_cigar_ops(
        &mut self,
        record: &bam::Record,
        contig: &str,
        mapped_in_pair: bool,
        mut overlap_mode: Option<OverlapMode<'_>>,
    ) -> Result<(), String> {
        let mut covered_targets = FxHashSet::default();
        let mut read_offset = 0usize;
        let mut reference_offset = record.pos().max(0) as u64;
        let qualities = record.qual();

        for cigar in record.cigar().iter().copied() {
            let (length, op) = match cigar {
                Cigar::Match(length) | Cigar::Equal(length) | Cigar::Diff(length) => {
                    (length as usize, CigarOp::Aligned)
                }
                Cigar::Ins(length) => (length as usize, CigarOp::Insertion),
                Cigar::Del(length) => (length as usize, CigarOp::Deletion),
                Cigar::RefSkip(length) => (length as usize, CigarOp::ReferenceSkip),
                Cigar::SoftClip(length) => (length as usize, CigarOp::SoftClip),
                Cigar::HardClip(_) | Cigar::Pad(_) => (0, CigarOp::Other),
            };

            match op {
                CigarOp::Aligned => {
                    for index in 0..length {
                        let quality = qualities.get(read_offset + index).copied().unwrap_or(0);
                        self.observe_base(
                            contig,
                            reference_offset + index as u64,
                            quality,
                            mapped_in_pair,
                            true,
                            &mut covered_targets,
                            &mut overlap_mode,
                        );
                    }
                    read_offset += length;
                    reference_offset += length as u64;
                }
                CigarOp::Insertion => {
                    if self.config.include_indels {
                        for index in 0..length {
                            let quality = qualities.get(read_offset + index).copied().unwrap_or(0);
                            self.observe_base(
                                contig,
                                reference_offset,
                                quality,
                                mapped_in_pair,
                                false,
                                &mut covered_targets,
                                &mut overlap_mode,
                            );
                        }
                    }
                    read_offset += length;
                }
                CigarOp::Deletion => {
                    if self.config.include_indels {
                        for index in 0..length {
                            let quality = qualities.get(read_offset).copied().unwrap_or(0);
                            self.observe_base(
                                contig,
                                reference_offset + index as u64,
                                quality,
                                mapped_in_pair,
                                false,
                                &mut covered_targets,
                                &mut overlap_mode,
                            );
                        }
                    }
                    reference_offset += length as u64;
                }
                CigarOp::ReferenceSkip => reference_offset += length as u64,
                CigarOp::SoftClip => read_offset += length,
                CigarOp::Other => {}
            }
        }
        Ok(())
    }

    fn observe_base(
        &mut self,
        contig: &str,
        reference_position: u64,
        quality: u8,
        mapped_in_pair: bool,
        contributes_coverage: bool,
        covered_targets: &mut FxHashSet<usize>,
        overlap_mode: &mut Option<OverlapMode<'_>>,
    ) {
        if overlap_mode
            .as_ref()
            .is_some_and(|mode| mode.is_mate_covered(reference_position))
        {
            self.excluded_overlap += 1;
            return;
        }

        if let Some(mode) = overlap_mode.as_mut() {
            mode.mark_first_read_position(reference_position);
        }

        let on_target = self.target_index.index_at(contig, reference_position);
        if quality < self.config.minimum_base_quality {
            self.excluded_baseq += 1;
        } else if on_target.is_none() {
            self.excluded_off_target += 1;
        } else {
            self.on_target_bases += 1;
            if mapped_in_pair {
                self.on_target_from_pair_bases += 1;
            }
        }

        let Some(target_index) = on_target else {
            return;
        };
        if quality <= 2 || !contributes_coverage {
            return;
        }

        let target = &mut self.targets[target_index];
        let target_offset = reference_position.saturating_sub(target.span.start) as usize;
        let Some(unfiltered_depth) = target.unfiltered_depths.get_mut(target_offset) else {
            return;
        };
        *unfiltered_depth = unfiltered_depth
            .saturating_add(1)
            .min(self.config.coverage_cap);
        if let Some(value) = self.baseq_histogram.get_mut(quality as usize)
            && *unfiltered_depth <= self.config.coverage_cap
        {
            *value += 1;
        }
        self.uncapped_baseq_histogram[quality as usize] += 1;

        if quality >= self.config.minimum_base_quality {
            if let Some(depth) = target.high_quality_depths.get_mut(target_offset) {
                *depth = depth.saturating_add(1);
            }
            if covered_targets.insert(target_index) {
                target.read_count = target.read_count.saturating_add(1);
            }
        }
    }

    fn to_picard_text(&self) -> String {
        let high_quality_histogram = self.high_quality_depth_histogram();
        let unfiltered_histogram = self.unfiltered_depth_histogram();
        let target_territory = self.target_territory;
        let genome_size = if self.config.genome_size > 0 {
            self.config.genome_size
        } else {
            self.config.reference_lengths.values().copied().sum()
        };
        let bait_denominator = self
            .on_bait_bases
            .saturating_add(self.near_bait_bases)
            .saturating_add(self.off_bait_bases);
        let selected_bases = self.on_bait_bases.saturating_add(self.near_bait_bases);
        let mean_bait_coverage = ratio(self.on_bait_bases, self.bait_territory);
        let mean_target_coverage = ratio(
            high_quality_histogram
                .iter()
                .enumerate()
                .map(|(depth, count)| depth as u64 * count)
                .sum(),
            target_territory,
        );
        let median_target_coverage = histogram_median(&high_quality_histogram);
        let max_target_coverage = high_quality_histogram
            .iter()
            .rposition(|count| *count > 0)
            .unwrap_or(0);
        let min_target_coverage = self
            .targets
            .iter()
            .flat_map(|target| target.high_quality_depths.iter().copied())
            .min()
            .unwrap_or(0);
        let zero_coverage_targets = self
            .targets
            .iter()
            .filter(|target| target.high_quality_depths.iter().all(|depth| *depth == 0))
            .count();
        let fold80 = histogram_percentile(&high_quality_histogram, 0.2);
        let fold80_text = if fold80 > 0.0 {
            super::format_float(mean_target_coverage / fold80)
        } else {
            "?".to_string()
        };
        let hs_library_size =
            estimate_library_size(self.selected_pairs, self.selected_unique_pairs);
        let penalties = hs_library_size
            .map(|library_size| {
                TARGET_DEPTH_THRESHOLDS
                    .iter()
                    .filter(|depth| matches!(**depth, 10 | 20 | 30 | 40 | 50 | 100))
                    .map(|depth| {
                        self.hs_penalty(
                            library_size,
                            *depth as u64,
                            mean_target_coverage,
                            &fold80_text,
                        )
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let het_sensitivity = if self.config.sample_size == 0 {
            0.0
        } else {
            super::het_snp_sensitivity_from_histograms(
                &unfiltered_histogram,
                &self.baseq_histogram,
                self.config.sample_size,
            )
        };
        let het_sensitivity_text = super::format_float(het_sensitivity);
        let het_q = super::het_snp_q(&het_sensitivity_text);
        let (at_dropout, gc_dropout) = self.gc_dropout();

        let penalty_text = |index: usize| {
            penalties
                .get(index)
                .cloned()
                .unwrap_or_else(|| "0".to_string())
        };
        let hs_library_text = hs_library_size
            .map(|value| value.to_string())
            .unwrap_or_default();

        let target_percentages = TARGET_DEPTH_THRESHOLDS
            .iter()
            .map(|depth| super::format_float(pct_at_least(&high_quality_histogram, *depth)))
            .collect::<Vec<_>>();

        let fields = vec![
            self.config.bait_set_name.clone(),
            self.bait_territory.to_string(),
            super::format_float(ratio(target_territory, self.bait_territory)),
            self.on_bait_bases.to_string(),
            self.near_bait_bases.to_string(),
            self.off_bait_bases.to_string(),
            super::format_float(ratio(selected_bases, bait_denominator)),
            super::format_float(ratio(self.off_bait_bases, bait_denominator)),
            super::format_float(ratio(self.on_bait_bases, selected_bases)),
            super::format_float(mean_bait_coverage),
            super::format_float(ratio(self.on_bait_bases, self.pf_bases)),
            super::format_float(ratio(self.on_target_bases, self.pf_bases)),
            super::format_float(
                if bait_denominator == 0 || self.bait_territory == 0 || genome_size == 0 {
                    0.0
                } else {
                    ratio(self.on_bait_bases, bait_denominator)
                        / ratio(self.bait_territory, genome_size)
                },
            ),
            hs_library_text,
            penalty_text(0),
            penalty_text(1),
            penalty_text(2),
            penalty_text(3),
            penalty_text(4),
            penalty_text(5),
            target_territory.to_string(),
            genome_size.to_string(),
            self.total_reads.to_string(),
            self.pf_reads.to_string(),
            self.pf_bases.to_string(),
            self.pf_unique_reads.to_string(),
            self.pf_uq_reads_aligned.to_string(),
            self.pf_bases_aligned.to_string(),
            self.pf_uq_bases_aligned.to_string(),
            self.on_target_bases.to_string(),
            super::format_float(ratio(self.pf_reads, self.total_reads)),
            super::format_float(ratio(self.pf_unique_reads, self.total_reads)),
            super::format_float(ratio(self.pf_uq_reads_aligned, self.pf_unique_reads)),
            super::format_float(mean_target_coverage),
            super::format_float(median_target_coverage),
            max_target_coverage.to_string(),
            min_target_coverage.to_string(),
            super::format_float(ratio(
                zero_coverage_targets as u64,
                self.targets.len() as u64,
            )),
            super::format_float(ratio(self.excluded_dupe, self.pf_bases_aligned)),
            super::format_float(ratio(self.excluded_adapter, self.pf_bases_aligned)),
            super::format_float(ratio(self.excluded_mapq, self.pf_bases_aligned)),
            super::format_float(ratio(self.excluded_baseq, self.pf_bases_aligned)),
            super::format_float(ratio(self.excluded_overlap, self.pf_bases_aligned)),
            super::format_float(ratio(self.excluded_off_target, self.pf_bases_aligned)),
            fold80_text,
        ];

        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.directed.HsMetrics\n");
        output.push_str(HS_METRICS_HEADER);
        output.push('\n');
        output.push_str(&fields.join("\t"));
        output.push('\t');
        output.push_str(&target_percentages.join("\t"));
        output.push('\t');
        output.push_str(&super::format_float(at_dropout));
        output.push('\t');
        output.push_str(&super::format_float(gc_dropout));
        output.push('\t');
        output.push_str(&het_sensitivity_text);
        output.push('\t');
        output.push_str(&het_q);
        output.push_str("\t\t\t\n\n");
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        output.push_str(
            "coverage_or_base_quality\thigh_quality_coverage_count\tunfiltered_baseq_count\n",
        );
        for coverage_or_quality in 0..=126 {
            output.push_str(&format!(
                "{}\t{}\t{}\n",
                coverage_or_quality,
                high_quality_histogram
                    .get(coverage_or_quality)
                    .copied()
                    .unwrap_or(0),
                self.uncapped_baseq_histogram[coverage_or_quality]
            ));
        }
        output
    }

    fn write_sidecar_outputs(&self) -> Result<(), String> {
        let high_quality_histogram = self.high_quality_depth_histogram();
        let mean_target_coverage = ratio(
            high_quality_histogram
                .iter()
                .enumerate()
                .map(|(depth, count)| depth as u64 * count)
                .sum(),
            self.target_territory,
        );
        if let Some(path) = self.config.per_target_coverage.as_deref() {
            fs::write(path, self.per_target_coverage_text(mean_target_coverage))
                .map_err(|error| error.to_string())?;
        }
        if let Some(path) = self.config.per_base_coverage.as_deref() {
            fs::write(path, self.per_base_coverage_text()).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn per_target_coverage_text(&self, mean_target_coverage: f64) -> String {
        let mut output = String::from(
            "chrom\tstart\tend\tlength\tname\t%gc\tmean_coverage\tnormalized_coverage\tmin_normalized_coverage\tmax_normalized_coverage\tmin_coverage\tmax_coverage\tpct_0x\tread_count\n",
        );
        for target in &self.targets {
            let length = target.span.end.saturating_sub(target.span.start);
            if length == 0 {
                continue;
            }
            let total_coverage = target
                .high_quality_depths
                .iter()
                .map(|depth| *depth as u64)
                .sum::<u64>();
            let mean_coverage = ratio(total_coverage, length);
            let min_coverage = target
                .high_quality_depths
                .iter()
                .copied()
                .min()
                .unwrap_or(0);
            let max_coverage = target
                .high_quality_depths
                .iter()
                .copied()
                .max()
                .unwrap_or(0);
            let zero_coverage = target
                .high_quality_depths
                .iter()
                .filter(|depth| **depth == 0)
                .count() as u64;
            let normalized = ratio_f64(mean_coverage, mean_target_coverage);
            let min_normalized = ratio_f64(min_coverage as f64, mean_target_coverage);
            let max_normalized = ratio_f64(max_coverage as f64, mean_target_coverage);
            output.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                target.span.contig,
                target.span.start.saturating_add(1),
                target.span.end,
                length,
                target.span.name,
                super::format_float(target.gc_fraction),
                super::format_float(mean_coverage),
                super::format_float(normalized),
                super::format_float(min_normalized),
                super::format_float(max_normalized),
                min_coverage,
                max_coverage,
                super::format_float(ratio(zero_coverage, length)),
                target.read_count,
            ));
        }
        output
    }

    fn per_base_coverage_text(&self) -> String {
        let mut output = String::from("chrom\tpos\ttarget\tcoverage\n");
        for target in &self.targets {
            for (offset, depth) in target.high_quality_depths.iter().enumerate() {
                output.push_str(&format!(
                    "{}\t{}\t{}\t{}\n",
                    target.span.contig,
                    target.span.start + offset as u64 + 1,
                    target.span.name,
                    depth,
                ));
            }
        }
        output
    }

    fn high_quality_depth_histogram(&self) -> Vec<u64> {
        let max_depth = self
            .targets
            .iter()
            .flat_map(|target| target.high_quality_depths.iter().copied())
            .max()
            .unwrap_or(0) as usize;
        let mut histogram = vec![0_u64; max_depth.saturating_add(1)];
        for target in &self.targets {
            for depth in &target.high_quality_depths {
                histogram[*depth as usize] += 1;
            }
        }
        histogram
    }

    fn unfiltered_depth_histogram(&self) -> Vec<u64> {
        let mut histogram = vec![0_u64; self.config.coverage_cap as usize + 1];
        for target in &self.targets {
            for depth in &target.unfiltered_depths {
                histogram[(*depth).min(self.config.coverage_cap) as usize] += 1;
            }
        }
        histogram
    }

    fn hs_penalty(
        &self,
        library_size: u64,
        coverage_goal: u64,
        mean_target_coverage: f64,
        fold80_text: &str,
    ) -> String {
        let Ok(fold80) = fold80_text.parse::<f64>() else {
            return "-1".to_string();
        };
        let mean_coverage = ratio(self.on_target_from_pair_bases, self.target_territory);
        let on_target_pct = ratio(self.on_target_bases, self.pf_uq_bases_aligned);
        if mean_coverage <= 0.0 || fold80 <= 0.0 || on_target_pct <= 0.0 {
            return "0".to_string();
        }
        let unique_pair_goal_multiplier = (coverage_goal as f64 / mean_coverage) * fold80;
        let mut pair_multiplier = unique_pair_goal_multiplier;
        let mut increment = 1.0;
        let mut going_up = unique_pair_goal_multiplier >= 1.0;
        let mut final_pair_multiplier = None;
        for _ in 0..10_000 {
            let unique_pair_multiplier = estimate_roi(
                library_size,
                pair_multiplier,
                self.selected_pairs,
                self.selected_unique_pairs,
            );
            if ((unique_pair_multiplier - unique_pair_goal_multiplier).abs()
                / unique_pair_goal_multiplier)
                <= 0.001
            {
                final_pair_multiplier = Some(pair_multiplier);
                break;
            }
            if (unique_pair_multiplier > unique_pair_goal_multiplier && going_up)
                || (unique_pair_multiplier < unique_pair_goal_multiplier && !going_up)
            {
                increment /= 2.0;
                going_up = !going_up;
            }
            pair_multiplier += if going_up { increment } else { -increment };
        }
        let Some(final_pair_multiplier) = final_pair_multiplier else {
            return "-1".to_string();
        };
        let unique_fraction = (self.selected_unique_pairs as f64 * unique_pair_goal_multiplier)
            / (self.selected_pairs as f64 * final_pair_multiplier);
        let _ = mean_target_coverage;
        super::format_float((1.0 / unique_fraction) * fold80 * (1.0 / on_target_pct))
    }

    fn gc_dropout(&self) -> (f64, f64) {
        let mut target_bases_by_gc = [0_u64; 101];
        let mut aligned_bases_by_gc = [0_u64; 101];
        for target in &self.targets {
            let gc = target.gc_percent.min(100);
            target_bases_by_gc[gc] += target.span.end.saturating_sub(target.span.start);
            aligned_bases_by_gc[gc] += target
                .high_quality_depths
                .iter()
                .map(|depth| *depth as u64)
                .sum::<u64>();
        }
        let total_target = target_bases_by_gc.iter().sum::<u64>();
        let total_aligned = aligned_bases_by_gc.iter().sum::<u64>();
        if total_target == 0 || total_aligned == 0 {
            return (0.0, 0.0);
        }
        let mut at_dropout = 0.0;
        let mut gc_dropout = 0.0;
        for index in 0..=100 {
            let target_fraction = target_bases_by_gc[index] as f64 / total_target as f64;
            let aligned_fraction = aligned_bases_by_gc[index] as f64 / total_aligned as f64;
            let dropout = target_fraction - aligned_fraction;
            if dropout > 0.0 {
                if index <= 50 {
                    at_dropout += dropout;
                }
                if index >= 50 {
                    gc_dropout += dropout;
                }
            }
        }
        (at_dropout * 100.0, gc_dropout * 100.0)
    }
}

enum OverlapMode<'a> {
    Buffer {
        overlap_start: u64,
        bitmap: &'a mut OverlapBitmap,
    },
    Pair(&'a CachedOverlap),
}

impl OverlapMode<'_> {
    fn is_mate_covered(&self, position: u64) -> bool {
        match self {
            Self::Buffer { .. } => false,
            Self::Pair(cached) => cached.covered_at(position),
        }
    }

    fn mark_first_read_position(&mut self, position: u64) {
        if let Self::Buffer {
            overlap_start,
            bitmap,
        } = self
            && position >= *overlap_start
        {
            bitmap.set((position - *overlap_start) as usize);
        }
    }
}

#[derive(Clone, Copy)]
enum CigarOp {
    Aligned,
    Insertion,
    Deletion,
    ReferenceSkip,
    SoftClip,
    Other,
}

fn normalize_intervals(
    intervals: &[GenomicInterval],
    reference_lengths: &BTreeMap<String, u64>,
) -> Result<Vec<Span>, String> {
    let mut spans = Vec::with_capacity(intervals.len());
    for interval in intervals {
        let reference_length = reference_lengths.get(&interval.contig).ok_or_else(|| {
            format!(
                "CollectHsMetrics interval contig {} is not present in reference",
                interval.contig
            )
        })?;
        if interval.start == 0 || interval.end < interval.start {
            return Err(format!(
                "CollectHsMetrics interval has invalid coordinates {}:{}-{}",
                interval.contig, interval.start, interval.end
            ));
        }
        if interval.end > *reference_length {
            return Err(format!(
                "CollectHsMetrics interval extends beyond reference {}:{}-{}",
                interval.contig, interval.start, interval.end
            ));
        }
        spans.push(Span {
            contig: interval.contig.clone(),
            start: interval.start - 1,
            end: interval.end,
            name: interval.name.clone(),
        });
    }
    spans.sort_by(|left, right| {
        left.contig
            .cmp(&right.contig)
            .then(left.start.cmp(&right.start))
            .then(left.end.cmp(&right.end))
    });

    let mut normalized: Vec<Span> = Vec::with_capacity(spans.len());
    for span in spans {
        if let Some(previous) = normalized.last_mut()
            && previous.contig == span.contig
            && span.start < previous.end
        {
            previous.end = previous.end.max(span.end);
        } else {
            normalized.push(span);
        }
    }
    Ok(normalized)
}

fn expanded_bait_overlaps(
    bait_index: &IntervalIndex,
    contig: &str,
    start: u64,
    end: u64,
    near_distance: u64,
) -> bool {
    let expanded_start = start.saturating_sub(near_distance);
    let expanded_end = end.saturating_add(near_distance);
    bait_index.overlaps(contig, expanded_start, expanded_end)
}

fn aligned_bases_in_baits(record: &bam::Record, bait_index: &IntervalIndex, contig: &str) -> u64 {
    let mut reference_position = record.pos().max(0) as u64;
    let mut total = 0;
    for cigar in record.cigar().iter().copied() {
        match cigar {
            Cigar::Match(length) | Cigar::Equal(length) | Cigar::Diff(length) => {
                total += bait_index.overlap_bases(
                    contig,
                    reference_position,
                    reference_position.saturating_add(length as u64),
                );
                reference_position += length as u64;
            }
            Cigar::Del(length) | Cigar::RefSkip(length) => reference_position += length as u64,
            _ => {}
        }
    }
    total
}

fn reference_consumed_len(record: &bam::Record) -> u64 {
    record
        .cigar()
        .iter()
        .map(|cigar| match *cigar {
            Cigar::Match(length)
            | Cigar::Equal(length)
            | Cigar::Diff(length)
            | Cigar::Del(length)
            | Cigar::RefSkip(length) => length as u64,
            _ => 0,
        })
        .sum()
}

fn aligned_block_len(record: &bam::Record) -> u64 {
    record
        .cigar()
        .iter()
        .map(|cigar| match *cigar {
            Cigar::Match(length) | Cigar::Equal(length) | Cigar::Diff(length) => length as u64,
            _ => 0,
        })
        .sum()
}

fn gc_fraction_for_slice(sequence: &[u8], start: u64, end: u64) -> f64 {
    let Some(slice) = sequence.get(start as usize..end as usize) else {
        return 0.0;
    };
    if slice.is_empty() {
        return 0.0;
    }
    let gc = slice
        .iter()
        .filter(|base| matches!(base.to_ascii_uppercase(), b'G' | b'C'))
        .count();
    gc as f64 / slice.len() as f64
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn ratio_f64(numerator: f64, denominator: f64) -> f64 {
    if denominator == 0.0 {
        0.0
    } else {
        numerator / denominator
    }
}

fn pct_at_least(histogram: &[u64], depth: usize) -> f64 {
    let total = histogram.iter().sum::<u64>();
    if total == 0 {
        0.0
    } else {
        histogram.iter().skip(depth).sum::<u64>() as f64 / total as f64
    }
}

fn histogram_median(histogram: &[u64]) -> f64 {
    histogram_percentile(histogram, 0.5)
}

fn histogram_percentile(histogram: &[u64], percentile: f64) -> f64 {
    let total = histogram.iter().sum::<u64>();
    if total == 0 {
        return 0.0;
    }
    let rank = percentile.clamp(0.0, 1.0) * (total.saturating_sub(1) as f64);
    let left_rank = rank.floor() as u64;
    let right_rank = rank.ceil() as u64;
    let left = histogram_value_at_rank(histogram, left_rank) as f64;
    let right = histogram_value_at_rank(histogram, right_rank) as f64;
    left + (right - left) * (rank - left_rank as f64)
}

fn histogram_value_at_rank(histogram: &[u64], rank: u64) -> usize {
    let mut seen = 0;
    for (value, count) in histogram.iter().enumerate() {
        seen += count;
        if seen > rank {
            return value;
        }
    }
    0
}

fn estimate_library_size(read_pairs: u64, unique_read_pairs: u64) -> Option<u64> {
    if read_pairs == 0 || unique_read_pairs >= read_pairs {
        return None;
    }
    let mut lower = 1.0;
    let mut upper = 100.0;
    while library_size_equation(
        upper * unique_read_pairs as f64,
        unique_read_pairs,
        read_pairs,
    ) > 0.0
    {
        upper *= 10.0;
    }
    for _ in 0..40 {
        let middle = (lower + upper) / 2.0;
        let value = library_size_equation(
            middle * unique_read_pairs as f64,
            unique_read_pairs,
            read_pairs,
        );
        if value > 0.0 {
            lower = middle;
        } else {
            upper = middle;
        }
    }
    Some((unique_read_pairs as f64 * (lower + upper) / 2.0) as u64)
}

fn library_size_equation(library_size: f64, unique_read_pairs: u64, read_pairs: u64) -> f64 {
    unique_read_pairs as f64 / library_size - 1.0 + (-(read_pairs as f64) / library_size).exp()
}

fn estimate_roi(
    library_size: u64,
    coverage_multiple: f64,
    read_pairs: u64,
    unique_pairs: u64,
) -> f64 {
    if unique_pairs == 0 {
        return 0.0;
    }
    let library_size = library_size as f64;
    library_size * (1.0 - (-(coverage_multiple * read_pairs as f64) / library_size).exp())
        / unique_pairs as f64
}

pub fn collect_hs_metrics<R: Read>(
    reader: &mut R,
    config: &HsMetricsConfig,
) -> Result<String, String> {
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let mut collector = HsMetricsCollector::new(config)?;
    let mut observed = 0_u32;
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        collector.observe(&record, &target_names)?;
        observed = observed.saturating_add(1);
        if config.stop_after > 0 && observed >= config.stop_after {
            break;
        }
    }
    collector.mate_buffer.clear();
    let metrics_text = collector.to_picard_text();
    collector.write_sidecar_outputs()?;
    Ok(metrics_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> HsMetricsConfig {
        HsMetricsConfig {
            bait_intervals: vec![GenomicInterval {
                contig: "chr1".to_string(),
                start: 1,
                end: 10,
                name: "bait".to_string(),
            }],
            target_intervals: vec![GenomicInterval {
                contig: "chr1".to_string(),
                start: 1,
                end: 10,
                name: "target".to_string(),
            }],
            reference_lengths: BTreeMap::from([(String::from("chr1"), 10)]),
            reference_sequences: BTreeMap::from([("chr1".to_string(), b"ACGTACGTAC".to_vec())]),
            per_target_coverage: None,
            per_base_coverage: None,
            genome_size: 10,
            bait_set_name: "test".to_string(),
            clip_overlapping_reads: true,
            near_distance: 250,
            minimum_mapping_quality: 0,
            minimum_base_quality: 20,
            coverage_cap: 200,
            sample_size: 0,
            include_indels: false,
            stop_after: 0,
        }
    }

    #[test]
    fn interval_normalization_is_one_based_inclusive_and_unique() {
        let config = HsMetricsConfig {
            bait_intervals: vec![
                GenomicInterval {
                    contig: "chr1".to_string(),
                    start: 2,
                    end: 5,
                    name: "bait-1".to_string(),
                },
                GenomicInterval {
                    contig: "chr1".to_string(),
                    start: 4,
                    end: 8,
                    name: "bait-2".to_string(),
                },
            ],
            target_intervals: vec![GenomicInterval {
                contig: "chr1".to_string(),
                start: 1,
                end: 1,
                name: "target".to_string(),
            }],
            reference_lengths: BTreeMap::from([(String::from("chr1"), 10)]),
            reference_sequences: BTreeMap::from([("chr1".to_string(), b"ACGTACGTAC".to_vec())]),
            per_target_coverage: None,
            per_base_coverage: None,
            genome_size: 10,
            bait_set_name: "test".to_string(),
            clip_overlapping_reads: true,
            near_distance: 0,
            minimum_mapping_quality: 0,
            minimum_base_quality: 0,
            coverage_cap: 200,
            sample_size: 0,
            include_indels: false,
            stop_after: 0,
        };
        let collector = HsMetricsCollector::new(&config).expect("collector");
        assert_eq!(collector.bait_territory, 7);
        assert_eq!(collector.target_territory, 1);
    }

    #[test]
    fn empty_collector_emits_full_picard_surface() {
        let collector = HsMetricsCollector::new(&test_config()).expect("collector");
        let text = collector.to_picard_text();
        assert!(text.contains("picard.analysis.directed.HsMetrics"));
        assert!(text.contains("BAIT_SET\tBAIT_TERRITORY"));
        assert!(text.contains("coverage_or_base_quality\thigh_quality_coverage_count"));
        assert_eq!(text.lines().filter(|line| *line == "0\t10\t0").count(), 1);
    }
}
