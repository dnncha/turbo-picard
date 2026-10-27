//! Picard-style `KEY=VALUE` argument normalisation.
//!
//! Picard accepts short aliases (`-I`), long options (`--INPUT`), and bare
//! key/value pairs (`INPUT=in.bam`). Some short names mean different fields in
//! different commands, so command-aware normalisation exists alongside the
//! generic form.
//!
//! # Example
//!
//! ```
//! use turbo_picard_core::picard_args::normalize_picard_args;
//!
//! let args = ["I=in.bam".to_string(), "O=out.bam".to_string()];
//! let parsed = normalize_picard_args(&args)?;
//!
//! assert_eq!(parsed["INPUT"], vec!["in.bam".to_string()]);
//! assert_eq!(parsed["OUTPUT"], vec!["out.bam".to_string()]);
//! # Ok::<(), turbo_picard_core::picard_args::PicardArgError>(())
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::ops::Deref;

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

pub type PicardArgMap = BTreeMap<String, Vec<String>>;

/// Normalized Picard arguments with the command that produced them.
///
/// The command travels with the values so every extraction error can name the
/// command it belongs to. Callers keep reading the map through [`Deref`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PicardArgs {
    command: String,
    values: PicardArgMap,
}

impl PicardArgs {
    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn typed(&self) -> CommandArgs<'_> {
        self.typed_for(&self.command)
    }

    /// Read these arguments as if they belonged to `command`.
    ///
    /// The override keeps explicit command names working for shared helpers
    /// without giving up typed extraction.
    pub fn typed_for<'b>(&'b self, command: &'b str) -> CommandArgs<'b> {
        CommandArgs {
            command,
            args: self,
        }
    }

    pub fn reject_unknown(&self, allowed: &[&str]) -> Result<(), CommandArgError> {
        self.typed().reject_unknown(allowed)
    }

    pub fn into_values(self) -> PicardArgMap {
        self.values
    }
}

impl Deref for PicardArgs {
    type Target = PicardArgMap;

    fn deref(&self) -> &Self::Target {
        &self.values
    }
}

/// Normalise the generic Picard aliases into canonical argument names.
///
/// Repeated keys accumulate values in input order, and parsed keys are sorted
/// because the result keeps a [`BTreeMap`]. Use
/// [`normalize_picard_args_for_command`] when a short alias is ambiguous.
///
/// # Example
///
/// ```
/// use turbo_picard_core::picard_args::normalize_picard_args;
///
/// let args = ["-I".to_string(), "in.bam".to_string(), "M=metrics.txt".to_string()];
/// let parsed = normalize_picard_args(&args)?;
///
/// assert_eq!(parsed["INPUT"], vec!["in.bam".to_string()]);
/// assert_eq!(parsed["METRICS_FILE"], vec!["metrics.txt".to_string()]);
/// # Ok::<(), turbo_picard_core::picard_args::PicardArgError>(())
/// ```
pub fn normalize_picard_args(args: &[String]) -> Result<PicardArgs, PicardArgError> {
    normalize_picard_args_with_aliases("Picard", args, canonical_key)
}

/// Normalise aliases using the option table for one Picard command.
///
/// The same short name can refer to different fields: `M` is `METRICS_FILE`
/// for `MarkDuplicates` but `MINIMUM_PCT` for `CollectInsertSizeMetrics`.
///
/// # Example
///
/// ```
/// use turbo_picard_core::picard_args::normalize_picard_args_for_command;
///
/// let args = ["M=0.05".to_string()];
/// let parsed = normalize_picard_args_for_command("CollectInsertSizeMetrics", &args)?;
/// assert_eq!(parsed["MINIMUM_PCT"], vec!["0.05".to_string()]);
///
/// let args = ["M=metrics.txt".to_string()];
/// let parsed = normalize_picard_args_for_command("MarkDuplicates", &args)?;
/// assert_eq!(parsed["METRICS_FILE"], vec!["metrics.txt".to_string()]);
/// # Ok::<(), turbo_picard_core::picard_args::PicardArgError>(())
/// ```
pub fn normalize_picard_args_for_command(
    command: &str,
    args: &[String],
) -> Result<PicardArgs, PicardArgError> {
    normalize_picard_args_with_aliases(command, args, |key| canonical_key_for_command(command, key))
}

fn normalize_picard_args_with_aliases(
    command: &str,
    args: &[String],
    canonicalize: impl Fn(&str) -> Result<String, PicardArgError>,
) -> Result<PicardArgs, PicardArgError> {
    let mut normalized = BTreeMap::new();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];

        if let Some(long) = arg.strip_prefix("--") {
            if long.is_empty() {
                return Err(PicardArgError::EmptyKey(arg.clone()));
            }

            if let Some((key, value)) = long.split_once('=') {
                push_arg_with_aliases(&mut normalized, key, value, &canonicalize)?;
                index += 1;
                continue;
            }

            let key = canonicalize(long)?;
            let value = args
                .get(index + 1)
                .ok_or_else(|| PicardArgError::MissingValue(key.clone()))?;

            if looks_like_flag_value(value) || (value.contains('=') && !value.starts_with('=')) {
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

            let key = canonicalize(short)?;
            let value = args
                .get(index + 1)
                .ok_or_else(|| PicardArgError::MissingValue(key.clone()))?;

            if looks_like_flag_value(value) || (value.contains('=') && !value.starts_with('=')) {
                return Err(PicardArgError::MissingValue(key));
            }

            normalized.entry(key).or_default().push(value.clone());
            index += 2;
            continue;
        }

        if let Some((key, value)) = arg.split_once('=') {
            push_arg_with_aliases(&mut normalized, key, value, &canonicalize)?;
            index += 1;
            continue;
        }

        return Err(PicardArgError::UnexpectedPositional(arg.clone()));
    }

    Ok(PicardArgs {
        command: command.to_string(),
        values: normalized,
    })
}

fn looks_like_flag_value(value: &str) -> bool {
    value.len() > 1 && value.starts_with('-') && !matches!(value.as_bytes()[1], b'.' | b'0'..=b'9')
}

fn push_arg_with_aliases(
    args: &mut PicardArgMap,
    key: &str,
    value: &str,
    canonicalize: &impl Fn(&str) -> Result<String, PicardArgError>,
) -> Result<(), PicardArgError> {
    let key = canonicalize(key)?;
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

fn canonical_key_for_command(command: &str, key: &str) -> Result<String, PicardArgError> {
    if key.is_empty() {
        return Err(PicardArgError::EmptyKey(key.to_string()));
    }

    let upper = key.to_ascii_uppercase();
    let canonical = match (command, upper.as_str()) {
        (_, "I") => "INPUT",
        (_, "O") => "OUTPUT",
        (_, "R") => "REFERENCE_SEQUENCE",
        (_, "SO") => "SORT_ORDER",
        (_, "CO") => "COMMENT",
        ("MarkDuplicates", "M") => "METRICS_FILE",
        ("MarkDuplicates", "AS") => "ASSUME_SORTED",
        ("MarkDuplicates", "ASO") => "ASSUME_SORT_ORDER",
        ("MarkDuplicates", "DS") => "DUPLICATE_SCORING_STRATEGY",
        ("MarkDuplicates", "MAX_FILE_HANDLES") => "MAX_FILE_HANDLES_FOR_READ_ENDS_MAP",
        ("MarkDuplicates", "MAX_SEQS") => "MAX_SEQUENCES_FOR_DISK_READ_ENDS_MAP",
        ("MarkDuplicates", "PG") => "PROGRAM_RECORD_ID",
        ("MarkDuplicates", "PG_COMMAND") => "PROGRAM_GROUP_COMMAND_LINE",
        ("MarkDuplicates", "PG_NAME") => "PROGRAM_GROUP_NAME",
        ("MarkDuplicates", "PG_VERSION") => "PROGRAM_GROUP_VERSION",
        ("SortSam" | "MergeSamFiles", "AS") => "ASSUME_SORTED",
        ("CollectAlignmentSummaryMetrics", "AS") => "ASSUME_SORTED",
        ("CollectAlignmentSummaryMetrics", "LEVEL") => "METRIC_ACCUMULATION_LEVEL",
        ("CollectBaseDistributionByCycle", "AS") => "ASSUME_SORTED",
        ("CollectBaseDistributionByCycle", "CHART") => "CHART_OUTPUT",
        ("CollectGcBiasMetrics", "AS") => "ASSUME_SORTED",
        ("CollectGcBiasMetrics", "CHART") => "CHART_OUTPUT",
        ("CollectGcBiasMetrics", "S") => "SUMMARY_OUTPUT",
        ("CollectGcBiasMetrics", "WINDOW_SIZE") => "SCAN_WINDOW_SIZE",
        ("CollectGcBiasMetrics", "MGF") => "MINIMUM_GENOME_FRACTION",
        ("CollectGcBiasMetrics", "BS") => "IS_BISULFITE_SEQUENCED",
        ("CollectHsMetrics", "BAIT" | "BI") => "BAIT_INTERVALS",
        ("CollectHsMetrics", "TARGET") => "TARGET_INTERVALS",
        ("CollectHsMetrics", "N") => "BAIT_SET_NAME",
        ("CollectHsMetrics", "MQ") => "MINIMUM_MAPPING_QUALITY",
        ("CollectHsMetrics", "Q") => "MINIMUM_BASE_QUALITY",
        ("CollectHsMetrics", "CAP") => "COVERAGE_CAP",
        ("CollectHsMetrics", "AS") => "ASSUME_SORTED",
        ("FastqToSam", "F1") => "FASTQ",
        ("FastqToSam", "F2") => "FASTQ2",
        ("FastqToSam", "RG") => "READ_GROUP_NAME",
        ("FastqToSam", "SM") => "SAMPLE_NAME",
        ("FastqToSam", "LB") => "LIBRARY_NAME",
        ("FastqToSam", "PL") => "PLATFORM",
        ("FastqToSam", "PU") => "PLATFORM_UNIT",
        ("FastqToSam", "CN") => "SEQUENCING_CENTER",
        ("FastqToSam", "DS") => "DESCRIPTION",
        ("FastqToSam", "DT") => "RUN_DATE",
        ("FastqToSam", "PI") => "PREDICTED_INSERT_SIZE",
        ("SamToFastq", "F") => "FASTQ",
        ("SamToFastq", "F2") => "SECOND_END_FASTQ",
        ("SamToFastq", "FU") => "UNPAIRED_FASTQ",
        ("SamToFastq", "Q") => "QUALITY",
        ("SamToFastq", "CLIP_ATTR") => "CLIPPING_ATTRIBUTE",
        ("SamToFastq", "CLIP_ACT") => "CLIPPING_ACTION",
        ("SamToFastq", "CLIP_MIN") => "CLIPPING_MIN_LENGTH",
        ("SamToFastq", "OPRG") => "OUTPUT_PER_RG",
        ("SamToFastq", "GZOPRG") => "COMPRESS_OUTPUTS_PER_RG",
        ("SamToFastq", "RGT") => "RG_TAG",
        ("SamToFastq", "ODIR") => "OUTPUT_DIR",
        ("SamToFastq", "R1_MAX_BASES") => "READ1_MAX_BASES_TO_WRITE",
        ("SamToFastq", "R2_MAX_BASES") => "READ2_MAX_BASES_TO_WRITE",
        ("QualityScoreDistribution" | "MeanQualityByCycle", "AS") => "ASSUME_SORTED",
        ("QualityScoreDistribution" | "MeanQualityByCycle", "CHART") => "CHART_OUTPUT",
        ("QualityScoreDistribution", "PF") => "PF_READS_ONLY",
        ("CollectInsertSizeMetrics", "AS") => "ASSUME_SORTED",
        ("CollectInsertSizeMetrics", "H" | "HISTOGRAM") => "HISTOGRAM_FILE",
        ("CollectInsertSizeMetrics", "M") => "MINIMUM_PCT",
        ("CollectInsertSizeMetrics", "LEVEL") => "METRIC_ACCUMULATION_LEVEL",
        ("CollectMultipleMetrics", "AS") => "ASSUME_SORTED",
        ("CollectMultipleMetrics", "EXT") => "FILE_EXTENSION",
        ("CollectMultipleMetrics", "LEVEL") => "METRIC_ACCUMULATION_LEVEL",
        ("CreateSequenceDictionary", "REFERENCE") => "REFERENCE_SEQUENCE",
        ("CreateSequenceDictionary", "AS") => "GENOME_ASSEMBLY",
        ("CreateSequenceDictionary", "UR") => "URI",
        ("CreateSequenceDictionary", "SP") => "SPECIES",
        ("CreateSequenceDictionary", "AN") => "ALT_NAMES",
        ("CollectWgsMetrics", "MQ") => "MINIMUM_MAPPING_QUALITY",
        ("CollectWgsMetrics", "Q") => "MINIMUM_BASE_QUALITY",
        ("CollectWgsMetrics", "CAP") => "COVERAGE_CAP",
        ("BedToIntervalList", "SD") => "SEQUENCE_DICTIONARY",
        ("ReplaceSamHeader", "H") => "HEADER",
        ("UpdateVcfSequenceDictionary", "D" | "SD") => "SEQUENCE_DICTIONARY",
        ("SortVcf", "D" | "SD") => "SEQUENCE_DICTIONARY",
        ("MergeVcfs", "D" | "SD") => "SEQUENCE_DICTIONARY",
        ("IntervalListTools", "M") => "SUBDIVISION_MODE",
        ("IntervalListTools", "SI") => "SECOND_INPUT",
        ("IntervalListTools", "BRK") => "BREAK_BANDS_AT_MULTIPLES_OF",
        ("ScatterIntervalsByNs", "OT") => "OUTPUT_TYPE",
        ("ScatterIntervalsByNs", "N") => "MAX_TO_MERGE",
        ("LiftoverVcf", "C") => "CHAIN",
        ("LiftoverVcf", "WMC") => "WARN_ON_MISSING_CONTIG",
        ("ValidateSamFile", "M") => "MODE",
        ("ValidateSamFile", "MO") => "MAX_OUTPUT",
        ("ValidateSamFile", "SMV") => "SKIP_MATE_VALIDATION",
        ("FixMateInformation", "AS") => "ASSUME_SORTED",
        ("FixMateInformation", "MC") => "ADD_MATE_CIGAR",
        ("RevertSam", "OQ") => "RESTORE_ORIGINAL_QUALITIES",
        ("RevertSam", "OM") => "OUTPUT_MAP",
        ("RevertSam", "OBR") => "OUTPUT_BY_READGROUP",
        ("RevertSam", "RHC") => "RESTORE_HARDCLIPS",
        ("RevertSam", "RV") => "ATTRIBUTE_TO_REVERSE",
        ("RevertSam", "RC") => "ATTRIBUTE_TO_REVERSE_COMPLEMENT",
        ("RevertSam", "ALIAS") => "SAMPLE_ALIAS",
        ("RevertSam", "LIB") => "LIBRARY_NAME",
        _ => upper.as_str(),
    };

    Ok(canonical.to_string())
}

/// A typed reader over [`PicardArgs`] that keeps the command in every error.
#[derive(Debug, Clone, Copy)]
pub struct CommandArgs<'a> {
    command: &'a str,
    args: &'a PicardArgs,
}

impl<'a> CommandArgs<'a> {
    pub fn command(self) -> &'a str {
        self.command
    }

    pub fn required_string(self, key: &str) -> Result<String, CommandArgError> {
        self.scalar(key).map(str::to_string)
    }

    /// Reject keys outside `allowed`, naming this reader's command.
    pub fn reject_unknown(self, allowed: &[&str]) -> Result<(), CommandArgError> {
        for key in self.args.values.keys() {
            if !allowed.contains(&key.as_str()) {
                return Err(CommandArgError::Unknown {
                    command: self.command.to_string(),
                    key: key.clone(),
                });
            }
        }
        Ok(())
    }

    pub fn required_values(self, key: &str) -> Result<Vec<String>, CommandArgError> {
        self.args
            .values
            .get(key)
            .filter(|values| !values.is_empty())
            .cloned()
            .ok_or_else(|| self.missing(key))
    }

    pub fn optional_string(self, key: &str) -> Result<Option<String>, CommandArgError> {
        if !self.args.values.contains_key(key) {
            return Ok(None);
        }
        self.scalar(key).map(|value| Some(value.to_string()))
    }

    pub fn optional_bool(self, key: &str) -> Result<Option<bool>, CommandArgError> {
        let Some(value) = self.optional_string(key)? else {
            return Ok(None);
        };
        match value.to_ascii_lowercase().as_str() {
            "true" => Ok(Some(true)),
            "false" => Ok(Some(false)),
            _ => Err(CommandArgError::InvalidBoolean {
                command: self.command.to_string(),
                key: key.to_string(),
                value,
            }),
        }
    }

    pub fn optional_u32(self, key: &str) -> Result<Option<u32>, CommandArgError> {
        self.optional_parsed(key)
    }

    pub fn optional_i64(self, key: &str) -> Result<Option<i64>, CommandArgError> {
        self.optional_parsed(key)
    }

    pub fn optional_f64(self, key: &str) -> Result<Option<f64>, CommandArgError> {
        self.optional_parsed(key)
    }

    fn optional_parsed<T>(self, key: &str) -> Result<Option<T>, CommandArgError>
    where
        T: std::str::FromStr,
    {
        let Some(value) = self.optional_string(key)? else {
            return Ok(None);
        };
        value
            .parse::<T>()
            .map(Some)
            .map_err(|_| CommandArgError::InvalidValue {
                command: self.command.to_string(),
                key: key.to_string(),
                value,
            })
    }

    fn scalar(self, key: &str) -> Result<&'a str, CommandArgError> {
        let values = self.args.values.get(key).ok_or_else(|| self.missing(key))?;
        if values.len() != 1 {
            return Err(CommandArgError::DuplicateScalar {
                command: self.command.to_string(),
                key: key.to_string(),
            });
        }
        Ok(values[0].as_str())
    }

    fn missing(self, key: &str) -> CommandArgError {
        CommandArgError::MissingRequired {
            command: self.command.to_string(),
            key: key.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandArgError {
    MissingRequired {
        command: String,
        key: String,
    },
    DuplicateScalar {
        command: String,
        key: String,
    },
    Unknown {
        command: String,
        key: String,
    },
    InvalidBoolean {
        command: String,
        key: String,
        value: String,
    },
    InvalidValue {
        command: String,
        key: String,
        value: String,
    },
}

impl fmt::Display for CommandArgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRequired { command, key } => {
                write!(f, "missing required {command} argument: {key}")
            }
            Self::DuplicateScalar { command, key } => {
                write!(f, "duplicate scalar {command} argument: {key}")
            }
            Self::Unknown { command, key } => {
                write!(f, "unsupported {command} argument: {key}")
            }
            Self::InvalidBoolean {
                command,
                key,
                value,
            } => write!(f, "invalid boolean for {command} argument {key}: {value}"),
            Self::InvalidValue {
                command,
                key,
                value,
            } => write!(f, "unsupported {command} argument {key}={value}"),
        }
    }
}

impl std::error::Error for CommandArgError {}

/// Picard's `SORT_ORDER` values, parsed once at the argument boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortOrder {
    Coordinate,
    QueryName,
    Unsorted,
}

impl SortOrder {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "coordinate" => Some(Self::Coordinate),
            "queryname" => Some(Self::QueryName),
            "unsorted" => Some(Self::Unsorted),
            _ => None,
        }
    }

    pub fn as_picard_str(self) -> &'static str {
        match self {
            Self::Coordinate => "coordinate",
            Self::QueryName => "queryname",
            Self::Unsorted => "unsorted",
        }
    }
}
