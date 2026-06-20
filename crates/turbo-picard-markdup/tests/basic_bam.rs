use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::record::{Aux, Cigar, CigarString};
use rust_htslib::bam::{self, Read};
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;

#[test]
fn marks_duplicate_records_in_bam() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/basic/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![99, 1123, 99, 147, 1171, 147]);

    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("lib1\t0\t3\t0\t0\t0\t1\t0\t0.333333\t3\n"));
    assert!(metrics_text.contains("BIN\tCoverageMult\tall_sets\tnon_optical_sets\n"));
}

#[test]
fn mate_unmapped_aligned_reads_are_unpaired_duplicate_candidates() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.bam");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    write_mate_unmapped_duplicate_fixture(&input);
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![73, 1097, 77, 77]);
    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(
        metrics_text.contains("lib1\t2\t0\t0\t2\t1\t0\t0\t0.5\t\n"),
        "{metrics_text}"
    );
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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let flags = read_flags(&output);
    assert_eq!(flags, vec![99, 1123, 99, 147, 1171, 147]);
}

#[test]
fn duplicate_scoring_ignores_bases_below_q15() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.bam");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    write_q15_scoring_fixture(&input);
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![1123, 1171, 99, 147]);
}

#[test]
fn paired_records_without_an_eligible_mate_are_not_marked_as_singleton_duplicates() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.bam");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    write_orphaned_pair_fixture(&input);
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![99, 99]);
    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(
        metrics_text.contains("lib1\t0\t1\t0\t0\t0\t0\t0\t0\t\n"),
        "{metrics_text}"
    );
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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: true,
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: true,
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let md5_path = output.with_extension("bam.md5");
    let md5_text = std::fs::read_to_string(md5_path).expect("MD5 sidecar exists");
    assert_eq!(md5_text.trim().len(), 32);
}

#[test]
fn adds_picard_program_group_header_and_read_tags_by_default() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/basic/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let mut reader = bam::Reader::from_path(&output).expect("BAM opens");
    let header = String::from_utf8_lossy(reader.header().as_bytes());
    assert!(header.contains("@PG\tID:MarkDuplicates"));
    assert!(reader.records().all(|record| matches!(
        record.expect("record decodes").aux(b"PG"),
        Ok(Aux::String("MarkDuplicates"))
    )));
}

#[test]
fn tags_library_duplicates_when_tagging_policy_is_all() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/basic/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("All".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    let duplicate_dt_tags = duplicate_dt_tags(&output);
    assert_eq!(duplicate_dt_tags, vec![Some("LB".to_string())]);
}

#[test]
fn tags_duplicate_set_members_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/paired/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: true,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: None,
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(
        duplicate_set_member_tags(&output),
        vec![
            ("pair-a".to_string(), Some((2, 0))),
            ("pair-b".to_string(), Some((2, 0))),
            ("pair-c".to_string(), None),
            ("pair-a".to_string(), Some((2, 0))),
            ("pair-b".to_string(), Some((2, 0))),
            ("pair-c".to_string(), None),
        ]
    );
}

#[test]
fn separates_duplicate_groups_by_barcode_tag() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/barcode-tag/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: None,
        barcode_tag: Some("RX".to_string()),
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![0, 1024, 0, 0]);
}

#[test]
fn separates_duplicate_groups_by_read_one_and_read_two_barcode_tags() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/read-barcode-tags/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: None,
        barcode_tag: None,
        read_one_barcode_tag: Some("BX".to_string()),
        read_two_barcode_tag: Some("BY".to_string()),
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![99, 1123, 99, 147, 1171, 147]);
}

#[test]
fn tags_optical_duplicate_pairs_and_reports_metrics() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/optical/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: Some("All".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: Some(100),
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(
        duplicate_dt_tags(&output),
        vec![Some("SQ".to_string()), Some("SQ".to_string())]
    );
    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("lib1\t0\t2\t0\t0\t0\t1\t1\t0.5\t\n"));
    assert!(metrics_text.contains("BIN\tCoverageMult\tall_sets\tnon_optical_sets\n"));
}

#[test]
fn removes_only_optical_duplicates_when_requested() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/remove-sequencing-duplicates/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: true,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: None,
        tagging_policy: Some("All".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: Some(100),
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![99, 1123, 147, 1171]);
    assert_eq!(
        duplicate_dt_tags(&output),
        vec![Some("LB".to_string()), Some("LB".to_string())]
    );
    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("lib1\t0\t3\t0\t0\t0\t2\t1\t0.666667\t1\n"));
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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: true,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
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
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: Some(2500),
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

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
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: false,
        optical_duplicate_pixel_distance: Some(2500),
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert!(read_has_dt_tags(&output));
}

#[test]
fn marks_duplicate_pairs_across_multiple_bam_inputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/multi-input");
    let input1 = fixture_dir.join("input1.bam");
    let input2 = fixture_dir.join("input2.bam");
    let config = MarkDuplicatesConfig {
        input: input1.display().to_string(),
        inputs: vec![input1.display().to_string(), input2.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![99, 1123, 147, 1171]);
    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("lib1\t0\t2\t0\t0\t0\t1\t0\t0.5\t1\n"));
}

#[test]
fn keeps_duplicate_positions_separate_by_library() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/multi-library/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![99, 99, 147, 147]);
    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("libA\t0\t1\t0\t0\t0\t0\t0\t0\t\n"));
    assert!(metrics_text.contains("libB\t0\t1\t0\t0\t0\t0\t0\t0\t\n"));
}

#[test]
fn leaves_single_bam_unmarked_when_no_duplicates_are_present() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/multi-library/input.bam");
    let config = MarkDuplicatesConfig {
        input: input.display().to_string(),
        inputs: vec![input.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: false,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: false,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), read_flags(&input));
    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("libA\t0\t1\t0\t0\t0\t0\t0\t0\t\n"));
    assert!(metrics_text.contains("libB\t0\t1\t0\t0\t0\t0\t0\t0\t\n"));
}

#[test]
fn preserves_libraries_from_later_bam_inputs() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let output = tempdir.path().join("output.bam");
    let metrics = tempdir.path().join("metrics.txt");
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/multi-input-libraries");
    let input1 = fixture_dir.join("input1.bam");
    let input2 = fixture_dir.join("input2.bam");
    let config = MarkDuplicatesConfig {
        input: input1.display().to_string(),
        inputs: vec![input1.display().to_string(), input2.display().to_string()],
        output: output.display().to_string(),
        metrics_file: metrics.display().to_string(),
        max_records_in_ram: 500_000,
        tmp_dirs: Vec::new(),
        remove_duplicates: false,
        remove_sequencing_duplicates: false,
        assume_sorted: true,
        assume_sort_order: None,
        validation_stringency: Some("SILENT".to_string()),
        quiet: true,
        create_index: false,
        create_md5_file: false,
        add_pg_tag_to_reads: true,
        tag_duplicate_set_members: false,
        duplicate_scoring_strategy: None,
        read_name_regex: Some("null".to_string()),
        tagging_policy: Some("DontTag".to_string()),
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: None,
    };

    turbo_picard_markdup::run(&config).expect("BAM duplicate marking succeeds");

    assert_eq!(read_flags(&output), vec![99, 99, 147, 147]);
    assert!(header_text(&output).contains("@RG\tID:rgB\tLB:libB\tSM:sample1"));
    let metrics_text = std::fs::read_to_string(&metrics).expect("metrics file exists");
    assert!(metrics_text.contains("libA\t0\t1\t0\t0\t0\t0\t0\t0\t\n"));
    assert!(metrics_text.contains("libB\t0\t1\t0\t0\t0\t0\t0\t0\t\n"));
}

fn write_mate_unmapped_duplicate_fixture(path: &std::path::Path) {
    let mut header = bam::Header::new();
    header.push_record(
        HeaderRecord::new(b"SQ")
            .push_tag(b"SN", "chr1")
            .push_tag(b"LN", 1000),
    );
    header.push_record(
        HeaderRecord::new(b"RG")
            .push_tag(b"ID", "rg1")
            .push_tag(b"LB", "lib1")
            .push_tag(b"SM", "sample1"),
    );
    let mut writer =
        bam::Writer::from_path(path, &header, bam::Format::Bam).expect("fixture writer opens");
    let cigar = CigarString(vec![Cigar::Match(4)]);
    for (qname, flag, pos, seq) in [
        (b"frag-a".as_slice(), 0x449, 10_i64, b"ACGT".as_slice()),
        (b"frag-b".as_slice(), 0x49, 10_i64, b"ACGT".as_slice()),
        (b"frag-a".as_slice(), 0x4d, -1_i64, b"TGCA".as_slice()),
        (b"frag-b".as_slice(), 0x4d, -1_i64, b"TGCA".as_slice()),
    ] {
        let mut record = bam::Record::new();
        if flag & 0x4 == 0 {
            record.set(qname, Some(&cigar), seq, b"FFFF");
            record.set_tid(0);
            record.set_pos(pos);
            record.set_mapq(60);
        } else {
            record.set(qname, None, seq, b"FFFF");
            record.set_tid(-1);
            record.set_pos(-1);
            record.set_mapq(0);
        }
        record.set_flags(flag);
        record.set_mtid(-1);
        record.set_mpos(-1);
        record.set_insert_size(0);
        record
            .push_aux(b"RG", Aux::String("rg1"))
            .expect("RG tag can be written");
        writer.write(&record).expect("fixture record writes");
    }
}

fn write_q15_scoring_fixture(path: &std::path::Path) {
    let mut writer = duplicate_fixture_writer(path);
    for (qname, flag, pos, mate_pos, insert_size, seq, qual) in [
        (
            b"low-q-frag".as_slice(),
            0x63,
            10_i64,
            80_i64,
            74_i64,
            b"ACGTACGT".as_slice(),
            b"\x0e\x0e\x0e\x0e\x0e\x0e\x0e\x0e".as_slice(),
        ),
        (
            b"low-q-frag".as_slice(),
            0x93,
            80_i64,
            10_i64,
            -74_i64,
            b"TGCATGCA".as_slice(),
            b"\x0e\x0e\x0e\x0e\x0e\x0e\x0e\x0e".as_slice(),
        ),
        (
            b"q15-frag".as_slice(),
            0x63,
            10_i64,
            80_i64,
            74_i64,
            b"ACGTACGT".as_slice(),
            b"\x0f\x0f\x0f\x0f\0\0\0\0".as_slice(),
        ),
        (
            b"q15-frag".as_slice(),
            0x93,
            80_i64,
            10_i64,
            -74_i64,
            b"TGCATGCA".as_slice(),
            b"\x0f\x0f\x0f\x0f\0\0\0\0".as_slice(),
        ),
    ] {
        write_mapped_pair_record(
            &mut writer,
            qname,
            flag,
            pos,
            mate_pos,
            insert_size,
            seq,
            qual,
        );
    }
}

fn write_orphaned_pair_fixture(path: &std::path::Path) {
    let mut writer = duplicate_fixture_writer(path);
    for qname in [b"orphan-a".as_slice(), b"orphan-b".as_slice()] {
        write_mapped_pair_record(&mut writer, qname, 0x63, 10, 80, 74, b"ACGT", b"FFFF");
    }
}

fn duplicate_fixture_writer(path: &std::path::Path) -> bam::Writer {
    let mut header = bam::Header::new();
    header.push_record(
        HeaderRecord::new(b"SQ")
            .push_tag(b"SN", "chr1")
            .push_tag(b"LN", 1000),
    );
    header.push_record(
        HeaderRecord::new(b"RG")
            .push_tag(b"ID", "rg1")
            .push_tag(b"LB", "lib1")
            .push_tag(b"SM", "sample1"),
    );
    bam::Writer::from_path(path, &header, bam::Format::Bam).expect("fixture writer opens")
}

#[allow(clippy::too_many_arguments)]
fn write_mapped_pair_record(
    writer: &mut bam::Writer,
    qname: &[u8],
    flag: u16,
    pos: i64,
    mate_pos: i64,
    insert_size: i64,
    seq: &[u8],
    qual: &[u8],
) {
    let cigar = CigarString(vec![Cigar::Match(
        u32::try_from(seq.len()).expect("sequence length fits in CIGAR"),
    )]);
    let mut record = bam::Record::new();
    record.set(qname, Some(&cigar), seq, qual);
    record.set_tid(0);
    record.set_pos(pos);
    record.set_mapq(60);
    record.set_flags(flag);
    record.set_mtid(0);
    record.set_mpos(mate_pos);
    record.set_insert_size(insert_size);
    record
        .push_aux(b"RG", Aux::String("rg1"))
        .expect("RG tag can be written");
    writer.write(&record).expect("fixture record writes");
}

fn read_flags(path: &std::path::Path) -> Vec<u16> {
    let mut reader = bam::Reader::from_path(path).expect("BAM opens");
    reader
        .records()
        .map(|record| record.expect("record decodes").flags())
        .collect()
}

fn header_text(path: &std::path::Path) -> String {
    let reader = bam::Reader::from_path(path).expect("BAM opens");
    String::from_utf8_lossy(reader.header().as_bytes()).into_owned()
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

fn duplicate_dt_tags(path: &std::path::Path) -> Vec<Option<String>> {
    let mut reader = bam::Reader::from_path(path).expect("BAM opens");
    reader
        .records()
        .filter_map(|record| {
            let record = record.expect("record decodes");
            if record.flags() & 0x400 == 0 {
                return None;
            }
            let tag = match record.aux(b"DT") {
                Ok(Aux::String(value)) => Some(value.to_string()),
                _ => None,
            };
            Some(tag)
        })
        .collect()
}

fn duplicate_set_member_tags(path: &std::path::Path) -> Vec<(String, Option<(i32, i32)>)> {
    let mut reader = bam::Reader::from_path(path).expect("BAM opens");
    reader
        .records()
        .map(|record| {
            let record = record.expect("record decodes");
            let name = String::from_utf8_lossy(record.qname()).into_owned();
            let ds = match record.aux(b"DS") {
                Ok(Aux::I32(value)) => Some(value),
                _ => None,
            };
            let di = match record.aux(b"DI") {
                Ok(Aux::I32(value)) => Some(value),
                _ => None,
            };
            (name, ds.zip(di))
        })
        .collect()
}
