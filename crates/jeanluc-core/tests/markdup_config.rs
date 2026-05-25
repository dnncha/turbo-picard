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
fn rejects_duplicate_scalar_values() {
    let args = vec![
        "I=in.bam".to_string(),
        "I=second.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let err = MarkDuplicatesConfig::try_from_args(&parsed).unwrap_err();

    assert_eq!(
        err,
        MarkDuplicatesConfigError::DuplicateScalar("INPUT".to_string())
    );
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
        "OPTICAL_DUPLICATE_PIXEL_DISTANCE=2500".to_string(),
    ];
    let parsed = normalize_picard_args(&args).expect("arguments parse");

    let err = MarkDuplicatesConfig::try_from_args(&parsed).unwrap_err();

    assert_eq!(
        err,
        MarkDuplicatesConfigError::UnsupportedOption(
            "OPTICAL_DUPLICATE_PIXEL_DISTANCE".to_string()
        )
    );
}
