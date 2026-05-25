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

fn read_flags(path: &std::path::Path) -> Vec<u16> {
    let mut reader = bam::Reader::from_path(path).expect("BAM opens");
    reader
        .records()
        .map(|record| record.expect("record decodes").flags())
        .collect()
}
