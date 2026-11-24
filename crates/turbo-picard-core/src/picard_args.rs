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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPicardArg {
    pub raw_key: String,
    pub canonical_key: String,
    pub value: String,
}

const BOOLEAN_OPTIONS: &[&str] = &[
    "ADD_MATE_CIGAR",
    "ADD_PG_TAG_TO_READS",
    "ALIGNED_READS_ONLY",
    "ALLOW_AND_IGNORE_EMPTY_LINES",
    "ALLOW_EMPTY_FASTQ",
    "ALSO_IGNORE_DUPLICATES",
    "ASSUME_SORTED",
    "CLEAR_DT",
    "CLIP_OVERLAPPING_READS",
    "COLLECT_ALIGNMENT_INFORMATION",
    "COMPRESS_OUTPUTS_PER_RG",
    "COUNT_UNPAIRED",
    "CREATE_INDEX",
    "CREATE_MD5_FILE",
    "DONT_MERGE_ABUTTING",
    "DROP_MISSING_CONTIGS",
    "EXCLUDE_SUPPLEMENTARY",
    "HEADER_ONLY",
    "IGNORE_MISSING_MATES",
    "INCLUDE_BQ_HISTOGRAM",
    "INCLUDE_DUPLICATES",
    "INCLUDE_INDELS",
    "INCLUDE_NON_PF_READS",
    "INCLUDE_NON_PRIMARY_ALIGNMENTS",
    "INCLUDE_NO_CALLS",
    "INCLUDE_SECONDARY_ALIGNMENTS",
    "INCLUDE_SUPPLEMENTAL_ALIGNMENTS",
    "INTERLEAVE",
    "INTERVAL_ITERATOR",
    "IS_BISULFITE_SEQUENCE",
    "IS_BISULFITE_SEQUENCED",
    "KEEP_LENGTH_ZERO_INTERVALS",
    "MERGE_SEQUENCE_DICTIONARIES",
    "OUTPUT_PER_RG",
    "PF_READS_ONLY",
    "QUIET",
    "RECORDS_ONLY",
    "REMOVE_ALIGNMENT_INFORMATION",
    "REMOVE_DUPLICATES",
    "REMOVE_DUPLICATE_INFORMATION",
    "REMOVE_SEQUENCING_DUPLICATES",
    "RESTORE_HARDCLIPS",
    "RESTORE_ORIGINAL_QUALITIES",
    "RE_REVERSE",
    "SET_ONLY_UQ",
    "SKIP_MATE_VALIDATION",
    "SORT",
    "TAG_DUPLICATE_SET_MEMBERS",
    "TRUNCATE_NAMES_AT_WHITESPACE",
    "TRUNCATE_SEQUENCE_NAMES_AT_WHITESPACE",
    "UNIQUE",
    "USE_FAST_ALGORITHM",
    "USE_JDK_DEFLATER",
    "USE_JDK_INFLATER",
    "USE_ORIGINAL_QUALITIES",
    "USE_SEQUENTIAL_FASTQS",
    "WARN_ON_MISSING_CONTIG",
];

pub fn is_boolean_option(canonical_key: &str) -> bool {
    BOOLEAN_OPTIONS.binary_search(&canonical_key).is_ok()
}

pub fn normalize_picard_args(args: &[String]) -> Result<PicardArgs, PicardArgError> {
    normalize_picard_args_with_aliases("Picard", args, canonical_key)
}

pub fn normalize_picard_args_for_command(
    command: &str,
    args: &[String],
) -> Result<PicardArgs, PicardArgError> {
    normalize_picard_args_with_aliases(command, args, |key| canonical_key_for_command(command, key))
}

pub fn parse_picard_args_for_command(
    command: &str,
    args: &[String],
) -> Result<Vec<ParsedPicardArg>, PicardArgError> {
    parse_picard_args_with_aliases(args, |key| canonical_key_for_command(command, key))
}

fn normalize_picard_args_with_aliases(
    command: &str,
    args: &[String],
    canonicalize: impl Fn(&str) -> Result<String, PicardArgError>,
) -> Result<PicardArgs, PicardArgError> {
    let parsed = parse_picard_args_with_aliases(args, canonicalize)?;
    let mut normalized: PicardArgMap = BTreeMap::new();
    for item in parsed {
        normalized
            .entry(item.canonical_key)
            .or_default()
            .push(item.value);
    }

    Ok(PicardArgs {
        command: command.to_string(),
        values: normalized,
    })
}

fn parse_picard_args_with_aliases(
    args: &[String],
    canonicalize: impl Fn(&str) -> Result<String, PicardArgError>,
) -> Result<Vec<ParsedPicardArg>, PicardArgError> {
    let mut parsed = Vec::new();
    let mut index = 0;

    while index < args.len() {
        let arg = &args[index];

        if let Some(long) = arg.strip_prefix("--") {
            if long.is_empty() {
                return Err(PicardArgError::EmptyKey(arg.clone()));
            }

            if let Some((raw_key, value)) = long.split_once('=') {
                let canonical_key = canonicalize(&normalize_long_key(raw_key))?;
                parsed.push(ParsedPicardArg {
                    raw_key: raw_key.to_string(),
                    canonical_key,
                    value: value.to_string(),
                });
                index += 1;
                continue;
            }

            let canonical_key = canonicalize(&normalize_long_key(long))?;
            let (value, step) = consume_flag_value(args, index, &canonical_key)?;

            parsed.push(ParsedPicardArg {
                raw_key: long.to_string(),
                canonical_key,
                value,
            });
            index += step;
            continue;
        }

        if let Some(short) = arg.strip_prefix('-') {
            if short.is_empty() {
                return Err(PicardArgError::EmptyKey(arg.clone()));
            }

            let canonical_key = canonicalize(short)?;
            let (value, step) = consume_flag_value(args, index, &canonical_key)?;

            parsed.push(ParsedPicardArg {
                raw_key: short.to_string(),
                canonical_key,
                value,
            });
            index += step;
            continue;
        }

        if let Some((raw_key, value)) = arg.split_once('=') {
            let canonical_key = canonicalize(raw_key)?;
            parsed.push(ParsedPicardArg {
                raw_key: raw_key.to_string(),
                canonical_key,
                value: value.to_string(),
            });
            index += 1;
            continue;
        }

        return Err(PicardArgError::UnexpectedPositional(arg.clone()));
    }

    Ok(parsed)
}

fn consume_flag_value(
    args: &[String],
    index: usize,
    canonical_key: &str,
) -> Result<(String, usize), PicardArgError> {
    let next_is_missing_or_flag = match args.get(index + 1) {
        None => true,
        Some(next) => looks_like_flag_value(next) || (next.contains('=') && !next.starts_with('=')),
    };
    if next_is_missing_or_flag {
        if is_boolean_option(canonical_key) {
            return Ok(("true".to_string(), 1));
        }
        return Err(PicardArgError::MissingValue(canonical_key.to_string()));
    }
    Ok((args[index + 1].clone(), 2))
}

fn normalize_long_key(key: &str) -> String {
    if key
        .as_bytes()
        .first()
        .is_some_and(u8::is_ascii_alphanumeric)
    {
        key.replace('-', "_")
    } else {
        key.to_string()
    }
}

fn looks_like_flag_value(value: &str) -> bool {
    value.len() > 1 && value.starts_with('-') && !matches!(value.as_bytes()[1], b'.' | b'0'..=b'9')
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
        (_, "R") | ("CreateSequenceDictionary", "REFERENCE") => "REFERENCE_SEQUENCE",
        (_, "SO") => "SORT_ORDER",
        (_, "CO") => "COMMENT",
        ("MarkDuplicates", "M") => "METRICS_FILE",
        (
            "MarkDuplicates"
            | "SortSam"
            | "MergeSamFiles"
            | "CollectAlignmentSummaryMetrics"
            | "CollectBaseDistributionByCycle"
            | "CollectGcBiasMetrics"
            | "CollectHsMetrics"
            | "CollectRnaSeqMetrics"
            | "QualityScoreDistribution"
            | "MeanQualityByCycle"
            | "CollectInsertSizeMetrics"
            | "CollectMultipleMetrics"
            | "FixMateInformation",
            "AS",
        ) => "ASSUME_SORTED",
        ("MarkDuplicates", "ASO") => "ASSUME_SORT_ORDER",
        ("MarkDuplicates", "DS") => "DUPLICATE_SCORING_STRATEGY",
        ("MarkDuplicates", "MAX_FILE_HANDLES") => "MAX_FILE_HANDLES_FOR_READ_ENDS_MAP",
        ("MarkDuplicates", "MAX_SEQS") => "MAX_SEQUENCES_FOR_DISK_READ_ENDS_MAP",
        ("MarkDuplicates", "PG") => "PROGRAM_RECORD_ID",
        ("MarkDuplicates", "PG_COMMAND") => "PROGRAM_GROUP_COMMAND_LINE",
        ("MarkDuplicates", "PG_NAME") => "PROGRAM_GROUP_NAME",
        ("MarkDuplicates", "PG_VERSION") => "PROGRAM_GROUP_VERSION",
        (
            "CollectAlignmentSummaryMetrics"
            | "CollectInsertSizeMetrics"
            | "CollectMultipleMetrics"
            | "CollectRnaSeqMetrics",
            "LEVEL",
        ) => "METRIC_ACCUMULATION_LEVEL",
        (
            "CollectBaseDistributionByCycle"
            | "CollectGcBiasMetrics"
            | "CollectRnaSeqMetrics"
            | "QualityScoreDistribution"
            | "MeanQualityByCycle",
            "CHART",
        ) => "CHART_OUTPUT",
        ("CollectRnaSeqMetrics", "STRAND") => "STRAND_SPECIFICITY",
        ("CollectSamErrorMetrics" | "CollectMultipleMetrics", "EXT") => "FILE_EXTENSION",
        ("CollectSamErrorMetrics", "L") => "INTERVALS",
        ("CollectSamErrorMetrics", "LBS") => "LOCATION_BIN_SIZE",
        ("CollectSamErrorMetrics", "LH") => "LONG_HOMOPOLYMER",
        ("CollectSamErrorMetrics", "MAX") => "MAX_LOCI",
        ("CollectSamErrorMetrics", "BQ") => "MIN_BASE_Q",
        ("CollectSamErrorMetrics", "MQ") => "MIN_MAPPING_Q",
        ("CollectSamErrorMetrics", "PE") => "PRIOR_Q",
        ("CollectSamErrorMetrics", "P") => "PROBABILITY",
        ("CollectSamErrorMetrics", "V") => "VCF",
        ("CollectGcBiasMetrics", "S") => "SUMMARY_OUTPUT",
        ("CollectGcBiasMetrics", "WINDOW_SIZE") => "SCAN_WINDOW_SIZE",
        ("CollectGcBiasMetrics", "MGF") => "MINIMUM_GENOME_FRACTION",
        ("CollectGcBiasMetrics", "BS") => "IS_BISULFITE_SEQUENCED",
        ("CollectHsMetrics", "BAIT" | "BI") => "BAIT_INTERVALS",
        ("CollectHsMetrics", "TARGET" | "TI") => "TARGET_INTERVALS",
        ("CollectHsMetrics", "N") => "BAIT_SET_NAME",
        ("CollectHsMetrics" | "CollectWgsMetrics", "MQ") => "MINIMUM_MAPPING_QUALITY",
        ("CollectHsMetrics" | "CollectWgsMetrics", "Q") => "MINIMUM_BASE_QUALITY",
        ("CollectHsMetrics" | "CollectWgsMetrics", "CAP") => "COVERAGE_CAP",
        ("FastqToSam", "F1") | ("SamToFastq", "F") => "FASTQ",
        ("FastqToSam", "F2") => "FASTQ2",
        ("FastqToSam", "RG") => "READ_GROUP_NAME",
        ("FastqToSam", "SM") => "SAMPLE_NAME",
        ("FastqToSam", "LB") | ("RevertSam", "LIB") => "LIBRARY_NAME",
        ("FastqToSam", "PL") => "PLATFORM",
        ("FastqToSam", "PU") => "PLATFORM_UNIT",
        ("FastqToSam", "CN") => "SEQUENCING_CENTER",
        ("FastqToSam", "DS") => "DESCRIPTION",
        ("FastqToSam", "DT") => "RUN_DATE",
        ("FastqToSam", "PI") => "PREDICTED_INSERT_SIZE",
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
        ("QualityScoreDistribution", "PF") => "PF_READS_ONLY",
        ("CollectInsertSizeMetrics", "H" | "HISTOGRAM") => "HISTOGRAM_FILE",
        ("CollectInsertSizeMetrics", "M") => "MINIMUM_PCT",
        ("CreateSequenceDictionary", "AS") => "GENOME_ASSEMBLY",
        ("CreateSequenceDictionary", "UR") => "URI",
        ("CreateSequenceDictionary", "SP") => "SPECIES",
        ("CreateSequenceDictionary", "AN") => "ALT_NAMES",
        ("BedToIntervalList", "SD")
        | ("UpdateVcfSequenceDictionary" | "SortVcf" | "MergeVcfs", "D" | "SD") => {
            "SEQUENCE_DICTIONARY"
        }
        ("ReplaceSamHeader", "H") => "HEADER",
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
        ("FixMateInformation", "MC") => "ADD_MATE_CIGAR",
        ("RevertSam", "OQ") => "RESTORE_ORIGINAL_QUALITIES",
        ("RevertSam", "OM") => "OUTPUT_MAP",
        ("RevertSam", "OBR") => "OUTPUT_BY_READGROUP",
        ("RevertSam", "RHC") => "RESTORE_HARDCLIPS",
        ("RevertSam", "RV") => "ATTRIBUTE_TO_REVERSE",
        ("RevertSam", "RC") => "ATTRIBUTE_TO_REVERSE_COMPLEMENT",
        ("RevertSam", "ALIAS") => "SAMPLE_ALIAS",
        _ => upper.as_str(),
    };

    Ok(canonical.to_string())
}

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

#[cfg(test)]
mod tests {
    use super::BOOLEAN_OPTIONS;

    #[test]
    fn boolean_options_table_is_strictly_sorted() {
        assert!(BOOLEAN_OPTIONS.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
