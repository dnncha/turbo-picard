use std::io::Write;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;
use turbo_picard_markdup::MarkDuplicatesError;

fn sam_config(
    input: &std::path::Path,
    output: &std::path::Path,
    metrics: &std::path::Path,
) -> MarkDuplicatesConfig {
    MarkDuplicatesConfig {
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
    }
}

#[test]
fn run_fails_fast_on_invalid_sam_position() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    let metrics = tempdir.path().join("metrics.txt");

    let mut file = std::fs::File::create(&input).expect("create sam");
    writeln!(file, "@SQ\tSN:chr1\tLN:100").expect("write header");
    writeln!(
        file,
        "read1\t0\tchr1\tbad\t255\t10M\t*\t0\t0\tACTGACTGAC\t*"
    )
    .expect("write read");

    let config = sam_config(&input, &output, &metrics);
    let error = turbo_picard_markdup::run(&config).expect_err("run should fail");
    let MarkDuplicatesError::MalformedSam {
        line_number,
        reason,
    } = error
    else {
        panic!("expected malformed SAM");
    };
    assert_eq!(line_number, 2);
    assert!(reason.contains("POS"));
}

#[test]
fn run_fails_fast_on_invalid_sam_cigar() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    let metrics = tempdir.path().join("metrics.txt");

    let mut file = std::fs::File::create(&input).expect("create sam");
    writeln!(file, "@SQ\tSN:chr1\tLN:100").expect("write header");
    writeln!(file, "read1\t0\tchr1\t1\t255\t10M5\t*\t0\t0\tACTGACTGAC\t*").expect("write read");

    let config = sam_config(&input, &output, &metrics);
    let error = turbo_picard_markdup::run(&config).expect_err("run should fail");
    let MarkDuplicatesError::MalformedSam {
        line_number,
        reason,
    } = error
    else {
        panic!("expected malformed SAM");
    };
    assert_eq!(line_number, 2);
    assert!(reason.contains("CIGAR"));
}

#[test]
fn run_fails_fast_on_invalid_sam_star_cigar() {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let input = tempdir.path().join("input.sam");
    let output = tempdir.path().join("output.sam");
    let metrics = tempdir.path().join("metrics.txt");

    let mut file = std::fs::File::create(&input).expect("create sam");
    writeln!(file, "@SQ\tSN:chr1\tLN:100").expect("write header");
    writeln!(file, "read1\t0\tchr1\t1\t255\t*\t*\t0\t0\tACTGACTGAC\t*").expect("write read");

    let config = sam_config(&input, &output, &metrics);
    let error = turbo_picard_markdup::run(&config).expect_err("run should fail");
    let MarkDuplicatesError::MalformedSam {
        line_number,
        reason,
    } = error
    else {
        panic!("expected malformed SAM");
    };
    assert_eq!(line_number, 2);
    assert!(reason.contains("CIGAR"));
}
