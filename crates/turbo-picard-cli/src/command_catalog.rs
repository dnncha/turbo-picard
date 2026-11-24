use crate::command_error::CommandError;
use crate::{
    VALIDATE_SAM_FILE_VALIDATION_ISSUES, ValidateSamFileStatus, print_addorreplacereadgroups_help,
    print_bedtointervallist_help, print_buildbamindex_help, print_cleansam_help,
    print_collectalignmentsummarymetrics_help, print_collectbasedistributionbycycle_help,
    print_collectgcbiasmetrics_help, print_collecthsmetrics_help,
    print_collectinsertsizemetrics_help, print_collectmultiplemetrics_help,
    print_collectqualityyieldmetrics_help, print_collectrnaseqmetrics_help,
    print_collectsamerrormetrics_help, print_collectwgsmetrics_help,
    print_createsequencedictionary_help, print_fastqtosam_help, print_fixmateinformation_help,
    print_gathervcfs_help, print_intervallisttools_help, print_liftovervcf_help,
    print_markduplicates_help, print_meanqualitybycycle_help, print_mergesamfiles_help,
    print_mergevcfs_help, print_normalizefasta_help, print_qualityscoredistribution_help,
    print_replacesamheader_help, print_revertsam_help, print_samtofastq_help,
    print_setnmmdanduqtags_help, print_sortsam_help, print_sortvcf_help,
    print_updatevcfsequencedictionary_help, print_validatesamfile_help, print_viewsam_help,
    rna_seq_metrics, run_addorreplacereadgroups, run_bedtointervallist, run_buildbamindex,
    run_cleansam, run_collectalignmentsummarymetrics, run_collectbasedistributionbycycle,
    run_collectgcbiasmetrics, run_collecthsmetrics, run_collectinsertsizemetrics,
    run_collectmultiplemetrics, run_collectqualityyieldmetrics, run_collectwgsmetrics,
    run_createsequencedictionary, run_fastqtosam, run_fixmateinformation, run_gathervcfs,
    run_intervallisttools, run_liftovervcf, run_markduplicates, run_meanqualitybycycle,
    run_mergesamfiles, run_mergevcfs, run_normalizefasta, run_qualityscoredistribution,
    run_replacesamheader, run_revertsam, run_samtofastq, run_setnmmdanduqtags, run_sortsam,
    run_sortvcf, run_updatevcfsequencedictionary, run_validatesamfile, run_viewsam,
    sam_error_metrics, try_run_fallback_for_native_error,
};

const UTILITY_COMMANDS: [&str; 6] = [
    "AccelerationStatus",
    "capabilities",
    "doctor",
    "explain",
    "mcp",
    "trial",
];

const FALLBACK_WHY: &str = "No native implementation is documented; use upstream Picard or choose a native command for a speed trial.";
const UTILITY_WHY: &str = "This is a turbo-picard utility command, not a Picard data-processing workload or speed-trial candidate.";
const MARKDUPLICATES_WHY: &str = "Duplicate marking is a common runtime and heap-pressure point; this keeps Picard-style arguments and writes BAM plus metrics outputs.";
const SORTSAM_WHY: &str = "Sorting is a frequent file-reshaping bottleneck; this keeps the Picard command boundary while testing native execution.";
const FASTQ_WHY: &str = "FASTQ conversion is easy to compare at the workflow boundary and often repeats across many samples.";
const FIXMATE_WHY: &str =
    "Mate-field repair is a focused preprocessing step with clear SAM/BAM output comparison.";
const BUILDBAMINDEX_WHY: &str =
    "Index generation is a small low-risk substitution before changing heavier Picard steps.";
const METRICS_WHY: &str = "Metrics commands are good trials when repeated quality-control steps add noticeable wait time.";
const DEFAULT_NATIVE_WHY: &str = "The command has documented native coverage; run it beside upstream Picard on representative input before switching a workflow.";

#[derive(Clone, Copy)]
pub(crate) enum CommandHandler {
    Standard(fn(&[String]) -> Result<(), CommandError>),
    CustomExit(fn(&[String]) -> Result<i32, CommandError>),
}

#[derive(Clone, Copy)]
pub(crate) struct NativeCommandSpec {
    pub(crate) name: &'static str,
    print_help: fn(),
    handler: CommandHandler,
    supports_version_flag: bool,
    trial_fit: &'static str,
    trial_why: &'static str,
}

impl NativeCommandSpec {
    const fn standard(
        name: &'static str,
        print_help: fn(),
        handler: fn(&[String]) -> Result<(), CommandError>,
        trial_fit: &'static str,
        trial_why: &'static str,
    ) -> Self {
        Self {
            name,
            print_help,
            handler: CommandHandler::Standard(handler),
            supports_version_flag: false,
            trial_fit,
            trial_why,
        }
    }
}

fn run_validatesamfile_exit(args: &[String]) -> Result<i32, CommandError> {
    match run_validatesamfile(args)? {
        ValidateSamFileStatus::Valid => Ok(0),
        ValidateSamFileStatus::Invalid { exit_code } => {
            eprintln!("{VALIDATE_SAM_FILE_VALIDATION_ISSUES}");
            Ok(exit_code)
        }
    }
}

pub(crate) const NATIVE_COMMANDS: &[NativeCommandSpec] = &[
    NativeCommandSpec {
        name: "MarkDuplicates",
        print_help: print_markduplicates_help,
        handler: CommandHandler::Standard(run_markduplicates),
        supports_version_flag: true,
        trial_fit: "recommended-first-trial",
        trial_why: MARKDUPLICATES_WHY,
    },
    NativeCommandSpec::standard(
        "SortSam",
        print_sortsam_help,
        run_sortsam,
        "good-first-trial",
        SORTSAM_WHY,
    ),
    NativeCommandSpec::standard(
        "CleanSam",
        print_cleansam_help,
        run_cleansam,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "MergeSamFiles",
        print_mergesamfiles_help,
        run_mergesamfiles,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "BuildBamIndex",
        print_buildbamindex_help,
        run_buildbamindex,
        "good-first-trial",
        BUILDBAMINDEX_WHY,
    ),
    NativeCommandSpec::standard(
        "SamToFastq",
        print_samtofastq_help,
        run_samtofastq,
        "good-first-trial",
        FASTQ_WHY,
    ),
    NativeCommandSpec::standard(
        "FastqToSam",
        print_fastqtosam_help,
        run_fastqtosam,
        "good-first-trial",
        FASTQ_WHY,
    ),
    NativeCommandSpec::standard(
        "AddOrReplaceReadGroups",
        print_addorreplacereadgroups_help,
        run_addorreplacereadgroups,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectAlignmentSummaryMetrics",
        print_collectalignmentsummarymetrics_help,
        run_collectalignmentsummarymetrics,
        "good-first-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectQualityYieldMetrics",
        print_collectqualityyieldmetrics_help,
        run_collectqualityyieldmetrics,
        "good-first-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectInsertSizeMetrics",
        print_collectinsertsizemetrics_help,
        run_collectinsertsizemetrics,
        "good-first-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectMultipleMetrics",
        print_collectmultiplemetrics_help,
        run_collectmultiplemetrics,
        "good-first-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectBaseDistributionByCycle",
        print_collectbasedistributionbycycle_help,
        run_collectbasedistributionbycycle,
        "good-first-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectGcBiasMetrics",
        print_collectgcbiasmetrics_help,
        run_collectgcbiasmetrics,
        "good-first-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectWgsMetrics",
        print_collectwgsmetrics_help,
        run_collectwgsmetrics,
        "good-first-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectHsMetrics",
        print_collecthsmetrics_help,
        run_collecthsmetrics,
        "supported-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectRnaSeqMetrics",
        print_collectrnaseqmetrics_help,
        rna_seq_metrics::run_collectrnaseqmetrics,
        "supported-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "CollectSamErrorMetrics",
        print_collectsamerrormetrics_help,
        sam_error_metrics::run_collectsamerrormetrics,
        "supported-trial",
        METRICS_WHY,
    ),
    NativeCommandSpec::standard(
        "FixMateInformation",
        print_fixmateinformation_help,
        run_fixmateinformation,
        "good-first-trial",
        FIXMATE_WHY,
    ),
    NativeCommandSpec::standard(
        "IntervalListTools",
        print_intervallisttools_help,
        run_intervallisttools,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "RevertSam",
        print_revertsam_help,
        run_revertsam,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "SetNmMdAndUqTags",
        print_setnmmdanduqtags_help,
        run_setnmmdanduqtags,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec {
        name: "ValidateSamFile",
        print_help: print_validatesamfile_help,
        handler: CommandHandler::CustomExit(run_validatesamfile_exit),
        supports_version_flag: false,
        trial_fit: "supported-trial",
        trial_why: DEFAULT_NATIVE_WHY,
    },
    NativeCommandSpec::standard(
        "LiftoverVcf",
        print_liftovervcf_help,
        run_liftovervcf,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "QualityScoreDistribution",
        print_qualityscoredistribution_help,
        run_qualityscoredistribution,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "MeanQualityByCycle",
        print_meanqualitybycycle_help,
        run_meanqualitybycycle,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "CreateSequenceDictionary",
        print_createsequencedictionary_help,
        run_createsequencedictionary,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "NormalizeFasta",
        print_normalizefasta_help,
        run_normalizefasta,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "BedToIntervalList",
        print_bedtointervallist_help,
        run_bedtointervallist,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "ViewSam",
        print_viewsam_help,
        run_viewsam,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "ReplaceSamHeader",
        print_replacesamheader_help,
        run_replacesamheader,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "UpdateVcfSequenceDictionary",
        print_updatevcfsequencedictionary_help,
        run_updatevcfsequencedictionary,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "GatherVcfs",
        print_gathervcfs_help,
        run_gathervcfs,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "SortVcf",
        print_sortvcf_help,
        run_sortvcf,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
    NativeCommandSpec::standard(
        "MergeVcfs",
        print_mergevcfs_help,
        run_mergevcfs,
        "supported-trial",
        DEFAULT_NATIVE_WHY,
    ),
];

pub(crate) fn find_native(command: &str) -> Option<&'static NativeCommandSpec> {
    NATIVE_COMMANDS.iter().find(|spec| spec.name == command)
}

pub(crate) fn dispatch_native(
    spec: &NativeCommandSpec,
    command_args: &[String],
    raw_args: &[String],
) -> i32 {
    if command_args
        .iter()
        .any(|arg| arg == "--help" || arg == "-h")
    {
        (spec.print_help)();
        return 0;
    }
    if spec.supports_version_flag && command_args.iter().any(|arg| arg == "--version") {
        println!("{} {}", spec.name, env!("CARGO_PKG_VERSION"));
        return 0;
    }
    let result = match spec.handler {
        CommandHandler::Standard(run) => run(command_args).map(|()| 0),
        CommandHandler::CustomExit(run) => run(command_args),
    };
    match result {
        Ok(exit_code) => exit_code,
        Err(error) => {
            if let Some(exit_code) = try_run_fallback_for_native_error(&error, raw_args) {
                return exit_code;
            }
            eprintln!("{error}");
            2
        }
    }
}

pub(crate) fn trial_fit_for(command: &str, status: &str) -> &'static str {
    if status == "fallback-only" {
        return "fallback-only";
    }
    if UTILITY_COMMANDS.contains(&command) {
        return "not-a-workload";
    }
    find_native(command).map_or("supported-trial", |spec| spec.trial_fit)
}

pub(crate) fn trial_why_for(command: &str, status: &str) -> &'static str {
    if status == "fallback-only" {
        return FALLBACK_WHY;
    }
    if UTILITY_COMMANDS.contains(&command) {
        return UTILITY_WHY;
    }
    if let Some(spec) = find_native(command) {
        return spec.trial_why;
    }
    if command.starts_with("Collect") {
        return METRICS_WHY;
    }
    DEFAULT_NATIVE_WHY
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn native_catalog_entries_are_unique_and_lookup_consistent() {
        let mut names = BTreeSet::new();
        for spec in NATIVE_COMMANDS {
            assert!(
                names.insert(spec.name),
                "duplicate native command in catalog: {}",
                spec.name
            );
            assert!(find_native(spec.name).is_some());
        }
        assert_eq!(NATIVE_COMMANDS.len(), 35);
        assert!(find_native("MarkDuplicates").unwrap().supports_version_flag);
        assert!(!find_native("SortSam").unwrap().supports_version_flag);
    }

    #[test]
    fn trial_metadata_matches_workload_contract() {
        assert_eq!(
            trial_fit_for("MarkDuplicates", "partial-native"),
            "recommended-first-trial"
        );
        assert_eq!(trial_fit_for("SortSam", "native"), "good-first-trial");
        assert_eq!(
            trial_fit_for("CollectHsMetrics", "native"),
            "supported-trial"
        );
        assert_eq!(trial_why_for("CollectHsMetrics", "native"), METRICS_WHY);
        assert_eq!(trial_fit_for("doctor", "native"), "not-a-workload");
        assert_eq!(trial_why_for("doctor", "native"), UTILITY_WHY);
        assert_eq!(
            trial_fit_for("EstimateLibraryComplexity", "fallback-only"),
            "fallback-only"
        );
        assert_eq!(
            trial_why_for("EstimateLibraryComplexity", "fallback-only"),
            FALLBACK_WHY
        );
    }

    #[test]
    fn every_catalog_command_appears_in_command_matrix() {
        let matrix = crate::COMMAND_MATRIX_YAML;
        for spec in NATIVE_COMMANDS {
            let needle = format!("name: {}", spec.name);
            assert!(
                matrix.contains(&needle),
                "catalog command {} missing from docs/command-matrix.yml",
                spec.name
            );
        }
    }
}
