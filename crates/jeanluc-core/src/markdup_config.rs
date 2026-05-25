use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkDuplicatesConfig {
    pub input: String,
    pub output: String,
    pub metrics_file: String,
    pub remove_duplicates: bool,
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
        })
    }
}

fn reject_unsupported(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), MarkDuplicatesConfigError> {
    let supported = BTreeSet::from(["INPUT", "OUTPUT", "METRICS_FILE", "REMOVE_DUPLICATES"]);

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

fn scalar_value(values: &[String], key: &str) -> Result<String, MarkDuplicatesConfigError> {
    if values.len() != 1 {
        return Err(MarkDuplicatesConfigError::DuplicateScalar(key.to_string()));
    }

    Ok(values[0].clone())
}
