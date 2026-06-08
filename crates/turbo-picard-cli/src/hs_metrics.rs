//! Scaffold for Picard-compatible `CollectHsMetrics`.
//!
//! Native bait/target read accumulation is not implemented yet. The types and
//! collector interface here are the integration surface `lib.rs` uses once the
//! hybrid-capture metrics pass is filled in.

use rust_htslib::bam::{self, Read};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenomicInterval {
    pub contig: String,
    pub start: u64,
    pub end: u64,
}

#[derive(Debug, Clone)]
pub struct HsMetricsConfig {
    pub bait_intervals: Vec<GenomicInterval>,
    pub target_intervals: Vec<GenomicInterval>,
    pub clip_overlapping_reads: bool,
    pub near_distance: u32,
}

impl HsMetricsConfig {
    pub fn clip_overlapping_reads(&self) -> bool {
        self.clip_overlapping_reads
    }

    pub fn near_distance(&self) -> u32 {
        self.near_distance
    }
}

#[derive(Debug, Default)]
pub struct HsMetricsCollector {
    total_reads: u64,
    pf_reads: u64,
}

impl HsMetricsCollector {
    pub fn new(_config: &HsMetricsConfig) -> Self {
        Self::default()
    }

    pub fn observe(&mut self, record: &bam::Record) -> Result<(), String> {
        if record.is_unmapped() {
            return Ok(());
        }
        self.total_reads = self.total_reads.saturating_add(1);
        if !record.is_duplicate() && !record.is_secondary() && !record.is_supplementary() {
            self.pf_reads = self.pf_reads.saturating_add(1);
        }
        Ok(())
    }

    pub fn to_picard_text(&self) -> String {
        format!(
            "## METRICS CLASS\tpicard.analysis.directed.HsMetrics\n\
             BAITS\t0\n\
             TARGETS\t0\n\
             TOTAL_READS\t{}\n\
             PF_READS\t{}\n",
            self.total_reads, self.pf_reads
        )
    }
}

pub fn collect_hs_metrics<R: Read>(
    reader: &mut R,
    config: &HsMetricsConfig,
) -> Result<String, String> {
    if config.bait_intervals.is_empty() {
        return Err("CollectHsMetrics bait intervals are empty".to_string());
    }
    if config.target_intervals.is_empty() {
        return Err("CollectHsMetrics target intervals are empty".to_string());
    }
    let _clip_overlapping_reads = config.clip_overlapping_reads();
    let _near_distance = config.near_distance();

    let mut collector = HsMetricsCollector::new(config);
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        collector.observe(&record)?;
    }

    // Scaffold: keep the collector and Picard-shaped output hook, but fail until
    // bait/target territory and on/off-target accounting match Picard.
    let _preview = collector.to_picard_text();
    Err(
        "CollectHsMetrics native bait/target read accumulation is not implemented yet"
            .to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collector_exposes_picard_shaped_preview_text() {
        let config = HsMetricsConfig {
            bait_intervals: vec![GenomicInterval {
                contig: "chr1".to_string(),
                start: 1,
                end: 10,
            }],
            target_intervals: vec![GenomicInterval {
                contig: "chr1".to_string(),
                start: 1,
                end: 100,
            }],
            clip_overlapping_reads: false,
            near_distance: 250,
        };
        let preview = HsMetricsCollector::new(&config).to_picard_text();
        assert!(preview.contains("HsMetrics"));
        assert!(preview.contains("TOTAL_READS"));
    }
}