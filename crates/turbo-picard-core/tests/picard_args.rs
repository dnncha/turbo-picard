use turbo_picard_core::picard_args::{PicardArgError, normalize_picard_args};

#[test]
fn normalizes_key_value_arguments() {
    let args = vec![
        "I=in.bam".to_string(),
        "O=out.bam".to_string(),
        "M=metrics.txt".to_string(),
    ];

    let parsed = normalize_picard_args(&args).expect("arguments parse");

    assert_eq!(parsed.get("INPUT").unwrap(), &vec!["in.bam".to_string()]);
    assert_eq!(parsed.get("OUTPUT").unwrap(), &vec!["out.bam".to_string()]);
    assert_eq!(
        parsed.get("METRICS_FILE").unwrap(),
        &vec!["metrics.txt".to_string()]
    );
}

#[test]
fn normalizes_long_options() {
    let args = vec![
        "--INPUT".to_string(),
        "in.bam".to_string(),
        "--OUTPUT=out.bam".to_string(),
        "--METRICS_FILE".to_string(),
        "metrics.txt".to_string(),
    ];

    let parsed = normalize_picard_args(&args).expect("arguments parse");

    assert_eq!(parsed.get("INPUT").unwrap(), &vec!["in.bam".to_string()]);
    assert_eq!(parsed.get("OUTPUT").unwrap(), &vec!["out.bam".to_string()]);
    assert_eq!(
        parsed.get("METRICS_FILE").unwrap(),
        &vec!["metrics.txt".to_string()]
    );
}

#[test]
fn normalizes_short_picard_options() {
    let args = vec![
        "-I".to_string(),
        "in.bam".to_string(),
        "-O".to_string(),
        "out.bam".to_string(),
        "-M".to_string(),
        "metrics.txt".to_string(),
    ];

    let parsed = normalize_picard_args(&args).expect("arguments parse");

    assert_eq!(parsed.get("INPUT").unwrap(), &vec!["in.bam".to_string()]);
    assert_eq!(parsed.get("OUTPUT").unwrap(), &vec!["out.bam".to_string()]);
    assert_eq!(
        parsed.get("METRICS_FILE").unwrap(),
        &vec!["metrics.txt".to_string()]
    );
}

#[test]
fn normalizes_common_markduplicates_short_aliases() {
    let args = vec![
        "-AS".to_string(),
        "true".to_string(),
        "-ASO".to_string(),
        "coordinate".to_string(),
        "-DS".to_string(),
        "SUM_OF_BASE_QUALITIES".to_string(),
        "-PG".to_string(),
        "null".to_string(),
        "-R".to_string(),
        "reference.fa".to_string(),
    ];

    let parsed = normalize_picard_args(&args).expect("arguments parse");

    assert_eq!(
        parsed.get("ASSUME_SORTED").unwrap(),
        &vec!["true".to_string()]
    );
    assert_eq!(
        parsed.get("ASSUME_SORT_ORDER").unwrap(),
        &vec!["coordinate".to_string()]
    );
    assert_eq!(
        parsed.get("DUPLICATE_SCORING_STRATEGY").unwrap(),
        &vec!["SUM_OF_BASE_QUALITIES".to_string()]
    );
    assert_eq!(
        parsed.get("PROGRAM_RECORD_ID").unwrap(),
        &vec!["null".to_string()]
    );
    assert_eq!(
        parsed.get("REFERENCE_SEQUENCE").unwrap(),
        &vec!["reference.fa".to_string()]
    );
}

#[test]
fn rejects_positional_arguments() {
    let args = vec!["in.bam".to_string()];

    let err = normalize_picard_args(&args).unwrap_err();

    assert_eq!(
        err,
        PicardArgError::UnexpectedPositional("in.bam".to_string())
    );
}

#[test]
fn rejects_long_option_without_value() {
    let args = vec!["--INPUT".to_string()];

    let err = normalize_picard_args(&args).unwrap_err();

    assert_eq!(err, PicardArgError::MissingValue("INPUT".to_string()));
}
