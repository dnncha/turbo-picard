use jeanluc_core::markdup_config::MarkDuplicatesConfig;
use rust_htslib::bam::{self, Read};

#[test]
fn marks_duplicate_records_in_bam() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/basic/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: false,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![0, 1024, 0]);
}

#[test]
fn marks_duplicate_pairs_and_reports_paired_metrics() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/paired/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: false,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![99, 1123, 99, 147, 1171, 147]);

    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("lib1\t0\t3\t0\t0\t0\t1\t0\t0.333333\t3\n"));
}

#[test]
fn keeps_highest_quality_duplicate_representative() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/scoring/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: false,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![1024, 0, 0]);
}

#[test]
fn groups_duplicates_by_unclipped_five_prime_position() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/softclip/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: false,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![0, 1024, 0]);
}

#[test]
fn excludes_secondary_alignments_from_duplicate_testing() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/secondary/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: false,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![0, 256, 1024, 0]);

    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("Unknown Library\t3\t0\t1\t0\t1\t0\t0\t0.333333\t\n"));
}

fn read_flags(path: &std::path::Path) -> Vec<u16> {
    let mut reader = bam::Reader::from_path(path).expect("BAM opens");
    reader
        .records()
        .map(|record| record.expect("record decodes").flags())
        .collect()
}
