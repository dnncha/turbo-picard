use rust_htslib::bam::{self, Read};
use std::path::Path;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;
use turbo_picard_markdup::run;

fn reference_fasta() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/reference/chr1.fa")
}

fn bam_to_cram(input_bam: &Path, output_cram: &Path, reference: &Path) {
    let mut reader = bam::Reader::from_path(input_bam).expect("input bam opens");
    let header = bam::Header::from_template(reader.header());
    let mut writer =
        bam::Writer::from_path(output_cram, &header, bam::Format::Cram).expect("output cram opens");
    writer
        .set_reference(reference)
        .expect("cram writer reference is set");
    for record in reader.records() {
        let record = record.expect("record reads");
        writer.write(&record).expect("record writes");
    }
}

#[test]
fn marks_duplicates_on_cram_input_and_output() {
    marks_duplicates_on_cram_input_and_output_with_regex(None);
}

#[test]
fn marks_duplicates_on_cram_input_and_output_with_external_plan() {
    marks_duplicates_on_cram_input_and_output_with_regex(Some("null"));
}

fn marks_duplicates_on_cram_input_and_output_with_regex(read_name_regex: Option<&str>) {
    let tempdir = tempfile::tempdir().expect("tempdir exists");
    let reference = reference_fasta();
    let input_bam = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/markduplicates/paired/input.bam");
    let input_cram = tempdir.path().join("input.cram");
    let output_cram = tempdir.path().join("output.cram");
    let metrics = tempdir.path().join("metrics.txt");
    bam_to_cram(&input_bam, &input_cram, &reference);

    let config = MarkDuplicatesConfig {
        input: input_cram.display().to_string(),
        inputs: vec![input_cram.display().to_string()],
        output: output_cram.display().to_string(),
        metrics_file: metrics.display().to_string(),
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
        read_name_regex: read_name_regex.map(str::to_string),
        tagging_policy: None,
        barcode_tag: None,
        read_one_barcode_tag: None,
        read_two_barcode_tag: None,
        clear_dt: true,
        optical_duplicate_pixel_distance: None,
        compression_level: None,
        reference_sequence: Some(reference.display().to_string()),
        tmp_dir: None,
    };

    run(&config).expect("CRAM duplicate marking succeeds");
    assert!(output_cram.exists());
}
