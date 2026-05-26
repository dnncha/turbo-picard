use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PicardArgError {
    EmptyKey(String),
    MissingValue(String),
    UnexpectedPositional(String),
}

impl fmt::Display for PicardArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyKey(arg) => write!(f, "empty Picard argument key in {arg}"),
            Self::MissingValue(key) => write!(f, "missing value for Picard argument: {key}"),
            Self::UnexpectedPositional(arg) => {
                write!(f, "unexpected positional Picard argument: {arg}")
            }
        }
    }
}

impl std::error::Error for PicardArgError {}

pub type PicardArgs = BTreeMap<String, Vec<String>>;

pub fn normalize_picard_args(args: &[String]) -> Result<PicardArgs, PicardArgError> {
    let mut normalized = BTreeMap::new();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];

        if let Some(long) = arg.strip_prefix("--") {
            if long.is_empty() {
                return Err(PicardArgError::EmptyKey(arg.clone()));
            }

            if let Some((key, value)) = long.split_once('=') {
                push_arg(&mut normalized, key, value)?;
                index += 1;
                continue;
            }

            let key = canonical_key(long)?;
            let value = args
                .get(index + 1)
                .ok_or_else(|| PicardArgError::MissingValue(key.clone()))?;

            if value.starts_with("--") || (value.contains('=') && !value.starts_with('=')) {
                return Err(PicardArgError::MissingValue(key));
            }

            normalized.entry(key).or_default().push(value.clone());
            index += 2;
            continue;
        }

        if let Some(short) = arg.strip_prefix('-') {
            if short.is_empty() {
                return Err(PicardArgError::EmptyKey(arg.clone()));
            }

            let key = canonical_key(short)?;
            let value = args
                .get(index + 1)
                .ok_or_else(|| PicardArgError::MissingValue(key.clone()))?;

            if value.starts_with('-') || (value.contains('=') && !value.starts_with('=')) {
                return Err(PicardArgError::MissingValue(key));
            }

            normalized.entry(key).or_default().push(value.clone());
            index += 2;
            continue;
        }

        if let Some((key, value)) = arg.split_once('=') {
            push_arg(&mut normalized, key, value)?;
            index += 1;
            continue;
        }

        return Err(PicardArgError::UnexpectedPositional(arg.clone()));
    }

    Ok(normalized)
}

fn push_arg(args: &mut PicardArgs, key: &str, value: &str) -> Result<(), PicardArgError> {
    let key = canonical_key(key)?;
    args.entry(key).or_default().push(value.to_string());
    Ok(())
}

fn canonical_key(key: &str) -> Result<String, PicardArgError> {
    if key.is_empty() {
        return Err(PicardArgError::EmptyKey(key.to_string()));
    }

    let upper = key.to_ascii_uppercase();
    let canonical = match upper.as_str() {
        "I" => "INPUT",
        "O" => "OUTPUT",
        "M" => "METRICS_FILE",
        "SO" => "SORT_ORDER",
        "AS" => "ASSUME_SORTED",
        "ASO" => "ASSUME_SORT_ORDER",
        "DS" => "DUPLICATE_SCORING_STRATEGY",
        "MAX_FILE_HANDLES" => "MAX_FILE_HANDLES_FOR_READ_ENDS_MAP",
        "MAX_SEQS" => "MAX_SEQUENCES_FOR_DISK_READ_ENDS_MAP",
        "PG" => "PROGRAM_RECORD_ID",
        "PG_COMMAND" => "PROGRAM_GROUP_COMMAND_LINE",
        "PG_NAME" => "PROGRAM_GROUP_NAME",
        "PG_VERSION" => "PROGRAM_GROUP_VERSION",
        "R" => "REFERENCE_SEQUENCE",
        "CO" => "COMMENT",
        _ => upper.as_str(),
    };

    Ok(canonical.to_string())
}
