use jeanluc_core::markdup_config::{MarkDuplicatesConfig, MarkDuplicatesConfigError};
use jeanluc_core::picard_args::normalize_picard_args;

#[test]
fn accepts_minimal_required_picard_arguments() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert_eq!(config.input, "in.bam");
    assert_eq!(config.inputs, vec!["in.bam".to_string()]);
    assert_eq!(config.output, "out.bam");
    assert_eq!(config.metrics_file, "metrics.txt");
    assert!(!config.remove_duplicates);
}

#[test]
fn parses_remove_duplicates_boolean() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "REMOVE_DUPLICATES=true".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert!(config.remove_duplicates);
}

#[test]
fn accepts_common_picard_runtime_options() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "ASSUME_SORTED=true".to_string(),
        "VALIDATION_STRINGENCY=SILENT".to_string(),
        "QUIET=true".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert!(config.assume_sorted);
    assert_eq!(config.validation_stringency.as_deref(), Some("SILENT"));
    assert!(config.quiet);
}

#[test]
fn add_pg_tag_to_reads_defaults_true_and_parses_false() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");
    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");
    assert!(config.add_pg_tag_to_reads);

    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "ADD_PG_TAG_TO_READS=false".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");
    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");
    assert!(!config.add_pg_tag_to_reads);
}

#[test]
fn parses_create_index_boolean() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "CREATE_INDEX=true".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert!(config.create_index);
}

#[test]
fn parses_create_md5_file_boolean() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "CREATE_MD5_FILE=true".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert!(config.create_md5_file);
}

#[test]
fn accepts_explicit_default_duplicate_scoring_strategy() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "DUPLICATE_SCORING_STRATEGY=SUM_OF_BASE_QUALITIES".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert_eq!(
        config.duplicate_scoring_strategy.as_deref(),
        Some("SUM_OF_BASE_QUALITIES")
    );
}

#[test]
fn accepts_common_duplicate_tagging_options() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "READ_NAME_REGEX=null".to_string(),
        "TAGGING_POLICY=All".to_string(),
        "TAG_DUPLICATE_SET_MEMBERS=true".to_string(),
        "BARCODE_TAG=RX".to_string(),
        "READ_ONE_BARCODE_TAG=BX".to_string(),
        "READ_TWO_BARCODE_TAG=BY".to_string(),
        "CLEAR_DT=true".to_string(),
        "OPTICAL_DUPLICATE_PIXEL_DISTANCE=2500".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert_eq!(config.read_name_regex.as_deref(), Some("null"));
    assert_eq!(config.tagging_policy.as_deref(), Some("All"));
    assert!(config.tag_duplicate_set_members);
    assert_eq!(config.barcode_tag.as_deref(), Some("RX"));
    assert_eq!(config.read_one_barcode_tag.as_deref(), Some("BX"));
    assert_eq!(config.read_two_barcode_tag.as_deref(), Some("BY"));
    assert!(config.clear_dt);
    assert_eq!(config.optical_duplicate_pixel_distance, Some(2500));
}

#[test]
fn accepts_common_workflow_passthrough_options_when_semantically_default() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "ASSUME_SORT_ORDER=coordinate".to_string(),
        "REMOVE_SEQUENCING_DUPLICATES=false".to_string(),
        "MAX_RECORDS_IN_RAM=1000000".to_string(),
        "MAX_FILE_HANDLES_FOR_READ_ENDS_MAP=4000".to_string(),
        "SORTING_COLLECTION_SIZE_RATIO=0.15".to_string(),
        "COMPRESSION_LEVEL=1".to_string(),
        "TMP_DIR=/tmp".to_string(),
        "TMP_DIR=/scratch".to_string(),
        "VERBOSITY=ERROR".to_string(),
        "ADD_PG_TAG_TO_READS=true".to_string(),
        "USE_JDK_INFLATER=false".to_string(),
        "USE_JDK_DEFLATER=false".to_string(),
        "PROGRAM_RECORD_ID=MarkDuplicates".to_string(),
        "PROGRAM_GROUP_NAME=MarkDuplicates".to_string(),
        "PROGRAM_GROUP_VERSION=1.0.0".to_string(),
        "PROGRAM_GROUP_COMMAND_LINE=picard MarkDuplicates I=in.bam O=out.bam M=metrics.txt"
            .to_string(),
        "REFERENCE_SEQUENCE=reference.fa".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert_eq!(config.assume_sort_order.as_deref(), Some("coordinate"));
}

#[test]
fn rejects_missing_metrics_file() {
    let args = vec!["I=in.bam".to_string(), "O=out.bam".to_string()];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let err = MarkDuplicatesConfig::try_from_args(&parsed).unwrap_err();

    assert_eq!(
        err,
        MarkDuplicatesConfigError::MissingRequired("METRICS_FILE")
    );
}

#[test]
fn accepts_multiple_input_values() {
    let args = vec![
        "I=in.bam".to_string(),
        "I=second.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let config = MarkDuplicatesConfig::try_from_args(&parsed).expect("config validates");

    assert_eq!(
        config.inputs,
        vec!["in.bam".to_string(), "second.bam".to_string()]
    );
    assert_eq!(config.input, "in.bam");
}

#[test]
fn rejects_invalid_boolean() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "REMOVE_DUPLICATES=maybe".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let err = MarkDuplicatesConfig::try_from_args(&parsed).unwrap_err();

    assert_eq!(
        err,
        MarkDuplicatesConfigError::InvalidBoolean {
            key: "REMOVE_DUPLICATES".to_string(),
            value: "maybe".to_string()
        }
    );
}

#[test]
fn rejects_unsupported_options() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
        "TAGGING_POLICY=Invalid".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let err = MarkDuplicatesConfig::try_from_args(&parsed).unwrap_err();

    assert_eq!(
        err,
        MarkDuplicatesConfigError::UnsupportedOption("TAGGING_POLICY=Invalid".to_string())
    );
}
