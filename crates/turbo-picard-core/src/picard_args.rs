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
    normalize_picard_args_with_aliases(args, canonical_key)
}

pub fn normalize_picard_args_for_command(
    command: &str,
    args: &[String],
) -> Result<PicardArgs, PicardArgError> {
    normalize_picard_args_with_aliases(args, |key| canonical_key_for_command(command, key))
}

fn normalize_picard_args_with_aliases(
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

    Ok(normalized)
}

fn looks_like_flag_value(value: &str) -> bool {
    value.len() > 1 && value.starts_with('-') && !matches!(value.as_bytes()[1], b'.' | b'0'..=b'9')
}

fn push_arg_with_aliases(
    args: &mut PicardArgs,
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
        ("CollectHsMetrics", "BAIT") => "BAIT_INTERVALS",
        ("CollectHsMetrics", "TARGET") => "TARGET_INTERVALS",
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
