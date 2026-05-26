use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkDuplicatesConfig {
    pub input: String,
    pub output: String,
    pub metrics_file: String,
    pub remove_duplicates: bool,
    pub assume_sorted: bool,
    pub validation_stringency: Option<String>,
    pub quiet: bool,
    pub create_index: bool,
    pub create_md5_file: bool,
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

        Ok(Self {
            input: required_scalar(args, "INPUT")?,
            output: required_scalar(args, "OUTPUT")?,
            metrics_file: required_scalar(args, "METRICS_FILE")?,
            remove_duplicates: optional_bool(args, "REMOVE_DUPLICATES")?.unwrap_or(false),
            assume_sorted: optional_bool(args, "ASSUME_SORTED")?.unwrap_or(false),
            validation_stringency: optional_scalar(args, "VALIDATION_STRINGENCY")?,
            quiet: optional_bool(args, "QUIET")?.unwrap_or(false),
            create_index: optional_bool(args, "CREATE_INDEX")?.unwrap_or(false),
            create_md5_file: optional_bool(args, "CREATE_MD5_FILE")?.unwrap_or(false),
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
        "ASSUME_SORTED",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "DUPLICATE_SCORING_STRATEGY",
        "READ_NAME_REGEX",
        "TAGGING_POLICY",
        "CLEAR_DT",
        "OPTICAL_DUPLICATE_PIXEL_DISTANCE",
    ]);

    for key in args.keys() {
        if !supported.contains(key.as_str()) {
            return Err(MarkDuplicatesConfigError::UnsupportedOption(key.clone()));
        }
    }

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

    if value == "DontTag" {
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

fn scalar_value(values: &[String], key: &str) -> Result<String, MarkDuplicatesConfigError> {
    if values.len() != 1 {
        return Err(MarkDuplicatesConfigError::DuplicateScalar(key.to_string()));
    }

    Ok(values[0].clone())
}
