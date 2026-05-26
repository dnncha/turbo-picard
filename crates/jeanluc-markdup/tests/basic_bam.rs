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
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
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
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
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
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
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
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
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
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![0, 256, 1024, 0]);

    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("Unknown Library\t3\t0\t1\t0\t1\t0\t0\t0.333333\t\n"));
}

#[test]
fn chooses_duplicate_representative_per_pair_not_per_mate() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/pair-score-tie/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: false,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![99, 1123, 99, 147, 1171, 147]);
}

#[test]
fn creates_bam_index_when_requested() {
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
        create_index: true,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert!(output.with_extension("bai").exists());
}

#[test]
fn creates_md5_sidecar_when_requested() {
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
        create_index: false,
        create_md5_file: true,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let md5_path = output.with_extension("bam.md5");
    let md5_text = std::fs::read_to_string(md5_path).expect("MD5 sidecar exists");
    assert_eq!(md5_text.trim().len(), 32);
}

#[test]
fn removes_duplicate_pairs_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/paired/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: true,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![99, 99, 147, 147]);
}

#[test]
fn clears_existing_duplicate_type_tags_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/clear-dt/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: false,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        clear_dt: true,
        optical_duplicate_pixel_distance: Some(2500),
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert!(read_has_no_dt_tags(&output));
}

#[test]
fn preserves_existing_duplicate_type_tags_when_clear_dt_is_false() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/clear-dt/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        remove_duplicates: false,
        assume_sorted: true,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        clear_dt: false,
        optical_duplicate_pixel_distance: Some(2500),
    };

    jeanluc_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert!(read_has_dt_tags(&output));
}

fn read_flags(path: &std::path::Path) -> Vec<u16> {
    let mut reader = bam::Reader::from_path(path).expect("BAM opens");
    reader
        .records()
        .map(|record| record.expect("record decodes").flags())
        .collect()
}

fn read_has_no_dt_tags(path: &std::path::Path) -> bool {
    let mut reader = bam::Reader::from_path(path).expect("BAM opens");
    reader
        .records()
        .all(|record| record.expect("record decodes").aux(b"DT").is_err())
}

fn read_has_dt_tags(path: &std::path::Path) -> bool {
    let mut reader = bam::Reader::from_path(path).expect("BAM opens");
    reader
        .records()
        .any(|record| record.expect("record decodes").aux(b"DT").is_ok())
}
