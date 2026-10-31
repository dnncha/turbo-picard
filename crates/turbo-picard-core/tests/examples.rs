use turbo_picard_core::external_sort::{ExternalSortConfig, ExternalSorter};
use turbo_picard_core::hts_io::output_format_for_path;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;
use turbo_picard_core::picard_args::{normalize_picard_args, normalize_picard_args_for_command};

#[test]
fn example_build_a_markduplicates_config_from_picard_arguments() {
    let args = [
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "REMOVE_DUPLICATES=true".to_string(),
    ];

    let parsed = normalize_picard_args_for_command("MarkDuplicates", &args).expect("arguments");
    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config");

    assert_eq!(config.input, "in.bam");
    assert_eq!(config.inputs, vec!["in.bam".to_string()]);
    assert_eq!(config.output, "out.bam");
    assert_eq!(config.metrics_file, "metrics.txt");
    assert!(config.remove_duplicates);
    assert!(config.add_pg_tag_to_reads, "Picard's default is true");
}

#[test]
fn example_multiple_inputs_preserve_argument_order() {
    let args = [
        "I=first.bam".to_string(),
        "I=second.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
    ];

    let parsed = normalize_picard_args_for_command("MarkDuplicates", &args).expect("arguments");
    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config");

    assert_eq!(
        config.inputs,
        vec!["first.bam".to_string(), "second.bam".to_string()]
    );
}

#[test]
fn example_short_aliases_are_scoped_to_the_command() {
    let insert_size =
        normalize_picard_args_for_command("CollectInsertSizeMetrics", &["M=0.05".to_string()])
            .expect("arguments");
    let mark_duplicates =
        normalize_picard_args_for_command("MarkDuplicates", &["M=metrics.txt".to_string()])
            .expect("arguments");

    assert_eq!(insert_size["MINIMUM_PCT"], vec!["0.05".to_string()]);
    assert_eq!(
        mark_duplicates["METRICS_FILE"],
        vec!["metrics.txt".to_string()]
    );
}

#[test]
fn example_generic_normalization_is_not_command_aware() {
    let parsed =
        normalize_picard_args(&["-I".to_string(), "in.bam".to_string()]).expect("arguments");

    assert_eq!(parsed["INPUT"], vec!["in.bam".to_string()]);
}

#[test]
fn example_stream_a_stable_external_sort() {
    let dir = tempfile::tempdir().expect("tempdir exists");
    let mut sorter =
        ExternalSorter::new(ExternalSortConfig::new(dir.path())).expect("sorter starts");

    sorter
        .push(b"b".to_vec(), b"second".to_vec())
        .expect("buffered");
    sorter
        .push(b"a".to_vec(), b"first".to_vec())
        .expect("buffered");

    let mut payloads = Vec::new();
    let metrics = sorter
        .finish_into(|item| {
            payloads.push(String::from_utf8(item.payload).expect("utf-8 payload"));
            Ok(())
        })
        .expect("sort completes");

    assert_eq!(payloads, vec!["first".to_string(), "second".to_string()]);
    assert_eq!(metrics.run_count, 1);
    assert_eq!(metrics.spills, 0);
}

#[test]
fn example_select_an_output_format_from_the_path() {
    assert!(output_format_for_path("out.bam", "MarkDuplicates").is_ok());
    assert!(output_format_for_path("out.sam", "MarkDuplicates").is_ok());

    let error = output_format_for_path("out.vcf", "MarkDuplicates").expect_err("unsupported");
    assert_eq!(
        error.to_string(),
        "unsupported MarkDuplicates output format for out.vcf; expected .sam, .bam, or .cram"
    );
}
