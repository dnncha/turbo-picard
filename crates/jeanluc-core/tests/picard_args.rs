use jeanluc_core::picard_args::{PicardArgError, normalize_picard_args};

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
