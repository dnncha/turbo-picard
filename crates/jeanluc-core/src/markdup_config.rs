use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkDuplicatesConfig {
    pub input: String,
    pub output: String,
    pub metrics_file: String,
    pub remove_duplicates: bool,
    pub assume_sorted: bool,
    pub assume_sort_order: Option<String>,
    pub validation_stringency: Option<String>,
    pub quiet: bool,
    pub create_index: bool,
    pub create_md5_file: bool,
    pub add_pg_tag_to_reads: bool,
    pub duplicate_scoring_strategy: Option<String>,
    pub read_name_regex: Option<String>,
    pub tagging_policy: Option<String>,
    pub clear_dt: bool,
    pub optical_duplicate_pixel_distance: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkDuplicatesConfigError {
    MissingRequired(&'static str),
    DuplicateScalar(String),
    InvalidBoolean { key: String, value: String },
    UnsupportedOption(String),
}

impl fmt::Display for MarkDuplicatesConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequired(key) => {
                write!(f, "missing required MarkDuplicates argument: {key}")
            }
            Self::DuplicateScalar(key) => {
                write!(f, "duplicate scalar MarkDuplicates argument: {key}")
            }
            Self::InvalidBoolean { key, value } => {
                write!(
                    f,
                    "invalid boolean for MarkDuplicates argument {key}: {value}"
                )
            }
            Self::UnsupportedOption(key) => {
                write!(f, "unsupported MarkDuplicates argument: {key}")
            }
        }
    }
}

impl std::error::Error for MarkDuplicatesConfigError {}

impl MarkDuplicatesConfig {
    pub fn try_from_args(
        args: &BTreeMap<String, Vec<String>>,
    ) -> Result<Self, MarkDuplicatesConfigError> {
        reject_unsupported(args)?;
        validate_passthrough_options(args)?;

        Ok(Self {
            input: required_scalar(args, "INPUT")?,
            output: required_scalar(args, "OUTPUT")?,
            metrics_file: required_scalar(args, "METRICS_FILE")?,
            remove_duplicates: optional_bool(args, "REMOVE_DUPLICATES")?.unwrap_or(false),
            assume_sorted: optional_bool(args, "ASSUME_SORTED")?.unwrap_or(false),
            assume_sort_order: optional_assume_sort_order(args)?,
            validation_stringency: optional_scalar(args, "VALIDATION_STRINGENCY")?,
            quiet: optional_bool(args, "QUIET")?.unwrap_or(false),
            create_index: optional_bool(args, "CREATE_INDEX")?.unwrap_or(false),
            create_md5_file: optional_bool(args, "CREATE_MD5_FILE")?.unwrap_or(false),
            add_pg_tag_to_reads: optional_bool(args, "ADD_PG_TAG_TO_READS")?.unwrap_or(true),
            duplicate_scoring_strategy: optional_duplicate_scoring_strategy(args)?,
            read_name_regex: optional_read_name_regex(args)?,
            tagging_policy: optional_tagging_policy(args)?,
            clear_dt: optional_bool(args, "CLEAR_DT")?.unwrap_or(true),
            optical_duplicate_pixel_distance: optional_u32(
                args,
                "OPTICAL_DUPLICATE_PIXEL_DISTANCE",
            )?,
        })
    }
}

fn reject_unsupported(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), MarkDuplicatesConfigError> {
    let supported = BTreeSet::from([
        "INPUT",
        "OUTPUT",
        "METRICS_FILE",
        "REMOVE_DUPLICATES",
        "REMOVE_SEQUENCING_DUPLICATES",
        "ASSUME_SORTED",
        "ASSUME_SORT_ORDER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "DUPLICATE_SCORING_STRATEGY",
        "READ_NAME_REGEX",
        "TAGGING_POLICY",
        "CLEAR_DT",
        "OPTICAL_DUPLICATE_PIXEL_DISTANCE",
        "MAX_RECORDS_IN_RAM",
        "MAX_FILE_HANDLES_FOR_READ_ENDS_MAP",
        "MAX_SEQUENCES_FOR_DISK_READ_ENDS_MAP",
        "SORTING_COLLECTION_SIZE_RATIO",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "VERBOSITY",
        "ADD_PG_TAG_TO_READS",
        "USE_JDK_INFLATER",
        "USE_JDK_DEFLATER",
        "PROGRAM_RECORD_ID",
        "PROGRAM_GROUP_NAME",
        "PROGRAM_GROUP_VERSION",
        "PROGRAM_GROUP_COMMAND_LINE",
        "REFERENCE_SEQUENCE",
        "COMMENT",
    ]);

    for key in args.keys() {
        if !supported.contains(key.as_str()) {
            return Err(MarkDuplicatesConfigError::UnsupportedOption(key.clone()));
        }
    }

    Ok(())
}

fn validate_passthrough_options(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), MarkDuplicatesConfigError> {
    optional_false_bool(args, "REMOVE_SEQUENCING_DUPLICATES")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_u32(args, "MAX_FILE_HANDLES_FOR_READ_ENDS_MAP")?;
    optional_u32(args, "MAX_SEQUENCES_FOR_DISK_READ_ENDS_MAP")?;
    optional_u32(args, "COMPRESSION_LEVEL")?;
    optional_f64(args, "SORTING_COLLECTION_SIZE_RATIO")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_scalar(args, "PROGRAM_RECORD_ID")?;
    optional_scalar(args, "PROGRAM_GROUP_NAME")?;
    optional_scalar(args, "PROGRAM_GROUP_VERSION")?;
    optional_scalar(args, "PROGRAM_GROUP_COMMAND_LINE")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    let _ = args.get("TMP_DIR");
    let _ = args.get("COMMENT");
    Ok(())
}

fn required_scalar(
    args: &BTreeMap<String, Vec<String>>,
    key: &'static str,
) -> Result<String, MarkDuplicatesConfigError> {
    let values = args
        .get(key)
        .ok_or(MarkDuplicatesConfigError::MissingRequired(key))?;

    scalar_value(values, key)
}

fn optional_bool(
    args: &BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<bool>, MarkDuplicatesConfigError> {
    let Some(values) = args.get(key) else {
        return Ok(None);
    };
    let value = scalar_value(values, key)?;

    match value.to_ascii_lowercase().as_str() {
        "true" => Ok(Some(true)),
        "false" => Ok(Some(false)),
        _ => Err(MarkDuplicatesConfigError::InvalidBoolean {
            key: key.to_string(),
            value,
        }),
    }
}

fn optional_false_bool(
    args: &BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<bool>, MarkDuplicatesConfigError> {
    let value = optional_bool(args, key)?;
    if value == Some(true) {
        Err(MarkDuplicatesConfigError::UnsupportedOption(format!(
            "{key}=true"
        )))
    } else {
        Ok(value)
    }
}

fn optional_scalar(
    args: &BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<String>, MarkDuplicatesConfigError> {
    let Some(values) = args.get(key) else {
        return Ok(None);
    };

    scalar_value(values, key).map(Some)
}

fn optional_duplicate_scoring_strategy(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<Option<String>, MarkDuplicatesConfigError> {
    let Some(strategy) = optional_scalar(args, "DUPLICATE_SCORING_STRATEGY")? else {
        return Ok(None);
    };

    if strategy == "SUM_OF_BASE_QUALITIES" {
        Ok(Some(strategy))
    } else {
        Err(MarkDuplicatesConfigError::UnsupportedOption(format!(
            "DUPLICATE_SCORING_STRATEGY={strategy}"
        )))
    }
}

fn optional_assume_sort_order(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<Option<String>, MarkDuplicatesConfigError> {
    let Some(value) = optional_scalar(args, "ASSUME_SORT_ORDER")? else {
        return Ok(None);
    };

    if value == "coordinate" {
        Ok(Some(value))
    } else {
        Err(MarkDuplicatesConfigError::UnsupportedOption(format!(
            "ASSUME_SORT_ORDER={value}"
        )))
    }
}

fn optional_read_name_regex(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<Option<String>, MarkDuplicatesConfigError> {
    let Some(value) = optional_scalar(args, "READ_NAME_REGEX")? else {
        return Ok(None);
    };

    if value == "null" {
        Ok(Some(value))
    } else {
        Err(MarkDuplicatesConfigError::UnsupportedOption(format!(
            "READ_NAME_REGEX={value}"
        )))
    }
}

fn optional_tagging_policy(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<Option<String>, MarkDuplicatesConfigError> {
    let Some(value) = optional_scalar(args, "TAGGING_POLICY")? else {
        return Ok(None);
    };

    if matches!(value.as_str(), "All" | "OpticalOnly" | "DontTag") {
        Ok(Some(value))
    } else {
        Err(MarkDuplicatesConfigError::UnsupportedOption(format!(
            "TAGGING_POLICY={value}"
        )))
    }
}

fn optional_u32(
    args: &BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<u32>, MarkDuplicatesConfigError> {
    let Some(value) = optional_scalar(args, key)? else {
        return Ok(None);
    };

    value
        .parse::<u32>()
        .map(Some)
        .map_err(|_| MarkDuplicatesConfigError::UnsupportedOption(format!("{key}={value}")))
}

fn optional_f64(
    args: &BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<f64>, MarkDuplicatesConfigError> {
    let Some(value) = optional_scalar(args, key)? else {
        return Ok(None);
    };

    value
        .parse::<f64>()
        .map(Some)
        .map_err(|_| MarkDuplicatesConfigError::UnsupportedOption(format!("{key}={value}")))
}

fn scalar_value(values: &[String], key: &str) -> Result<String, MarkDuplicatesConfigError> {
    if values.len() != 1 {
        return Err(MarkDuplicatesConfigError::DuplicateScalar(key.to_string()));
    }

    Ok(values[0].clone())
}
