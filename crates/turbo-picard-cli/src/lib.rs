#![forbid(unsafe_code)]

mod cmm_pipeline;
mod hs_metrics;

const PICARD_REFERENCE_COMMANDS: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/picard-3.4.0-commands.txt"
));
const COMMAND_MATRIX_YAML: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../docs/command-matrix.yml"
));

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::index;
use rust_htslib::bam::record::{Aux, Cigar, CigarString};
use rust_htslib::bam::{self, Read};
use rustc_hash::{FxBuildHasher, FxHashMap};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read as IoRead, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{self, Command};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use turbo_picard_core::external_sort::{ExternalSortConfig, ExternalSorter};
use turbo_picard_core::hts_io;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;
use turbo_picard_core::picard_args::normalize_picard_args_for_command;

pub fn run_cli(program_name: &str, raw_args: impl IntoIterator<Item = String>) -> i32 {
    let raw_args = raw_args.into_iter().collect::<Vec<_>>();
    if raw_args.as_slice() == ["--list-commands"] {
        print_picard_command_list();
        return 0;
    }
    if raw_args
        .first()
        .is_some_and(|arg| is_leading_jvm_option(arg))
    {
        if let Some(exit_code) = try_run_fallback(&raw_args) {
            return exit_code;
        }
        eprintln!("{program_name} accepts JVM options only when upstream Picard is available");
        return 2;
    }
    let mut args = raw_args.iter();

    match args.next().map(String::as_str) {
        Some("--help" | "-h" | "help") => {
            print_top_level_help(program_name);
            0
        }
        Some("--version" | "version") => {
            println!("{program_name} {}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("doctor") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_doctor_help(program_name);
                return 0;
            }
            run_doctor(program_name);
            0
        }
        Some("explain") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_explain_help(program_name);
                return 0;
            }
            if let Err(error) = run_explain(&command_args) {
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("AccelerationStatus") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_accelerationstatus_help();
                return 0;
            }
            if let Err(error) = run_acceleration_status() {
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("MarkDuplicates") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_markduplicates_help();
                return 0;
            }
            if command_args.iter().any(|arg| arg == "--version") {
                println!("MarkDuplicates {}", env!("CARGO_PKG_VERSION"));
                return 0;
            }
            if let Err(error) = run_markduplicates(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("SortSam") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_sortsam_help();
                return 0;
            }
            if let Err(error) = run_sortsam(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CleanSam") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_cleansam_help();
                return 0;
            }
            if let Err(error) = run_cleansam(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("MergeSamFiles") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_mergesamfiles_help();
                return 0;
            }
            if let Err(error) = run_mergesamfiles(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("BuildBamIndex") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_buildbamindex_help();
                return 0;
            }
            if let Err(error) = run_buildbamindex(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("SamToFastq") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_samtofastq_help();
                return 0;
            }
            if let Err(error) = run_samtofastq(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("FastqToSam") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_fastqtosam_help();
                return 0;
            }
            if let Err(error) = run_fastqtosam(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("AddOrReplaceReadGroups") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_addorreplacereadgroups_help();
                return 0;
            }
            if let Err(error) = run_addorreplacereadgroups(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CollectAlignmentSummaryMetrics") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_collectalignmentsummarymetrics_help();
                return 0;
            }
            if let Err(error) = run_collectalignmentsummarymetrics(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CollectQualityYieldMetrics") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_collectqualityyieldmetrics_help();
                return 0;
            }
            if let Err(error) = run_collectqualityyieldmetrics(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CollectInsertSizeMetrics") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_collectinsertsizemetrics_help();
                return 0;
            }
            if let Err(error) = run_collectinsertsizemetrics(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CollectMultipleMetrics") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_collectmultiplemetrics_help();
                return 0;
            }
            if let Err(error) = run_collectmultiplemetrics(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CollectBaseDistributionByCycle") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_collectbasedistributionbycycle_help();
                return 0;
            }
            if let Err(error) = run_collectbasedistributionbycycle(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CollectGcBiasMetrics") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_collectgcbiasmetrics_help();
                return 0;
            }
            if let Err(error) = run_collectgcbiasmetrics(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CollectWgsMetrics") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_collectwgsmetrics_help();
                return 0;
            }
            if let Err(error) = run_collectwgsmetrics(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CollectHsMetrics") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_collecthsmetrics_help();
                return 0;
            }
            if let Err(error) = run_collecthsmetrics(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("FixMateInformation") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_fixmateinformation_help();
                return 0;
            }
            if let Err(error) = run_fixmateinformation(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("IntervalListTools") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_intervallisttools_help();
                return 0;
            }
            if let Err(error) = run_intervallisttools(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("RevertSam") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_revertsam_help();
                return 0;
            }
            if let Err(error) = run_revertsam(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("SetNmMdAndUqTags") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_setnmmdanduqtags_help();
                return 0;
            }
            if let Err(error) = run_setnmmdanduqtags(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("ValidateSamFile") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_validatesamfile_help();
                return 0;
            }
            if let Err(error) = run_validatesamfile(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                if error == "ValidateSamFile found validation issues" {
                    return 3;
                }
                return 2;
            }
            0
        }
        Some("LiftoverVcf") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_liftovervcf_help();
                return 0;
            }
            if let Err(error) = run_liftovervcf(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("QualityScoreDistribution") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_qualityscoredistribution_help();
                return 0;
            }
            if let Err(error) = run_qualityscoredistribution(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("MeanQualityByCycle") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_meanqualitybycycle_help();
                return 0;
            }
            if let Err(error) = run_meanqualitybycycle(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("CreateSequenceDictionary") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_createsequencedictionary_help();
                return 0;
            }
            if let Err(error) = run_createsequencedictionary(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("NormalizeFasta") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_normalizefasta_help();
                return 0;
            }
            if let Err(error) = run_normalizefasta(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("BedToIntervalList") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_bedtointervallist_help();
                return 0;
            }
            if let Err(error) = run_bedtointervallist(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("ViewSam") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_viewsam_help();
                return 0;
            }
            if let Err(error) = run_viewsam(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("ReplaceSamHeader") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_replacesamheader_help();
                return 0;
            }
            if let Err(error) = run_replacesamheader(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("UpdateVcfSequenceDictionary") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_updatevcfsequencedictionary_help();
                return 0;
            }
            if let Err(error) = run_updatevcfsequencedictionary(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("GatherVcfs") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_gathervcfs_help();
                return 0;
            }
            if let Err(error) = run_gathervcfs(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("SortVcf") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_sortvcf_help();
                return 0;
            }
            if let Err(error) = run_sortvcf(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some("MergeVcfs") => {
            let command_args = args.cloned().collect::<Vec<_>>();
            if command_args
                .iter()
                .any(|arg| arg == "--help" || arg == "-h")
            {
                print_mergevcfs_help();
                return 0;
            }
            if let Err(error) = run_mergevcfs(&command_args) {
                if let Some(exit_code) = try_run_fallback_for_native_error(&error, &raw_args) {
                    return exit_code;
                }
                eprintln!("{error}");
                return 2;
            }
            0
        }
        Some(command) => {
            if let Some(exit_code) = try_run_fallback(&raw_args) {
                return exit_code;
            }
            if is_picard_reference_command(command) {
                eprintln!(
                    "Picard command {command} requires upstream Picard; set TURBO_PICARD_FALLBACK_COMMAND or install Picard 3.4.0"
                );
            } else {
                eprintln!("unsupported Picard command: {command}");
            }
            2
        }
        None => {
            eprintln!("usage: {program_name} <PicardCommand> [KEY=VALUE ...]");
            2
        }
    }
}

fn is_leading_jvm_option(arg: &str) -> bool {
    arg.starts_with("-X") || arg.starts_with("-D")
}

fn print_top_level_help(program_name: &str) {
    println!(
        "\
Usage: {program_name} <PicardCommand> [KEY=VALUE ...]

Available commands:
  doctor            Reports install, PATH, acceleration, reference, and fallback state
  explain COMMAND   Explains whether a command is native, partial-native, or fallback-only
  AddOrReplaceReadGroups
                    Adds or replaces a single read group in SAM/BAM/CRAM files
  AccelerationStatus
                    Reports the active CPU/GPU acceleration policy
  BedToIntervalList Converts BED files to Picard interval_list files
  CleanSam          Cleans common SAM/BAM/CRAM alignment issues
  CollectAlignmentSummaryMetrics
                    Writes basic alignment summary metrics for SAM/BAM/CRAM files
  CollectBaseDistributionByCycle
                    Writes nucleotide distribution by sequencing cycle
  CollectGcBiasMetrics
                    Writes GC-bias detail and summary metrics
  CollectInsertSizeMetrics
                    Writes paired-read insert size metrics
  CollectMultipleMetrics
                    Runs supported metric collectors from one Picard-shaped command
  CollectQualityYieldMetrics
                    Writes Picard-style read/base quality yield metrics
  CollectWgsMetrics
                    Writes whole-genome coverage metrics for SAM/BAM/CRAM files
  CreateSequenceDictionary
                    Creates a Picard sequence dictionary from a FASTA file
  FixMateInformation
                    Fixes paired-read mate fields for queryname grouped SAM/BAM/CRAM
  GatherVcfs        Concatenates block-sorted VCF shards
  BuildBamIndex     Builds a BAI index for a coordinate-sorted BAM file
  IntervalListTools Concatenates, sorts, and uniques interval_list files
  LiftoverVcf      Lifts simple positive-strand VCF records through UCSC chains
  MarkDuplicates    Identifies duplicate reads in SAM/BAM/CRAM files
  MeanQualityByCycle
                    Writes mean base quality by sequencing cycle
  MergeVcfs         Merges compatible VCF files by coordinate
  MergeSamFiles     Merges SAM/BAM/CRAM files with optional output sorting
  NormalizeFasta    Rewrites FASTA records with fixed-width sequence lines
  QualityScoreDistribution
                    Writes base quality score distribution metrics
  ReplaceSamHeader  Replaces a SAM/BAM/CRAM header while streaming records
  RevertSam         Reverts aligned SAM/BAM/CRAM records to unmapped queryname output
  SamToFastq        Converts SAM/BAM/CRAM records to FASTQ
  FastqToSam        Converts FASTQ records to unmapped SAM or BAM
  SetNmMdAndUqTags  Computes NM, MD, and UQ tags from a reference FASTA
  SortSam           Sorts SAM/BAM/CRAM files by coordinate or query name
  SortVcf           Sorts VCF records by sequence dictionary and position
  UpdateVcfSequenceDictionary
                    Replaces VCF contig headers from a Picard dictionary
  ValidateSamFile   Validates common SAM/BAM/CRAM structural issues in summary mode
  ViewSam           Views SAM/BAM/CRAM records or writes SAM to stdout"
    );
}

fn print_doctor_help(program_name: &str) {
    println!(
        "\
Usage: {program_name} doctor

Reports the local turbo-picard runtime state without running a Picard command.
The report includes version, executable path, CPU/thread policy, reference
discovery, fallback resolution, and whether `picard` on PATH appears to be the
turbo-picard shim."
    );
}

fn print_explain_help(program_name: &str) {
    println!(
        "\
Usage: {program_name} explain <PicardCommand> [KEY=VALUE ...]

Explains the documented execution path for a Picard-shaped command. The report
shows native/fallback status, documented native scope, documented fallback scope,
resolved fallback command, and declared output arguments from the provided
KEY=VALUE arguments."
    );
}

fn run_doctor(program_name: &str) {
    println!("turbo_picard_version={}", env!("CARGO_PKG_VERSION"));
    println!("program_name={program_name}");
    match env::current_exe() {
        Ok(path) => println!("current_exe={}", path.display()),
        Err(error) => println!("current_exe=unavailable ({error})"),
    }
    println!("picard_reference_version={}", picard_reference_version());
    println!("cpu_arch={}", env::consts::ARCH);
    println!("cpu_os={}", env::consts::OS);
    print_acceleration_status_lines();
    match env::var("TURBO_PICARD_REFERENCE")
        .ok()
        .filter(|value| !value.trim().is_empty())
    {
        Some(reference) => println!("reference={reference}"),
        None => println!("reference=not-set"),
    }
    match resolve_fallback_command() {
        Some(command) => println!("fallback_command={command}"),
        None => println!("fallback_command=not-found"),
    }
    match discover_picard_on_path() {
        Some(command) => println!("path_picard={command}"),
        None => println!("path_picard=not-found-or-shim-only"),
    }
    println!(
        "auto_fallback={}",
        if env::var("TURBO_PICARD_DISABLE_AUTO_FALLBACK").is_ok() {
            "disabled"
        } else {
            "enabled"
        }
    );
}

fn run_explain(args: &[String]) -> Result<(), String> {
    let command = args
        .first()
        .ok_or_else(|| "usage: turbo-picard explain <PicardCommand> [KEY=VALUE ...]".to_string())?;
    let Some(metadata) = command_matrix_entry(command) else {
        if is_picard_reference_command(command) {
            println!("command={command}");
            println!("status=fallback-only");
            println!("native_scope=No native metadata is available for this Picard command.");
            println!(
                "fallback_scope=Transparent upstream Picard delegation when fallback is configured or auto-discovered."
            );
            print_explain_fallback();
            print_declared_outputs(&args[1..]);
            return Ok(());
        }
        return Err(format!("unsupported Picard command: {command}"));
    };

    println!("command={}", metadata.name);
    println!("status={}", metadata.status);
    println!("native_scope={}", metadata.native_scope);
    println!("fallback_scope={}", metadata.fallback_scope);
    println!(
        "execution_path={}",
        match metadata.status.as_str() {
            "native" => "native",
            "partial-native" => "native-when-inside-documented-scope-otherwise-fallback",
            "fallback-only" => "fallback",
            _ => "see-command-matrix",
        }
    );
    print_explain_fallback();
    print_declared_outputs(&args[1..]);
    Ok(())
}

fn print_explain_fallback() {
    match resolve_fallback_command() {
        Some(command) => println!("fallback_command={command}"),
        None => println!("fallback_command=not-found"),
    }
}

fn print_declared_outputs(args: &[String]) {
    let outputs = args
        .iter()
        .filter_map(|arg| arg.split_once('='))
        .filter(|(key, value)| {
            !value.is_empty()
                && matches!(
                    key.to_ascii_uppercase().as_str(),
                    "O" | "OUTPUT"
                        | "M"
                        | "METRICS_FILE"
                        | "CHART_OUTPUT"
                        | "HISTOGRAM_FILE"
                        | "F"
                        | "FASTQ"
                        | "F2"
                        | "SECOND_END_FASTQ"
                        | "FU"
                        | "UNPAIRED_FASTQ"
                )
        })
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    if outputs.is_empty() {
        println!("declared_outputs=none");
    } else {
        println!("declared_outputs={}", outputs.join(","));
    }
}

#[derive(Debug, PartialEq, Eq)]
struct CommandMatrixEntry {
    name: String,
    status: String,
    native_scope: String,
    fallback_scope: String,
}

fn picard_reference_version() -> &'static str {
    COMMAND_MATRIX_YAML
        .lines()
        .find_map(|line| line.trim().strip_prefix("picard_reference:"))
        .map(|value| value.trim().trim_matches('"'))
        .unwrap_or("unknown")
}

fn command_matrix_entry(command: &str) -> Option<CommandMatrixEntry> {
    let mut entries = Vec::new();
    let mut current: Option<CommandMatrixEntry> = None;
    for line in COMMAND_MATRIX_YAML.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("- name:") {
            if let Some(entry) = current.take() {
                entries.push(entry);
            }
            current = Some(CommandMatrixEntry {
                name: unquote_yaml_scalar(name.trim()),
                status: String::new(),
                native_scope: String::new(),
                fallback_scope: String::new(),
            });
            continue;
        }
        let Some(entry) = current.as_mut() else {
            continue;
        };
        if let Some(value) = trimmed.strip_prefix("status:") {
            entry.status = unquote_yaml_scalar(value.trim());
        } else if let Some(value) = trimmed.strip_prefix("native_scope:") {
            entry.native_scope = unquote_yaml_scalar(value.trim());
        } else if let Some(value) = trimmed.strip_prefix("fallback_scope:") {
            entry.fallback_scope = unquote_yaml_scalar(value.trim());
        }
    }
    if let Some(entry) = current {
        entries.push(entry);
    }
    entries.into_iter().find(|entry| entry.name == command)
}

fn unquote_yaml_scalar(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        value[1..value.len() - 1]
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    } else {
        value.to_string()
    }
}

fn print_accelerationstatus_help() {
    println!(
        "\
Usage: picard AccelerationStatus

Reports turbo-picard's effective acceleration policy.

Output fields:
  backend                 Active execution backend for this build
  policy                  TURBO_PICARD_ACCELERATOR setting after validation
  htslib_worker_threads   Worker threads used for HTSlib index construction
  htslib_reader_threads   Worker threads used for normal BAM/CRAM readers
  htslib_writer_threads   Worker threads used for BAM/CRAM writers
  htslib_pipeline_reader_threads
                          Worker threads used when a command also has a reader thread
  gpu_runtime             Detected GPU runtime, if visible
  gpu_acceleration        Whether production GPU acceleration is enabled"
    );
}

fn run_acceleration_status() -> Result<(), String> {
    let policy = accelerator_policy()?;
    print_acceleration_status_lines_for_policy(&policy);

    if policy == "gpu-required" {
        return Err(
            "TURBO_PICARD_ACCELERATOR=gpu-required was set, but this build has no production GPU backend"
                .to_string(),
        );
    }

    Ok(())
}

fn print_acceleration_status_lines() {
    let policy = accelerator_policy().unwrap_or_else(|error| format!("invalid ({error})"));
    print_acceleration_status_lines_for_policy(&policy);
}

fn print_acceleration_status_lines_for_policy(policy: &str) {
    let workers = turbo_picard_core::bgzf_threads::htslib_worker_threads();
    let reader_threads = turbo_picard_core::bgzf_threads::bgzf_threads_for(
        turbo_picard_core::bgzf_threads::HtsThreadRole::Reader,
    )
    .unwrap_or(1);
    let writer_threads = turbo_picard_core::bgzf_threads::bgzf_threads_for(
        turbo_picard_core::bgzf_threads::HtsThreadRole::Writer,
    )
    .unwrap_or(1);
    let pipeline_reader_threads = turbo_picard_core::bgzf_threads::bgzf_threads_for(
        turbo_picard_core::bgzf_threads::HtsThreadRole::PipelineReader,
    )
    .unwrap_or(1);
    let gpu_runtime = detect_gpu_runtime();

    println!("backend=cpu");
    println!("policy={policy}");
    println!("htslib_worker_threads={workers}");
    println!("htslib_reader_threads={reader_threads}");
    println!("htslib_writer_threads={writer_threads}");
    println!("htslib_pipeline_reader_threads={pipeline_reader_threads}");
    println!("gpu_runtime={}", gpu_runtime.unwrap_or("none"));
    println!("gpu_acceleration=not-enabled");
}

fn accelerator_policy() -> Result<String, String> {
    match env::var("TURBO_PICARD_ACCELERATOR") {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "" => Ok("auto".to_string()),
                "auto" | "cpu" | "off" | "gpu-required" => Ok(normalized),
                _ => Err(format!(
                    "unsupported TURBO_PICARD_ACCELERATOR={value}; use auto, cpu, off, or gpu-required"
                )),
            }
        }
        Err(env::VarError::NotPresent) => Ok("auto".to_string()),
        Err(env::VarError::NotUnicode(_)) => {
            Err("TURBO_PICARD_ACCELERATOR must be valid UTF-8".to_string())
        }
    }
}

fn env_bool(name: &str) -> Result<Option<bool>, String> {
    match env::var(name) {
        Ok(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            match normalized.as_str() {
                "" => Ok(None),
                "1" | "true" | "yes" | "on" => Ok(Some(true)),
                "0" | "false" | "no" | "off" => Ok(Some(false)),
                _ => Err(format!(
                    "unsupported {name}={value}; use true/false, 1/0, yes/no, or on/off"
                )),
            }
        }
        Err(env::VarError::NotPresent) => Ok(None),
        Err(env::VarError::NotUnicode(_)) => Err(format!("{name} must be valid UTF-8")),
    }
}

fn detect_gpu_runtime() -> Option<&'static str> {
    if command_exists("nvidia-smi") {
        Some("cuda")
    } else if command_exists("rocminfo") || command_exists("rocm-smi") {
        Some("rocm")
    } else if cfg!(target_os = "macos") {
        Some("metal")
    } else {
        None
    }
}

fn command_exists(program: &str) -> bool {
    Command::new(program)
        .arg("--help")
        .output()
        .map(|output| output.status.success() || output.status.code().is_some())
        .unwrap_or(false)
}

fn print_markduplicates_help() {
    println!(
        "\
Usage: picard MarkDuplicates I=<input.bam|input.cram> O=<output.bam|output.cram> M=<metrics.txt> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file; may be repeated
  OUTPUT / O            Output SAM, BAM, or CRAM file
  METRICS_FILE / M      Duplication metrics file

Common options:
  ASSUME_SORTED / AS
  ASSUME_SORT_ORDER / ASO
  REMOVE_DUPLICATES
  CREATE_INDEX
  CREATE_MD5_FILE
  VALIDATION_STRINGENCY
  TMP_DIR
  MAX_RECORDS_IN_RAM"
    );
}

fn print_sortsam_help() {
    println!(
        "\
Usage: picard SortSam I=<input.bam|input.cram> O=<output.bam|output.cram> SORT_ORDER=<coordinate|queryname>

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Output SAM, BAM, or CRAM file
  SORT_ORDER / SO       coordinate or queryname"
    );
}

fn print_cleansam_help() {
    println!(
        "\
Usage: picard CleanSam I=<input.bam|input.cram> O=<output.bam|output.cram> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Output SAM, BAM, or CRAM file

Common options:
  CREATE_INDEX
  CREATE_MD5_FILE
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_mergesamfiles_help() {
    println!(
        "\
Usage: picard MergeSamFiles I=<input.bam|input.cram> [I=<input2.bam|input2.cram> ...] O=<output.bam|output.cram> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file; may be repeated
  OUTPUT / O            Output SAM, BAM, or CRAM file

Common options:
  SORT_ORDER / SO       coordinate, queryname, or unsorted; defaults to coordinate
  ASSUME_SORTED / AS    Skip sortedness validation for trusted sorted inputs
  COMMENT / CO          Add one or more @CO header comments
  CREATE_INDEX
  CREATE_MD5_FILE
  MERGE_SEQUENCE_DICTIONARIES
                        Accepted when input dictionaries already match"
    );
}

fn print_buildbamindex_help() {
    println!(
        "\
Usage: picard BuildBamIndex I=<input.bam> [O=<output.bai>]

Required arguments:
  INPUT / I             Coordinate-sorted BAM input

Common options:
  OUTPUT / O            Output BAI path; defaults to input with .bai extension
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_samtofastq_help() {
    println!(
        "\
Usage: picard SamToFastq I=<input.bam|input.cram> FASTQ=<reads.fastq> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  FASTQ                 Output FASTQ file

Common options:
  SECOND_END_FASTQ      Output FASTQ for second-of-pair reads
  UNPAIRED_FASTQ        Output FASTQ for unpaired reads
  INTERLEAVE            Write paired reads interleaved to FASTQ
  RE_REVERSE            Reverse-complement reverse-strand reads
  READ1_TRIM            Trim this many bases from first/unpaired reads
  READ2_TRIM            Trim this many bases from second-of-pair reads
  READ1_MAX_BASES_TO_WRITE
                        Maximum first/unpaired read length after trimming
  READ2_MAX_BASES_TO_WRITE
                        Maximum second-of-pair read length after trimming
  QUALITY / Q           End-trim reads using Picard's quality trimming
  CLIPPING_ATTRIBUTE    Integer SAM tag that stores a 1-based clip point
  CLIPPING_ACTION       X to trim, N to mask bases, or a quality value
  CLIPPING_MIN_LENGTH   Minimum retained clipped length
  CREATE_MD5_FILE       Write Picard-style .md5 sidecars for FASTQ outputs
  CREATE_INDEX          Accepted for Picard command-line compatibility
  REFERENCE_SEQUENCE / R Accepted for Picard command-line compatibility
  TMP_DIR
  MAX_RECORDS_IN_RAM
  USE_JDK_DEFLATER
  USE_JDK_INFLATER"
    );
}

fn print_fastqtosam_help() {
    println!(
        "\
Usage: picard FastqToSam FASTQ=<reads.fastq> O=<unmapped.bam> SM=<sample> [options]

Required arguments:
  FASTQ / F1            First or single-end FASTQ file
  OUTPUT / O            Output SAM or BAM file
  SAMPLE_NAME / SM      Read-group sample name

Common options:
  FASTQ2 / F2           Second-end FASTQ file
  USE_SEQUENTIAL_FASTQS Concatenate _001, _002, ... FASTQ shards
  READ_GROUP_NAME / RG  Read-group ID; defaults to A
  LIBRARY_NAME / LB
  PLATFORM / PL
  PLATFORM_UNIT / PU
  QUALITY_FORMAT        Auto, Standard, Illumina, or Solexa
  MIN_Q / MAX_Q         Validate decoded input qualities
  ALLOW_AND_IGNORE_EMPTY_LINES
  ALLOW_EMPTY_FASTQ
  SORT_ORDER            queryname, coordinate, or unsorted
  COMMENT               Add @CO header line; may be repeated
  CREATE_MD5_FILE       Write Picard-style .md5 sidecar for OUTPUT
  CREATE_INDEX          Accepted for Picard command-line compatibility
  REFERENCE_SEQUENCE / R Accepted for Picard command-line compatibility
  TMP_DIR
  MAX_RECORDS_IN_RAM
  USE_JDK_DEFLATER
  USE_JDK_INFLATER"
    );
}

fn print_addorreplacereadgroups_help() {
    println!(
        "\
Usage: picard AddOrReplaceReadGroups I=<input.bam|input.cram> O=<output.bam|output.cram> RGLB=<library> RGPL=<platform> RGPU=<unit> RGSM=<sample> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Output SAM, BAM, or CRAM file
  RGLB                  Read-group library
  RGPL                  Read-group platform
  RGPU                  Read-group platform unit
  RGSM                  Read-group sample

Common options:
  RGID                  Read-group ID; defaults to 1
  RGCN
  RGDS
  RGDT
  RGPI
  RGPG
  RGPM
  RGKS
  RGFO
  CREATE_MD5_FILE       Write Picard-style .md5 sidecar for OUTPUT
  CREATE_INDEX          Create BAM index sidecar for BAM output; accepted without index for SAM
  REFERENCE_SEQUENCE / R Accepted for Picard command-line compatibility
  COMPRESSION_LEVEL
  MAX_RECORDS_IN_RAM
  TMP_DIR
  USE_JDK_DEFLATER
  USE_JDK_INFLATER"
    );
}

fn print_collectalignmentsummarymetrics_help() {
    println!(
        "\
Usage: picard CollectAlignmentSummaryMetrics I=<input.bam|input.cram> O=<metrics.txt> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Alignment summary metrics file

Supported options:
  METRIC_ACCUMULATION_LEVEL=ALL_READS|SAMPLE|LIBRARY|READ_GROUP
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_collectbasedistributionbycycle_help() {
    println!(
        "\
Usage: picard CollectBaseDistributionByCycle I=<input.bam|input.cram> O=<metrics.txt> CHART=<chart.pdf> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Base distribution metrics file
  CHART_OUTPUT / CHART  Chart artifact path"
    );
}

fn print_collectgcbiasmetrics_help() {
    println!(
        "\
Usage: picard CollectGcBiasMetrics I=<input.bam|input.cram> O=<detail.txt> S=<summary.txt> CHART=<chart.pdf> R=<reference.fa> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            GC-bias detail metrics file
  SUMMARY_OUTPUT / S    GC-bias summary metrics file
  CHART_OUTPUT / CHART  Chart artifact path
  REFERENCE_SEQUENCE / R Reference FASTA file

Implemented options:
  SCAN_WINDOW_SIZE
  MINIMUM_GENOME_FRACTION
  ALSO_IGNORE_DUPLICATES
  ASSUME_SORTED
  STOP_AFTER"
    );
}

fn print_collecthsmetrics_help() {
    println!(
        "\
Usage: picard CollectHsMetrics I=<input.bam|input.cram> O=<metrics.txt> BAIT=<baits.interval_list> TARGET=<targets.interval_list> R=<reference.fa> [options]

Required arguments:
  INPUT / I             Coordinate-sorted SAM, BAM, or CRAM file
  OUTPUT / O            Hybrid-capture metrics file
  BAIT_INTERVALS / BAIT Bait interval_list file
  TARGET_INTERVALS / TARGET Target interval_list file
  REFERENCE_SEQUENCE / R Reference FASTA file

Scaffold options accepted for argument validation:
  CLIP_OVERLAPPING_READS
  NEAR_DISTANCE
  METRIC_ACCUMULATION_LEVEL=ALL_READS
  ASSUME_SORTED
  STOP_AFTER

Native bait/target accumulation is not implemented yet. Configure
TURBO_PICARD_FALLBACK_COMMAND to run upstream Picard for production use."
    );
}

fn print_collectqualityyieldmetrics_help() {
    println!(
        "\
Usage: picard CollectQualityYieldMetrics I=<input.bam|input.cram> O=<metrics.txt> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Quality yield metrics file

Common options:
  USE_ORIGINAL_QUALITIES
  INCLUDE_SECONDARY_ALIGNMENTS
  INCLUDE_SUPPLEMENTAL_ALIGNMENTS
  STOP_AFTER"
    );
}

fn print_collectinsertsizemetrics_help() {
    println!(
        "\
Usage: picard CollectInsertSizeMetrics I=<input.bam|input.cram> O=<metrics.txt> H=<histogram.pdf> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Insert size metrics file
  HISTOGRAM_FILE / H    Histogram artifact path

Supported options:
  INCLUDE_DUPLICATES=false
  METRIC_ACCUMULATION_LEVEL=ALL_READS
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_collectmultiplemetrics_help() {
    println!(
        "\
Usage: picard CollectMultipleMetrics I=<input.bam|input.cram> O=<output-prefix> PROGRAM=null PROGRAM=<collector> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Base output prefix

Supported PROGRAM values:
  CollectAlignmentSummaryMetrics
  CollectBaseDistributionByCycle
  CollectGcBiasMetrics
  CollectInsertSizeMetrics
  QualityScoreDistribution
  MeanQualityByCycle
  CollectQualityYieldMetrics
  CollectWgsMetrics

Common options:
  FILE_EXTENSION / EXT  Appended to metric text outputs"
    );
}

fn print_collectwgsmetrics_help() {
    println!(
        "\
Usage: picard CollectWgsMetrics I=<input.bam|input.cram> O=<metrics.txt> R=<reference.fa> [options]

Required arguments:
  INPUT / I             Coordinate-sorted SAM, BAM, or CRAM file
  OUTPUT / O            Whole-genome metrics file
  REFERENCE_SEQUENCE / R Reference FASTA file

Supported options:
  COUNT_UNPAIRED
  MINIMUM_MAPPING_QUALITY
  MINIMUM_BASE_QUALITY
  COVERAGE_CAP
  LOCUS_ACCUMULATION_CAP
  INTERVALS
  STOP_AFTER
  SAMPLE_SIZE
  INCLUDE_BQ_HISTOGRAM
  USE_FAST_ALGORITHM
  VALIDATION_STRINGENCY
  QUIET

INCLUDE_BQ_HISTOGRAM defaults to false to match Picard 3.4.0 histogram output.
USE_FAST_ALGORITHM=true switches to a leaner native WGS mode by defaulting
SAMPLE_SIZE to 0 unless the caller sets it explicitly.
TURBO_PICARD_WGS_FAST_DEFAULT=true applies that SAMPLE_SIZE default when
USE_FAST_ALGORITHM is not set on the command line."
    );
}

fn print_fixmateinformation_help() {
    println!(
        "\
Usage: picard FixMateInformation I=<input.bam|input.cram> O=<output.bam|output.cram> [options]

Required arguments:
  INPUT / I             SAM, BAM, or CRAM input file; may be repeated
  OUTPUT / O            Output SAM, BAM, or CRAM file

Supported options:
  ADD_MATE_CIGAR / MC
  ASSUME_SORTED
  SORT_ORDER=queryname|coordinate|unsorted
  IGNORE_MISSING_MATES=true
  CREATE_INDEX
  CREATE_MD5_FILE
  TMP_DIR
  MAX_RECORDS_IN_RAM
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_intervallisttools_help() {
    println!(
        "\
Usage: picard IntervalListTools I=<input.interval_list> O=<output.interval_list> [options]

Required arguments:
  INPUT / I             One or more interval_list files
  OUTPUT / O            Output interval_list file

Supported options:
  ACTION=CONCAT
  SORT
  UNIQUE
  PADDING
  DONT_MERGE_ABUTTING=false
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_revertsam_help() {
    println!(
        "\
Usage: picard RevertSam I=<input.bam|input.cram> O=<output.bam|output.cram> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Output SAM, BAM, or CRAM file

Supported options:
  REMOVE_ALIGNMENT_INFORMATION=true
  REMOVE_DUPLICATE_INFORMATION=true
  RESTORE_ORIGINAL_QUALITIES=true
  RESTORE_HARDCLIPS=false
  SORT_ORDER=queryname|coordinate|unsorted
  CREATE_INDEX
  CREATE_MD5_FILE
  COMPRESSION_LEVEL
  TMP_DIR
  MAX_RECORDS_IN_RAM
  USE_JDK_DEFLATER
  USE_JDK_INFLATER
  ATTRIBUTE_TO_CLEAR
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_setnmmdanduqtags_help() {
    println!(
        "\
Usage: picard SetNmMdAndUqTags I=<input.bam|input.cram> O=<output.bam|output.cram> R=<reference.fa> [options]

Required arguments:
  INPUT / I             Coordinate-sorted SAM, BAM, or CRAM input file
  OUTPUT / O            Output SAM, BAM, or CRAM file
  REFERENCE_SEQUENCE / R Reference FASTA file

Supported options:
  IS_BISULFITE_SEQUENCE=false
  SET_ONLY_UQ
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_validatesamfile_help() {
    println!(
        "\
Usage: picard ValidateSamFile I=<input.sam|input.bam|input.cram> [O=<summary.txt>] [MODE=SUMMARY]

Supported options:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Optional summary output; defaults to stdout
  MODE / M              SUMMARY only
  SKIP_MATE_VALIDATION / SMV
  MAX_OUTPUT / MO       Accepted for SUMMARY mode
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_liftovervcf_help() {
    println!(
        "\
Usage: picard LiftoverVcf I=<input.vcf> O=<output.vcf> CHAIN=<chain> REJECT=<reject.vcf> R=<reference.fa>

Supported options:
  INPUT / I
  OUTPUT / O
  CHAIN / C             UCSC chain file with positive-strand single-block mappings
  REJECT                Reject VCF path
  REFERENCE_SEQUENCE / R Target reference FASTA with adjacent .dict
  WARN_ON_MISSING_CONTIG / WMC
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_qualityscoredistribution_help() {
    println!(
        "\
Usage: picard QualityScoreDistribution I=<input.bam|input.cram> O=<metrics.txt> CHART=<chart.pdf> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Quality score distribution metrics file
  CHART_OUTPUT / CHART  Chart artifact path

Supported options:
  ALIGNED_READS_ONLY
  PF_READS_ONLY / PF
  INCLUDE_NO_CALLS
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_meanqualitybycycle_help() {
    println!(
        "\
Usage: picard MeanQualityByCycle I=<input.bam|input.cram> O=<metrics.txt> CHART=<chart.pdf> [options]

Required arguments:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Mean quality by cycle metrics file
  CHART_OUTPUT / CHART  Chart artifact path

Supported options:
  ALIGNED_READS_ONLY
  PF_READS_ONLY
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_createsequencedictionary_help() {
    println!(
        "\
Usage: picard CreateSequenceDictionary R=<reference.fasta> O=<reference.dict> [options]

Required arguments:
  REFERENCE_SEQUENCE / R Reference FASTA file
  OUTPUT / O            Output sequence dictionary"
    );
}

fn print_normalizefasta_help() {
    println!(
        "\
Usage: picard NormalizeFasta I=<input.fasta> O=<output.fasta> [options]

Required arguments:
  INPUT / I             Input FASTA file
  OUTPUT / O            Output normalized FASTA file"
    );
}

fn print_bedtointervallist_help() {
    println!(
        "\
Usage: picard BedToIntervalList I=<input.bed> O=<output.interval_list> SD=<reference.dict>

Required arguments:
  INPUT / I             Input BED file
  OUTPUT / O            Output interval_list file
  SEQUENCE_DICTIONARY / SD Reference sequence dictionary"
    );
}

fn print_viewsam_help() {
    println!(
        "\
Usage: picard ViewSam I=<input.sam|input.bam|input.cram> [O=<output.sam|output.bam|output.cram>]

Supported options:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Output SAM, BAM, or CRAM file; defaults to SAM on stdout
  INTERVAL_LIST         Restrict output to records overlapping intervals
  ALIGNMENT_STATUS      All, Aligned, or Unaligned
  PF_STATUS             All, PF, or NonPF
  HEADER_ONLY           Emit only SAM header
  RECORDS_ONLY          Emit only SAM records for SAM/stdout output
  COMPRESSION_LEVEL     0-9 for BAM output
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_replacesamheader_help() {
    println!(
        "\
Usage: picard ReplaceSamHeader I=<input.sam|input.bam|input.cram> O=<output.sam|output.bam|output.cram> HEADER=<header.sam|header.bam|header.cram>

Supported options:
  INPUT / I             Input SAM, BAM, or CRAM file
  OUTPUT / O            Output SAM, BAM, or CRAM file
  HEADER / H            SAM/BAM/CRAM file whose header replaces the input header
  CREATE_MD5_FILE       Write Picard-style .md5 sidecar for OUTPUT
  COMPRESSION_LEVEL     0-9 for BAM output
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_updatevcfsequencedictionary_help() {
    println!(
        "\
Usage: picard UpdateVcfSequenceDictionary I=<input.vcf> O=<output.vcf> SD=<reference.dict>

Supported options:
  INPUT / I
  OUTPUT / O
  SEQUENCE_DICTIONARY / SD / D
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_gathervcfs_help() {
    println!(
        "\
Usage: picard GatherVcfs I=<input.vcf> [I=<input2.vcf> ...] O=<output.vcf>

Supported options:
  INPUT / I             Input VCF file; may be repeated
  OUTPUT / O            Output VCF file
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_sortvcf_help() {
    println!(
        "\
Usage: picard SortVcf I=<input.vcf> [I=<input2.vcf> ...] O=<output.vcf> [SD=<reference.dict>]

Supported options:
  INPUT / I             Input VCF file; may be repeated
  OUTPUT / O            Output VCF file
  SEQUENCE_DICTIONARY / SD / D Optional sort dictionary
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_mergevcfs_help() {
    println!(
        "\
Usage: picard MergeVcfs I=<input.vcf> [I=<input2.vcf> ...] O=<output.vcf> [SD=<reference.dict>]

Supported options:
  INPUT / I             Input VCF file; may be repeated
  OUTPUT / O            Output VCF file
  SEQUENCE_DICTIONARY / SD / D Optional sort dictionary
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn run_markduplicates(args: &[String]) -> Result<(), String> {
    let picard_args = normalize_picard_args_for_command("MarkDuplicates", args)
        .map_err(|error| error.to_string())?;
    let config =
        MarkDuplicatesConfig::try_from_args(&picard_args).map_err(|error| error.to_string())?;

    turbo_picard_markdup::run(&config).map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortOrder {
    Coordinate,
    QueryName,
    Unsorted,
}

fn run_sortsam(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("SortSam", args).map_err(|error| error.to_string())?;
    reject_unsupported_sortsam_args(&args)?;
    let input = required_scalar(&args, "INPUT")?;
    let output = required_scalar(&args, "OUTPUT")?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let sort_order = match required_scalar(&args, "SORT_ORDER")?.as_str() {
        "coordinate" => SortOrder::Coordinate,
        "queryname" => SortOrder::QueryName,
        value => return Err(format!("unsupported SortSam SORT_ORDER: {value}")),
    };
    if create_index && sort_order != SortOrder::Coordinate {
        return Err("SortSam CREATE_INDEX=true requires SORT_ORDER=coordinate".to_string());
    }
    let tmp_dir = optional_scalar(&args, "TMP_DIR")?
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let max_records_in_ram = optional_u32(&args, "MAX_RECORDS_IN_RAM")?
        .map(|value| value as usize)
        .unwrap_or(500_000);

    if has_sam_extension(&input)
        && has_sam_extension(&output)
        && !create_index
        && !create_md5_file
        && compression_level.is_none()
    {
        return run_sortsam_sam_text(&input, &output, sort_order, tmp_dir, max_records_in_ram);
    }

    let reference = picard_reference(&args)?;
    let reader = open_bam_reader_with_reference(&input, reference.as_deref())
        .map_err(|error| error.to_string())?;
    let header = sorted_header(reader.header(), sort_order);
    let format = output_format(&output)?;
    if create_index && format != bam::Format::Bam {
        return Err("SortSam CREATE_INDEX=true requires BAM output".to_string());
    }

    if input_is_sorted(&input, sort_order, reference.as_deref())? {
        let mut reader = open_bam_reader_with_reference(&input, reference.as_deref())?;
        let mut writer = bam_writer_for_path_with_reference(
            &output,
            &header,
            format,
            reference.as_deref(),
            compression_level,
        )?;
        for record in reader.records() {
            let record = record.map_err(|error| error.to_string())?;
            writer.write(&record).map_err(|error| error.to_string())?;
        }
        drop(writer);
        write_requested_sidecars(&output, create_md5_file, create_index)?;
        return Ok(());
    }

    let mut reader = open_bam_reader_with_reference(&input, reference.as_deref())?;
    let mut records = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    match sort_order {
        SortOrder::Coordinate => records.sort_unstable_by(compare_coordinate),
        SortOrder::QueryName => records.sort_unstable_by(compare_queryname),
        SortOrder::Unsorted => unreachable!("SortSam rejects SORT_ORDER=unsorted"),
    }

    let mut writer = bam_writer_for_path_with_reference(
        &output,
        &header,
        format,
        reference.as_deref(),
        compression_level,
    )?;
    for record in records {
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    drop(writer);

    write_requested_sidecars(&output, create_md5_file, create_index)
}

fn run_cleansam(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("CleanSam", args).map_err(|error| error.to_string())?;
    reject_unsupported_cleansam_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CleanSam")?;
    let output = required_scalar_for(&args, "OUTPUT", "CleanSam")?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);

    if has_sam_extension(&input)
        && has_sam_extension(&output)
        && !create_index
        && !create_md5_file
        && compression_level.is_none()
    {
        return run_cleansam_sam_text(&input, &output);
    }

    let reference = picard_reference(&args)?;
    let mut reader = open_bam_reader_with_reference(&input, reference.as_deref())?;
    let header = bam::Header::from_template(reader.header());
    let target_lengths = (0..reader.header().target_count())
        .map(|tid| reader.header().target_len(tid).unwrap_or(0))
        .collect::<Vec<_>>();
    let format = output_format_for(&output, "CleanSam")?;
    if create_index && format != bam::Format::Bam {
        return Err("CleanSam CREATE_INDEX=true requires BAM output".to_string());
    }
    let mut writer = bam_writer_for_path_with_reference(
        &output,
        &header,
        format,
        reference.as_deref(),
        compression_level,
    )?;
    for record in reader.records() {
        let mut record = record.map_err(|error| error.to_string())?;
        clean_sam_record(&mut record, &target_lengths)?;
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    drop(writer);

    write_requested_sidecars(&output, create_md5_file, create_index)
}

fn run_cleansam_sam_text(input: &str, output: &str) -> Result<(), String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut writer = BufWriter::with_capacity(
        1024 * 1024,
        fs::File::create(output).map_err(|error| error.to_string())?,
    );
    let mut target_lengths = BTreeMap::<String, u64>::new();
    let mut line = String::new();
    let mut output_buffer = Vec::with_capacity(8 * 1024 * 1024);

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with('@') {
            if line.starts_with("@SQ\t") {
                let mut name = None;
                let mut len = None;
                for field in line.trim_end_matches(['\r', '\n']).split('\t').skip(1) {
                    if let Some(value) = field.strip_prefix("SN:") {
                        name = Some(value.to_string());
                    } else if let Some(value) = field.strip_prefix("LN:") {
                        len = Some(
                            value
                                .parse::<u64>()
                                .map_err(|_| "malformed CleanSam @SQ LN".to_string())?,
                        );
                    }
                }
                if let (Some(name), Some(len)) = (name, len) {
                    target_lengths.insert(name, len);
                }
            }
            writer
                .write_all(line.as_bytes())
                .map_err(|error| error.to_string())?;
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        append_cleaned_sam_text_record(&mut output_buffer, &line, &target_lengths)?;
        if output_buffer.len() >= 8 * 1024 * 1024 {
            writer
                .write_all(&output_buffer)
                .map_err(|error| error.to_string())?;
            output_buffer.clear();
        }
    }
    if !output_buffer.is_empty() {
        writer
            .write_all(&output_buffer)
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn append_cleaned_sam_text_record(
    output: &mut Vec<u8>,
    line: &str,
    target_lengths: &BTreeMap<String, u64>,
) -> Result<(), String> {
    let fields = line
        .trim_end_matches(['\r', '\n'])
        .split('\t')
        .collect::<Vec<_>>();
    if fields.len() < 11 {
        return Err("malformed CleanSam SAM record".to_string());
    }
    let flags = fields[1]
        .parse::<u16>()
        .map_err(|_| "malformed CleanSam SAM flag".to_string())?;
    let mut new_mapq: Option<&'static str> = None;
    let mut new_cigar: Option<String> = None;
    if flags & 0x4 != 0 {
        new_mapq = Some("0");
    } else if fields[2] != "*"
        && let Some(target_len) = target_lengths.get(fields[2]).copied()
    {
        let pos = fields[3]
            .parse::<u64>()
            .map_err(|_| "malformed CleanSam SAM position".to_string())?;
        let start = pos.saturating_sub(1);
        if start >= target_len {
            return Err("unsupported CleanSam alignment starting beyond reference end".to_string());
        }
        if let Some(cleaned) = clean_cigar_text(fields[5], start, target_len)? {
            new_cigar = Some(cleaned);
        }
    }

    for (index, field) in fields.iter().enumerate() {
        if index > 0 {
            output.extend_from_slice(b"\t");
        }
        match index {
            4 => output.extend_from_slice(new_mapq.unwrap_or(field).as_bytes()),
            5 => output.extend_from_slice(new_cigar.as_deref().unwrap_or(field).as_bytes()),
            _ => output.extend_from_slice(field.as_bytes()),
        }
    }
    output.extend_from_slice(b"\n");
    Ok(())
}

fn clean_cigar_text(cigar: &str, start: u64, target_len: u64) -> Result<Option<String>, String> {
    if cigar == "*" {
        return Ok(None);
    }
    let parsed = parse_cigar_text(cigar)?;
    let reference_end = start.saturating_add(cigar_reference_len_text(&parsed));
    let read_len = cigar_read_len_text(&parsed);
    if reference_end > target_len && read_len > 0 {
        let overhang = reference_end - target_len;
        if overhang >= read_len {
            return Ok(Some(format!("{overhang}S")));
        }
    }
    let mut ref_pos = start;
    let mut changed = false;
    let mut cleaned = Vec::<(u64, char)>::new();
    for (len, op) in parsed {
        match op {
            'M' | '=' | 'X' => {
                if ref_pos >= target_len {
                    push_text_cigar(&mut cleaned, len, 'S');
                    changed = true;
                } else if ref_pos + len > target_len {
                    let keep = target_len - ref_pos;
                    push_text_cigar(&mut cleaned, keep, op);
                    push_text_cigar(&mut cleaned, len - keep, 'S');
                    ref_pos += len;
                    changed = true;
                } else {
                    push_text_cigar(&mut cleaned, len, op);
                    ref_pos += len;
                }
            }
            'D' | 'N' => {
                if ref_pos >= target_len {
                    changed = true;
                } else if ref_pos + len > target_len {
                    push_text_cigar(&mut cleaned, target_len - ref_pos, op);
                    ref_pos += len;
                    changed = true;
                } else {
                    push_text_cigar(&mut cleaned, len, op);
                    ref_pos += len;
                }
            }
            'I' => {
                if ref_pos >= target_len {
                    push_text_cigar(&mut cleaned, len, 'S');
                    changed = true;
                } else {
                    push_text_cigar(&mut cleaned, len, op);
                }
            }
            'S' | 'H' | 'P' => push_text_cigar(&mut cleaned, len, op),
            _ => return Err(format!("malformed CleanSam CIGAR op: {op}")),
        }
    }
    if !changed {
        return Ok(None);
    }
    let mut text = String::new();
    for (len, op) in cleaned {
        if len > 0 {
            text.push_str(&len.to_string());
            text.push(op);
        }
    }
    Ok(Some(text))
}

fn bam_cigar_to_op(cigar: Cigar) -> Option<(u32, u8)> {
    match cigar {
        Cigar::Match(len) | Cigar::Equal(len) | Cigar::Diff(len) => Some((len, b'M')),
        Cigar::Ins(len) => Some((len, b'I')),
        Cigar::Del(len) => Some((len, b'D')),
        Cigar::RefSkip(len) => Some((len, b'N')),
        Cigar::SoftClip(len) => Some((len, b'S')),
        Cigar::HardClip(_) | Cigar::Pad(_) => None,
    }
}

fn parse_cigar_text(cigar: &str) -> Result<Vec<(u64, char)>, String> {
    let mut ops = Vec::new();
    let mut len = 0_u64;
    let mut saw_digit = false;
    for byte in cigar.bytes() {
        if byte.is_ascii_digit() {
            saw_digit = true;
            len = len
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(byte - b'0')))
                .ok_or_else(|| "malformed CleanSam CIGAR length".to_string())?;
        } else {
            if !saw_digit || len == 0 {
                return Err("malformed CleanSam CIGAR".to_string());
            }
            ops.push((len, char::from(byte)));
            len = 0;
            saw_digit = false;
        }
    }
    if saw_digit {
        return Err("malformed CleanSam CIGAR".to_string());
    }
    Ok(ops)
}

fn cigar_reference_len_text(cigars: &[(u64, char)]) -> u64 {
    cigars
        .iter()
        .filter(|(_, op)| matches!(op, 'M' | '=' | 'X' | 'D' | 'N'))
        .map(|(len, _)| *len)
        .sum()
}

fn cigar_read_len_text(cigars: &[(u64, char)]) -> u64 {
    cigars
        .iter()
        .filter(|(_, op)| matches!(op, 'M' | '=' | 'X' | 'I' | 'S'))
        .map(|(len, _)| *len)
        .sum()
}

fn push_text_cigar(cigars: &mut Vec<(u64, char)>, len: u64, op: char) {
    if len == 0 {
        return;
    }
    if let Some((last_len, last_op)) = cigars.last_mut()
        && *last_op == op
    {
        *last_len += len;
        return;
    }
    cigars.push((len, op));
}

fn run_sortsam_sam_text(
    input: &str,
    output: &str,
    sort_order: SortOrder,
    tmp_dir: PathBuf,
    max_records_in_ram: usize,
) -> Result<(), String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut header_lines = Vec::<String>::new();
    let mut contig_order = BTreeMap::<String, i32>::new();
    let mut sort_config = ExternalSortConfig::new(tmp_dir);
    sort_config.max_records_in_ram = max_records_in_ram.max(1);
    sort_config.prefix = "turbo-picard-sortsam-sam".to_string();
    let mut sorter = ExternalSorter::new(sort_config)?;
    let mut line = String::new();

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with('@') {
            if line.starts_with("@SQ\t")
                && let Some(name) = line
                    .split('\t')
                    .skip(1)
                    .find_map(|field| field.strip_prefix("SN:"))
            {
                contig_order.insert(
                    name.trim_end_matches(['\r', '\n']).to_string(),
                    contig_order.len() as i32,
                );
            }
            header_lines.push(line.clone());
        } else if !line.trim().is_empty() {
            let record = SamTextSortRecord::parse(&line, &contig_order)?;
            let key = record.sort_key(sort_order);
            sorter.push(key, line.as_bytes().to_vec())?;
        }
    }
    let (records, _metrics) = sorter.finish()?;

    let mut writer = BufWriter::with_capacity(
        1024 * 1024,
        fs::File::create(output).map_err(|error| error.to_string())?,
    );
    write_sorted_sam_text_header(&mut writer, &header_lines, sort_order)?;
    for record in records {
        writer
            .write_all(&record.payload)
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

#[derive(Debug)]
struct SamTextSortRecord {
    qname: String,
    flags: u16,
    tid: i32,
    pos: i64,
}

impl SamTextSortRecord {
    fn parse(line: &str, contig_order: &BTreeMap<String, i32>) -> Result<Self, String> {
        let mut fields = line.trim_end_matches(['\r', '\n']).split('\t');
        let qname = fields
            .next()
            .ok_or_else(|| "malformed SortSam SAM record".to_string())?
            .to_string();
        let flags = fields
            .next()
            .ok_or_else(|| "malformed SortSam SAM record".to_string())?
            .parse::<u16>()
            .map_err(|_| "malformed SortSam SAM flag".to_string())?;
        let rname = fields
            .next()
            .ok_or_else(|| "malformed SortSam SAM record".to_string())?;
        let tid = if rname == "*" {
            i32::MAX
        } else {
            *contig_order
                .get(rname)
                .ok_or_else(|| format!("SortSam record contig {rname} missing from header"))?
        };
        let pos = fields
            .next()
            .ok_or_else(|| "malformed SortSam SAM record".to_string())?
            .parse::<i64>()
            .map_err(|_| "malformed SortSam SAM position".to_string())?
            - 1;
        Ok(Self {
            qname,
            flags,
            tid,
            pos,
        })
    }

    fn sort_key(&self, sort_order: SortOrder) -> Vec<u8> {
        let mut key = Vec::with_capacity(self.qname.len() * 4 + 32);
        match sort_order {
            SortOrder::Coordinate => self.push_coordinate_key(&mut key),
            SortOrder::QueryName => {
                push_lex_bytes(&mut key, self.qname.as_bytes());
                self.push_coordinate_key(&mut key);
            }
            SortOrder::Unsorted => unreachable!("SortSam rejects SORT_ORDER=unsorted"),
        }
        key
    }

    fn push_coordinate_key(&self, key: &mut Vec<u8>) {
        push_i64_sort_key(key, i64::from(self.tid));
        push_i64_sort_key(key, self.pos);
        push_lex_bytes(key, self.qname.as_bytes());
        key.extend_from_slice(&self.flags.to_be_bytes());
    }
}

fn push_i64_sort_key(key: &mut Vec<u8>, value: i64) {
    key.extend_from_slice(&((value as u64) ^ (1_u64 << 63)).to_be_bytes());
}

fn push_lex_bytes(key: &mut Vec<u8>, bytes: &[u8]) {
    for byte in bytes {
        key.push(1);
        key.push(*byte);
    }
    key.push(0);
}

fn write_sorted_sam_text_header(
    writer: &mut dyn Write,
    header_lines: &[String],
    sort_order: SortOrder,
) -> Result<(), String> {
    let sort_value = match sort_order {
        SortOrder::Coordinate => "coordinate",
        SortOrder::QueryName => "queryname",
        SortOrder::Unsorted => "unsorted",
    };
    let mut saw_hd = false;
    for line in header_lines {
        if line.starts_with("@HD\t") {
            saw_hd = true;
            let mut fields = vec!["@HD".to_string()];
            let mut saw_so = false;
            for field in line.trim_end_matches(['\r', '\n']).split('\t').skip(1) {
                if field.starts_with("SO:") {
                    fields.push(format!("SO:{sort_value}"));
                    saw_so = true;
                } else {
                    fields.push(field.to_string());
                }
            }
            if !saw_so {
                fields.push(format!("SO:{sort_value}"));
            }
            writer
                .write_all(fields.join("\t").as_bytes())
                .and_then(|_| writer.write_all(b"\n"))
                .map_err(|error| error.to_string())?;
        } else {
            writer
                .write_all(line.as_bytes())
                .map_err(|error| error.to_string())?;
        }
    }
    if !saw_hd {
        writer
            .write_all(format!("@HD\tVN:1.6\tSO:{sort_value}\n").as_bytes())
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn run_mergesamfiles(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("MergeSamFiles", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_mergesamfiles_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "MergeSamFiles")?;
    let output = required_scalar_for(&args, "OUTPUT", "MergeSamFiles")?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let assume_sorted = optional_bool(&args, "ASSUME_SORTED")?.unwrap_or(false);
    let sort_order = match optional_scalar(&args, "SORT_ORDER")?
        .unwrap_or_else(|| "coordinate".to_string())
        .as_str()
    {
        "coordinate" => SortOrder::Coordinate,
        "queryname" => SortOrder::QueryName,
        "unsorted" => SortOrder::Unsorted,
        value => return Err(format!("unsupported MergeSamFiles SORT_ORDER: {value}")),
    };
    if create_index && sort_order != SortOrder::Coordinate {
        return Err("MergeSamFiles CREATE_INDEX=true requires SORT_ORDER=coordinate".to_string());
    }

    let format = output_format_for(&output, "MergeSamFiles")?;
    if create_index && format != bam::Format::Bam {
        return Err("MergeSamFiles CREATE_INDEX=true requires BAM output".to_string());
    }

    let reference = picard_reference(&args)?;
    let merge_plan = build_merge_plan(&inputs, sort_order, assume_sorted, reference.as_deref())?;
    let interval_filter = merge_interval_filter(args.get("INTERVALS"), &merge_plan.target_names)?;
    let all_inputs_sorted = merge_plan.inputs.iter().all(|input| input.is_sorted);
    let mut header_builder = merge_plan.header_builder;
    for comment in args.get("COMMENT").into_iter().flatten() {
        header_builder.push_comment(comment);
    }
    let header = header_builder.into_header();
    let mut writer = bam_writer_for_path_with_reference(
        &output,
        &header,
        format,
        reference.as_deref(),
        compression_level,
    )?;

    if sort_order != SortOrder::Unsorted && all_inputs_sorted {
        write_kway_merged_records(
            &mut writer,
            &merge_plan.inputs,
            sort_order,
            reference.as_deref(),
            interval_filter.as_ref(),
        )?;
    } else {
        let mut records = collect_merge_records(
            &merge_plan.inputs,
            reference.as_deref(),
            interval_filter.as_ref(),
        )?;
        match sort_order {
            SortOrder::Coordinate => records.sort_by(compare_coordinate),
            SortOrder::QueryName => records.sort_by(compare_queryname),
            SortOrder::Unsorted => {}
        }
        for record in records {
            writer.write(&record).map_err(|error| error.to_string())?;
        }
    }
    drop(writer);

    write_requested_sidecars(&output, create_md5_file, create_index)
}

fn run_buildbamindex(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("BuildBamIndex", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_buildbamindex_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "BuildBamIndex")?;
    let output = optional_scalar(&args, "OUTPUT")?.unwrap_or_else(|| picard_bai_path(&input));
    if !has_extension(&input, "bam") {
        return Err(format!(
            "unsupported BuildBamIndex input format for {input}; expected .bam"
        ));
    }

    let reader = open_bam_reader(&input).map_err(|error| error.to_string())?;
    if header_sort_order(reader.header()).as_deref() != Some("coordinate") {
        return Err("BuildBamIndex requires coordinate-sorted BAM input".to_string());
    }
    drop(reader);

    index::build(
        &input,
        Some(&output),
        index::Type::Bai,
        turbo_picard_core::bgzf_threads::htslib_worker_threads(),
    )
    .map_err(|error| error.to_string())
}

fn run_samtofastq(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("SamToFastq", args).map_err(|error| error.to_string())?;
    reject_unsupported_samtofastq_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "SamToFastq")?;
    let output_per_rg = optional_bool(&args, "OUTPUT_PER_RG")?.unwrap_or(false);
    let compress_outputs_per_rg = optional_bool(&args, "COMPRESS_OUTPUTS_PER_RG")?.unwrap_or(false);
    let rg_tag = optional_scalar(&args, "RG_TAG")?.unwrap_or_else(|| "PU".to_string());
    let output_dir = optional_scalar(&args, "OUTPUT_DIR")?;
    let fastq = if output_per_rg {
        None
    } else {
        Some(required_scalar_for(&args, "FASTQ", "SamToFastq")?)
    };
    let second_end_fastq = if output_per_rg {
        None
    } else {
        optional_scalar(&args, "SECOND_END_FASTQ")?
    };
    let unpaired_fastq = if output_per_rg {
        None
    } else {
        optional_scalar(&args, "UNPAIRED_FASTQ")?
    };
    let interleave = optional_bool(&args, "INTERLEAVE")?.unwrap_or(false);
    let re_reverse = optional_bool(&args, "RE_REVERSE")?.unwrap_or(true);
    let include_non_pf_reads = optional_bool(&args, "INCLUDE_NON_PF_READS")?.unwrap_or(false);
    let include_non_primary_alignments =
        optional_bool(&args, "INCLUDE_NON_PRIMARY_ALIGNMENTS")?.unwrap_or(false);
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?.unwrap_or(5);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let transform = SamToFastqTransform {
        read1_trim: optional_u32(&args, "READ1_TRIM")?.unwrap_or(0) as usize,
        read2_trim: optional_u32(&args, "READ2_TRIM")?.unwrap_or(0) as usize,
        read1_max_bases_to_write: optional_u32(&args, "READ1_MAX_BASES_TO_WRITE")?
            .map(|value| value as usize),
        read2_max_bases_to_write: optional_u32(&args, "READ2_MAX_BASES_TO_WRITE")?
            .map(|value| value as usize),
        quality: optional_u32(&args, "QUALITY")?.map(|value| value as u8),
        clipping: samtofastq_clipping(&args)?,
    };

    if interleave && second_end_fastq.is_some() {
        return Err("SamToFastq INTERLEAVE=true cannot be used with SECOND_END_FASTQ".to_string());
    }

    let per_rg = if output_per_rg || compress_outputs_per_rg {
        Some(SamToFastqPerRgMode::new(
            rg_tag,
            output_dir,
            compress_outputs_per_rg,
            interleave,
        )?)
    } else {
        None
    };

    if has_sam_extension(&input) {
        return run_samtofastq_from_sam_text(
            &input,
            fastq.as_deref(),
            second_end_fastq.as_deref(),
            unpaired_fastq.as_deref(),
            interleave,
            re_reverse,
            include_non_pf_reads,
            include_non_primary_alignments,
            compression_level,
            create_md5_file,
            transform,
            per_rg,
        );
    }

    let reference = picard_reference(&args)?;
    let mut reader = open_bam_reader_with_reference(input, reference.as_deref())
        .map_err(|error| error.to_string())?;
    let mut first_writer = match fastq.as_ref() {
        Some(path) => Some(fastq_writer(path, compression_level)?),
        None => None,
    };
    let mut second_writer = match second_end_fastq {
        Some(ref path) => Some(fastq_writer(path, compression_level)?),
        None => None,
    };
    let mut unpaired_writer = match unpaired_fastq {
        Some(ref path) => Some(fastq_writer(path, compression_level)?),
        None => None,
    };
    let mut per_rg_outputs = match per_rg {
        Some(config) => Some(SamToFastqPerRgOutputs::from_bam_header(
            reader.header(),
            config,
            compression_level,
        )?),
        None => None,
    };
    let mut first_seen_mates: HashMap<Vec<u8>, bam::Record> = HashMap::new();

    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        if record.is_quality_check_failed() && !include_non_pf_reads {
            continue;
        }
        if (record.is_secondary() || record.is_supplementary()) && !include_non_primary_alignments {
            continue;
        }
        if record.is_paired() && !interleave && second_writer.is_none() && per_rg_outputs.is_none()
        {
            return Err(
                "SamToFastq input contains paired reads but no SECOND_END_FASTQ was specified"
                    .to_string(),
            );
        }
        if record.is_paired() {
            let key = record.qname().to_vec();
            if let Some(first_record) = first_seen_mates.remove(&key) {
                let (read1, read2) = if record.is_first_in_template() {
                    (&record, &first_record)
                } else {
                    (&first_record, &record)
                };
                if let Some(outputs) = per_rg_outputs.as_mut() {
                    outputs.write_bam_pair(read1, read2, &transform, re_reverse)?;
                } else {
                    let first = first_writer
                        .as_mut()
                        .expect("first writer exists for standard SamToFastq output");
                    write_fastq_record(
                        first,
                        read1,
                        &transform,
                        re_reverse,
                        fastq_name_suffix(read1),
                        transform.trim_for(read1),
                        transform.quality,
                        transform.max_bases_for(read1),
                    )?;
                    let writer = if interleave {
                        first
                    } else {
                        second_writer
                            .as_mut()
                            .expect("second writer exists for paired output")
                    };
                    write_fastq_record(
                        writer,
                        read2,
                        &transform,
                        re_reverse,
                        fastq_name_suffix(read2),
                        transform.trim_for(read2),
                        transform.quality,
                        transform.max_bases_for(read2),
                    )?;
                }
            } else {
                first_seen_mates.insert(key, record);
            }
            continue;
        }
        let writer: &mut dyn Write = if let Some(outputs) = per_rg_outputs.as_mut() {
            outputs.unpaired_writer_for_bam_record(&record)?
        } else if !record.is_paired() {
            match unpaired_writer.as_mut() {
                Some(writer) => writer.as_mut(),
                None => first_writer
                    .as_mut()
                    .expect("first writer exists for standard SamToFastq output")
                    .as_mut(),
            }
        } else {
            first_writer
                .as_mut()
                .expect("first writer exists for standard SamToFastq output")
                .as_mut()
        };
        write_fastq_record(
            writer,
            &record,
            &transform,
            re_reverse,
            fastq_name_suffix(&record),
            transform.trim_for(&record),
            transform.quality,
            transform.max_bases_for(&record),
        )?;
    }

    if let Some(writer) = first_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    if let Some(writer) = second_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    if let Some(writer) = unpaired_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    if let Some(outputs) = per_rg_outputs.as_mut() {
        outputs.flush_all()?;
    }
    drop(first_writer);
    drop(second_writer);
    drop(unpaired_writer);
    if let Some(outputs) = per_rg_outputs {
        outputs.write_md5_sidecars(create_md5_file)
    } else {
        write_samtofastq_sidecars(
            fastq.as_deref().expect("fastq path exists"),
            second_end_fastq.as_deref(),
            unpaired_fastq.as_deref(),
            create_md5_file,
        )
    }
}

fn run_fastqtosam(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("FastqToSam", args).map_err(|error| error.to_string())?;
    reject_unsupported_fastqtosam_args(&args)?;
    let fastq = required_scalar_for(&args, "FASTQ", "FastqToSam")?;
    let fastq2 = optional_scalar(&args, "FASTQ2")?;
    let use_sequential_fastqs = optional_bool(&args, "USE_SEQUENTIAL_FASTQS")?.unwrap_or(false);
    let fastq_paths = if use_sequential_fastqs {
        sequential_fastq_paths(&fastq)?
    } else {
        vec![fastq.clone()]
    };
    let fastq2_paths = match fastq2.as_ref() {
        Some(path) if use_sequential_fastqs => {
            let paths = sequential_fastq_paths(path)?;
            if paths.len() != fastq_paths.len() {
                return Err(format!(
                    "Found {} files for FASTQ and {} files for FASTQ2.",
                    fastq_paths.len(),
                    paths.len()
                ));
            }
            Some(paths)
        }
        Some(path) => Some(vec![path.clone()]),
        None => None,
    };
    let output = required_scalar_for(&args, "OUTPUT", "FastqToSam")?;
    let read_group = FastqReadGroup {
        id: optional_scalar(&args, "READ_GROUP_NAME")?.unwrap_or_else(|| "A".to_string()),
        sample: required_scalar_for(&args, "SAMPLE_NAME", "FastqToSam")?,
        library: optional_scalar(&args, "LIBRARY_NAME")?,
        platform: optional_scalar(&args, "PLATFORM")?,
        platform_unit: optional_scalar(&args, "PLATFORM_UNIT")?,
        sequencing_center: optional_scalar(&args, "SEQUENCING_CENTER")?,
        description: optional_scalar(&args, "DESCRIPTION")?,
        run_date: optional_scalar(&args, "RUN_DATE")?,
        predicted_insert_size: optional_scalar(&args, "PREDICTED_INSERT_SIZE")?,
        program_group: optional_scalar(&args, "PROGRAM_GROUP")?,
        platform_model: optional_scalar(&args, "PLATFORM_MODEL")?,
        sort_order: optional_scalar(&args, "SORT_ORDER")?
            .unwrap_or_else(|| "queryname".to_string()),
        comments: args.get("COMMENT").cloned().unwrap_or_default(),
    };
    let allow_and_ignore_empty_lines =
        optional_bool(&args, "ALLOW_AND_IGNORE_EMPTY_LINES")?.unwrap_or(false);
    let allow_empty_fastq = optional_bool(&args, "ALLOW_EMPTY_FASTQ")?.unwrap_or(false);
    let quality_format = optional_scalar(&args, "QUALITY_FORMAT")?;
    let quality_format = match quality_format.as_deref() {
        Some("Standard") => FastqQualityFormat::Standard,
        Some("Illumina") => FastqQualityFormat::Illumina,
        Some("Solexa") => FastqQualityFormat::Solexa,
        Some("Auto") | None => detect_fastqtosam_quality_format(
            &fastq_paths,
            fastq2_paths.as_deref(),
            allow_and_ignore_empty_lines,
        )?,
        Some(quality_format) => {
            return Err(format!(
                "unsupported FastqToSam QUALITY_FORMAT={quality_format}"
            ));
        }
    };
    let min_q = optional_u32(&args, "MIN_Q")?.unwrap_or(0);
    let max_q = optional_u32(&args, "MAX_Q")?.unwrap_or(93);
    if min_q > u8::MAX as u32 {
        return Err(format!("unsupported FastqToSam MIN_Q: {min_q}"));
    }
    if max_q > u8::MAX as u32 {
        return Err(format!("unsupported FastqToSam MAX_Q: {max_q}"));
    }
    let options = FastqToSamOptions {
        quality_format,
        min_q: min_q as u8,
        max_q: max_q as u8,
        allow_and_ignore_empty_lines,
        allow_empty_fastq,
    };
    if options.min_q > options.max_q {
        return Err("FastqToSam MIN_Q must be less than or equal to MAX_Q".to_string());
    }
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?.unwrap_or(5);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let output_format = output_format_for(&output, "FastqToSam")?;
    if matches!(output_format, bam::Format::Sam) && quality_format == FastqQualityFormat::Standard {
        run_fastqtosam_standard_sam(
            &fastq_paths,
            fastq2_paths.as_deref(),
            &output,
            &read_group,
            options,
        )?;
        return write_requested_sidecars(&output, create_md5_file, false);
    }
    let mut writer = if matches!(output_format, bam::Format::Sam) {
        FastqToSamWriter::Sam(BufWriter::with_capacity(
            1024 * 1024,
            fs::File::create(&output).map_err(|error| error.to_string())?,
        ))
    } else {
        let writer = hts_io::open_writer(
            &output,
            &fastqtosam_header(&read_group),
            output_format,
            None,
            Some(compression_level),
        )?;
        FastqToSamWriter::Bam(writer)
    };
    writer.write_header(&read_group)?;

    let mut first_readers = fastq_paths
        .iter()
        .map(|path| FastqReader::from_path(path, options))
        .collect::<Result<Vec<_>, _>>()?;
    let mut second_readers = match fastq2_paths.as_ref() {
        Some(paths) => Some(
            paths
                .iter()
                .map(|path| FastqReader::from_path(path, options))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        None => None,
    };
    let mut first_reader_index = 0usize;
    let mut second_reader_index = 0usize;

    let mut first_record = FastqRecord::default();
    let mut second_record = FastqRecord::default();
    let mut records_written = 0_u64;
    loop {
        if !next_fastq_record_from_readers(
            &mut first_readers,
            &mut first_reader_index,
            &mut first_record,
        )? {
            if let Some(readers) = second_readers.as_mut()
                && next_fastq_record_from_readers(
                    readers,
                    &mut second_reader_index,
                    &mut second_record,
                )?
            {
                return Err("malformed FastqToSam FASTQ2 has more records than FASTQ".to_string());
            }
            break;
        }
        if let Some(readers) = second_readers.as_mut() {
            if !next_fastq_record_from_readers(
                readers,
                &mut second_reader_index,
                &mut second_record,
            )? {
                return Err("malformed FastqToSam FASTQ has more records than FASTQ2".to_string());
            }
            if first_record.name != second_record.name {
                return Err(format!(
                    "malformed FastqToSam paired read names differ: {} vs {}",
                    first_record.name, second_record.name
                ));
            }
            writer.write_record(&first_record, 77, &read_group.id, quality_format)?;
            writer.write_record(&second_record, 141, &read_group.id, quality_format)?;
            records_written += 2;
        } else {
            writer.write_record(&first_record, 4, &read_group.id, quality_format)?;
            records_written += 1;
        }
    }
    if records_written == 0 && !options.allow_empty_fastq {
        return Err("malformed FastqToSam empty FASTQ input".to_string());
    }

    drop(writer);
    write_requested_sidecars(&output, create_md5_file, false)
}

fn run_addorreplacereadgroups(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("AddOrReplaceReadGroups", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_addorreplacereadgroups_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "AddOrReplaceReadGroups")?;
    let output = required_scalar_for(&args, "OUTPUT", "AddOrReplaceReadGroups")?;
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let read_group = ReadGroup {
        id: optional_scalar(&args, "RGID")?.unwrap_or_else(|| "1".to_string()),
        library: required_scalar_for(&args, "RGLB", "AddOrReplaceReadGroups")?,
        platform: required_scalar_for(&args, "RGPL", "AddOrReplaceReadGroups")?,
        platform_unit: required_scalar_for(&args, "RGPU", "AddOrReplaceReadGroups")?,
        sample: required_scalar_for(&args, "RGSM", "AddOrReplaceReadGroups")?,
        sequencing_center: optional_scalar(&args, "RGCN")?,
        description: optional_scalar(&args, "RGDS")?,
        run_date: optional_scalar(&args, "RGDT")?,
        predicted_insert_size: optional_scalar(&args, "RGPI")?,
        program_group: optional_scalar(&args, "RGPG")?,
        platform_model: optional_scalar(&args, "RGPM")?,
        key_sequence: optional_scalar(&args, "RGKS")?,
        flow_order: optional_scalar(&args, "RGFO")?,
    };

    if has_sam_extension(&input)
        && has_sam_extension(&output)
        && optional_u32(&args, "COMPRESSION_LEVEL")?.is_none()
    {
        run_addorreplacereadgroups_sam_text(&input, &output, &read_group)?;
        return write_requested_sidecars(&output, create_md5_file, false);
    }

    let reference = picard_reference(&args)?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let mut reader = open_bam_reader_with_reference(&input, reference.as_deref())
        .map_err(|error| error.to_string())?;
    let header = read_group_header(reader.header(), &read_group);
    let format = output_format(&output)?;
    let mut writer = bam_writer_for_path_with_reference(
        &output,
        &header,
        format,
        reference.as_deref(),
        compression_level,
    )?;

    for record in reader.records() {
        let mut record = record.map_err(|error| error.to_string())?;
        set_record_read_group(&mut record, &read_group.id)?;
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    drop(writer);

    write_requested_sidecars(
        &output,
        create_md5_file,
        create_index && has_extension(&output, "bam"),
    )
}

fn run_addorreplacereadgroups_sam_text(
    input: &str,
    output: &str,
    read_group: &ReadGroup,
) -> Result<(), String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut writer = BufWriter::with_capacity(
        1024 * 1024,
        fs::File::create(output).map_err(|error| error.to_string())?,
    );
    let mut line = Vec::new();
    let mut saw_read_group = false;
    let mut output_buffer = Vec::with_capacity(8 * 1024 * 1024);

    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@RG\t") {
            if !saw_read_group {
                write_read_group_header_line(&mut writer, read_group)?;
                saw_read_group = true;
            }
            continue;
        }
        if line.starts_with(b"@") {
            writer.write_all(&line).map_err(|error| error.to_string())?;
            continue;
        }
        if !saw_read_group {
            write_read_group_header_line(&mut writer, read_group)?;
            saw_read_group = true;
        }
        append_read_group_replaced_sam_line(&mut output_buffer, &line, read_group.id.as_bytes());
        flush_large_addorreplacereadgroups_buffer(&mut writer, &mut output_buffer)?;
    }

    if !output_buffer.is_empty() {
        writer
            .write_all(&output_buffer)
            .map_err(|error| error.to_string())?;
    }
    if !saw_read_group {
        write_read_group_header_line(&mut writer, read_group)?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn write_read_group_header_line(
    writer: &mut dyn Write,
    read_group: &ReadGroup,
) -> Result<(), String> {
    let mut line = String::from("@RG");
    push_sam_tag(&mut line, "ID", Some(&read_group.id));
    push_sam_tag(&mut line, "LB", Some(&read_group.library));
    push_sam_tag(&mut line, "PL", Some(&read_group.platform));
    push_sam_tag(&mut line, "SM", Some(&read_group.sample));
    push_sam_tag(&mut line, "PU", Some(&read_group.platform_unit));
    push_sam_tag(&mut line, "CN", read_group.sequencing_center.as_deref());
    push_sam_tag(&mut line, "DS", read_group.description.as_deref());
    push_sam_tag(&mut line, "DT", read_group.run_date.as_deref());
    push_sam_tag(&mut line, "PI", read_group.predicted_insert_size.as_deref());
    push_sam_tag(&mut line, "PG", read_group.program_group.as_deref());
    push_sam_tag(&mut line, "PM", read_group.platform_model.as_deref());
    push_sam_tag(&mut line, "KS", read_group.key_sequence.as_deref());
    push_sam_tag(&mut line, "FO", read_group.flow_order.as_deref());
    line.push('\n');
    writer
        .write_all(line.as_bytes())
        .map_err(|error| error.to_string())
}

fn flush_large_addorreplacereadgroups_buffer(
    writer: &mut BufWriter<fs::File>,
    output_buffer: &mut Vec<u8>,
) -> Result<(), String> {
    if output_buffer.len() >= 8 * 1024 * 1024 {
        writer
            .write_all(output_buffer)
            .map_err(|error| error.to_string())?;
        output_buffer.clear();
    }
    Ok(())
}

fn append_read_group_replaced_sam_line(output: &mut Vec<u8>, line: &[u8], read_group_id: &[u8]) {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut wrote_any = false;
    for field in line.split(|byte| *byte == b'\t') {
        if field.starts_with(b"RG:Z:") {
            continue;
        }
        if wrote_any {
            output.extend_from_slice(b"\t");
        }
        output.extend_from_slice(field);
        wrote_any = true;
    }
    output.extend_from_slice(b"\tRG:Z:");
    output.extend_from_slice(read_group_id);
    output.extend_from_slice(b"\n");
}

fn run_collectalignmentsummarymetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectAlignmentSummaryMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collectalignment_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectAlignmentSummaryMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectAlignmentSummaryMetrics")?;
    let stop_after = optional_u32(&args, "STOP_AFTER")?.unwrap_or(0);
    let accumulation = alignment_accumulation_level(&args)?;

    if has_sam_extension(&input) {
        let metrics = collect_alignment_sam_text(&input, stop_after, accumulation)?;
        return fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string());
    }

    let mut reader = open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
    let read_groups =
        insert_size_read_groups_from_header(&String::from_utf8_lossy(reader.header().as_bytes()));
    let mut metrics = AlignmentSummaryCollection::new(accumulation);
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        let read_group = insert_size_read_group_for_bam_record(&record, &read_groups);
        metrics.observe(&record, read_group.as_ref());
    }

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())
}

fn run_collectqualityyieldmetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectQualityYieldMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collectqualityyield_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectQualityYieldMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectQualityYieldMetrics")?;
    let use_original_qualities = optional_bool(&args, "USE_ORIGINAL_QUALITIES")?.unwrap_or(true);
    let include_secondary = optional_bool(&args, "INCLUDE_SECONDARY_ALIGNMENTS")?.unwrap_or(false);
    let include_supplemental =
        optional_bool(&args, "INCLUDE_SUPPLEMENTAL_ALIGNMENTS")?.unwrap_or(false);
    let stop_after = optional_u32(&args, "STOP_AFTER")?.unwrap_or(0);

    if has_sam_extension(&input) {
        let metrics = collect_quality_yield_sam_text(
            &input,
            use_original_qualities,
            include_secondary,
            include_supplemental,
            stop_after,
        )?;
        return fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string());
    }

    let mut reader = open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
    let mut metrics = QualityYieldSummary::default();
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        metrics.observe(
            &record,
            use_original_qualities,
            include_secondary,
            include_supplemental,
        );
    }

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())
}

fn run_collectinsertsizemetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectInsertSizeMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collectinsertsize_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectInsertSizeMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectInsertSizeMetrics")?;
    let histogram = required_scalar_for(&args, "HISTOGRAM_FILE", "CollectInsertSizeMetrics")?;
    let include_duplicates = optional_bool(&args, "INCLUDE_DUPLICATES")?.unwrap_or(false);
    let stop_after = optional_u32(&args, "STOP_AFTER")?.unwrap_or(0);
    let accumulation = insert_size_accumulation_level(&args)?;
    let minimum_pct = optional_f64(&args, "MINIMUM_PCT")?.unwrap_or(0.05);
    let deviations = optional_f64(&args, "DEVIATIONS")?.unwrap_or(10.0);

    if has_sam_extension(&input) {
        let metrics =
            collect_insert_size_sam_text(&input, include_duplicates, stop_after, accumulation)?;
        fs::write(output, metrics.to_picard_text(minimum_pct, deviations))
            .map_err(|error| error.to_string())?;
        return write_summary_chart_pdf(&histogram, "CollectInsertSizeMetrics");
    }

    let mut reader = open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
    let read_groups =
        insert_size_read_groups_from_header(&String::from_utf8_lossy(reader.header().as_bytes()));
    let mut metrics = InsertSizeCollection::new(accumulation);
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        let read_group = insert_size_read_group_for_bam_record(&record, &read_groups);
        metrics.observe(&record, include_duplicates, read_group.as_ref());
    }

    fs::write(output, metrics.to_picard_text(minimum_pct, deviations))
        .map_err(|error| error.to_string())?;
    write_summary_chart_pdf(&histogram, "CollectInsertSizeMetrics")
}

fn run_collectbasedistributionbycycle(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectBaseDistributionByCycle", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collectbasedistributionbycycle_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectBaseDistributionByCycle")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectBaseDistributionByCycle")?;
    let chart = required_scalar_for(&args, "CHART_OUTPUT", "CollectBaseDistributionByCycle")?;
    let aligned_reads_only = optional_bool(&args, "ALIGNED_READS_ONLY")?.unwrap_or(false);
    let pf_reads_only = optional_bool(&args, "PF_READS_ONLY")?.unwrap_or(false);
    let stop_after = optional_u32(&args, "STOP_AFTER")?.unwrap_or(0);

    let metrics = if has_sam_extension(&input) {
        collect_base_distribution_by_cycle_sam_text(
            &input,
            aligned_reads_only,
            pf_reads_only,
            stop_after,
        )?
    } else {
        let mut reader =
            open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
        let mut metrics = BaseDistributionByCycleSummary::default();
        for record in limited_records(&mut reader, stop_after) {
            let record = record.map_err(|error| error.to_string())?;
            metrics.observe(&record, aligned_reads_only, pf_reads_only);
        }
        metrics
    };

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())?;
    write_summary_chart_pdf(&chart, "CollectBaseDistributionByCycle")
}

fn run_collectgcbiasmetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectGcBiasMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collectgcbiasmetrics_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectGcBiasMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectGcBiasMetrics")?;
    let summary_output = required_scalar_for(&args, "SUMMARY_OUTPUT", "CollectGcBiasMetrics")?;
    let chart = required_scalar_for(&args, "CHART_OUTPUT", "CollectGcBiasMetrics")?;
    let reference = required_scalar_for(&args, "REFERENCE_SEQUENCE", "CollectGcBiasMetrics")?;
    let window_size = optional_u32(&args, "SCAN_WINDOW_SIZE")?.unwrap_or(100) as usize;
    let minimum_genome_fraction =
        optional_f64(&args, "MINIMUM_GENOME_FRACTION")?.unwrap_or(0.00001);
    let also_ignore_duplicates = optional_bool(&args, "ALSO_IGNORE_DUPLICATES")?.unwrap_or(false);
    let stop_after = optional_u32(&args, "STOP_AFTER")?.unwrap_or(0);

    let mut reader = open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let mut metrics = GcBiasMetricsSummary::new(&reference, window_size, also_ignore_duplicates)?;
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        metrics.observe(&record, &target_names, window_size)?;
    }

    fs::write(
        output,
        metrics.detail_text(window_size, minimum_genome_fraction),
    )
    .map_err(|error| error.to_string())?;
    fs::write(
        summary_output,
        metrics.summary_text(window_size, minimum_genome_fraction),
    )
    .map_err(|error| error.to_string())?;
    write_summary_chart_pdf(&chart, "CollectGcBiasMetrics")
}

fn run_collecthsmetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectHsMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collecthsmetrics_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectHsMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectHsMetrics")?;
    let bait_intervals_path = required_scalar_for(&args, "BAIT_INTERVALS", "CollectHsMetrics")?;
    let target_intervals_path = required_scalar_for(&args, "TARGET_INTERVALS", "CollectHsMetrics")?;
    let reference = required_scalar_for(&args, "REFERENCE_SEQUENCE", "CollectHsMetrics")?;
    let clip_overlapping_reads = optional_bool(&args, "CLIP_OVERLAPPING_READS")?.unwrap_or(false);
    let near_distance = optional_u32(&args, "NEAR_DISTANCE")?.unwrap_or(250);

    let references = read_fasta_sequences(&reference, true)?;
    let contig_order = references
        .iter()
        .enumerate()
        .map(|(index, sequence)| (sequence.name.clone(), index))
        .collect::<BTreeMap<_, _>>();

    let bait_text = fs::read_to_string(&bait_intervals_path).map_err(|error| error.to_string())?;
    let target_text =
        fs::read_to_string(&target_intervals_path).map_err(|error| error.to_string())?;
    let bait_intervals = read_interval_list_intervals(&bait_text, &contig_order)?
        .into_iter()
        .map(hs_metrics_interval_from_bed)
        .collect::<Vec<_>>();
    let target_intervals = read_interval_list_intervals(&target_text, &contig_order)?
        .into_iter()
        .map(hs_metrics_interval_from_bed)
        .collect::<Vec<_>>();

    let config = hs_metrics::HsMetricsConfig {
        bait_intervals,
        target_intervals,
        clip_overlapping_reads,
        near_distance,
    };

    let mut reader = open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
    let metrics_text = hs_metrics::collect_hs_metrics(&mut reader, &config)?;
    fs::write(output, metrics_text).map_err(|error| error.to_string())
}

fn hs_metrics_interval_from_bed(interval: BedInterval) -> hs_metrics::GenomicInterval {
    hs_metrics::GenomicInterval {
        contig: interval.contig,
        start: interval.start,
        end: interval.end,
    }
}

fn collectmultiplemetrics_can_single_pass(input: &str, programs: &[String]) -> bool {
    if !hts_io::is_hts_container_input(input) || programs.len() < 2 {
        return false;
    }
    programs.iter().all(|program| {
        matches!(
            program.as_str(),
            "CollectAlignmentSummaryMetrics"
                | "CollectQualityYieldMetrics"
                | "CollectBaseDistributionByCycle"
                | "CollectGcBiasMetrics"
                | "CollectInsertSizeMetrics"
                | "QualityScoreDistribution"
                | "MeanQualityByCycle"
                | "CollectWgsMetrics"
        )
    })
}

fn cmm_collector_thread_count(active_collectors: usize) -> usize {
    if active_collectors < 2 {
        return 1;
    }
    if let Ok(value) = std::env::var("TURBO_PICARD_CMM_THREADS") {
        if value.trim().eq_ignore_ascii_case("auto") {
            return default_cmm_collector_thread_count(active_collectors);
        }
        if let Ok(threads) = value.parse::<usize>() {
            return threads.max(1).min(active_collectors);
        }
    }
    default_cmm_collector_thread_count(active_collectors)
}

fn default_cmm_collector_thread_count(active_collectors: usize) -> usize {
    std::thread::available_parallelism()
        .ok()
        .map(|parallelism| parallelism.get().saturating_sub(2))
        .unwrap_or(1)
        .min(active_collectors)
        .clamp(1, 6)
}

fn run_collectmultiplemetrics_single_pass(
    args: &BTreeMap<String, Vec<String>>,
    input: &str,
    output: &str,
    file_extension: &str,
    programs: &[String],
) -> Result<(), String> {
    let stop_after = optional_u32(args, "STOP_AFTER")?.unwrap_or(0);
    let reference = optional_scalar(args, "REFERENCE_SEQUENCE")?;
    if programs.iter().any(|program| {
        matches!(
            program.as_str(),
            "CollectGcBiasMetrics" | "CollectWgsMetrics"
        )
    }) && reference.is_none()
    {
        return Err(
            "missing required CollectMultipleMetrics argument: REFERENCE_SEQUENCE".to_string(),
        );
    }

    let mut alignment = None;
    let mut insert_size = None;
    let mut base_distribution = None;
    let mut quality_distribution = None;
    let mut mean_quality = None;
    let mut quality_yield = None;
    let mut gc_bias = None;
    let mut wgs = None;

    for program in programs {
        match program.as_str() {
            "CollectAlignmentSummaryMetrics" => {
                let accumulation = alignment_accumulation_level(args)?;
                alignment = Some((
                    AlignmentSummaryCollection::new(accumulation),
                    collectmultiplemetrics_metric_path(
                        output,
                        ".alignment_summary_metrics",
                        file_extension,
                    ),
                ));
            }
            "CollectInsertSizeMetrics" => {
                let accumulation = insert_size_accumulation_level(args)?;
                let include_duplicates = optional_bool(args, "INCLUDE_DUPLICATES")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "INCLUDE_DUPLICATES")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                let minimum_pct = optional_f64(args, "MINIMUM_PCT")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "MINIMUM_PCT")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(0.05);
                let deviations = optional_f64(args, "DEVIATIONS")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "DEVIATIONS")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(10.0);
                insert_size = Some((
                    InsertSizeCollection::new(accumulation),
                    collectmultiplemetrics_metric_path(
                        output,
                        ".insert_size_metrics",
                        file_extension,
                    ),
                    format!("{output}.insert_size_histogram.pdf"),
                    include_duplicates,
                    minimum_pct,
                    deviations,
                ));
            }
            "CollectBaseDistributionByCycle" => {
                let aligned_reads_only = optional_bool(args, "ALIGNED_READS_ONLY")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "ALIGNED_READS_ONLY")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                let pf_reads_only = optional_bool(args, "PF_READS_ONLY")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "PF_READS_ONLY")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                base_distribution = Some((
                    BaseDistributionByCycleSummary::default(),
                    collectmultiplemetrics_metric_path(
                        output,
                        ".base_distribution_by_cycle_metrics",
                        file_extension,
                    ),
                    format!("{output}.base_distribution_by_cycle.pdf"),
                    aligned_reads_only,
                    pf_reads_only,
                ));
            }
            "QualityScoreDistribution" => {
                let aligned_reads_only = optional_bool(args, "ALIGNED_READS_ONLY")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "ALIGNED_READS_ONLY")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                let pf_reads_only = optional_bool(args, "PF_READS_ONLY")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "PF_READS_ONLY")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                let include_no_calls = optional_bool(args, "INCLUDE_NO_CALLS")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "INCLUDE_NO_CALLS")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                quality_distribution = Some((
                    QualityScoreDistributionSummary::default(),
                    collectmultiplemetrics_metric_path(
                        output,
                        ".quality_distribution_metrics",
                        file_extension,
                    ),
                    format!("{output}.quality_distribution.pdf"),
                    aligned_reads_only,
                    pf_reads_only,
                    include_no_calls,
                ));
            }
            "MeanQualityByCycle" => {
                let aligned_reads_only = optional_bool(args, "ALIGNED_READS_ONLY")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "ALIGNED_READS_ONLY")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                let pf_reads_only = optional_bool(args, "PF_READS_ONLY")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "PF_READS_ONLY")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                mean_quality = Some((
                    MeanQualityByCycleSummary::default(),
                    collectmultiplemetrics_metric_path(
                        output,
                        ".quality_by_cycle_metrics",
                        file_extension,
                    ),
                    format!("{output}.quality_by_cycle.pdf"),
                    aligned_reads_only,
                    pf_reads_only,
                ));
            }
            "CollectQualityYieldMetrics" => {
                let use_original_qualities = optional_bool(args, "USE_ORIGINAL_QUALITIES")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(
                            args,
                            program,
                            "USE_ORIGINAL_QUALITIES",
                        )
                        .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(true);
                let include_secondary = optional_bool(args, "INCLUDE_SECONDARY_ALIGNMENTS")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(
                            args,
                            program,
                            "INCLUDE_SECONDARY_ALIGNMENTS",
                        )
                        .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                let include_supplemental = optional_bool(args, "INCLUDE_SUPPLEMENTAL_ALIGNMENTS")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(
                            args,
                            program,
                            "INCLUDE_SUPPLEMENTAL_ALIGNMENTS",
                        )
                        .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                quality_yield = Some((
                    QualityYieldSummary::default(),
                    collectmultiplemetrics_metric_path(
                        output,
                        ".quality_yield_metrics",
                        file_extension,
                    ),
                    use_original_qualities,
                    include_secondary,
                    include_supplemental,
                ));
            }
            "CollectGcBiasMetrics" => {
                let reference = reference.as_ref().expect("reference checked above");
                let window_size = optional_u32(args, "SCAN_WINDOW_SIZE")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(args, program, "SCAN_WINDOW_SIZE")
                            .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(100) as usize;
                let minimum_genome_fraction = optional_f64(args, "MINIMUM_GENOME_FRACTION")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(
                            args,
                            program,
                            "MINIMUM_GENOME_FRACTION",
                        )
                        .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(0.00001);
                let also_ignore_duplicates = optional_bool(args, "ALSO_IGNORE_DUPLICATES")?
                    .or_else(|| {
                        collectmultiplemetrics_extra_argument(
                            args,
                            program,
                            "ALSO_IGNORE_DUPLICATES",
                        )
                        .and_then(|value| value.parse().ok())
                    })
                    .unwrap_or(false);
                gc_bias = Some((
                    GcBiasMetricsSummary::new(reference, window_size, also_ignore_duplicates)?,
                    collectmultiplemetrics_metric_path(
                        output,
                        ".gc_bias.detail_metrics",
                        file_extension,
                    ),
                    collectmultiplemetrics_metric_path(
                        output,
                        ".gc_bias.summary_metrics",
                        file_extension,
                    ),
                    format!("{output}.gc_bias.pdf"),
                    window_size,
                    minimum_genome_fraction,
                ));
            }
            "CollectWgsMetrics" => {
                let reference = reference.as_ref().expect("reference checked above");
                let reference_contigs = read_reference_contigs_for_wgs(reference)?;
                let coverage_cap = optional_u32(args, "COVERAGE_CAP")?.unwrap_or(250);
                let use_fast_algorithm =
                    if let Some(use_fast_algorithm) = optional_bool(args, "USE_FAST_ALGORITHM")? {
                        use_fast_algorithm
                    } else {
                        env_bool("TURBO_PICARD_WGS_FAST_DEFAULT")?.unwrap_or(false)
                    };
                let sample_size = optional_u32(args, "SAMPLE_SIZE")?
                    .unwrap_or(if use_fast_algorithm { 0 } else { 10_000 });
                let include_bq_histogram =
                    optional_bool(args, "INCLUDE_BQ_HISTOGRAM")?.unwrap_or(false);
                let mut summary = WgsMetricsSummary::new(&reference_contigs, None, coverage_cap);
                if let Some(limit) = optional_i64(args, "STOP_AFTER")?
                    && limit >= 0
                {
                    summary.limit_included_loci(limit as usize);
                }
                wgs = Some((
                    summary,
                    collectmultiplemetrics_metric_path(output, ".wgs_metrics", file_extension),
                    20_u8,
                    20_u8,
                    coverage_cap,
                    100_000_u32,
                    true,
                    sample_size,
                    include_bq_histogram,
                ));
            }
            _ => unreachable!("caller filters programs"),
        }
    }

    let mut reader = open_bam_reader_with_reference(input, reference.as_deref())
        .map_err(|error| error.to_string())?;
    let header_text = String::from_utf8_lossy(reader.header().as_bytes());
    let read_groups = insert_size_read_groups_from_header(&header_text);
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();

    let active_collectors = [
        alignment.is_some(),
        insert_size.is_some(),
        base_distribution.is_some(),
        quality_distribution.is_some(),
        mean_quality.is_some(),
        quality_yield.is_some(),
        gc_bias.is_some(),
        wgs.is_some(),
    ]
    .into_iter()
    .filter(|enabled| *enabled)
    .count();
    let thread_count = cmm_collector_thread_count(active_collectors);
    let has_order_dependent_wgs = wgs.is_some();

    let mut observe_record = |record: &bam::Record| -> Result<(), String> {
        let read_group = if alignment.is_some() || insert_size.is_some() {
            insert_size_read_group_for_bam_record(record, &read_groups)
        } else {
            None
        };

        if let Some((metrics, ..)) = alignment.as_mut() {
            metrics.observe(record, read_group.as_ref());
        }
        if let Some((metrics, _, _, include_duplicates, ..)) = insert_size.as_mut() {
            metrics.observe(record, *include_duplicates, read_group.as_ref());
        }
        if let Some((metrics, .., aligned_reads_only, pf_reads_only)) = base_distribution.as_mut() {
            metrics.observe(record, *aligned_reads_only, *pf_reads_only);
        }
        if let Some((metrics, _, _, aligned_reads_only, pf_reads_only, include_no_calls)) =
            quality_distribution.as_mut()
        {
            metrics.observe(
                record,
                *aligned_reads_only,
                *pf_reads_only,
                *include_no_calls,
            );
        }
        if let Some((metrics, _, _, aligned_reads_only, pf_reads_only)) = mean_quality.as_mut() {
            metrics.observe(record, *aligned_reads_only, *pf_reads_only);
        }
        if let Some((metrics, _, use_original_qualities, include_secondary, include_supplemental)) =
            quality_yield.as_mut()
        {
            metrics.observe(
                record,
                *use_original_qualities,
                *include_secondary,
                *include_supplemental,
            );
        }
        if let Some((metrics, _, _, _, window_size, ..)) = gc_bias.as_mut() {
            metrics.observe(record, &target_names, *window_size)?;
        }
        if let Some((
            metrics,
            _,
            minimum_mapping_quality,
            minimum_base_quality,
            coverage_cap,
            locus_accumulation_cap,
            count_unpaired,
            _sample_size,
            _include_bq_histogram,
        )) = wgs.as_mut()
        {
            metrics.observe(
                record,
                &target_names,
                *minimum_mapping_quality,
                *minimum_base_quality,
                *coverage_cap,
                *locus_accumulation_cap,
                *count_unpaired,
            )?;
        }
        Ok(())
    };

    if thread_count <= 1 || has_order_dependent_wgs {
        for record in limited_records(&mut reader, stop_after) {
            let record = record.map_err(|error| error.to_string())?;
            observe_record(&record)?;
        }
    } else {
        let alignment_worker = alignment
            .take()
            .map(|(metrics, path)| (Arc::new(Mutex::new(metrics)), path));
        let insert_size_worker = insert_size.take().map(
            |(metrics, path, chart_path, include_duplicates, minimum_pct, deviations)| {
                (
                    Arc::new(Mutex::new(metrics)),
                    path,
                    chart_path,
                    include_duplicates,
                    minimum_pct,
                    deviations,
                )
            },
        );
        let base_distribution_worker = base_distribution.take().map(
            |(metrics, path, chart_path, aligned_reads_only, pf_reads_only)| {
                (
                    Arc::new(Mutex::new(metrics)),
                    path,
                    chart_path,
                    aligned_reads_only,
                    pf_reads_only,
                )
            },
        );
        let quality_distribution_worker = quality_distribution.take().map(
            |(metrics, path, chart_path, aligned_reads_only, pf_reads_only, include_no_calls)| {
                (
                    Arc::new(Mutex::new(metrics)),
                    path,
                    chart_path,
                    aligned_reads_only,
                    pf_reads_only,
                    include_no_calls,
                )
            },
        );
        let mean_quality_worker = mean_quality.take().map(
            |(metrics, path, chart_path, aligned_reads_only, pf_reads_only)| {
                (
                    Arc::new(Mutex::new(metrics)),
                    path,
                    chart_path,
                    aligned_reads_only,
                    pf_reads_only,
                )
            },
        );
        let quality_yield_flags =
            quality_yield
                .as_ref()
                .map(|(_, _, _, include_secondary, include_supplemental)| {
                    (*include_secondary, *include_supplemental)
                });
        let quality_yield_worker = quality_yield.take().map(
            |(metrics, path, use_original_qualities, include_secondary, include_supplemental)| {
                (
                    Arc::new(Mutex::new(metrics)),
                    path,
                    use_original_qualities,
                    include_secondary,
                    include_supplemental,
                )
            },
        );
        let gc_bias_worker = gc_bias.take().map(
            |(
                metrics,
                detail_path,
                summary_path,
                chart_path,
                window_size,
                minimum_genome_fraction,
            )| {
                (
                    Arc::new(Mutex::new(metrics)),
                    detail_path,
                    summary_path,
                    chart_path,
                    window_size,
                    minimum_genome_fraction,
                )
            },
        );
        let wgs_worker = wgs.take().map(
            |(
                metrics,
                path,
                minimum_mapping_quality,
                minimum_base_quality,
                coverage_cap,
                locus_accumulation_cap,
                count_unpaired,
                sample_size,
                include_bq_histogram,
            )| {
                (
                    Arc::new(Mutex::new(metrics)),
                    path,
                    minimum_mapping_quality,
                    minimum_base_quality,
                    coverage_cap,
                    locus_accumulation_cap,
                    count_unpaired,
                    sample_size,
                    include_bq_histogram,
                )
            },
        );

        let read_groups = Arc::new(read_groups);
        let target_names = Arc::new(target_names);
        let mut handlers: Vec<cmm_pipeline::CmmBatchHandler> = Vec::new();
        let combine_alignment_and_insert_size =
            alignment_worker.is_some() && insert_size_worker.is_some();

        if combine_alignment_and_insert_size {
            let alignment_worker =
                Arc::clone(&alignment_worker.as_ref().expect("alignment worker").0);
            let include_duplicates = insert_size_worker.as_ref().expect("insert-size worker").3;
            let insert_size_worker =
                Arc::clone(&insert_size_worker.as_ref().expect("insert-size worker").0);
            let read_groups = Arc::clone(&read_groups);
            handlers.push(Box::new(move |batch| {
                let mut alignment_metrics =
                    alignment_worker.lock().expect("alignment collector lock");
                let mut insert_size_metrics = insert_size_worker
                    .lock()
                    .expect("insert-size collector lock");
                for entry in batch {
                    if !entry.gates.alignment && !entry.gates.insert_size {
                        continue;
                    }
                    let record = &entry.record;
                    let read_group =
                        insert_size_read_group_for_bam_record(record, read_groups.as_ref());
                    if entry.gates.alignment {
                        alignment_metrics.observe(record, read_group.as_ref());
                    }
                    if entry.gates.insert_size {
                        insert_size_metrics.observe(
                            record,
                            include_duplicates,
                            read_group.as_ref(),
                        );
                    }
                }
                Ok(())
            }));
        }

        if !combine_alignment_and_insert_size && let Some((worker, _)) = &alignment_worker {
            let worker = Arc::clone(worker);
            let read_groups = Arc::clone(&read_groups);
            handlers.push(Box::new(move |batch| {
                let mut metrics = worker.lock().expect("alignment collector lock");
                for entry in batch {
                    if !entry.gates.alignment {
                        continue;
                    }
                    let record = &entry.record;
                    let read_group =
                        insert_size_read_group_for_bam_record(record, read_groups.as_ref());
                    metrics.observe(record, read_group.as_ref());
                }
                Ok(())
            }));
        }
        if !combine_alignment_and_insert_size
            && let Some((worker, _, _, include_duplicates, _, _)) = &insert_size_worker
        {
            let worker = Arc::clone(worker);
            let read_groups = Arc::clone(&read_groups);
            let include_duplicates = *include_duplicates;
            handlers.push(Box::new(move |batch| {
                let mut metrics = worker.lock().expect("insert-size collector lock");
                for entry in batch {
                    if !entry.gates.insert_size {
                        continue;
                    }
                    let record = &entry.record;
                    let read_group =
                        insert_size_read_group_for_bam_record(record, read_groups.as_ref());
                    metrics.observe(record, include_duplicates, read_group.as_ref());
                }
                Ok(())
            }));
        }
        if let Some((worker, _, _, aligned_reads_only, pf_reads_only)) = &base_distribution_worker {
            let worker = Arc::clone(worker);
            let aligned_reads_only = *aligned_reads_only;
            let pf_reads_only = *pf_reads_only;
            handlers.push(Box::new(move |batch| {
                let mut metrics = worker.lock().expect("base-distribution collector lock");
                for entry in batch {
                    if !entry.gates.base_distribution {
                        continue;
                    }
                    let record = &entry.record;
                    metrics.observe(record, aligned_reads_only, pf_reads_only);
                }
                Ok(())
            }));
        }
        if let Some((worker, _, _, aligned_reads_only, pf_reads_only, include_no_calls)) =
            &quality_distribution_worker
        {
            let worker = Arc::clone(worker);
            let aligned_reads_only = *aligned_reads_only;
            let pf_reads_only = *pf_reads_only;
            let include_no_calls = *include_no_calls;
            handlers.push(Box::new(move |batch| {
                let mut metrics = worker.lock().expect("quality-distribution collector lock");
                for entry in batch {
                    if !entry.gates.quality_distribution {
                        continue;
                    }
                    let record = &entry.record;
                    metrics.observe(record, aligned_reads_only, pf_reads_only, include_no_calls);
                }
                Ok(())
            }));
        }
        if let Some((worker, _, _, aligned_reads_only, pf_reads_only)) = &mean_quality_worker {
            let worker = Arc::clone(worker);
            let aligned_reads_only = *aligned_reads_only;
            let pf_reads_only = *pf_reads_only;
            handlers.push(Box::new(move |batch| {
                let mut metrics = worker.lock().expect("mean-quality collector lock");
                for entry in batch {
                    if !entry.gates.mean_quality {
                        continue;
                    }
                    let record = &entry.record;
                    metrics.observe(record, aligned_reads_only, pf_reads_only);
                }
                Ok(())
            }));
        }
        if let Some((worker, _, use_original_qualities, include_secondary, include_supplemental)) =
            &quality_yield_worker
        {
            let worker = Arc::clone(worker);
            let use_original_qualities = *use_original_qualities;
            let include_secondary = *include_secondary;
            let include_supplemental = *include_supplemental;
            handlers.push(Box::new(move |batch| {
                let mut metrics = worker.lock().expect("quality-yield collector lock");
                for entry in batch {
                    if !entry.gates.quality_yield {
                        continue;
                    }
                    let record = &entry.record;
                    metrics.observe(
                        record,
                        use_original_qualities,
                        include_secondary,
                        include_supplemental,
                    );
                }
                Ok(())
            }));
        }
        if let Some((worker, _, _, _, window_size, _)) = &gc_bias_worker {
            let worker = Arc::clone(worker);
            let target_names = Arc::clone(&target_names);
            let window_size = *window_size;
            handlers.push(Box::new(move |batch| {
                let mut metrics = worker.lock().expect("gc-bias collector lock");
                for entry in batch {
                    if !entry.gates.gc_bias {
                        continue;
                    }
                    let record = &entry.record;
                    if let Err(error) =
                        metrics.observe(record, target_names.as_slice(), window_size)
                    {
                        return Err(format!("CollectGcBiasMetrics failed: {error}"));
                    }
                }
                Ok(())
            }));
        }
        if let Some((
            worker,
            _,
            minimum_mapping_quality,
            minimum_base_quality,
            coverage_cap,
            locus_accumulation_cap,
            count_unpaired,
            _,
            _,
        )) = &wgs_worker
        {
            let worker = Arc::clone(worker);
            let target_names = Arc::clone(&target_names);
            let minimum_mapping_quality = *minimum_mapping_quality;
            let minimum_base_quality = *minimum_base_quality;
            let coverage_cap = *coverage_cap;
            let locus_accumulation_cap = *locus_accumulation_cap;
            let count_unpaired = *count_unpaired;
            handlers.push(Box::new(move |batch| {
                let mut metrics = worker.lock().expect("wgs collector lock");
                for entry in batch {
                    if !entry.gates.wgs {
                        continue;
                    }
                    let record = &entry.record;
                    if let Err(error) = metrics.observe(
                        record,
                        target_names.as_slice(),
                        minimum_mapping_quality,
                        minimum_base_quality,
                        coverage_cap,
                        locus_accumulation_cap,
                        count_unpaired,
                    ) {
                        return Err(format!("CollectWgsMetrics failed: {error}"));
                    }
                }
                Ok(())
            }));
        }

        let (include_secondary_yield, include_supplemental_yield) =
            quality_yield_flags.unwrap_or((false, false));
        // Quality collectors each own their own ALIGNED_READS_ONLY / PF_READS_ONLY flags.
        // Use conservative pre-filters here and keep per-collector filtering in the
        // collector logic via each command's captured settings.
        cmm_pipeline::CmmWorkerPool::new(handlers, thread_count).run_parallel_bam_pass(
            reader,
            stop_after,
            false,
            false,
            include_secondary_yield,
            include_supplemental_yield,
        )?;

        alignment = alignment_worker.map(|(worker, path)| {
            (
                Arc::try_unwrap(worker)
                    .expect("alignment collector thread still running")
                    .into_inner()
                    .expect("alignment collector lock poisoned"),
                path,
            )
        });
        insert_size = insert_size_worker.map(
            |(worker, path, chart_path, include_duplicates, minimum_pct, deviations)| {
                (
                    Arc::try_unwrap(worker)
                        .expect("insert-size collector thread still running")
                        .into_inner()
                        .expect("insert-size collector lock poisoned"),
                    path,
                    chart_path,
                    include_duplicates,
                    minimum_pct,
                    deviations,
                )
            },
        );
        base_distribution = base_distribution_worker.map(
            |(worker, path, chart_path, aligned_reads_only, pf_reads_only)| {
                (
                    Arc::try_unwrap(worker)
                        .expect("base-distribution collector thread still running")
                        .into_inner()
                        .expect("base-distribution collector lock poisoned"),
                    path,
                    chart_path,
                    aligned_reads_only,
                    pf_reads_only,
                )
            },
        );
        quality_distribution = quality_distribution_worker.map(
            |(worker, path, chart_path, aligned_reads_only, pf_reads_only, include_no_calls)| {
                (
                    Arc::try_unwrap(worker)
                        .expect("quality-distribution collector thread still running")
                        .into_inner()
                        .expect("quality-distribution collector lock poisoned"),
                    path,
                    chart_path,
                    aligned_reads_only,
                    pf_reads_only,
                    include_no_calls,
                )
            },
        );
        mean_quality = mean_quality_worker.map(
            |(worker, path, chart_path, aligned_reads_only, pf_reads_only)| {
                (
                    Arc::try_unwrap(worker)
                        .expect("mean-quality collector thread still running")
                        .into_inner()
                        .expect("mean-quality collector lock poisoned"),
                    path,
                    chart_path,
                    aligned_reads_only,
                    pf_reads_only,
                )
            },
        );
        quality_yield = quality_yield_worker.map(
            |(worker, path, use_original_qualities, include_secondary, include_supplemental)| {
                (
                    Arc::try_unwrap(worker)
                        .expect("quality-yield collector thread still running")
                        .into_inner()
                        .expect("quality-yield collector lock poisoned"),
                    path,
                    use_original_qualities,
                    include_secondary,
                    include_supplemental,
                )
            },
        );
        gc_bias = gc_bias_worker.map(
            |(
                worker,
                detail_path,
                summary_path,
                chart_path,
                window_size,
                minimum_genome_fraction,
            )| {
                (
                    Arc::try_unwrap(worker)
                        .expect("gc-bias collector thread still running")
                        .into_inner()
                        .expect("gc-bias collector lock poisoned"),
                    detail_path,
                    summary_path,
                    chart_path,
                    window_size,
                    minimum_genome_fraction,
                )
            },
        );
        wgs = wgs_worker.map(
            |(
                worker,
                path,
                minimum_mapping_quality,
                minimum_base_quality,
                coverage_cap,
                locus_accumulation_cap,
                count_unpaired,
                sample_size,
                include_bq_histogram,
            )| {
                (
                    Arc::try_unwrap(worker)
                        .expect("wgs collector thread still running")
                        .into_inner()
                        .expect("wgs collector lock poisoned"),
                    path,
                    minimum_mapping_quality,
                    minimum_base_quality,
                    coverage_cap,
                    locus_accumulation_cap,
                    count_unpaired,
                    sample_size,
                    include_bq_histogram,
                )
            },
        );
    }

    if let Some((metrics, output_path)) = alignment {
        fs::write(output_path, metrics.to_picard_text()).map_err(|error| error.to_string())?;
        write_summary_chart_pdf(
            &format!("{output}.read_length_histogram.pdf"),
            "CollectAlignmentSummaryMetrics",
        )?;
    }
    if let Some((metrics, output_path, chart_path, _include_duplicates, minimum_pct, deviations)) =
        insert_size
    {
        fs::write(output_path, metrics.to_picard_text(minimum_pct, deviations))
            .map_err(|error| error.to_string())?;
        write_summary_chart_pdf(&chart_path, "CollectInsertSizeMetrics")?;
    }
    if let Some((metrics, output_path, chart_path, ..)) = base_distribution {
        fs::write(output_path, metrics.to_picard_text()).map_err(|error| error.to_string())?;
        write_summary_chart_pdf(&chart_path, "CollectBaseDistributionByCycle")?;
    }
    if let Some((metrics, output_path, chart_path, ..)) = quality_distribution {
        fs::write(output_path, metrics.to_picard_text()).map_err(|error| error.to_string())?;
        write_summary_chart_pdf(&chart_path, "QualityScoreDistribution")?;
    }
    if let Some((metrics, output_path, chart_path, ..)) = mean_quality {
        fs::write(output_path, metrics.to_picard_text()).map_err(|error| error.to_string())?;
        write_summary_chart_pdf(&chart_path, "MeanQualityByCycle")?;
    }
    if let Some((metrics, output_path, ..)) = quality_yield {
        fs::write(output_path, metrics.to_picard_text()).map_err(|error| error.to_string())?;
    }
    if let Some((
        metrics,
        detail_output,
        summary_output,
        chart_path,
        window_size,
        minimum_genome_fraction,
        ..,
    )) = gc_bias
    {
        fs::write(
            detail_output,
            metrics.detail_text(window_size, minimum_genome_fraction),
        )
        .map_err(|error| error.to_string())?;
        fs::write(
            summary_output,
            metrics.summary_text(window_size, minimum_genome_fraction),
        )
        .map_err(|error| error.to_string())?;
        write_summary_chart_pdf(&chart_path, "CollectGcBiasMetrics")?;
    }
    if let Some((
        mut metrics,
        output_path,
        _minimum_mapping_quality,
        _minimum_base_quality,
        _coverage_cap,
        _locus_accumulation_cap,
        _count_unpaired,
        sample_size,
        include_bq_histogram,
    )) = wgs
    {
        metrics.finish();
        write_text_or_gzip(
            &output_path,
            &metrics.to_picard_text(sample_size, include_bq_histogram),
        )?;
    }

    Ok(())
}

fn run_collectmultiplemetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectMultipleMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collectmultiplemetrics_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectMultipleMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectMultipleMetrics")?;
    let file_extension = optional_scalar(&args, "FILE_EXTENSION")?.unwrap_or_default();
    let programs = collectmultiplemetrics_programs(&args)?;
    if collectmultiplemetrics_can_single_pass(&input, &programs) {
        return run_collectmultiplemetrics_single_pass(
            &args,
            &input,
            &output,
            &file_extension,
            &programs,
        );
    }
    let stop_after_arg = optional_scalar(&args, "STOP_AFTER")?
        .map(|value| format!("STOP_AFTER={value}"))
        .into_iter()
        .collect::<Vec<_>>();
    let accumulation_arg = optional_scalar(&args, "METRIC_ACCUMULATION_LEVEL")?
        .map(|value| format!("METRIC_ACCUMULATION_LEVEL={value}"))
        .into_iter()
        .collect::<Vec<_>>();

    for program in programs {
        match program.as_str() {
            "CollectAlignmentSummaryMetrics" => {
                let mut child_args = vec![
                    format!("I={input}"),
                    format!(
                        "O={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".alignment_summary_metrics",
                            &file_extension
                        )
                    ),
                    "VALIDATION_STRINGENCY=SILENT".to_string(),
                    "QUIET=true".to_string(),
                ];
                child_args.extend(accumulation_arg.clone());
                child_args.extend(stop_after_arg.clone());
                run_collectalignmentsummarymetrics(&child_args)?;
                write_summary_chart_pdf(
                    &format!("{output}.read_length_histogram.pdf"),
                    "CollectAlignmentSummaryMetrics",
                )?;
            }
            "CollectInsertSizeMetrics" => {
                let mut child_args = vec![
                    format!("I={input}"),
                    format!(
                        "O={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".insert_size_metrics",
                            &file_extension
                        )
                    ),
                    format!("H={output}.insert_size_histogram.pdf"),
                    "VALIDATION_STRINGENCY=SILENT".to_string(),
                    "QUIET=true".to_string(),
                ];
                extend_collectmultiplemetrics_extra_arguments(
                    &args,
                    &program,
                    &["INCLUDE_DUPLICATES", "DEVIATIONS", "MINIMUM_PCT"],
                    &mut child_args,
                );
                child_args.extend(accumulation_arg.clone());
                child_args.extend(stop_after_arg.clone());
                run_collectinsertsizemetrics(&child_args)?;
            }
            "CollectBaseDistributionByCycle" => {
                let mut child_args = vec![
                    format!("I={input}"),
                    format!(
                        "O={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".base_distribution_by_cycle_metrics",
                            &file_extension
                        )
                    ),
                    format!("CHART={output}.base_distribution_by_cycle.pdf"),
                    "VALIDATION_STRINGENCY=SILENT".to_string(),
                    "QUIET=true".to_string(),
                ];
                extend_collectmultiplemetrics_extra_arguments(
                    &args,
                    &program,
                    &["ALIGNED_READS_ONLY", "PF_READS_ONLY"],
                    &mut child_args,
                );
                child_args.extend(stop_after_arg.clone());
                run_collectbasedistributionbycycle(&child_args)?;
            }
            "CollectGcBiasMetrics" => {
                let reference = optional_scalar(&args, "REFERENCE_SEQUENCE")?.ok_or_else(|| {
                    "missing required CollectMultipleMetrics argument for CollectGcBiasMetrics: REFERENCE_SEQUENCE"
                        .to_string()
                })?;
                let mut child_args = vec![
                    format!("I={input}"),
                    format!(
                        "O={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".gc_bias.detail_metrics",
                            &file_extension
                        )
                    ),
                    format!(
                        "S={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".gc_bias.summary_metrics",
                            &file_extension
                        )
                    ),
                    format!("CHART={output}.gc_bias.pdf"),
                    format!("R={reference}"),
                    "VALIDATION_STRINGENCY=SILENT".to_string(),
                    "QUIET=true".to_string(),
                ];
                if let Some(window_size) =
                    optional_scalar(&args, "SCAN_WINDOW_SIZE")?.or_else(|| {
                        collectmultiplemetrics_extra_argument(&args, &program, "SCAN_WINDOW_SIZE")
                    })
                {
                    child_args.push(format!("SCAN_WINDOW_SIZE={window_size}"));
                }
                if let Some(minimum_genome_fraction) =
                    optional_scalar(&args, "MINIMUM_GENOME_FRACTION")?.or_else(|| {
                        collectmultiplemetrics_extra_argument(
                            &args,
                            &program,
                            "MINIMUM_GENOME_FRACTION",
                        )
                    })
                {
                    child_args.push(format!("MINIMUM_GENOME_FRACTION={minimum_genome_fraction}"));
                }
                extend_collectmultiplemetrics_extra_arguments(
                    &args,
                    &program,
                    &["ALSO_IGNORE_DUPLICATES"],
                    &mut child_args,
                );
                child_args.extend(stop_after_arg.clone());
                run_collectgcbiasmetrics(&child_args)?;
            }
            "QualityScoreDistribution" => {
                let mut child_args = vec![
                    format!("I={input}"),
                    format!(
                        "O={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".quality_distribution_metrics",
                            &file_extension
                        )
                    ),
                    format!("CHART={output}.quality_distribution.pdf"),
                    "VALIDATION_STRINGENCY=SILENT".to_string(),
                    "QUIET=true".to_string(),
                ];
                extend_collectmultiplemetrics_extra_arguments(
                    &args,
                    &program,
                    &["ALIGNED_READS_ONLY", "PF_READS_ONLY", "INCLUDE_NO_CALLS"],
                    &mut child_args,
                );
                child_args.extend(stop_after_arg.clone());
                run_qualityscoredistribution(&child_args)?;
            }
            "MeanQualityByCycle" => {
                let mut child_args = vec![
                    format!("I={input}"),
                    format!(
                        "O={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".quality_by_cycle_metrics",
                            &file_extension
                        )
                    ),
                    format!("CHART={output}.quality_by_cycle.pdf"),
                    "VALIDATION_STRINGENCY=SILENT".to_string(),
                    "QUIET=true".to_string(),
                ];
                extend_collectmultiplemetrics_extra_arguments(
                    &args,
                    &program,
                    &["ALIGNED_READS_ONLY", "PF_READS_ONLY"],
                    &mut child_args,
                );
                child_args.extend(stop_after_arg.clone());
                run_meanqualitybycycle(&child_args)?;
            }
            "CollectQualityYieldMetrics" => {
                let mut child_args = vec![
                    format!("I={input}"),
                    format!(
                        "O={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".quality_yield_metrics",
                            &file_extension
                        )
                    ),
                    "VALIDATION_STRINGENCY=SILENT".to_string(),
                    "QUIET=true".to_string(),
                ];
                extend_collectmultiplemetrics_extra_arguments(
                    &args,
                    &program,
                    &[
                        "INCLUDE_SECONDARY_ALIGNMENTS",
                        "INCLUDE_SUPPLEMENTAL_ALIGNMENTS",
                        "USE_ORIGINAL_QUALITIES",
                    ],
                    &mut child_args,
                );
                child_args.extend(stop_after_arg.clone());
                run_collectqualityyieldmetrics(&child_args)?;
            }
            "CollectWgsMetrics" => {
                let reference = optional_scalar(&args, "REFERENCE_SEQUENCE")?.ok_or_else(|| {
                    "missing required CollectMultipleMetrics argument for CollectWgsMetrics: REFERENCE_SEQUENCE"
                        .to_string()
                })?;
                let use_fast_algorithm =
                    if let Some(use_fast_algorithm) = optional_bool(&args, "USE_FAST_ALGORITHM")? {
                        use_fast_algorithm
                    } else {
                        env_bool("TURBO_PICARD_WGS_FAST_DEFAULT")?.unwrap_or(false)
                    };
                let sample_size = optional_u32(&args, "SAMPLE_SIZE")?
                    .unwrap_or(if use_fast_algorithm { 0 } else { 10_000 });
                let include_bq_histogram =
                    optional_bool(&args, "INCLUDE_BQ_HISTOGRAM")?.unwrap_or(false);
                let mut child_args = vec![
                    format!("I={input}"),
                    format!(
                        "O={}",
                        collectmultiplemetrics_metric_path(
                            &output,
                            ".wgs_metrics",
                            &file_extension
                        )
                    ),
                    format!("R={reference}"),
                    "COUNT_UNPAIRED=true".to_string(),
                    format!("SAMPLE_SIZE={sample_size}"),
                    format!("INCLUDE_BQ_HISTOGRAM={include_bq_histogram}"),
                    "VALIDATION_STRINGENCY=SILENT".to_string(),
                    "QUIET=true".to_string(),
                ];
                child_args.extend(stop_after_arg.clone());
                run_collectwgsmetrics(&child_args)?;
            }
            _ => {
                return Err(format!(
                    "unsupported CollectMultipleMetrics PROGRAM={program}"
                ));
            }
        }
    }

    Ok(())
}

fn collectmultiplemetrics_metric_path(prefix: &str, suffix: &str, file_extension: &str) -> String {
    format!("{prefix}{suffix}{file_extension}")
}

fn run_collectwgsmetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectWgsMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collectwgsmetrics_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectWgsMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectWgsMetrics")?;
    let reference = required_scalar_for(&args, "REFERENCE_SEQUENCE", "CollectWgsMetrics")?;
    let minimum_mapping_quality = optional_u32(&args, "MINIMUM_MAPPING_QUALITY")?.unwrap_or(20);
    let minimum_base_quality = optional_u32(&args, "MINIMUM_BASE_QUALITY")?.unwrap_or(20);
    let coverage_cap = optional_u32(&args, "COVERAGE_CAP")?.unwrap_or(250);
    let locus_accumulation_cap = optional_u32(&args, "LOCUS_ACCUMULATION_CAP")?.unwrap_or(100_000);
    let stop_after = optional_i64(&args, "STOP_AFTER")?.unwrap_or(-1);
    let count_unpaired = optional_bool(&args, "COUNT_UNPAIRED")?.unwrap_or(false);
    let use_fast_algorithm =
        if let Some(use_fast_algorithm) = optional_bool(&args, "USE_FAST_ALGORITHM")? {
            use_fast_algorithm
        } else {
            env_bool("TURBO_PICARD_WGS_FAST_DEFAULT")?.unwrap_or(false)
        };
    let sample_size =
        optional_u32(&args, "SAMPLE_SIZE")?.unwrap_or(if use_fast_algorithm { 0 } else { 10_000 });
    let include_bq_histogram = optional_bool(&args, "INCLUDE_BQ_HISTOGRAM")?.unwrap_or(false);

    let reference_contigs = read_reference_contigs_for_wgs(&reference)?;
    let interval_masks = collectwgs_interval_masks(args.get("INTERVALS"), &reference_contigs)?;
    let mut summary = WgsMetricsSummary::new(&reference_contigs, interval_masks, coverage_cap);
    if stop_after >= 0 {
        summary.limit_included_loci(stop_after as usize);
    }
    let mut reader = open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let stop_after_u32 = if stop_after < 0 { 0 } else { stop_after as u32 };
    if hts_io::is_hts_container_input(&input) {
        cmm_pipeline::pipeline_bam_records(reader, stop_after_u32, 8192, |record| {
            summary.observe(
                &record,
                &target_names,
                minimum_mapping_quality as u8,
                minimum_base_quality as u8,
                coverage_cap,
                locus_accumulation_cap,
                count_unpaired,
            )
        })?;
    } else {
        for record in limited_records(&mut reader, stop_after_u32) {
            let record = record.map_err(|error| error.to_string())?;
            summary.observe(
                &record,
                &target_names,
                minimum_mapping_quality as u8,
                minimum_base_quality as u8,
                coverage_cap,
                locus_accumulation_cap,
                count_unpaired,
            )?;
        }
    }
    summary.finish();

    write_text_or_gzip(
        &output,
        &summary.to_picard_text(sample_size, include_bq_histogram),
    )
}

fn run_fixmateinformation(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("FixMateInformation", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_fixmateinformation_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "FixMateInformation")?;
    let Some(output) = optional_scalar(&args, "OUTPUT")? else {
        return Err("unsupported FixMateInformation missing OUTPUT".to_string());
    };
    let add_mate_cigar = optional_bool(&args, "ADD_MATE_CIGAR")?.unwrap_or(true);
    let ignore_missing_mates = optional_bool(&args, "IGNORE_MISSING_MATES")?.unwrap_or(true);
    let assume_sorted = optional_bool(&args, "ASSUME_SORTED")?.unwrap_or(false);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let reference = picard_reference(&args)?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let mut reader = open_bam_reader_with_reference(&inputs[0], reference.as_deref())
        .map_err(|error| error.to_string())?;
    let first_input_sort_order = header_sort_order(reader.header());
    let sort_order = match optional_scalar(&args, "SORT_ORDER")? {
        Some(sort_order) => match sort_order.as_str() {
            "queryname" => SortOrder::QueryName,
            "coordinate" => SortOrder::Coordinate,
            "unsorted" => SortOrder::Unsorted,
            value => return Err(format!("unsupported FixMateInformation SORT_ORDER={value}")),
        },
        None => match first_input_sort_order.as_deref() {
            Some("coordinate") => SortOrder::Coordinate,
            Some("queryname") => SortOrder::QueryName,
            _ => SortOrder::Unsorted,
        },
    };
    let output_format = output_format_for(&output, "FixMateInformation")?;
    if create_index && sort_order != SortOrder::Coordinate {
        return Err(
            "FixMateInformation CREATE_INDEX=true requires SORT_ORDER=coordinate".to_string(),
        );
    }
    if create_index && output_format != bam::Format::Bam {
        return Err("FixMateInformation CREATE_INDEX=true requires BAM output".to_string());
    }

    let requires_queryname_sort =
        !assume_sorted && first_input_sort_order.as_deref() != Some("queryname");
    if !requires_queryname_sort {
        for input in inputs.iter().skip(1) {
            let reader = open_bam_reader_with_reference(input, reference.as_deref())
                .map_err(|error| error.to_string())?;
            if header_sort_order(reader.header()).as_deref() != Some("queryname") {
                return Err(
                    "FixMateInformation non-queryname input should use upstream Picard".to_string(),
                );
            }
        }
    } else {
        return Err(
            "FixMateInformation non-queryname input should use upstream Picard".to_string(),
        );
    }
    let header = sorted_header_with_group_order(
        reader.header(),
        sort_order,
        (inputs.len() > 1).then_some("none"),
    );
    let mut writer = bam_writer_for_path_with_reference(
        &output,
        &header,
        output_format,
        reference.as_deref(),
        compression_level,
    )?;
    let mut pending = Vec::<bam::Record>::new();
    let mut fixed_records = Vec::<bam::Record>::new();

    process_fixmate_reader(
        &mut reader,
        &mut writer,
        &mut pending,
        &mut fixed_records,
        sort_order,
        add_mate_cigar,
        ignore_missing_mates,
    )?;
    for input in inputs.iter().skip(1) {
        let mut reader = open_bam_reader_with_reference(input, reference.as_deref())
            .map_err(|error| error.to_string())?;
        process_fixmate_reader(
            &mut reader,
            &mut writer,
            &mut pending,
            &mut fixed_records,
            sort_order,
            add_mate_cigar,
            ignore_missing_mates,
        )?;
    }
    if sort_order == SortOrder::Coordinate {
        fixed_records.extend(drain_fixed_mate_group(
            &mut pending,
            add_mate_cigar,
            ignore_missing_mates,
        )?);
        fixed_records.sort_by(compare_coordinate);
        for record in fixed_records {
            writer.write(&record).map_err(|error| error.to_string())?;
        }
    } else {
        write_fixed_mate_group(
            &mut writer,
            &mut pending,
            add_mate_cigar,
            ignore_missing_mates,
        )?;
    }
    drop(writer);

    write_requested_sidecars(&output, create_md5_file, create_index)
}

fn process_fixmate_reader(
    reader: &mut bam::Reader,
    writer: &mut bam::Writer,
    pending: &mut Vec<bam::Record>,
    fixed_records: &mut Vec<bam::Record>,
    sort_order: SortOrder,
    add_mate_cigar: bool,
    ignore_missing_mates: bool,
) -> Result<(), String> {
    for record in reader.records() {
        process_fixmate_record(
            record.map_err(|error| error.to_string())?,
            writer,
            pending,
            fixed_records,
            sort_order,
            add_mate_cigar,
            ignore_missing_mates,
        )?;
    }
    Ok(())
}

fn process_fixmate_record(
    record: bam::Record,
    writer: &mut bam::Writer,
    pending: &mut Vec<bam::Record>,
    fixed_records: &mut Vec<bam::Record>,
    sort_order: SortOrder,
    add_mate_cigar: bool,
    ignore_missing_mates: bool,
) -> Result<(), String> {
    if pending
        .first()
        .is_some_and(|first| first.qname() != record.qname())
    {
        if sort_order == SortOrder::Coordinate {
            fixed_records.extend(drain_fixed_mate_group(
                pending,
                add_mate_cigar,
                ignore_missing_mates,
            )?);
        } else {
            write_fixed_mate_group(writer, pending, add_mate_cigar, ignore_missing_mates)?;
        }
    }
    pending.push(record);
    Ok(())
}

fn run_qualityscoredistribution(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("QualityScoreDistribution", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_qualityscoredistribution_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "QualityScoreDistribution")?;
    let output = required_scalar_for(&args, "OUTPUT", "QualityScoreDistribution")?;
    let chart = required_scalar_for(&args, "CHART_OUTPUT", "QualityScoreDistribution")?;
    let aligned_reads_only = optional_bool(&args, "ALIGNED_READS_ONLY")?.unwrap_or(false);
    let pf_reads_only = optional_bool(&args, "PF_READS_ONLY")?.unwrap_or(false);
    let include_no_calls = optional_bool(&args, "INCLUDE_NO_CALLS")?.unwrap_or(false);
    let stop_after = optional_u32(&args, "STOP_AFTER")?.unwrap_or(0);

    let metrics = if has_sam_extension(&input) {
        collect_quality_score_distribution_sam_text(
            &input,
            aligned_reads_only,
            pf_reads_only,
            include_no_calls,
            stop_after,
        )?
    } else {
        let mut reader =
            open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
        let mut metrics = QualityScoreDistributionSummary::default();
        for record in limited_records(&mut reader, stop_after) {
            let record = record.map_err(|error| error.to_string())?;
            metrics.observe(&record, aligned_reads_only, pf_reads_only, include_no_calls);
        }
        metrics
    };

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())?;
    write_summary_chart_pdf(&chart, "QualityScoreDistribution")
}

fn run_meanqualitybycycle(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("MeanQualityByCycle", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_meanqualitybycycle_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "MeanQualityByCycle")?;
    let output = required_scalar_for(&args, "OUTPUT", "MeanQualityByCycle")?;
    let chart = required_scalar_for(&args, "CHART_OUTPUT", "MeanQualityByCycle")?;
    let aligned_reads_only = optional_bool(&args, "ALIGNED_READS_ONLY")?.unwrap_or(false);
    let pf_reads_only = optional_bool(&args, "PF_READS_ONLY")?.unwrap_or(false);
    let stop_after = optional_u32(&args, "STOP_AFTER")?.unwrap_or(0);

    let metrics = if has_sam_extension(&input) {
        collect_mean_quality_by_cycle_sam_text(
            &input,
            aligned_reads_only,
            pf_reads_only,
            stop_after,
        )?
    } else {
        let mut reader =
            open_bam_reader_for_args(&input, &args).map_err(|error| error.to_string())?;
        let mut metrics = MeanQualityByCycleSummary::default();
        for record in limited_records(&mut reader, stop_after) {
            let record = record.map_err(|error| error.to_string())?;
            metrics.observe(&record, aligned_reads_only, pf_reads_only);
        }
        metrics
    };

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())?;
    write_summary_chart_pdf(&chart, "MeanQualityByCycle")
}

fn run_createsequencedictionary(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CreateSequenceDictionary", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_createsequencedictionary_args(&args)?;
    let reference = required_scalar_for(&args, "REFERENCE_SEQUENCE", "CreateSequenceDictionary")?;
    let output = optional_scalar(&args, "OUTPUT")?.unwrap_or_else(|| derived_dict_path(&reference));
    let truncate_names = optional_bool(&args, "TRUNCATE_NAMES_AT_WHITESPACE")?.unwrap_or(true);
    let uri = optional_scalar(&args, "URI")?.unwrap_or_else(|| format!("file://{reference}"));
    let assembly = optional_scalar(&args, "GENOME_ASSEMBLY")?;
    let species = optional_scalar(&args, "SPECIES")?;
    let num_sequences = optional_u32(&args, "NUM_SEQUENCES")?;
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let alt_names = match optional_scalar(&args, "ALT_NAMES")? {
        Some(path) => read_alt_names(&path)?,
        None => BTreeMap::new(),
    };

    let mut records = read_fasta_sequences(&reference, truncate_names)?;
    if let Some(limit) = num_sequences {
        records.truncate(limit as usize);
    }
    let mut dictionary = String::from("@HD\tVN:1.6\n");
    for record in records {
        dictionary.push_str(&format!(
            "@SQ\tSN:{}\tLN:{}\tM5:{:x}\tUR:{}",
            record.name,
            record.sequence.len(),
            md5::compute(&record.sequence),
            uri,
        ));
        if let Some(assembly) = assembly.as_deref() {
            dictionary.push_str(&format!("\tAS:{assembly}"));
        }
        if let Some(species) = species.as_deref() {
            dictionary.push_str(&format!("\tSP:{species}"));
        }
        if let Some(names) = alt_names.get(&record.name) {
            dictionary.push_str("\tAN:");
            dictionary.push_str(&names.join(","));
        }
        dictionary.push('\n');
    }

    fs::write(&output, dictionary).map_err(|error| error.to_string())?;
    write_requested_sidecars(&output, create_md5_file, false)
}

fn read_alt_names(path: &str) -> Result<BTreeMap<String, Vec<String>>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut alt_names = BTreeMap::<String, Vec<String>>::new();
    for (line_index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() < 2 {
            return Err(format!(
                "malformed CreateSequenceDictionary ALT_NAMES line {}",
                line_index + 1
            ));
        }
        alt_names
            .entry(fields[0].to_string())
            .or_default()
            .push(fields[1].to_string());
    }
    Ok(alt_names)
}

fn run_normalizefasta(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("NormalizeFasta", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_normalizefasta_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "NormalizeFasta")?;
    let output = required_scalar_for(&args, "OUTPUT", "NormalizeFasta")?;
    let line_length = optional_u32(&args, "LINE_LENGTH")?.unwrap_or(100) as usize;
    let truncate_names =
        optional_bool(&args, "TRUNCATE_SEQUENCE_NAMES_AT_WHITESPACE")?.unwrap_or(false);
    if line_length == 0 {
        return Err("unsupported NormalizeFasta LINE_LENGTH=0".to_string());
    }

    let text = read_text_or_gzip(&input)?;
    let mut normalized = String::new();
    let mut current_sequence = Vec::<u8>::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            flush_fasta_sequence(&mut normalized, &current_sequence, line_length);
            current_sequence.clear();
            let header = if truncate_names {
                header.split_whitespace().next().unwrap_or_default()
            } else {
                header
            };
            normalized.push('>');
            normalized.push_str(header);
            normalized.push('\n');
        } else {
            current_sequence.extend(line.trim().as_bytes());
        }
    }
    flush_fasta_sequence(&mut normalized, &current_sequence, line_length);
    fs::write(output, normalized).map_err(|error| error.to_string())
}

fn run_bedtointervallist(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("BedToIntervalList", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_bedtointervallist_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "BedToIntervalList")?;
    let output = required_scalar_for(&args, "OUTPUT", "BedToIntervalList")?;
    let dictionary_path = required_scalar_for(&args, "SEQUENCE_DICTIONARY", "BedToIntervalList")?;
    let sort = optional_bool(&args, "SORT")?.unwrap_or(true);
    let unique = optional_bool(&args, "UNIQUE")?.unwrap_or(false);
    let drop_missing_contigs = optional_bool(&args, "DROP_MISSING_CONTIGS")?.unwrap_or(false);
    let keep_length_zero_intervals =
        optional_bool(&args, "KEEP_LENGTH_ZERO_INTERVALS")?.unwrap_or(false);

    let dictionary_text =
        fs::read_to_string(&dictionary_path).map_err(|error| error.to_string())?;
    let contig_order = dictionary_contig_order(&dictionary_text);
    let mut intervals = read_bed_intervals(
        &input,
        &contig_order,
        drop_missing_contigs,
        keep_length_zero_intervals,
    )?;
    if sort {
        intervals.sort_by(|left, right| {
            left.contig_index
                .cmp(&right.contig_index)
                .then_with(|| left.start.cmp(&right.start))
                .then_with(|| left.end.cmp(&right.end))
                .then_with(|| left.name.cmp(&right.name))
        });
    }
    if unique {
        intervals.dedup_by(|left, right| {
            left.contig == right.contig
                && left.start == right.start
                && left.end == right.end
                && left.strand == right.strand
                && left.name == right.name
        });
    }

    let mut text = bed_interval_list_header(&dictionary_text, sort);
    for interval in intervals {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            interval.contig, interval.start, interval.end, interval.strand, interval.name
        ));
    }
    fs::write(output, text).map_err(|error| error.to_string())
}

fn bed_interval_list_header(dictionary_text: &str, _sort: bool) -> String {
    let sort_order = "coordinate";
    let mut text = String::new();
    let mut saw_hd = false;
    for line in dictionary_text.lines().filter(|line| line.starts_with('@')) {
        if line.starts_with("@HD\t") {
            saw_hd = true;
            let mut fields = vec!["@HD".to_string()];
            let mut saw_so = false;
            for field in line.split('\t').skip(1) {
                if field.starts_with("SO:") {
                    fields.push(format!("SO:{sort_order}"));
                    saw_so = true;
                } else {
                    fields.push(field.to_string());
                }
            }
            if !saw_so {
                fields.push(format!("SO:{sort_order}"));
            }
            text.push_str(&fields.join("\t"));
            text.push('\n');
        } else {
            text.push_str(line);
            text.push('\n');
        }
    }
    if saw_hd {
        text
    } else {
        format!("@HD\tVN:1.6\tSO:{sort_order}\n{text}")
    }
}

fn run_intervallisttools(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("IntervalListTools", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_intervallisttools_args(&args)?;
    let inputs = args
        .get("INPUT")
        .filter(|values| !values.is_empty())
        .ok_or_else(|| "missing required IntervalListTools argument: INPUT".to_string())?;
    let output = required_scalar_for(&args, "OUTPUT", "IntervalListTools")?;
    let sort = optional_bool(&args, "SORT")?.unwrap_or(true);
    let unique = optional_bool(&args, "UNIQUE")?.unwrap_or(false);
    let dont_merge_abutting = optional_bool(&args, "DONT_MERGE_ABUTTING")?.unwrap_or(false);
    let padding = optional_i64(&args, "PADDING")?.unwrap_or(0);

    let first_text = fs::read_to_string(&inputs[0]).map_err(|error| error.to_string())?;
    let header_text = interval_list_header_text(&first_text);
    let contig_order = dictionary_contig_order(&header_text);
    let contig_lengths = dictionary_contig_lengths(&header_text);
    let mut intervals = Vec::<BedInterval>::new();
    intervals.extend(read_interval_list_intervals(&first_text, &contig_order)?);
    for input in inputs.iter().skip(1) {
        let text = fs::read_to_string(input).map_err(|error| error.to_string())?;
        intervals.extend(read_interval_list_intervals(&text, &contig_order)?);
    }
    if padding > 0 {
        apply_interval_padding(&mut intervals, &contig_lengths, padding as u64)?;
    }

    if sort || unique {
        sort_intervals(&mut intervals);
    }
    if unique {
        intervals = unique_intervals(intervals, dont_merge_abutting);
    }

    let mut text = interval_list_output_header(&header_text, inputs.len() > 1);
    for interval in intervals {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\n",
            interval.contig, interval.start, interval.end, interval.strand, interval.name
        ));
    }
    fs::write(output, text).map_err(|error| error.to_string())
}

fn revertsam_can_use_sam_text_fast_path(
    input: &str,
    output: &str,
    compression_level: Option<u32>,
    restore_original_qualities: bool,
    remove_alignment_information: bool,
    attributes_to_clear: &[[u8; 2]],
    attributes_to_reverse: &[[u8; 2]],
    attributes_to_reverse_complement: &[[u8; 2]],
) -> bool {
    has_sam_extension(input)
        && (has_sam_extension(output) || has_extension(output, "bam"))
        && compression_level.is_none()
        && restore_original_qualities
        && remove_alignment_information
        && attributes_to_clear.is_empty()
        && attributes_to_reverse.is_empty()
        && attributes_to_reverse_complement.is_empty()
}

#[derive(Debug)]
struct RevertsamTextRecord {
    line: String,
    qname: String,
    flags: u16,
    serial: usize,
}

fn collect_revertsam_sam_text_records(
    input: &str,
    remove_duplicate_information: bool,
    restore_hardclips: bool,
    sort_order: SortOrder,
) -> Result<(Vec<String>, Vec<RevertsamTextRecord>), String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut header_lines = Vec::<String>::new();
    let mut records = Vec::<RevertsamTextRecord>::new();
    let mut line = String::with_capacity(512);
    let mut serial = 0usize;

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with('@') {
            header_lines.push(line.clone());
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        let flags = line
            .split('\t')
            .nth(1)
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(0);
        if flags & 0x100 != 0 || flags & 0x800 != 0 {
            continue;
        }
        let (reverted, qname, flags) = revert_sam_text_record_line(
            line.trim_end_matches(['\r', '\n']),
            remove_duplicate_information,
            restore_hardclips,
        )?;
        records.push(RevertsamTextRecord {
            line: reverted,
            qname,
            flags,
            serial,
        });
        serial += 1;
    }

    if sort_order == SortOrder::QueryName {
        records.sort_unstable_by(|left, right| {
            left.qname
                .as_bytes()
                .cmp(right.qname.as_bytes())
                .then_with(|| left.flags.cmp(&right.flags))
                .then_with(|| left.serial.cmp(&right.serial))
        });
    }

    Ok((header_lines, records))
}

fn run_revertsam_sam_text(
    input: &str,
    output: &str,
    remove_alignment_information: bool,
    remove_duplicate_information: bool,
    restore_hardclips: bool,
    sort_order: SortOrder,
) -> Result<(), String> {
    let (header_lines, records) = collect_revertsam_sam_text_records(
        input,
        remove_duplicate_information,
        restore_hardclips,
        sort_order,
    )?;

    let mut writer = BufWriter::with_capacity(
        1024 * 1024,
        fs::File::create(output).map_err(|error| error.to_string())?,
    );
    write_revertsam_sam_text_header(
        &mut writer,
        &header_lines,
        sort_order,
        remove_alignment_information,
    )?;
    for record in records {
        writer
            .write_all(record.line.as_bytes())
            .map_err(|error| error.to_string())?;
        writer.write_all(b"\n").map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn write_revertsam_sam_text_header(
    writer: &mut dyn Write,
    header_lines: &[String],
    sort_order: SortOrder,
    remove_alignment_information: bool,
) -> Result<(), String> {
    let sort_value = match sort_order {
        SortOrder::Coordinate => "coordinate",
        SortOrder::QueryName => "queryname",
        SortOrder::Unsorted => "unsorted",
    };
    let mut saw_hd = false;
    for line in header_lines {
        if remove_alignment_information && (line.starts_with("@SQ\t") || line.starts_with("@PG\t"))
        {
            continue;
        }
        if line.starts_with("@HD\t") {
            saw_hd = true;
            let mut fields = vec!["@HD".to_string()];
            let mut saw_so = false;
            for field in line.trim_end_matches(['\r', '\n']).split('\t').skip(1) {
                if field.starts_with("SO:") {
                    fields.push(format!("SO:{sort_value}"));
                    saw_so = true;
                } else {
                    fields.push(field.to_string());
                }
            }
            if !saw_so {
                fields.push(format!("SO:{sort_value}"));
            }
            writeln!(writer, "{}", fields.join("\t")).map_err(|error| error.to_string())?;
            continue;
        }
        write!(writer, "{line}").map_err(|error| error.to_string())?;
        if !line.ends_with('\n') {
            writeln!(writer).map_err(|error| error.to_string())?;
        }
    }
    if !saw_hd {
        writeln!(writer, "@HD\tVN:1.6\tSO:{sort_value}").map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn run_revertsam_sam_text_to_bam(
    input: &str,
    output: &str,
    output_format: bam::Format,
    compression_level: Option<u32>,
    reference: Option<&str>,
    remove_duplicate_information: bool,
    restore_hardclips: bool,
    sort_order: SortOrder,
    create_md5_file: bool,
    create_index: bool,
) -> Result<(), String> {
    let (header_lines, records) = collect_revertsam_sam_text_records(
        input,
        remove_duplicate_information,
        restore_hardclips,
        sort_order,
    )?;
    let header = reverted_header_from_sam_text(&header_lines, true, sort_order)?;
    let mut writer = bam_writer_for_path_with_reference(
        output,
        &header,
        output_format,
        reference,
        compression_level,
    )?;
    for record in records {
        writer
            .write(&reverted_sam_line_to_bam_record(&record.line)?)
            .map_err(|error| error.to_string())?;
    }
    drop(writer);
    write_requested_sidecars(
        output,
        create_md5_file,
        create_index && sort_order == SortOrder::Coordinate,
    )
}

fn reverted_header_from_sam_text(
    header_lines: &[String],
    remove_alignment_information: bool,
    sort_order: SortOrder,
) -> Result<bam::Header, String> {
    let sort_value = match sort_order {
        SortOrder::Coordinate => "coordinate",
        SortOrder::QueryName => "queryname",
        SortOrder::Unsorted => "unsorted",
    };
    let mut header = bam::Header::new();
    let mut saw_hd = false;
    for line in header_lines {
        if remove_alignment_information && (line.starts_with("@SQ\t") || line.starts_with("@PG\t"))
        {
            continue;
        }
        if line.starts_with("@HD\t") {
            saw_hd = true;
            let mut record = HeaderRecord::new(b"HD");
            let mut saw_so = false;
            for field in line.trim_end_matches(['\r', '\n']).split('\t').skip(1) {
                if let Some(value) = field.strip_prefix("SO:") {
                    record.push_tag(b"SO", sort_value);
                    saw_so = saw_so || !value.is_empty();
                } else if let Some((tag, value)) = field.split_once(':') {
                    record.push_tag(tag.as_bytes(), value);
                }
            }
            if !saw_so {
                record.push_tag(b"SO", sort_value);
            }
            header.push_record(&record);
            continue;
        }
        if let Some(record) = parse_sam_header_record_line(line) {
            header.push_record(&record);
        }
    }
    if !saw_hd {
        header.push_record(
            HeaderRecord::new(b"HD")
                .push_tag(b"VN", "1.6")
                .push_tag(b"SO", sort_value),
        );
    }
    Ok(header)
}

fn parse_sam_header_record_line(line: &str) -> Option<HeaderRecord<'_>> {
    let mut parts = line.trim_end_matches(['\r', '\n']).split('\t');
    let record_type = parts.next()?;
    if !record_type.starts_with('@') {
        return None;
    }
    let mut record = HeaderRecord::new(&record_type.as_bytes()[1..]);
    for field in parts {
        if let Some((tag, value)) = field.split_once(':') {
            record.push_tag(tag.as_bytes(), value);
        }
    }
    Some(record)
}

fn reverted_sam_line_to_bam_record(line: &str) -> Result<bam::Record, String> {
    let mut fields = line.split('\t');
    let qname = fields
        .next()
        .ok_or_else(|| "malformed RevertSam SAM record".to_string())?;
    let flags = fields
        .next()
        .ok_or_else(|| "malformed RevertSam SAM record".to_string())?
        .parse::<u16>()
        .map_err(|_| "malformed RevertSam SAM flag".to_string())?;
    fields.next();
    fields.next();
    fields.next();
    fields.next();
    fields.next();
    fields.next();
    fields.next();
    let sequence = fields
        .next()
        .ok_or_else(|| "malformed RevertSam SAM record".to_string())?;
    let qualities = fields
        .next()
        .ok_or_else(|| "malformed RevertSam SAM record".to_string())?;
    let sequence = sequence.as_bytes();
    let qualities = qualities
        .as_bytes()
        .iter()
        .map(|quality| quality.saturating_sub(33))
        .collect::<Vec<_>>();
    let mut record = bam::Record::new();
    record.set(qname.as_bytes(), None, sequence, &qualities);
    record.set_flags(flags);
    record.set_tid(-1);
    record.set_pos(-1);
    record.set_mapq(0);
    record.set_mtid(-1);
    record.set_mpos(-1);
    record.set_insert_size(0);
    for tag_field in fields {
        push_sam_aux_tag(&mut record, tag_field)?;
    }
    Ok(record)
}

fn push_sam_aux_tag(record: &mut bam::Record, tag_field: &str) -> Result<(), String> {
    let Some((tag, tag_type, value)) = parse_sam_aux_field(tag_field) else {
        return Err(format!("malformed RevertSam SAM tag: {tag_field}"));
    };
    match tag_type {
        b'Z' => record
            .push_aux(tag, Aux::String(value))
            .map_err(|error| error.to_string()),
        b'i' | b'I' => record
            .push_aux(
                tag,
                Aux::I32(
                    value
                        .parse::<i32>()
                        .map_err(|_| format!("malformed RevertSam SAM tag: {tag_field}"))?,
                ),
            )
            .map_err(|error| error.to_string()),
        _ => Ok(()),
    }
}

fn parse_sam_aux_field(tag_field: &str) -> Option<(&[u8], u8, &str)> {
    let bytes = tag_field.as_bytes();
    if bytes.len() < 5 || bytes[2] != b':' {
        return None;
    }
    let tag_type = bytes[3];
    if bytes.get(4) != Some(&b':') {
        return None;
    }
    Some((&bytes[..2], tag_type, &tag_field[5..]))
}

fn revert_sam_text_record_line(
    line: &str,
    remove_duplicate_information: bool,
    restore_hardclips: bool,
) -> Result<(String, String, u16), String> {
    let fields = line.split('\t').collect::<Vec<_>>();
    if fields.len() < 11 {
        return Err("malformed RevertSam SAM record".to_string());
    }
    let qname = fields[0].to_string();
    let mut flags = fields[1]
        .parse::<u16>()
        .map_err(|_| "malformed RevertSam SAM flag".to_string())?;
    let mut sequence = fields[9].as_bytes().to_vec();
    let mut qualities = fields[10].as_bytes().to_vec();
    let mut kept_aux = Vec::<String>::new();
    let mut hardclip_bases = None::<Vec<u8>>;
    let mut hardclip_qualities = None::<Vec<u8>>;

    for tag_field in &fields[11..] {
        if let Some(original_qualities) = tag_field.strip_prefix("OQ:Z:") {
            qualities = original_qualities.as_bytes().to_vec();
            continue;
        }
        if restore_hardclips && let Some(bases) = tag_field.strip_prefix("XB:Z:") {
            hardclip_bases = Some(bases.as_bytes().to_vec());
            continue;
        }
        if restore_hardclips && let Some(qualities) = tag_field.strip_prefix("XQ:Z:") {
            hardclip_qualities = Some(qualities.as_bytes().to_vec());
            continue;
        }
        if revertsam_default_removed_alignment_tag_field(tag_field) {
            continue;
        }
        kept_aux.push((*tag_field).to_string());
    }

    if flags & 0x10 != 0 {
        reverse_complement(&mut sequence);
        qualities.reverse();
    }
    if let (Some(bases), Some(quals)) = (hardclip_bases, hardclip_qualities) {
        if bases.len() != quals.len() {
            return Err("malformed RevertSam XB/XQ lengths differ".to_string());
        }
        sequence.extend(bases);
        qualities.extend(quals);
    }

    flags |= 0x4;
    if flags & 0x1 != 0 {
        flags |= 0x8;
    } else {
        flags &= !0x8;
    }
    flags &= !(0x2 | 0x10 | 0x20 | 0x100 | 0x800);
    if remove_duplicate_information {
        flags &= !0x400;
    }

    kept_aux.sort();
    let mut reverted = format!(
        "{qname}\t{flags}\t*\t0\t0\t*\t*\t0\t0\t{seq}\t{qual}",
        qname = qname,
        flags = flags,
        seq = String::from_utf8(sequence).map_err(|_| "malformed RevertSam sequence".to_string())?,
        qual =
            String::from_utf8(qualities).map_err(|_| "malformed RevertSam qualities".to_string())?,
    );
    for tag in kept_aux {
        reverted.push('\t');
        reverted.push_str(&tag);
    }
    Ok((reverted, qname, flags))
}

fn revertsam_default_removed_alignment_tag_field(tag_field: &str) -> bool {
    tag_field.starts_with("NM:")
        || tag_field.starts_with("UQ:")
        || tag_field.starts_with("PG:")
        || tag_field.starts_with("MD:")
        || tag_field.starts_with("MQ:")
        || tag_field.starts_with("SA:")
        || tag_field.starts_with("MC:")
        || tag_field.starts_with("AS:")
}

fn run_revertsam(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("RevertSam", args).map_err(|error| error.to_string())?;
    reject_unsupported_revertsam_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "RevertSam")?;
    let output = required_scalar_for(&args, "OUTPUT", "RevertSam")?;
    let output_format = output_format_for(&output, "RevertSam")?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let restore_original_qualities =
        optional_bool(&args, "RESTORE_ORIGINAL_QUALITIES")?.unwrap_or(true);
    let remove_alignment_information =
        optional_bool(&args, "REMOVE_ALIGNMENT_INFORMATION")?.unwrap_or(true);
    let remove_duplicate_information =
        optional_bool(&args, "REMOVE_DUPLICATE_INFORMATION")?.unwrap_or(true);
    let restore_hardclips = optional_bool(&args, "RESTORE_HARDCLIPS")?.unwrap_or(true);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let attributes_to_reverse = attributes_for_revertsam(&args, "ATTRIBUTE_TO_REVERSE")?;
    let attributes_to_reverse_complement =
        attributes_for_revertsam(&args, "ATTRIBUTE_TO_REVERSE_COMPLEMENT")?;
    let sort_order = match optional_scalar(&args, "SORT_ORDER")?
        .unwrap_or_else(|| "queryname".to_string())
        .as_str()
    {
        "queryname" => SortOrder::QueryName,
        "coordinate" => SortOrder::Coordinate,
        "unsorted" => SortOrder::Unsorted,
        value => return Err(format!("unsupported RevertSam SORT_ORDER={value}")),
    };
    if create_index
        && sort_order == SortOrder::Coordinate
        && !matches!(output_format, bam::Format::Bam)
    {
        return Err("RevertSam CREATE_INDEX=true requires BAM output".to_string());
    }
    let attributes_to_clear = attributes_to_clear_for_revertsam(&args)?;
    let reference = picard_reference(&args)?;

    if revertsam_can_use_sam_text_fast_path(
        &input,
        &output,
        compression_level,
        restore_original_qualities,
        remove_alignment_information,
        &attributes_to_clear,
        &attributes_to_reverse,
        &attributes_to_reverse_complement,
    ) {
        if has_sam_extension(&output) {
            return run_revertsam_sam_text(
                &input,
                &output,
                remove_alignment_information,
                remove_duplicate_information,
                restore_hardclips,
                sort_order,
            );
        }
        return run_revertsam_sam_text_to_bam(
            &input,
            &output,
            output_format,
            compression_level,
            reference.as_deref(),
            remove_duplicate_information,
            restore_hardclips,
            sort_order,
            create_md5_file,
            create_index,
        );
    }

    if let Some(()) = try_stream_revertsam(
        &input,
        &output,
        output_format,
        compression_level,
        reference.as_deref(),
        restore_original_qualities,
        remove_alignment_information,
        remove_duplicate_information,
        restore_hardclips,
        &attributes_to_clear,
        &attributes_to_reverse,
        &attributes_to_reverse_complement,
        sort_order,
    )? {
        return write_requested_sidecars(
            &output,
            create_md5_file,
            create_index && sort_order == SortOrder::Coordinate,
        );
    }

    let mut reader = open_bam_reader_with_reference(&input, reference.as_deref())?;
    let header = reverted_header(reader.header(), remove_alignment_information, sort_order);
    let mut records = Vec::new();
    for record in reader.records() {
        let mut record = record.map_err(|error| error.to_string())?;
        if record.is_secondary() || record.is_supplementary() {
            continue;
        }
        revert_record(
            &mut record,
            restore_original_qualities,
            remove_alignment_information,
            remove_duplicate_information,
            restore_hardclips,
            &attributes_to_clear,
            &attributes_to_reverse,
            &attributes_to_reverse_complement,
        )?;
        records.push(record);
    }
    if sort_order == SortOrder::QueryName && !queryname_records_are_monotonic(&records) {
        records.sort_unstable_by(compare_queryname);
    }

    let mut writer = bam_writer_for_path_with_reference(
        &output,
        &header,
        output_format,
        reference.as_deref(),
        compression_level,
    )?;
    for record in records {
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    drop(writer);

    write_requested_sidecars(
        &output,
        create_md5_file,
        create_index && sort_order == SortOrder::Coordinate,
    )
}

fn try_stream_revertsam(
    input: &str,
    output: &str,
    output_format: bam::Format,
    compression_level: Option<u32>,
    reference: Option<&str>,
    restore_original_qualities: bool,
    remove_alignment_information: bool,
    remove_duplicate_information: bool,
    restore_hardclips: bool,
    attributes_to_clear: &[[u8; 2]],
    attributes_to_reverse: &[[u8; 2]],
    attributes_to_reverse_complement: &[[u8; 2]],
    sort_order: SortOrder,
) -> Result<Option<()>, String> {
    if sort_order == SortOrder::Coordinate {
        return Ok(None);
    }

    let stream_output = if sort_order == SortOrder::QueryName {
        temp_revertsam_output_path(output)
    } else {
        output.to_string()
    };
    let mut reader = open_bam_reader_with_reference(input, reference)?;
    if sort_order == SortOrder::QueryName
        && header_sort_order(reader.header()).as_deref() == Some("coordinate")
    {
        return Ok(None);
    }
    let header = reverted_header(reader.header(), remove_alignment_information, sort_order);
    let mut writer = bam_writer_for_path_with_reference(
        &stream_output,
        &header,
        output_format,
        reference,
        compression_level,
    )?;
    let mut last_query_name = Vec::<u8>::new();
    let mut have_last_query_name = false;

    for record in reader.records() {
        let mut record = record.map_err(|error| error.to_string())?;
        if record.is_secondary() || record.is_supplementary() {
            continue;
        }
        revert_record(
            &mut record,
            restore_original_qualities,
            remove_alignment_information,
            remove_duplicate_information,
            restore_hardclips,
            attributes_to_clear,
            attributes_to_reverse,
            attributes_to_reverse_complement,
        )?;
        if sort_order == SortOrder::QueryName {
            let qname = record.qname();
            if have_last_query_name && last_query_name.as_slice() > qname {
                drop(writer);
                let _ = fs::remove_file(&stream_output);
                return Ok(None);
            }
            last_query_name.clear();
            last_query_name.extend_from_slice(qname);
            have_last_query_name = true;
        }
        writer.write(&record).map_err(|error| error.to_string())?;
    }

    drop(writer);
    if sort_order == SortOrder::QueryName {
        if Path::new(output).exists() {
            fs::remove_file(output).map_err(|error| error.to_string())?;
        }
        fs::rename(&stream_output, output).map_err(|error| error.to_string())?;
    }
    Ok(Some(()))
}

fn temp_revertsam_output_path(output: &str) -> String {
    format!("{output}.tmp.{}.revertsam", process::id())
}

fn run_setnmmdanduqtags(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("SetNmMdAndUqTags", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_setnmmdanduqtags_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "SetNmMdAndUqTags")?;
    let output = required_scalar_for(&args, "OUTPUT", "SetNmMdAndUqTags")?;
    let reference_fasta = required_scalar_for(&args, "REFERENCE_SEQUENCE", "SetNmMdAndUqTags")?;
    let output_format = output_format_for(&output, "SetNmMdAndUqTags")?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let set_only_uq = optional_bool(&args, "SET_ONLY_UQ")?.unwrap_or(false);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);

    let reference = reference_sequences_by_name(&reference_fasta)?;
    let mut reader = open_bam_reader_with_reference(&input, Some(reference_fasta.as_str()))
        .map_err(|error| error.to_string())?;
    if header_sort_order(reader.header()).as_deref() != Some("coordinate") {
        return Err("unsupported SetNmMdAndUqTags input must be coordinate sorted".to_string());
    }
    let header = bam::Header::from_template(reader.header());
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let references_by_tid = target_names
        .iter()
        .map(|name| reference.get(name).map(Vec::as_slice))
        .collect::<Vec<_>>();
    let mut writer = bam_writer_for_path_with_reference(
        &output,
        &header,
        output_format,
        Some(reference_fasta.as_str()),
        compression_level,
    )?;

    for record in reader.records() {
        let mut record = record.map_err(|error| error.to_string())?;
        set_nm_md_uq_tags(&mut record, &references_by_tid, set_only_uq)?;
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    drop(writer);

    write_requested_sidecars(
        &output,
        create_md5_file,
        create_index && has_extension(&output, "bam"),
    )
}

fn run_validatesamfile(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("ValidateSamFile", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_validatesamfile_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "ValidateSamFile")?;
    let output = optional_scalar(&args, "OUTPUT")?;
    let skip_mate_validation = optional_bool(&args, "SKIP_MATE_VALIDATION")?.unwrap_or(false);
    let ignored = validate_sam_ignored_summary_keys(&args)?;
    let mode = validate_sam_mode(&args)?;
    let max_output = optional_u32(&args, "MAX_OUTPUT")?;
    let reference = picard_reference(&args)?;
    if let Some(reference_path) = reference.as_deref() {
        fs::metadata(reference_path).map_err(|_| {
            format!("ValidateSamFile reference sequence {reference_path} does not exist")
        })?;
    }

    let reference_by_name = match reference.as_deref() {
        Some(reference_path) => Some(reference_sequences_by_name(reference_path)?),
        None => None,
    };

    let mut report = if has_sam_extension(&input) && mode == ValidateSamMode::Summary {
        validate_sam_summary_sam_text(&input, skip_mate_validation)?
    } else {
        let mut reader = open_bam_reader_with_reference(&input, reference.as_deref())
            .map_err(|error| error.to_string())?;
        validate_sam_summary(
            &mut reader,
            skip_mate_validation,
            reference_by_name.as_ref(),
        )?
    };
    for key in ignored {
        report.counts.remove(&key);
        report.details.retain(|detail| detail.key != key);
    }
    match mode {
        ValidateSamMode::Summary => write_validate_sam_summary(output.as_deref(), &report.counts)?,
        ValidateSamMode::Verbose => {
            write_validate_sam_verbose(output.as_deref(), &report.details, max_output)?
        }
    }

    if report.counts.is_empty() {
        Ok(())
    } else {
        Err("ValidateSamFile found validation issues".to_string())
    }
}

fn run_viewsam(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("ViewSam", args).map_err(|error| error.to_string())?;
    reject_unsupported_viewsam_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "ViewSam")?;
    let output = optional_scalar(&args, "OUTPUT")?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let header_only = optional_bool(&args, "HEADER_ONLY")?.unwrap_or(false);
    let records_only = optional_bool(&args, "RECORDS_ONLY")?.unwrap_or(false);
    let alignment_status =
        optional_scalar(&args, "ALIGNMENT_STATUS")?.unwrap_or_else(|| "All".to_string());
    let pf_status = optional_scalar(&args, "PF_STATUS")?.unwrap_or_else(|| "All".to_string());

    let reference = picard_reference(&args)?;
    let mut reader = open_bam_reader_with_reference(&input, reference.as_deref())
        .map_err(|error| error.to_string())?;
    let header = bam::Header::from_template(reader.header());
    let interval_filter = viewsam_interval_filter(args.get("INTERVAL_LIST"), reader.header())?;
    if header_only {
        let header_text = String::from_utf8_lossy(reader.header().as_bytes());
        match output {
            Some(output) => {
                fs::write(output, header_text.as_bytes()).map_err(|error| error.to_string())?
            }
            None => std::io::stdout()
                .write_all(header_text.as_bytes())
                .map_err(|error| error.to_string())?,
        }
        return Ok(());
    }
    if records_only {
        return run_viewsam_records_only(
            reader,
            &header,
            output.as_deref(),
            compression_level,
            &alignment_status,
            &pf_status,
            interval_filter.as_ref(),
        );
    }
    match output {
        Some(output) => {
            let format = output_format_for(&output, "ViewSam")?;
            let mut writer = bam_writer_for_path_with_reference(
                &output,
                &header,
                format,
                reference.as_deref(),
                compression_level,
            )?;
            for record in reader.records() {
                let record = record.map_err(|error| error.to_string())?;
                if viewsam_record_matches(
                    &record,
                    &alignment_status,
                    &pf_status,
                    interval_filter.as_ref(),
                )? {
                    writer.write(&record).map_err(|error| error.to_string())?;
                }
            }
        }
        None => {
            let mut writer = bam::Writer::from_stdout(&header, bam::Format::Sam)
                .map_err(|error| error.to_string())?;
            for record in reader.records() {
                let record = record.map_err(|error| error.to_string())?;
                if viewsam_record_matches(
                    &record,
                    &alignment_status,
                    &pf_status,
                    interval_filter.as_ref(),
                )? {
                    writer.write(&record).map_err(|error| error.to_string())?;
                }
            }
        }
    }
    Ok(())
}

fn run_viewsam_records_only(
    mut reader: bam::Reader,
    header: &bam::Header,
    output: Option<&str>,
    compression_level: Option<u32>,
    alignment_status: &str,
    pf_status: &str,
    interval_filter: Option<&BTreeMap<i32, Vec<(u64, u64)>>>,
) -> Result<(), String> {
    if let Some(output) = output {
        if !has_sam_extension(output) {
            return Err("unsupported ViewSam RECORDS_ONLY=true with non-SAM OUTPUT".to_string());
        }
    }

    let temp_path = env::temp_dir().join(format!(
        "turbo-picard-viewsam-records-only-{}-{}.sam",
        process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_nanos()
    ));
    let temp_path_text = temp_path.to_string_lossy().to_string();
    {
        let mut writer =
            bam_writer_for_path(&temp_path_text, header, bam::Format::Sam, compression_level)?;
        for record in reader.records() {
            let record = record.map_err(|error| error.to_string())?;
            if viewsam_record_matches(&record, alignment_status, pf_status, interval_filter)? {
                writer.write(&record).map_err(|error| error.to_string())?;
            }
        }
    }

    let result = write_sam_records_without_header(&temp_path, output);
    let _ = fs::remove_file(&temp_path);
    result
}

fn write_sam_records_without_header(path: &Path, output: Option<&str>) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    match output {
        Some(output) => {
            let file = fs::File::create(output).map_err(|error| error.to_string())?;
            let mut writer = BufWriter::with_capacity(1024 * 1024, file);
            copy_sam_records_without_header(&mut reader, &mut writer)?;
            writer.flush().map_err(|error| error.to_string())
        }
        None => {
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            copy_sam_records_without_header(&mut reader, &mut handle)
        }
    }
}

fn copy_sam_records_without_header<R: BufRead, W: Write>(
    reader: &mut R,
    writer: &mut W,
) -> Result<(), String> {
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@") {
            continue;
        }
        writer.write_all(&line).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn run_replacesamheader(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("ReplaceSamHeader", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_replacesamheader_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "ReplaceSamHeader")?;
    let output = required_scalar_for(&args, "OUTPUT", "ReplaceSamHeader")?;
    let header_input = required_scalar_for(&args, "HEADER", "ReplaceSamHeader")?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let reference = picard_reference(&args)?;

    let header_reader = open_bam_reader_with_reference(&header_input, reference.as_deref())
        .map_err(|error| error.to_string())?;
    let header = bam::Header::from_template(header_reader.header());
    let replacement_sort_order = header_sort_order(header_reader.header());
    drop(header_reader);

    let mut reader = open_bam_reader_with_reference(&input, reference.as_deref())
        .map_err(|error| error.to_string())?;
    let input_sort_order = header_sort_order(reader.header());
    if input_sort_order != replacement_sort_order {
        return Err(format!(
            "ReplaceSamHeader sort orders of INPUT ({}) and HEADER ({}) do not agree",
            input_sort_order.unwrap_or_else(|| "unknown".to_string()),
            replacement_sort_order.unwrap_or_else(|| "unknown".to_string())
        ));
    }
    let format = output_format_for(&output, "ReplaceSamHeader")?;
    let mut writer = bam_writer_for_path_with_reference(
        &output,
        &header,
        format,
        reference.as_deref(),
        compression_level,
    )?;
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    drop(writer);
    write_requested_sidecars(&output, create_md5_file, false)
}

fn run_updatevcfsequencedictionary(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("UpdateVcfSequenceDictionary", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_updatevcfsequencedictionary_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "UpdateVcfSequenceDictionary")?;
    let output = required_scalar_for(&args, "OUTPUT", "UpdateVcfSequenceDictionary")?;
    let dictionary_path =
        required_scalar_for(&args, "SEQUENCE_DICTIONARY", "UpdateVcfSequenceDictionary")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);

    let dictionary_text = fs::read_to_string(dictionary_path).map_err(|error| error.to_string())?;
    let contig_lines = vcf_contig_lines_from_dictionary(&dictionary_text)?;
    let input_text = read_text_or_gzip(&input)?;
    let output_text = replace_vcf_contig_header(&input_text, &contig_lines)?;
    write_text_or_gzip(&output, &output_text)?;
    if create_index && has_extension(&output, "vcf") {
        write_vcf_idx_sidecar(&output, &output_text)?;
    }
    Ok(())
}

fn run_liftovervcf(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("LiftoverVcf", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_liftovervcf_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "LiftoverVcf")?;
    let output = required_scalar_for(&args, "OUTPUT", "LiftoverVcf")?;
    let chain = required_scalar_for(&args, "CHAIN", "LiftoverVcf")?;
    let reject = required_scalar_for(&args, "REJECT", "LiftoverVcf")?;
    let reference = required_scalar_for(&args, "REFERENCE_SEQUENCE", "LiftoverVcf")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let tmp_dir = optional_scalar(&args, "TMP_DIR")?
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let max_records_in_ram = optional_u32(&args, "MAX_RECORDS_IN_RAM")?
        .map(|value| value as usize)
        .unwrap_or(500_000);

    let mappings = read_simple_chain_mappings(&chain)?;
    let document = read_vcf_document(&input)?;
    let reference_sequences = reference_sequences_by_name(&reference)?;
    let dictionary_text = fs::read_to_string(reference_dictionary_path(&reference)?)
        .map_err(|error| error.to_string())?;
    let contig_lines = vcf_contig_lines_from_dictionary(&dictionary_text)?;
    let contig_order = dictionary_contig_order(&dictionary_text);
    let reference_line = format!("##reference=file:{}", reference);

    let mut lifted = Vec::new();
    let mut rejected = Vec::new();
    for record in document.records.iter().cloned() {
        match liftover_vcf_record(record, &mappings, &reference_sequences)? {
            LiftoverRecordResult::Lifted(record) => lifted.push(record),
            LiftoverRecordResult::Rejected(record) => rejected.push(record),
        }
    }

    for record in &lifted {
        if !contig_order.contains_key(&record.contig) {
            return Err(format!(
                "VCF contig {} is not present in sequence dictionary",
                record.contig
            ));
        }
    }
    let lifted = sort_vcf_records_with_external_sort(
        lifted,
        &contig_order,
        tmp_dir,
        max_records_in_ram,
        "turbo-picard-liftovervcf",
        "LiftoverVcf",
    )?;

    let output_text = liftover_output_vcf_text(&document, &contig_lines, &reference_line, &lifted);
    let reject_text = liftover_reject_vcf_text(&document, &contig_lines, &rejected);
    write_text_or_gzip(&output, &output_text)?;
    if create_index && has_extension(&output, "vcf") {
        write_vcf_idx_sidecar(&output, &output_text)?;
    }
    write_text_or_gzip(&reject, &reject_text)
}

fn run_gathervcfs(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("GatherVcfs", args).map_err(|error| error.to_string())?;
    reject_unsupported_gathervcfs_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "GatherVcfs")?;
    let output = required_scalar_for(&args, "OUTPUT", "GatherVcfs")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);

    let mut documents = Vec::with_capacity(inputs.len());
    for input in &inputs {
        documents.push(read_vcf_document(input)?);
    }
    let first = documents
        .first()
        .ok_or_else(|| "missing required GatherVcfs argument: INPUT".to_string())?;
    for document in documents.iter().skip(1) {
        if document.column_header != first.column_header {
            return Err("unsupported GatherVcfs inputs with different sample columns".to_string());
        }
        if document.contig_ids() != first.contig_ids() {
            return Err(
                "unsupported GatherVcfs inputs with different sequence dictionaries".to_string(),
            );
        }
    }

    let mut text = first.header_text();
    for document in documents {
        for record in document.records {
            text.push_str(&record.line);
            text.push('\n');
        }
    }
    write_text_or_gzip(&output, &text)?;
    if create_index && has_extension(&output, "vcf") {
        write_vcf_idx_sidecar(&output, &text)?;
    }
    Ok(())
}

fn run_sortvcf(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("SortVcf", args).map_err(|error| error.to_string())?;
    reject_unsupported_sortvcf_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "SortVcf")?;
    let output = required_scalar_for(&args, "OUTPUT", "SortVcf")?;
    let dictionary_path = optional_scalar(&args, "SEQUENCE_DICTIONARY")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let tmp_dir = optional_scalar(&args, "TMP_DIR")?
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let max_records_in_ram = optional_u32(&args, "MAX_RECORDS_IN_RAM")?
        .map(|value| value as usize)
        .unwrap_or(500_000);

    let mut documents = Vec::with_capacity(inputs.len());
    for input in &inputs {
        documents.push(read_vcf_document(input)?);
    }
    let first = documents
        .first()
        .ok_or_else(|| "missing required SortVcf argument: INPUT".to_string())?;
    for document in documents.iter().skip(1) {
        if document.column_header != first.column_header {
            return Err("unsupported SortVcf inputs with different sample columns".to_string());
        }
    }

    let (contig_order, contig_lines) = if let Some(dictionary_path) = dictionary_path {
        let dictionary_text =
            fs::read_to_string(dictionary_path).map_err(|error| error.to_string())?;
        let contig_lines = vcf_contig_lines_from_dictionary(&dictionary_text)?;
        validate_vcf_sequence_dictionaries(&documents, &contig_lines, "SortVcf")?;
        (
            dictionary_contig_order(&dictionary_text),
            Some(contig_lines),
        )
    } else {
        let contig_lines = first.contig_lines();
        validate_vcf_sequence_dictionaries(&documents, &contig_lines, "SortVcf")?;
        (first.contig_order(), None)
    };
    if contig_order.is_empty() {
        return Err("unsupported SortVcf input without sequence dictionary".to_string());
    }

    let mut text = vcf_header_text_with_contigs(first, contig_lines.as_deref())?;
    let mut sort_config = ExternalSortConfig::new(tmp_dir);
    sort_config.max_records_in_ram = max_records_in_ram.max(1);
    sort_config.prefix = "turbo-picard-sortvcf".to_string();
    let mut sorter = ExternalSorter::new(sort_config)?;
    for document in documents {
        for record in document.records {
            let Some(contig_rank) = contig_order.get(&record.contig).copied() else {
                return Err(format!(
                    "VCF contig {} is not present in sequence dictionary",
                    record.contig
                ));
            };
            sorter.push(
                vcf_sort_key(contig_rank, record.position),
                record.line.into_bytes(),
            )?;
        }
    }
    let (records, _metrics) = sorter.finish()?;
    for record in records {
        let line = String::from_utf8(record.payload)
            .map_err(|_| "SortVcf record payload is not UTF-8".to_string())?;
        if line.is_empty() {
            return Err(format!(
                "malformed SortVcf sorted record from ordinal {}",
                record.ordinal
            ));
        }
        text.push_str(&line);
        text.push('\n');
    }
    write_text_or_gzip(&output, &text)?;
    if create_index && has_extension(&output, "vcf") {
        write_vcf_idx_sidecar(&output, &text)?;
    }
    Ok(())
}

fn run_mergevcfs(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("MergeVcfs", args).map_err(|error| error.to_string())?;
    reject_unsupported_mergevcfs_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "MergeVcfs")?;
    let output = required_scalar_for(&args, "OUTPUT", "MergeVcfs")?;
    let dictionary_path = optional_scalar(&args, "SEQUENCE_DICTIONARY")?;
    let create_index = optional_bool(&args, "CREATE_INDEX")?.unwrap_or(false);
    let tmp_dir = optional_scalar(&args, "TMP_DIR")?
        .map(PathBuf::from)
        .unwrap_or_else(env::temp_dir);
    let max_records_in_ram = optional_u32(&args, "MAX_RECORDS_IN_RAM")?
        .map(|value| value as usize)
        .unwrap_or(500_000);

    let mut documents = Vec::with_capacity(inputs.len());
    for input in &inputs {
        documents.push(read_vcf_document(input)?);
    }
    let first = documents
        .first()
        .ok_or_else(|| "missing required MergeVcfs argument: INPUT".to_string())?;
    for document in documents.iter().skip(1) {
        if document.column_header != first.column_header {
            return Err("unsupported MergeVcfs inputs with different sample columns".to_string());
        }
    }

    let (contig_order, contig_lines) = if let Some(dictionary_path) = dictionary_path {
        let dictionary_text =
            fs::read_to_string(dictionary_path).map_err(|error| error.to_string())?;
        let contig_lines = vcf_contig_lines_from_dictionary(&dictionary_text)?;
        validate_vcf_sequence_dictionaries(&documents, &contig_lines, "MergeVcfs")?;
        (
            dictionary_contig_order(&dictionary_text),
            Some(contig_lines),
        )
    } else {
        let contig_lines = first.contig_lines();
        validate_vcf_sequence_dictionaries(&documents, &contig_lines, "MergeVcfs")?;
        (first.contig_order(), None)
    };
    if contig_order.is_empty() {
        return Err("unsupported MergeVcfs input without sequence dictionary".to_string());
    }

    let mut text = vcf_header_text_with_contigs(first, contig_lines.as_deref())?;
    let mut sort_config = ExternalSortConfig::new(tmp_dir);
    sort_config.max_records_in_ram = max_records_in_ram.max(1);
    sort_config.prefix = "turbo-picard-mergevcfs".to_string();
    let mut sorter = ExternalSorter::new(sort_config)?;
    for document in documents {
        for record in document.records {
            let Some(contig_rank) = contig_order.get(&record.contig).copied() else {
                return Err(format!(
                    "VCF contig {} is not present in sequence dictionary",
                    record.contig
                ));
            };
            sorter.push(
                vcf_sort_key(contig_rank, record.position),
                record.line.into_bytes(),
            )?;
        }
    }
    let (records, _metrics) = sorter.finish()?;
    for record in records {
        let line = String::from_utf8(record.payload)
            .map_err(|_| "MergeVcfs record payload is not UTF-8".to_string())?;
        if line.is_empty() {
            return Err(format!(
                "malformed MergeVcfs sorted record from ordinal {}",
                record.ordinal
            ));
        }
        text.push_str(&line);
        text.push('\n');
    }
    write_text_or_gzip(&output, &text)?;
    if create_index && has_extension(&output, "vcf") {
        write_vcf_idx_sidecar(&output, &text)?;
    }
    Ok(())
}

fn reject_unsupported_viewsam_args(args: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "INTERVAL_LIST",
        "ALIGNMENT_STATUS",
        "PF_STATUS",
        "HEADER_ONLY",
        "RECORDS_ONLY",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "REFERENCE_SEQUENCE",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported ViewSam argument: {key}"));
        }
    }
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_scalar(args, "ALIGNMENT_STATUS")?;
    optional_scalar(args, "PF_STATUS")?;
    let _ = args.get("INTERVAL_LIST");
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    let header_only = optional_bool(args, "HEADER_ONLY")?.unwrap_or(false);
    let records_only = optional_bool(args, "RECORDS_ONLY")?.unwrap_or(false);
    if header_only && records_only {
        return Err("unsupported ViewSam HEADER_ONLY=true with RECORDS_ONLY=true".to_string());
    }
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported ViewSam COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn viewsam_record_matches(
    record: &bam::Record,
    alignment_status: &str,
    pf_status: &str,
    interval_filter: Option<&BTreeMap<i32, Vec<(u64, u64)>>>,
) -> Result<bool, String> {
    let alignment_matches = match alignment_status {
        "All" => true,
        "Aligned" => !record.is_unmapped(),
        "Unaligned" => record.is_unmapped(),
        value => return Err(format!("unsupported ViewSam ALIGNMENT_STATUS={value}")),
    };
    let pf_matches = match pf_status {
        "All" => true,
        "PF" => !record.is_quality_check_failed(),
        "NonPF" => record.is_quality_check_failed(),
        value => return Err(format!("unsupported ViewSam PF_STATUS={value}")),
    };
    Ok(alignment_matches && pf_matches && record_overlaps_intervals(record, interval_filter))
}

fn viewsam_interval_filter(
    interval_paths: Option<&Vec<String>>,
    header: &bam::HeaderView,
) -> Result<Option<BTreeMap<i32, Vec<(u64, u64)>>>, String> {
    let Some(interval_paths) = interval_paths else {
        return Ok(None);
    };
    let contig_order = header
        .target_names()
        .iter()
        .enumerate()
        .map(|(index, name)| (String::from_utf8_lossy(name).to_string(), index))
        .collect::<BTreeMap<_, _>>();
    let mut intervals_by_tid = BTreeMap::<i32, Vec<(u64, u64)>>::new();
    for interval_path in interval_paths {
        let text = read_text_or_gzip(interval_path)?;
        for interval in read_interval_list_intervals(&text, &contig_order)? {
            intervals_by_tid
                .entry(interval.contig_index as i32)
                .or_default()
                .push((interval.start, interval.end));
        }
    }
    Ok(Some(intervals_by_tid))
}

fn merge_interval_filter(
    interval_paths: Option<&Vec<String>>,
    target_names: &[String],
) -> Result<Option<BTreeMap<i32, Vec<(u64, u64)>>>, String> {
    let Some(interval_paths) = interval_paths else {
        return Ok(None);
    };
    let contig_order = target_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut intervals_by_tid = BTreeMap::<i32, Vec<(u64, u64)>>::new();
    for interval_path in interval_paths {
        let text = read_text_or_gzip(interval_path)?;
        for interval in read_interval_list_intervals(&text, &contig_order)? {
            intervals_by_tid
                .entry(interval.contig_index as i32)
                .or_default()
                .push((interval.start, interval.end));
        }
    }
    Ok(Some(intervals_by_tid))
}

fn record_overlaps_intervals(
    record: &bam::Record,
    interval_filter: Option<&BTreeMap<i32, Vec<(u64, u64)>>>,
) -> bool {
    let Some(interval_filter) = interval_filter else {
        return true;
    };
    if record.is_unmapped() || record.tid() < 0 || record.pos() < 0 {
        return false;
    }
    let Some(intervals) = interval_filter.get(&record.tid()) else {
        return false;
    };
    let record_start = record.pos() as u64 + 1;
    let record_end = record.cigar().end_pos().max(record.pos() + 1) as u64;
    intervals.iter().any(|(interval_start, interval_end)| {
        record_start <= *interval_end && record_end >= *interval_start
    })
}

fn reject_unsupported_updatevcfsequencedictionary_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SEQUENCE_DICTIONARY",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "CREATE_MD5_FILE",
        "CREATE_INDEX",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "COMPRESSION_LEVEL",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported UpdateVcfSequenceDictionary argument: {key}"
            ));
        }
    }
    optional_scalar(args, "SEQUENCE_DICTIONARY")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported UpdateVcfSequenceDictionary COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_liftovervcf_args(args: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "CHAIN",
        "REJECT",
        "REFERENCE_SEQUENCE",
        "WARN_ON_MISSING_CONTIG",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported LiftoverVcf argument: {key}"));
        }
    }
    optional_bool(args, "WARN_ON_MISSING_CONTIG")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported LiftoverVcf COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_gathervcfs_args(args: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported GatherVcfs argument: {key}"));
        }
    }
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported GatherVcfs COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn reject_unsupported_sortvcf_args(args: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SEQUENCE_DICTIONARY",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "COMPRESSION_LEVEL",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported SortVcf argument: {key}"));
        }
    }
    optional_scalar(args, "SEQUENCE_DICTIONARY")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported SortVcf COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn reject_unsupported_mergevcfs_args(args: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SEQUENCE_DICTIONARY",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported MergeVcfs argument: {key}"));
        }
    }
    optional_scalar(args, "SEQUENCE_DICTIONARY")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported MergeVcfs COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct VcfDocument {
    meta_lines: Vec<String>,
    column_header: String,
    records: Vec<VcfRecord>,
}

impl VcfDocument {
    fn header_text(&self) -> String {
        let mut text = String::new();
        for line in &self.meta_lines {
            text.push_str(line);
            text.push('\n');
        }
        text.push_str(&self.column_header);
        text.push('\n');
        text
    }

    fn contig_ids(&self) -> Vec<String> {
        self.contig_lines()
            .iter()
            .filter_map(|line| parse_vcf_contig_id(line.as_str()))
            .collect()
    }

    fn contig_lines(&self) -> Vec<String> {
        self.meta_lines
            .iter()
            .filter(|line| line.starts_with("##contig=<"))
            .cloned()
            .collect()
    }

    fn contig_order(&self) -> BTreeMap<String, usize> {
        self.contig_ids()
            .into_iter()
            .enumerate()
            .map(|(index, contig)| (contig, index))
            .collect()
    }
}

fn validate_vcf_sequence_dictionaries(
    documents: &[VcfDocument],
    expected_contig_lines: &[String],
    command: &str,
) -> Result<(), String> {
    for document in documents {
        if document.contig_lines() != expected_contig_lines {
            return Err(format!(
                "unsupported {command} input sequence dictionary differs from expected dictionary"
            ));
        }
    }
    Ok(())
}

fn vcf_header_text_with_contigs(
    document: &VcfDocument,
    contig_lines: Option<&[String]>,
) -> Result<String, String> {
    let header_text = document.header_text();
    if let Some(contig_lines) = contig_lines {
        replace_vcf_contig_header(&header_text, contig_lines)
    } else {
        Ok(header_text)
    }
}

#[derive(Debug, Clone)]
struct VcfRecord {
    line: String,
    contig: String,
    position: u64,
    serial: usize,
}

fn read_vcf_document(path: &str) -> Result<VcfDocument, String> {
    let text = read_text_or_gzip(path)?;
    parse_vcf_document(&text, path)
}

fn parse_vcf_document(text: &str, source: &str) -> Result<VcfDocument, String> {
    let mut meta_lines = Vec::new();
    let mut column_header = None;
    let mut records = Vec::new();

    for (line_index, line) in text.lines().enumerate() {
        if line.starts_with("##") {
            if column_header.is_some() {
                return Err(format!("malformed VCF header in {source}"));
            }
            meta_lines.push(line.to_string());
        } else if line.starts_with("#CHROM") {
            column_header = Some(line.to_string());
        } else if line.starts_with('#') {
            return Err(format!(
                "unsupported VCF header line {} in {source}",
                line_index + 1
            ));
        } else if !line.trim().is_empty() {
            if column_header.is_none() {
                return Err(format!("VCF input {source} is missing #CHROM header"));
            }
            records.push(parse_vcf_record(
                line,
                records.len(),
                source,
                line_index + 1,
            )?);
        }
    }

    let column_header =
        column_header.ok_or_else(|| format!("VCF input {source} is missing #CHROM header"))?;
    Ok(VcfDocument {
        meta_lines,
        column_header,
        records,
    })
}

fn parse_vcf_record(
    line: &str,
    serial: usize,
    source: &str,
    line_number: usize,
) -> Result<VcfRecord, String> {
    let mut fields = line.split('\t');
    let contig = fields
        .next()
        .ok_or_else(|| format!("malformed VCF record on line {line_number} in {source}"))?;
    let position = fields
        .next()
        .ok_or_else(|| format!("malformed VCF record on line {line_number} in {source}"))?
        .parse::<u64>()
        .map_err(|_| format!("malformed VCF POS on line {line_number} in {source}"))?;
    Ok(VcfRecord {
        line: line.to_string(),
        contig: contig.to_string(),
        position,
        serial,
    })
}

fn vcf_sort_key(contig_rank: usize, position: u64) -> Vec<u8> {
    let mut key = Vec::with_capacity(16);
    key.extend_from_slice(&(contig_rank as u64).to_be_bytes());
    key.extend_from_slice(&position.to_be_bytes());
    key
}

fn sort_vcf_records_with_external_sort(
    records: Vec<VcfRecord>,
    contig_order: &BTreeMap<String, usize>,
    tmp_dir: PathBuf,
    max_records_in_ram: usize,
    prefix: &str,
    command: &str,
) -> Result<Vec<VcfRecord>, String> {
    let mut sort_config = ExternalSortConfig::new(tmp_dir);
    sort_config.max_records_in_ram = max_records_in_ram.max(1);
    sort_config.prefix = prefix.to_string();
    let mut sorter = ExternalSorter::new(sort_config)?;
    for record in records {
        let Some(contig_rank) = contig_order.get(&record.contig).copied() else {
            return Err(format!(
                "VCF contig {} is not present in sequence dictionary",
                record.contig
            ));
        };
        sorter.push(
            vcf_sort_key(contig_rank, record.position),
            record.line.into_bytes(),
        )?;
    }
    let (items, _metrics) = sorter.finish()?;
    items
        .into_iter()
        .enumerate()
        .map(|(serial, item)| {
            let line = String::from_utf8(item.payload)
                .map_err(|_| format!("{command} record payload is not UTF-8"))?;
            parse_vcf_record(&line, serial, command, serial + 1)
        })
        .collect()
}

#[derive(Debug, Clone)]
struct ChainMapping {
    source_contig: String,
    source_start: u64,
    source_end: u64,
    target_contig: String,
    target_start: u64,
    target_end: u64,
}

enum LiftoverRecordResult {
    Lifted(VcfRecord),
    Rejected(VcfRecord),
}

fn read_simple_chain_mappings(path: &str) -> Result<Vec<ChainMapping>, String> {
    let text = read_text_or_gzip(path)?;
    let mut mappings = Vec::new();
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim().is_empty() {
            continue;
        }
        let Some(header) = line.strip_prefix("chain ") else {
            return Err("unsupported LiftoverVcf chain file without chain header".to_string());
        };
        let fields = header.split_whitespace().collect::<Vec<_>>();
        if fields.len() != 12 {
            return Err("unsupported LiftoverVcf malformed chain header".to_string());
        }
        let target_contig = fields[1].to_string();
        let target_size = fields[2]
            .parse::<u64>()
            .map_err(|_| "unsupported LiftoverVcf malformed target size".to_string())?;
        let target_strand = fields[3];
        let target_start = fields[4]
            .parse::<u64>()
            .map_err(|_| "unsupported LiftoverVcf malformed target start".to_string())?;
        let target_end = fields[5]
            .parse::<u64>()
            .map_err(|_| "unsupported LiftoverVcf malformed target end".to_string())?;
        let source_contig = fields[6].to_string();
        let source_size = fields[7]
            .parse::<u64>()
            .map_err(|_| "unsupported LiftoverVcf malformed source size".to_string())?;
        let source_strand = fields[8];
        let source_start = fields[9]
            .parse::<u64>()
            .map_err(|_| "unsupported LiftoverVcf malformed source start".to_string())?;
        let source_end = fields[10]
            .parse::<u64>()
            .map_err(|_| "unsupported LiftoverVcf malformed source end".to_string())?;
        if target_strand != "+" || source_strand != "+" {
            return Err("unsupported LiftoverVcf reverse-strand chain".to_string());
        }
        if target_end > target_size || source_end > source_size || target_start >= target_end {
            return Err("unsupported LiftoverVcf malformed chain bounds".to_string());
        }
        let block = lines
            .next()
            .ok_or_else(|| "unsupported LiftoverVcf chain without block".to_string())?;
        let block_fields = block.split_whitespace().collect::<Vec<_>>();
        if block_fields.len() != 1 {
            return Err("unsupported LiftoverVcf gapped or multi-block chain".to_string());
        }
        let block_size = block_fields[0]
            .parse::<u64>()
            .map_err(|_| "unsupported LiftoverVcf malformed chain block".to_string())?;
        if block_size != source_end - source_start || block_size != target_end - target_start {
            return Err("unsupported LiftoverVcf chain block does not span interval".to_string());
        }
        mappings.push(ChainMapping {
            source_contig,
            source_start,
            source_end,
            target_contig,
            target_start,
            target_end,
        });
    }
    if mappings.is_empty() {
        return Err("unsupported LiftoverVcf empty chain".to_string());
    }
    Ok(mappings)
}

fn liftover_vcf_record(
    record: VcfRecord,
    mappings: &[ChainMapping],
    reference: &BTreeMap<String, Vec<u8>>,
) -> Result<LiftoverRecordResult, String> {
    let fields = record.line.split('\t').collect::<Vec<_>>();
    if fields.len() < 8 {
        return Err("malformed VCF record for LiftoverVcf".to_string());
    }
    let ref_allele = fields[3];
    if ref_allele == "."
        || ref_allele
            .chars()
            .any(|base| matches!(base, '<' | '>' | '[' | ']'))
    {
        return Err("unsupported LiftoverVcf symbolic or missing REF allele".to_string());
    }
    let source_start = record.position - 1;
    let source_end = source_start + ref_allele.len() as u64;
    let Some(mapping) = mappings.iter().find(|mapping| {
        mapping.source_contig == record.contig
            && source_start >= mapping.source_start
            && source_end <= mapping.source_end
    }) else {
        return Ok(LiftoverRecordResult::Rejected(reject_liftover_record(
            &record, "NoTarget", ".",
        )));
    };
    let target_start = mapping.target_start + (source_start - mapping.source_start);
    let target_end = target_start + ref_allele.len() as u64;
    if target_end > mapping.target_end {
        return Ok(LiftoverRecordResult::Rejected(reject_liftover_record(
            &record, "NoTarget", ".",
        )));
    }
    let Some(target_reference) = reference.get(&mapping.target_contig) else {
        return Err(format!(
            "LiftoverVcf reference missing contig {}",
            mapping.target_contig
        ));
    };
    let reference_slice = target_reference
        .get(target_start as usize..target_end as usize)
        .ok_or_else(|| "LiftoverVcf target interval extends beyond reference".to_string())?;
    if !reference_slice.eq_ignore_ascii_case(ref_allele.as_bytes()) {
        let info = format!(
            "AttemptedAlleles={}*->{};AttemptedLocus={}:{}-{}",
            fields[3],
            fields[4],
            mapping.target_contig,
            target_start + 1,
            target_end
        );
        return Ok(LiftoverRecordResult::Rejected(reject_liftover_record(
            &record,
            "MismatchedRefAllele",
            &info,
        )));
    }

    let mut lifted_fields = fields
        .iter()
        .map(|field| field.to_string())
        .collect::<Vec<_>>();
    lifted_fields[0] = mapping.target_contig.clone();
    lifted_fields[1] = (target_start + 1).to_string();
    Ok(LiftoverRecordResult::Lifted(VcfRecord {
        line: lifted_fields.join("\t"),
        contig: mapping.target_contig.clone(),
        position: target_start + 1,
        serial: record.serial,
    }))
}

fn reject_liftover_record(record: &VcfRecord, filter: &str, info: &str) -> VcfRecord {
    let mut fields = record
        .line
        .split('\t')
        .map(|field| field.to_string())
        .collect::<Vec<_>>();
    if fields.len() >= 8 {
        fields[6] = filter.to_string();
        fields[7] = info.to_string();
    }
    VcfRecord {
        line: fields.join("\t"),
        contig: record.contig.clone(),
        position: record.position,
        serial: record.serial,
    }
}

fn liftover_output_vcf_text(
    document: &VcfDocument,
    contig_lines: &[String],
    reference_line: &str,
    records: &[VcfRecord],
) -> String {
    let mut text = String::new();
    for line in &document.meta_lines {
        if line.starts_with("##contig=<")
            || line.starts_with("##reference=")
            || line.starts_with("##INFO=<ID=ReverseComplementedAlleles,")
            || line.starts_with("##INFO=<ID=SwappedAlleles,")
        {
            continue;
        }
        text.push_str(line);
        text.push('\n');
        if line.starts_with("##fileformat=") {
            text.push_str("##INFO=<ID=ReverseComplementedAlleles,Number=0,Type=Flag,Description=\"The REF and the ALT alleles have been reverse complemented in liftover since the mapping from the previous reference to the current one was on the negative strand.\">\n");
            text.push_str("##INFO=<ID=SwappedAlleles,Number=0,Type=Flag,Description=\"The REF and the ALT alleles have been swapped in liftover due to changes in the reference. It is possible that not all INFO annotations reflect this swap, and in the genotypes, only the GT, PL, and AD fields have been modified. You should check the TAGS_TO_REVERSE parameter that was used during the LiftOver to be sure.\">\n");
        }
    }
    for line in contig_lines {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(reference_line);
    text.push('\n');
    text.push_str(&document.column_header);
    text.push('\n');
    for record in records {
        text.push_str(&record.line);
        text.push('\n');
    }
    text
}

fn liftover_reject_vcf_text(
    document: &VcfDocument,
    contig_lines: &[String],
    records: &[VcfRecord],
) -> String {
    let mut text = String::new();
    for line in &document.meta_lines {
        if line.starts_with("##contig=<")
            || line.starts_with("##FILTER=<ID=CannotLiftOver,")
            || line.starts_with("##FILTER=<ID=IndelStraddlesMultipleIntervals,")
            || line.starts_with("##FILTER=<ID=MismatchedRefAllele,")
            || line.starts_with("##FILTER=<ID=NoTarget,")
            || line.starts_with("##INFO=<ID=AttemptedAlleles,")
            || line.starts_with("##INFO=<ID=AttemptedLocus,")
        {
            continue;
        }
        text.push_str(line);
        text.push('\n');
        if line.starts_with("##fileformat=") {
            text.push_str("##FILTER=<ID=CannotLiftOver,Description=\"Liftover of a variant that needed reverse-complementing failed for unknown reasons.\">\n");
            text.push_str("##FILTER=<ID=IndelStraddlesMultipleIntervals,Description=\"Reference allele in Indel is straddling multiple intervals in the chain, and so the results are not well defined.\">\n");
            text.push_str("##FILTER=<ID=MismatchedRefAllele,Description=\"Reference allele does not match reference genome sequence after liftover.\">\n");
            text.push_str("##FILTER=<ID=NoTarget,Description=\"Variant could not be lifted between genome builds.\">\n");
            text.push_str("##INFO=<ID=AttemptedAlleles,Number=1,Type=String,Description=\"The alleles of the variant in the TARGET prior to failing due to reference allele mismatching to the target reference.\">\n");
            text.push_str("##INFO=<ID=AttemptedLocus,Number=1,Type=String,Description=\"The locus of the variant in the TARGET prior to failing due to reference allele mismatching to the target reference.\">\n");
        }
    }
    for line in contig_lines {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(&document.column_header);
    text.push('\n');
    for record in records {
        text.push_str(&record.line);
        text.push('\n');
    }
    text
}

fn reference_dictionary_path(reference: &str) -> Result<String, String> {
    let path = Path::new(reference);
    let stem_dict = path.with_extension("dict");
    if stem_dict.exists() {
        return Ok(stem_dict.display().to_string());
    }
    let fa_dict = format!("{reference}.dict");
    if Path::new(&fa_dict).exists() {
        return Ok(fa_dict);
    }
    Err(format!(
        "LiftoverVcf reference {reference} must have an adjacent .dict file"
    ))
}

fn parse_vcf_contig_id(line: &str) -> Option<String> {
    let body = line.strip_prefix("##contig=<")?.strip_suffix('>')?;
    body.split(',').find_map(|field| {
        field
            .strip_prefix("ID=")
            .map(|value| value.trim_matches('"').to_string())
    })
}

fn vcf_contig_lines_from_dictionary(dictionary_text: &str) -> Result<Vec<String>, String> {
    let mut contigs = Vec::new();
    for line in dictionary_text
        .lines()
        .filter(|line| line.starts_with("@SQ\t"))
    {
        let fields = line
            .split('\t')
            .skip(1)
            .filter_map(|field| field.split_once(':'))
            .collect::<BTreeMap<_, _>>();
        let id = fields
            .get("SN")
            .ok_or_else(|| "sequence dictionary @SQ line missing SN".to_string())?;
        let length = fields
            .get("LN")
            .ok_or_else(|| "sequence dictionary @SQ line missing LN".to_string())?;
        let mut attributes = vec![format!("ID={id}"), format!("length={length}")];
        if let Some(md5) = fields.get("M5") {
            attributes.push(format!("md5={md5}"));
        }
        if let Some(assembly) = fields.get("AS") {
            attributes.push(format!("assembly={assembly}"));
        }
        if let Some(species) = fields.get("SP") {
            attributes.push(format!("species={species}"));
        }
        if let Some(uri) = fields.get("UR") {
            attributes.push(format!("URI={uri}"));
        }
        contigs.push(format!("##contig=<{}>", attributes.join(",")));
    }
    if contigs.is_empty() {
        return Err("sequence dictionary contains no @SQ records".to_string());
    }
    Ok(contigs)
}

fn replace_vcf_contig_header(input_text: &str, contig_lines: &[String]) -> Result<String, String> {
    let mut output = String::new();
    let mut inserted_contigs = false;
    let mut saw_column_header = false;

    for line in input_text.lines() {
        if line.starts_with("##contig=<") {
            continue;
        }
        if line.starts_with("#CHROM") && !inserted_contigs {
            for contig in contig_lines {
                output.push_str(contig);
                output.push('\n');
            }
            inserted_contigs = true;
        }
        if line.starts_with("#CHROM") {
            saw_column_header = true;
        }
        output.push_str(line);
        output.push('\n');
    }

    if !saw_column_header {
        return Err("VCF input is missing #CHROM header".to_string());
    }
    Ok(output)
}

fn reject_unsupported_replacesamheader_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "HEADER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "CREATE_MD5_FILE",
        "CREATE_INDEX",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "COMPRESSION_LEVEL",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported ReplaceSamHeader argument: {key}"));
        }
    }
    optional_scalar(args, "HEADER")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported ReplaceSamHeader COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_createsequencedictionary_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "REFERENCE_SEQUENCE",
        "OUTPUT",
        "TRUNCATE_NAMES_AT_WHITESPACE",
        "URI",
        "GENOME_ASSEMBLY",
        "SPECIES",
        "ALT_NAMES",
        "NUM_SEQUENCES",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "COMPRESSION_LEVEL",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported CreateSequenceDictionary argument: {key}"
            ));
        }
    }

    optional_bool(args, "TRUNCATE_NAMES_AT_WHITESPACE")?;
    optional_scalar(args, "URI")?;
    optional_scalar(args, "GENOME_ASSEMBLY")?;
    optional_scalar(args, "SPECIES")?;
    optional_scalar(args, "ALT_NAMES")?;
    optional_u32(args, "NUM_SEQUENCES")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CreateSequenceDictionary COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    Ok(())
}

fn reject_unsupported_normalizefasta_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "LINE_LENGTH",
        "TRUNCATE_SEQUENCE_NAMES_AT_WHITESPACE",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "COMPRESSION_LEVEL",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported NormalizeFasta argument: {key}"));
        }
    }
    optional_u32(args, "LINE_LENGTH")?;
    optional_bool(args, "TRUNCATE_SEQUENCE_NAMES_AT_WHITESPACE")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported NormalizeFasta COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    Ok(())
}

fn reject_unsupported_bedtointervallist_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SEQUENCE_DICTIONARY",
        "SORT",
        "UNIQUE",
        "DROP_MISSING_CONTIGS",
        "KEEP_LENGTH_ZERO_INTERVALS",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "COMPRESSION_LEVEL",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported BedToIntervalList argument: {key}"));
        }
    }
    optional_bool(args, "SORT")?;
    optional_bool(args, "UNIQUE")?;
    optional_bool(args, "DROP_MISSING_CONTIGS")?;
    optional_bool(args, "KEEP_LENGTH_ZERO_INTERVALS")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported BedToIntervalList COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    Ok(())
}

fn reject_unsupported_intervallisttools_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "ACTION",
        "SORT",
        "UNIQUE",
        "PADDING",
        "DONT_MERGE_ABUTTING",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported IntervalListTools argument: {key}"));
        }
    }
    if let Some(action) = optional_scalar(args, "ACTION")? {
        if action != "CONCAT" {
            return Err(format!("unsupported IntervalListTools ACTION={action}"));
        }
    }
    optional_bool(args, "SORT")?;
    optional_bool(args, "UNIQUE")?;
    if optional_i64(args, "PADDING")?.unwrap_or(0) < 0 {
        return Err("Padding values must be >= 0.".to_string());
    }
    optional_bool(args, "DONT_MERGE_ABUTTING")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported IntervalListTools COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_revertsam_args(args: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "REFERENCE_SEQUENCE",
        "REMOVE_ALIGNMENT_INFORMATION",
        "REMOVE_DUPLICATE_INFORMATION",
        "RESTORE_ORIGINAL_QUALITIES",
        "RESTORE_HARDCLIPS",
        "SORT_ORDER",
        "ATTRIBUTE_TO_CLEAR",
        "ATTRIBUTE_TO_REVERSE",
        "ATTRIBUTE_TO_REVERSE_COMPLEMENT",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "CREATE_MD5_FILE",
        "CREATE_INDEX",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported RevertSam argument: {key}"));
        }
    }
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    let remove_alignment_information =
        optional_bool(args, "REMOVE_ALIGNMENT_INFORMATION")?.unwrap_or(true);
    let restore_hardclips = optional_bool(args, "RESTORE_HARDCLIPS")?.unwrap_or(true);
    if !remove_alignment_information && restore_hardclips {
        return Err(
            "Cannot revert sam file when RESTORE_HARDCLIPS is true and REMOVE_ALIGNMENT_INFORMATION is false."
                .to_string(),
        );
    }
    optional_bool(args, "REMOVE_DUPLICATE_INFORMATION")?;
    optional_bool(args, "RESTORE_ORIGINAL_QUALITIES")?;
    let explicit_sort_order = optional_scalar(args, "SORT_ORDER")?;
    optional_bool(args, "CREATE_INDEX")?;
    if let Some(sort_order) = explicit_sort_order {
        if sort_order != "queryname" && sort_order != "coordinate" && sort_order != "unsorted" {
            return Err(format!("unsupported RevertSam SORT_ORDER={sort_order}"));
        }
    }
    let _ = attributes_to_clear_for_revertsam(args)?;
    let _ = attributes_for_revertsam(args, "ATTRIBUTE_TO_REVERSE")?;
    let _ = attributes_for_revertsam(args, "ATTRIBUTE_TO_REVERSE_COMPLEMENT")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported RevertSam COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn attributes_to_clear_for_revertsam(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<[u8; 2]>, String> {
    attributes_for_revertsam(args, "ATTRIBUTE_TO_CLEAR")
}

fn attributes_for_revertsam(
    args: &BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Vec<[u8; 2]>, String> {
    let mut attributes = Vec::new();
    for attribute in args.get(key).into_iter().flatten() {
        let bytes = attribute.as_bytes();
        if bytes.len() != 2 || !bytes.iter().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(format!("unsupported RevertSam {key}={attribute}"));
        }
        attributes.push([bytes[0], bytes[1]]);
    }
    Ok(attributes)
}

fn reject_unsupported_setnmmdanduqtags_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "REFERENCE_SEQUENCE",
        "IS_BISULFITE_SEQUENCE",
        "SET_ONLY_UQ",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported SetNmMdAndUqTags argument: {key}"));
        }
    }
    if optional_bool(args, "IS_BISULFITE_SEQUENCE")?.unwrap_or(false) {
        return Err("unsupported SetNmMdAndUqTags IS_BISULFITE_SEQUENCE=true".to_string());
    }
    optional_bool(args, "SET_ONLY_UQ")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported SetNmMdAndUqTags COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_validatesamfile_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "MODE",
        "MAX_OUTPUT",
        "IGNORE",
        "SKIP_MATE_VALIDATION",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "COMPRESSION_LEVEL",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported ValidateSamFile argument: {key}"));
        }
    }
    validate_sam_mode(args)?;
    optional_u32(args, "MAX_OUTPUT")?;
    optional_bool(args, "SKIP_MATE_VALIDATION")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported ValidateSamFile COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    validate_sam_ignored_summary_keys(args)?;
    Ok(())
}

#[derive(Debug)]
struct FastaSequence {
    name: String,
    sequence: Vec<u8>,
}

fn read_fai_contig_lengths(path: &str) -> Result<Vec<(String, usize)>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut contigs = Vec::new();
    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let Some(name) = fields.next() else {
            continue;
        };
        let Some(length_text) = fields.next() else {
            return Err(format!("invalid FAI entry in {path}"));
        };
        let length = length_text
            .parse::<usize>()
            .map_err(|error| format!("invalid FAI length in {path}: {error}"))?;
        contigs.push((name.to_string(), length));
    }
    if contigs.is_empty() {
        return Err(format!("FAI index {path} contains no contigs"));
    }
    Ok(contigs)
}

fn read_fasta_contig_lengths(
    path: &str,
    truncate_names: bool,
) -> Result<Vec<(String, usize)>, String> {
    let text = read_text_or_gzip(path)?;
    let mut contigs = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_length = 0usize;

    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            if let Some(name) = current_name.take() {
                contigs.push((name, current_length));
            }
            current_length = 0;
            let name = if truncate_names {
                header.split_whitespace().next().unwrap_or_default()
            } else {
                header
            };
            if name.is_empty() {
                return Err("empty FASTA sequence name".to_string());
            }
            current_name = Some(name.to_string());
        } else if current_name.is_some() {
            current_length += line.trim().len();
        } else if !line.trim().is_empty() {
            return Err("FASTA sequence data before first header".to_string());
        }
    }

    if let Some(name) = current_name {
        contigs.push((name, current_length));
    }
    if contigs.is_empty() {
        return Err("FASTA contains no sequences".to_string());
    }
    Ok(contigs)
}

fn read_reference_contigs_for_wgs(path: &str) -> Result<Vec<(String, usize)>, String> {
    let fai_path = format!("{path}.fai");
    if Path::new(&fai_path).is_file() {
        return read_fai_contig_lengths(&fai_path);
    }
    read_fasta_contig_lengths(path, true)
}

#[derive(Debug, Clone, Copy)]
struct FaiEntry {
    length: usize,
    offset: u64,
}

fn read_fai_entry(fai_path: &str, contig: &str) -> Result<FaiEntry, String> {
    let text = fs::read_to_string(fai_path).map_err(|error| error.to_string())?;
    for line in text.lines() {
        let mut fields = line.split('\t');
        let Some(name) = fields.next() else {
            continue;
        };
        if name != contig {
            continue;
        }
        let length = fields
            .next()
            .ok_or_else(|| format!("invalid FAI entry for {contig} in {fai_path}"))?
            .parse::<usize>()
            .map_err(|error| format!("invalid FAI length for {contig}: {error}"))?;
        let offset = fields
            .next()
            .ok_or_else(|| format!("invalid FAI entry for {contig} in {fai_path}"))?
            .parse::<u64>()
            .map_err(|error| format!("invalid FAI offset for {contig}: {error}"))?;
        fields
            .next()
            .ok_or_else(|| format!("invalid FAI entry for {contig} in {fai_path}"))?
            .parse::<usize>()
            .map_err(|error| format!("invalid FAI bases-per-line for {contig}: {error}"))?;
        fields
            .next()
            .ok_or_else(|| format!("invalid FAI entry for {contig} in {fai_path}"))?
            .parse::<usize>()
            .map_err(|error| format!("invalid FAI line width for {contig}: {error}"))?;
        return Ok(FaiEntry { length, offset });
    }
    Err(format!("FAI index {fai_path} missing contig {contig}"))
}

fn load_fasta_contig_sequence_scan(path: &str, contig: &str) -> Result<Vec<u8>, String> {
    let text = read_text_or_gzip(path)?;
    let mut current_name: Option<String> = None;
    let mut sequence = Vec::new();
    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            if let Some(name) = current_name.take() {
                if name == contig {
                    return Ok(sequence);
                }
                sequence.clear();
            }
            let name = header.split_whitespace().next().unwrap_or_default();
            if name == contig {
                current_name = Some(name.to_string());
            }
        } else if current_name.is_some() {
            sequence.extend(line.trim().as_bytes().iter().map(u8::to_ascii_uppercase));
        }
    }
    if current_name.as_deref() == Some(contig) {
        return Ok(sequence);
    }
    Err(format!("FASTA {path} missing contig {contig}"))
}

fn load_fasta_contig_sequence_indexed(
    path: &str,
    contig: &str,
    entry: FaiEntry,
) -> Result<Vec<u8>, String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    file.seek(SeekFrom::Start(entry.offset))
        .map_err(|error| error.to_string())?;
    let mut reader = BufReader::new(file);
    let mut sequence = Vec::with_capacity(entry.length);
    let mut remaining = entry.length;
    while remaining > 0 {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        if line.is_empty() || line.starts_with('>') {
            continue;
        }
        let take = remaining.min(line.len());
        sequence.extend(line.as_bytes()[..take].iter().map(u8::to_ascii_uppercase));
        remaining -= take;
    }
    if sequence.len() != entry.length {
        return Err(format!(
            "FASTA {path} contig {contig} truncated: expected {} bases, read {}",
            entry.length,
            sequence.len()
        ));
    }
    Ok(sequence)
}

fn load_fasta_contig_sequence(path: &str, contig: &str) -> Result<Vec<u8>, String> {
    let fai_path = format!("{path}.fai");
    if Path::new(&fai_path).is_file() {
        let entry = read_fai_entry(&fai_path, contig)?;
        return load_fasta_contig_sequence_indexed(path, contig, entry);
    }
    load_fasta_contig_sequence_scan(path, contig)
}

fn count_gc_bias_windows(reference_path: &str, window_size: usize) -> Result<[u64; 101], String> {
    let mut windows = [0u64; 101];
    for (name, length) in read_reference_contigs_for_wgs(reference_path)? {
        if length < window_size {
            continue;
        }
        let sequence = load_fasta_contig_sequence(reference_path, &name)?;
        let window_count = sequence.len().saturating_sub(window_size + 1);
        for start in 0..window_count {
            if let Some(window) = sequence.get(start..start + window_size) {
                windows[gc_percent(window, window_size)] += 1;
            }
        }
    }
    Ok(windows)
}

fn read_fasta_sequences(path: &str, truncate_names: bool) -> Result<Vec<FastaSequence>, String> {
    let text = read_text_or_gzip(path)?;
    let mut records = Vec::new();
    let mut current_name: Option<String> = None;
    let mut current_sequence = Vec::new();

    for line in text.lines() {
        if let Some(header) = line.strip_prefix('>') {
            if let Some(name) = current_name.take() {
                records.push(FastaSequence {
                    name,
                    sequence: std::mem::take(&mut current_sequence),
                });
            }
            let name = if truncate_names {
                header.split_whitespace().next().unwrap_or_default()
            } else {
                header
            };
            if name.is_empty() {
                return Err("empty FASTA sequence name".to_string());
            }
            current_name = Some(name.to_string());
        } else if current_name.is_some() {
            current_sequence.extend(line.trim().as_bytes().iter().map(u8::to_ascii_uppercase));
        } else if !line.trim().is_empty() {
            return Err("FASTA sequence data before first header".to_string());
        }
    }

    if let Some(name) = current_name {
        records.push(FastaSequence {
            name,
            sequence: current_sequence,
        });
    }
    if records.is_empty() {
        return Err("FASTA contains no sequences".to_string());
    }
    Ok(records)
}

fn reference_sequences_by_name(path: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    Ok(read_fasta_sequences(path, true)?
        .into_iter()
        .map(|record| (record.name, record.sequence.to_ascii_uppercase()))
        .collect())
}

#[derive(Debug, Default)]
struct ValidateSamReport {
    counts: BTreeMap<String, u64>,
    details: Vec<ValidateSamDetail>,
}

#[derive(Debug)]
struct ValidateSamDetail {
    key: String,
    line: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ValidateSamMode {
    Summary,
    Verbose,
}

fn validate_sam_summary(
    reader: &mut bam::Reader,
    skip_mate_validation: bool,
    reference_by_name: Option<&BTreeMap<String, Vec<u8>>>,
) -> Result<ValidateSamReport, String> {
    let header_text = String::from_utf8_lossy(reader.header().as_bytes()).to_string();
    let read_groups = read_group_platforms(&header_text);
    let target_count = reader.header().target_count();
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect::<Vec<_>>();
    let references_by_tid = reference_by_name.map(|references| {
        target_names
            .iter()
            .map(|name| references.get(name).map(Vec::as_slice))
            .collect::<Vec<_>>()
    });
    let mut report = ValidateSamReport::default();
    let mut pending_mates = BTreeMap::<Vec<u8>, ValidateSamMate>::new();

    if target_count == 0 {
        add_validate_issue(
            &mut report,
            "ERROR:MISSING_SEQUENCE_DICTIONARY",
            "ERROR::MISSING_SEQUENCE_DICTIONARY:Sequence dictionary is empty".to_string(),
        );
    }
    if read_groups.is_empty() {
        add_validate_issue(
            &mut report,
            "ERROR:MISSING_READ_GROUP",
            "ERROR::MISSING_READ_GROUP:Read groups is empty".to_string(),
        );
    } else {
        for has_platform in read_groups.values() {
            if !has_platform {
                add_validate_issue(
                    &mut report,
                    "ERROR:MISSING_PLATFORM_VALUE",
                    "ERROR::MISSING_PLATFORM_VALUE:A platform value is missing".to_string(),
                );
            }
        }
    }

    let mut record_number = 0_u64;
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        record_number += 1;
        if record.is_paired() && !skip_mate_validation {
            validate_sam_mate_summary(&record, &mut pending_mates, &mut report);
        }
        validate_sam_record_summary(
            &record,
            record_number,
            target_count,
            &read_groups,
            references_by_tid.as_deref(),
            &mut report,
        )?;
    }
    for _ in pending_mates.values() {
        add_validate_issue(
            &mut report,
            "ERROR:MATE_NOT_FOUND",
            "ERROR::MATE_NOT_FOUND:Mate not found for paired read".to_string(),
        );
    }

    Ok(report)
}

fn validate_sam_summary_sam_text(
    input: &str,
    skip_mate_validation: bool,
) -> Result<ValidateSamReport, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut target_count = 0_u32;
    let mut report = ValidateSamReport::default();
    let mut pending_mates = BTreeMap::<Vec<u8>, ValidateSamMate>::new();
    let mut read_groups = BTreeMap::<String, bool>::new();
    let mut record_number = 0_u64;

    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@") {
            let text = String::from_utf8_lossy(&line);
            if line.starts_with(b"@SQ\t") {
                target_count += 1;
            }
            if line.starts_with(b"@RG\t") {
                if let Some(id) = read_group_id(&text) {
                    let has_platform = text.split('\t').any(|field| {
                        field
                            .strip_prefix("PL:")
                            .is_some_and(|value| !value.trim_end_matches(['\r', '\n']).is_empty())
                    });
                    read_groups.insert(id, has_platform);
                }
            }
            continue;
        }
        if line.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }

        record_number += 1;
        validate_sam_record_summary_sam_line(
            &line,
            record_number,
            target_count,
            &read_groups,
            &mut report,
        )?;
        if !skip_mate_validation {
            validate_sam_mate_summary_sam_line(&line, &mut pending_mates, &mut report)?;
        }
    }

    if target_count == 0 {
        add_validate_issue(
            &mut report,
            "ERROR:MISSING_SEQUENCE_DICTIONARY",
            "ERROR::MISSING_SEQUENCE_DICTIONARY:Sequence dictionary is empty".to_string(),
        );
    }
    if read_groups.is_empty() {
        add_validate_issue(
            &mut report,
            "ERROR:MISSING_READ_GROUP",
            "ERROR::MISSING_READ_GROUP:Read groups is empty".to_string(),
        );
    } else {
        for has_platform in read_groups.values() {
            if !has_platform {
                add_validate_issue(
                    &mut report,
                    "ERROR:MISSING_PLATFORM_VALUE",
                    "ERROR::MISSING_PLATFORM_VALUE:A platform value is missing".to_string(),
                );
            }
        }
    }
    for _ in pending_mates.values() {
        add_validate_issue(
            &mut report,
            "ERROR:MATE_NOT_FOUND",
            "ERROR::MATE_NOT_FOUND:Mate not found for paired read".to_string(),
        );
    }

    Ok(report)
}

fn validate_sam_record_summary_sam_line(
    line: &[u8],
    record_number: u64,
    target_count: u32,
    read_groups: &BTreeMap<String, bool>,
    report: &mut ValidateSamReport,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    let qname = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    let read_name = String::from_utf8_lossy(qname).into_owned();
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?,
    )?;
    let rname = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    let _pos = fields.next();
    let mapq = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    for _ in 0..5 {
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    }
    let mut read_group = None::<&[u8]>;
    let mut has_nm = false;
    for field in fields {
        if let Some(value) = field.strip_prefix(b"RG:Z:") {
            read_group = Some(value);
        }
        if field.starts_with(b"NM:") {
            has_nm = true;
        }
    }
    match read_group {
        Some(read_group) => {
            let read_group = String::from_utf8_lossy(read_group);
            if !read_groups.contains_key(read_group.as_ref()) {
                add_validate_issue(
                    report,
                    "ERROR:READ_GROUP_NOT_FOUND",
                    format!(
                        "ERROR::READ_GROUP_NOT_FOUND:Read name {read_name}, RG ID on record not found in header"
                    ),
                );
            }
        }
        None => add_validate_issue(
            report,
            "WARNING:RECORD_MISSING_READ_GROUP",
            format!(
                "WARNING::RECORD_MISSING_READ_GROUP:Read name {read_name}, A record is missing a read group"
            ),
        ),
    }

    if rname != b"*" {
        if target_count == 0 {
            add_validate_issue(
                report,
                "ERROR:MISSING_SEQUENCE_DICTIONARY",
                format!(
                    "ERROR::MISSING_SEQUENCE_DICTIONARY:Read name {read_name}, Reference sequence is missing from the sequence dictionary"
                ),
            );
        }
        if !has_nm {
            add_validate_issue(
                report,
                "WARNING:MISSING_TAG_NM",
                format!(
                    "WARNING::MISSING_TAG_NM:Record {record_number}, Read name {read_name}, NM tag (nucleotide differences) is missing"
                ),
            );
        }
    } else if mapq != b"0" {
        add_validate_issue(
            report,
            "ERROR:INVALID_MAPPING_QUALITY",
            format!(
                "ERROR::INVALID_MAPPING_QUALITY:Record {record_number}, Read name {read_name}, MAPQ should be 0 for unmapped read"
            ),
        );
    }
    let _ = flags;
    Ok(())
}

fn validate_sam_mate_summary_sam_line(
    line: &[u8],
    pending_mates: &mut BTreeMap<Vec<u8>, ValidateSamMate>,
    report: &mut ValidateSamReport,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    let qname = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?
        .to_vec();
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?,
    )?;
    if flags & 0x100 != 0 || flags & 0x800 != 0 {
        return Ok(());
    }
    if flags & 0x1 == 0 {
        return Ok(());
    }
    let rname = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    let pos = parse_i64_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?,
    )?;
    let pos = pos.saturating_sub(1);
    for _ in 0..2 {
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    }
    let rnext = fields
        .next()
        .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?;
    let pnext = parse_i64_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed ValidateSamFile SAM record".to_string())?,
    )?;
    let pnext = pnext.saturating_sub(1);
    let tid = if rname == b"*" { -1 } else { 0 };
    let mtid = if rnext == b"*" || rnext == b"=" {
        tid
    } else {
        0
    };
    let mate = ValidateSamMate {
        tid,
        pos,
        mtid,
        mpos: pnext,
    };
    if let Some(pending) = pending_mates.remove(&qname) {
        if !pending.is_reciprocal_with(&mate) {
            add_validate_issue(
                report,
                "ERROR:MATE_NOT_FOUND",
                format!(
                    "ERROR::MATE_NOT_FOUND:Read name {}, Mate not found for paired read",
                    String::from_utf8_lossy(&qname)
                ),
            );
        }
    } else {
        pending_mates.insert(qname, mate);
    }
    Ok(())
}

#[derive(Clone)]
struct ValidateSamMate {
    tid: i32,
    pos: i64,
    mtid: i32,
    mpos: i64,
}

impl ValidateSamMate {
    fn from_record(record: &bam::Record) -> Self {
        Self {
            tid: record.tid(),
            pos: record.pos(),
            mtid: record.mtid(),
            mpos: record.mpos(),
        }
    }

    fn is_reciprocal_with(&self, mate: &Self) -> bool {
        self.mtid == mate.tid
            && self.mpos == mate.pos
            && mate.mtid == self.tid
            && mate.mpos == self.pos
    }
}

fn validate_sam_mate_summary(
    record: &bam::Record,
    pending_mates: &mut BTreeMap<Vec<u8>, ValidateSamMate>,
    report: &mut ValidateSamReport,
) {
    if record.is_secondary() || record.is_supplementary() {
        return;
    }
    let qname = record.qname().to_vec();
    let mate = ValidateSamMate::from_record(record);
    if let Some(pending) = pending_mates.remove(&qname) {
        if !pending.is_reciprocal_with(&mate) {
            add_validate_issue(
                report,
                "ERROR:MATE_NOT_FOUND",
                format!(
                    "ERROR::MATE_NOT_FOUND:Read name {}, Mate not found for paired read",
                    validate_qname(record)
                ),
            );
        }
    } else {
        pending_mates.insert(qname, mate);
    }
}

fn validate_sam_record_summary(
    record: &bam::Record,
    record_number: u64,
    target_count: u32,
    read_groups: &BTreeMap<String, bool>,
    references_by_tid: Option<&[Option<&[u8]>]>,
    report: &mut ValidateSamReport,
) -> Result<(), String> {
    let read_name = validate_qname(record);
    match record.aux(b"RG") {
        Ok(Aux::String(read_group)) => {
            if !read_groups.contains_key(read_group) {
                add_validate_issue(
                    report,
                    "ERROR:READ_GROUP_NOT_FOUND",
                    format!(
                        "ERROR::READ_GROUP_NOT_FOUND:Read name {read_name}, RG ID on record not found in header"
                    ),
                );
            }
        }
        Ok(_) => add_validate_issue(
            report,
            "ERROR:INVALID_TAG_TYPE",
            format!("ERROR::INVALID_TAG_TYPE:Read name {read_name}, RG tag has invalid type"),
        ),
        Err(_) => add_validate_issue(
            report,
            "WARNING:RECORD_MISSING_READ_GROUP",
            format!(
                "WARNING::RECORD_MISSING_READ_GROUP:Read name {read_name}, A record is missing a read group"
            ),
        ),
    }

    if !record.is_unmapped() {
        if record.tid() < 0 || record.tid() as u32 >= target_count {
            add_validate_issue(
                report,
                "ERROR:MISSING_SEQUENCE_DICTIONARY",
                format!(
                    "ERROR::MISSING_SEQUENCE_DICTIONARY:Read name {read_name}, Reference sequence is missing from the sequence dictionary"
                ),
            );
        }
        match record.aux(b"NM") {
            Ok(aux) => {
                if let Some(references_by_tid) = references_by_tid {
                    if let Some(actual_nm) = aux_i32(aux) {
                        if let Some(expected_nm) = expected_record_nm(record, references_by_tid)? {
                            if actual_nm != expected_nm {
                                add_validate_issue(
                                    report,
                                    "ERROR:INVALID_TAG_NM",
                                    format!(
                                        "ERROR::INVALID_TAG_NM:Record {record_number}, Read name {read_name}, NM tag is incorrect"
                                    ),
                                );
                            }
                        }
                    }
                }
            }
            Err(_) => {
                add_validate_issue(
                    report,
                    "WARNING:MISSING_TAG_NM",
                    format!(
                        "WARNING::MISSING_TAG_NM:Record {record_number}, Read name {read_name}, NM tag (nucleotide differences) is missing"
                    ),
                );
            }
        }
    } else if record.mapq() != 0 {
        add_validate_issue(
            report,
            "ERROR:INVALID_MAPPING_QUALITY",
            format!(
                "ERROR::INVALID_MAPPING_QUALITY:Record {record_number}, Read name {read_name}, MAPQ should be 0 for unmapped read"
            ),
        );
    }
    Ok(())
}

fn aux_i32(value: Aux<'_>) -> Option<i32> {
    match value {
        Aux::I8(value) => Some(value as i32),
        Aux::U8(value) => Some(value as i32),
        Aux::I16(value) => Some(value as i32),
        Aux::U16(value) => Some(value as i32),
        Aux::I32(value) => Some(value),
        Aux::U32(value) => i32::try_from(value).ok(),
        _ => None,
    }
}

fn expected_record_nm(
    record: &bam::Record,
    references_by_tid: &[Option<&[u8]>],
) -> Result<Option<i32>, String> {
    if record.is_unmapped() || record.is_secondary() || record.is_supplementary() {
        return Ok(None);
    }
    if record.tid() < 0 || record.pos() < 0 {
        return Ok(None);
    }
    let Some(reference) = references_by_tid
        .get(record.tid() as usize)
        .copied()
        .flatten()
    else {
        return Ok(None);
    };
    let read_bases = record.seq();
    let mut read_offset = 0usize;
    let mut ref_offset = record.pos() as usize;
    let mut nm = 0i32;

    for cigar in &record.cigar() {
        match *cigar {
            Cigar::Match(length) => {
                for _ in 0..length {
                    if read_offset >= read_bases.len() {
                        return Err("ValidateSamFile read sequence shorter than CIGAR".to_string());
                    }
                    let Some(ref_base) = reference.get(ref_offset).copied() else {
                        return Ok(None);
                    };
                    if !dna_bases_equal(read_bases[read_offset], ref_base) {
                        nm += 1;
                    }
                    read_offset += 1;
                    ref_offset += 1;
                }
            }
            Cigar::Equal(length) => {
                read_offset += length as usize;
                ref_offset += length as usize;
            }
            Cigar::Diff(length) => {
                read_offset += length as usize;
                ref_offset += length as usize;
                nm += length as i32;
            }
            Cigar::Ins(length) => {
                read_offset += length as usize;
                nm += length as i32;
            }
            Cigar::Del(length) => {
                ref_offset += length as usize;
                nm += length as i32;
            }
            Cigar::SoftClip(length) => {
                read_offset += length as usize;
            }
            Cigar::HardClip(_) | Cigar::Pad(_) => {}
            Cigar::RefSkip(length) => {
                ref_offset += length as usize;
            }
        }
    }

    Ok(Some(nm))
}

fn validate_sam_ignored_summary_keys(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<BTreeSet<String>, String> {
    let mut ignored = BTreeSet::new();
    for value in args.get("IGNORE").into_iter().flatten() {
        let key = match value.as_str() {
            "MISSING_SEQUENCE_DICTIONARY" => "ERROR:MISSING_SEQUENCE_DICTIONARY",
            "MISSING_READ_GROUP" => "ERROR:MISSING_READ_GROUP",
            "MISSING_PLATFORM_VALUE" => "ERROR:MISSING_PLATFORM_VALUE",
            "MISSING_TAG_NM" => "WARNING:MISSING_TAG_NM",
            "INVALID_TAG_NM" => "ERROR:INVALID_TAG_NM",
            "READ_GROUP_NOT_FOUND" => "ERROR:READ_GROUP_NOT_FOUND",
            "INVALID_TAG_TYPE" => "ERROR:INVALID_TAG_TYPE",
            "INVALID_MAPPING_QUALITY" => "ERROR:INVALID_MAPPING_QUALITY",
            "RECORD_MISSING_READ_GROUP" => "WARNING:RECORD_MISSING_READ_GROUP",
            "MATE_NOT_FOUND" => "ERROR:MATE_NOT_FOUND",
            _ => return Err(format!("unsupported ValidateSamFile IGNORE={value}")),
        };
        ignored.insert(key.to_string());
    }
    Ok(ignored)
}

fn validate_sam_mode(args: &BTreeMap<String, Vec<String>>) -> Result<ValidateSamMode, String> {
    match optional_scalar(args, "MODE")?
        .unwrap_or_else(|| "SUMMARY".to_string())
        .to_ascii_uppercase()
        .as_str()
    {
        "SUMMARY" => Ok(ValidateSamMode::Summary),
        "VERBOSE" => Ok(ValidateSamMode::Verbose),
        mode => Err(format!("unsupported ValidateSamFile MODE={mode}")),
    }
}

fn read_group_platforms(header_text: &str) -> BTreeMap<String, bool> {
    let mut read_groups = BTreeMap::new();
    for line in header_text.lines().filter(|line| line.starts_with("@RG\t")) {
        if let Some(id) = read_group_id(line) {
            let has_platform = line.split('\t').skip(1).any(|field| {
                field
                    .strip_prefix("PL:")
                    .is_some_and(|value| !value.is_empty())
            });
            read_groups.insert(id, has_platform);
        }
    }
    read_groups
}

fn insert_size_read_groups_from_header(header_text: &str) -> BTreeMap<String, InsertSizeReadGroup> {
    let mut read_groups = BTreeMap::new();
    for line in header_text.lines().filter(|line| line.starts_with("@RG\t")) {
        let mut id = None;
        let mut sample = None;
        let mut library = None;
        let mut platform_unit = None;
        for field in line.split('\t').skip(1) {
            if let Some(value) = field.strip_prefix("ID:") {
                id = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("SM:") {
                sample = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("LB:") {
                library = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("PU:") {
                platform_unit = Some(value.to_string());
            }
        }
        if let (Some(id), Some(sample)) = (id, sample) {
            read_groups.insert(
                id,
                InsertSizeReadGroup {
                    sample,
                    library: library.unwrap_or_default(),
                    platform_unit: platform_unit.unwrap_or_else(|| "unknown".to_string()),
                },
            );
        }
    }
    read_groups
}

fn insert_size_read_group_for_bam_record(
    record: &bam::Record,
    read_groups: &BTreeMap<String, InsertSizeReadGroup>,
) -> Option<InsertSizeReadGroup> {
    let Ok(Aux::String(read_group)) = record.aux(b"RG") else {
        return None;
    };
    read_groups.get(read_group).cloned()
}

fn validate_qname(record: &bam::Record) -> String {
    String::from_utf8_lossy(record.qname()).into_owned()
}

fn add_validate_issue(report: &mut ValidateSamReport, key: &str, detail: String) {
    add_validate_count(report, key);
    report.details.push(ValidateSamDetail {
        key: key.to_string(),
        line: detail,
    });
}

fn add_validate_count(report: &mut ValidateSamReport, key: &str) {
    *report.counts.entry(key.to_string()).or_default() += 1;
}

fn write_validate_sam_summary(
    output: Option<&str>,
    counts: &BTreeMap<String, u64>,
) -> Result<(), String> {
    let text = if counts.is_empty() {
        "No errors found\n".to_string()
    } else {
        let mut text = "\n\n## HISTOGRAM\tjava.lang.String\nError Type\tCount\n".to_string();
        for (key, count) in counts {
            text.push_str(&format!("{key}\t{count}\n"));
        }
        text.push('\n');
        text
    };
    match output {
        Some(output) => fs::write(output, text).map_err(|error| error.to_string()),
        None => std::io::stdout()
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string()),
    }
}

fn write_validate_sam_verbose(
    output: Option<&str>,
    details: &[ValidateSamDetail],
    max_output: Option<u32>,
) -> Result<(), String> {
    let text = if details.is_empty() {
        "No errors found\n".to_string()
    } else {
        let mut text = String::new();
        let max_output = max_output.map(|value| value as usize).unwrap_or(100);
        for detail in details.iter().take(max_output) {
            text.push_str(&detail.line);
            text.push('\n');
        }
        if details.len() > max_output {
            text.push_str(&format!(
                "Maximum output of [{max_output}] errors reached.\n"
            ));
        }
        text
    };
    match output {
        Some(output) => fs::write(output, text).map_err(|error| error.to_string()),
        None => std::io::stdout()
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string()),
    }
}

fn read_text_or_gzip(path: &str) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut text = String::new();
    if has_gzip_extension(path) {
        GzDecoder::new(file)
            .read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
    } else {
        let mut reader = std::io::BufReader::new(file);
        reader
            .read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
    }
    Ok(text)
}

fn write_text_or_gzip(path: &str, text: &str) -> Result<(), String> {
    if has_gzip_extension(path) {
        let file = fs::File::create(path).map_err(|error| error.to_string())?;
        let mut writer = GzEncoder::new(file, Compression::default());
        writer
            .write_all(text.as_bytes())
            .and_then(|_| writer.finish().map(|_| ()))
            .map_err(|error| error.to_string())
    } else {
        fs::write(path, text).map_err(|error| error.to_string())
    }
}

fn write_summary_chart_pdf(path: &str, command: &str) -> Result<(), String> {
    let title = format!("{command} summary chart");
    let filename = Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("chart.pdf");
    let content = format!(
        "BT\n/F1 16 Tf\n72 740 Td\n({}) Tj\n/F1 10 Tf\n0 -28 Td\n({}) Tj\n0 -16 Td\n({}) Tj\nET\n",
        escape_pdf_text(&title),
        escape_pdf_text(filename),
        escape_pdf_text("Metrics text remains the parity target for Picard comparisons.")
    );
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            content.len(),
            content
        ),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{}\nendobj\n", index + 1, object));
    }
    let xref_start = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref_start
    ));
    fs::write(path, pdf).map_err(|error| error.to_string())
}

fn escape_pdf_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn derived_dict_path(reference: &str) -> String {
    let mut path = Path::new(reference).to_path_buf();
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "gz" | "gzip"))
    {
        path.set_extension("");
    }
    path.with_extension("dict").display().to_string()
}

fn flush_fasta_sequence(output: &mut String, sequence: &[u8], line_length: usize) {
    for chunk in sequence.chunks(line_length) {
        output.push_str(&String::from_utf8_lossy(chunk));
        output.push('\n');
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BedInterval {
    contig: String,
    contig_index: usize,
    start: u64,
    end: u64,
    strand: String,
    name: String,
}

fn dictionary_contig_order(dictionary_text: &str) -> BTreeMap<String, usize> {
    dictionary_text
        .lines()
        .filter(|line| line.starts_with("@SQ\t"))
        .enumerate()
        .filter_map(|(index, line)| {
            line.split('\t')
                .skip(1)
                .find_map(|field| field.strip_prefix("SN:"))
                .map(|name| (name.to_string(), index))
        })
        .collect()
}

fn dictionary_contig_lengths(dictionary_text: &str) -> BTreeMap<String, u64> {
    dictionary_text
        .lines()
        .filter(|line| line.starts_with("@SQ\t"))
        .filter_map(|line| {
            let name = line
                .split('\t')
                .find_map(|field| field.strip_prefix("SN:"))?;
            let length = line
                .split('\t')
                .find_map(|field| field.strip_prefix("LN:"))?
                .parse::<u64>()
                .ok()?;
            Some((name.to_string(), length))
        })
        .collect()
}

fn interval_list_header_text(text: &str) -> String {
    let mut header = String::new();
    for line in text.lines() {
        if line.starts_with('@') {
            if !line.starts_with("@PG\t") {
                header.push_str(line);
                header.push('\n');
            }
        } else {
            break;
        }
    }
    header
}

fn interval_list_output_header(header_text: &str, force_unsorted: bool) -> String {
    let mut output = String::new();
    let mut saw_hd = false;
    for line in header_text.lines() {
        if line.starts_with("@HD\t") {
            saw_hd = true;
            let mut fields = vec!["@HD".to_string()];
            let mut saw_so = false;
            for field in line.split('\t').skip(1) {
                if field.starts_with("SO:") {
                    if force_unsorted {
                        fields.push("SO:unsorted".to_string());
                    } else {
                        fields.push(field.to_string());
                    }
                    saw_so = true;
                } else {
                    fields.push(field.to_string());
                }
            }
            if !saw_so {
                fields.push(if force_unsorted {
                    "SO:unsorted".to_string()
                } else {
                    "SO:coordinate".to_string()
                });
            }
            output.push_str(&fields.join("\t"));
            output.push('\n');
        } else {
            output.push_str(line);
            output.push('\n');
        }
    }
    if !saw_hd {
        let sort_order = if force_unsorted {
            "unsorted"
        } else {
            "coordinate"
        };
        format!("@HD\tVN:1.6\tSO:{sort_order}\n{output}")
    } else {
        output
    }
}

fn read_interval_list_intervals(
    text: &str,
    contig_order: &BTreeMap<String, usize>,
) -> Result<Vec<BedInterval>, String> {
    let mut intervals = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        if line.starts_with('@') || line.trim().is_empty() {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() < 5 {
            return Err(format!("malformed interval_list line {}", line_index + 1));
        }
        let contig = fields[0].to_string();
        let Some(contig_index) = contig_order.get(&contig).copied() else {
            return Err(format!(
                "interval_list contig {contig} is not present in sequence dictionary"
            ));
        };
        let start = fields[1]
            .parse::<u64>()
            .map_err(|_| format!("malformed interval start on line {}", line_index + 1))?;
        let end = fields[2]
            .parse::<u64>()
            .map_err(|_| format!("malformed interval end on line {}", line_index + 1))?;
        if end < start {
            return Err(format!(
                "interval end before start on line {}",
                line_index + 1
            ));
        }
        intervals.push(BedInterval {
            contig,
            contig_index,
            start,
            end,
            strand: fields[3].to_string(),
            name: fields[4..].join("\t"),
        });
    }
    Ok(intervals)
}

fn apply_interval_padding(
    intervals: &mut [BedInterval],
    contig_lengths: &BTreeMap<String, u64>,
    padding: u64,
) -> Result<(), String> {
    for interval in intervals {
        let Some(contig_length) = contig_lengths.get(&interval.contig).copied() else {
            return Err(format!(
                "interval_list contig {} is missing length in sequence dictionary",
                interval.contig
            ));
        };
        interval.start = interval.start.saturating_sub(padding).max(1);
        interval.end = interval.end.saturating_add(padding).min(contig_length);
    }
    Ok(())
}

fn collectwgs_interval_masks(
    interval_paths: Option<&Vec<String>>,
    reference_contigs: &[(String, usize)],
) -> Result<Option<BTreeMap<String, Vec<bool>>>, String> {
    let Some(interval_paths) = interval_paths else {
        return Ok(None);
    };
    let reference_lengths = reference_contigs
        .iter()
        .cloned()
        .collect::<BTreeMap<_, _>>();
    let contig_order = reference_contigs
        .iter()
        .enumerate()
        .map(|(index, (name, _))| (name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut masks = reference_contigs
        .iter()
        .map(|(name, length)| (name.clone(), vec![false; *length]))
        .collect::<BTreeMap<_, _>>();

    for interval_path in interval_paths {
        let text = read_text_or_gzip(interval_path)?;
        let intervals = read_interval_list_intervals(&text, &contig_order)?;
        for interval in intervals {
            let length = *reference_lengths.get(&interval.contig).ok_or_else(|| {
                format!(
                    "CollectWgsMetrics interval contig {} is not present in reference",
                    interval.contig
                )
            })?;
            if interval.start == 0
                || interval.end < interval.start
                || interval.end as usize > length
            {
                return Err(format!(
                    "CollectWgsMetrics interval {}:{}-{} is outside reference bounds",
                    interval.contig, interval.start, interval.end
                ));
            }
            let mask = masks.get_mut(&interval.contig).ok_or_else(|| {
                format!(
                    "CollectWgsMetrics missing interval contig {}",
                    interval.contig
                )
            })?;
            for included in &mut mask[(interval.start as usize - 1)..interval.end as usize] {
                *included = true;
            }
        }
    }
    Ok(Some(masks))
}

fn sort_intervals(intervals: &mut [BedInterval]) {
    intervals.sort_by(|left, right| {
        left.contig_index
            .cmp(&right.contig_index)
            .then_with(|| left.start.cmp(&right.start))
            .then_with(|| left.end.cmp(&right.end))
            .then_with(|| left.strand.cmp(&right.strand))
            .then_with(|| left.name.cmp(&right.name))
    });
}

fn unique_intervals(intervals: Vec<BedInterval>, dont_merge_abutting: bool) -> Vec<BedInterval> {
    let mut unique = Vec::<BedInterval>::new();
    for interval in intervals {
        let Some(last) = unique.last_mut() else {
            unique.push(interval);
            continue;
        };
        let merge_boundary = if dont_merge_abutting {
            last.end
        } else {
            last.end.saturating_add(1)
        };
        if last.contig == interval.contig
            && last.strand == interval.strand
            && interval.start <= merge_boundary
        {
            last.end = last.end.max(interval.end);
            last.name.push('|');
            last.name.push_str(&interval.name);
        } else {
            unique.push(interval);
        }
    }
    unique
}

fn read_bed_intervals(
    path: &str,
    contig_order: &BTreeMap<String, usize>,
    drop_missing_contigs: bool,
    keep_length_zero_intervals: bool,
) -> Result<Vec<BedInterval>, String> {
    let text = fs::read_to_string(path).map_err(|error| error.to_string())?;
    let mut intervals = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty()
            || line.starts_with('#')
            || line.starts_with("track")
            || line.starts_with("browser")
        {
            continue;
        }
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() < 3 {
            return Err(format!("malformed BED line {}", line_index + 1));
        }
        let contig = fields[0].to_string();
        let Some(contig_index) = contig_order.get(&contig).copied() else {
            if drop_missing_contigs {
                continue;
            }
            return Err(format!(
                "BED contig {contig} is not present in sequence dictionary"
            ));
        };
        let start0 = fields[1]
            .parse::<u64>()
            .map_err(|_| format!("malformed BED start on line {}", line_index + 1))?;
        let end = fields[2]
            .parse::<u64>()
            .map_err(|_| format!("malformed BED end on line {}", line_index + 1))?;
        if end < start0 {
            return Err(format!("BED end before start on line {}", line_index + 1));
        }
        if end == start0 && !keep_length_zero_intervals {
            continue;
        }
        intervals.push(BedInterval {
            contig,
            contig_index,
            start: start0 + 1,
            end,
            strand: fields.get(5).copied().unwrap_or("+").to_string(),
            name: fields.get(3).copied().unwrap_or(".").to_string(),
        });
    }
    Ok(intervals)
}

fn reject_unsupported_collectalignment_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "REFERENCE_SEQUENCE",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "METRIC_ACCUMULATION_LEVEL",
        "ASSUME_SORTED",
        "COLLECT_ALIGNMENT_INFORMATION",
        "STOP_AFTER",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported CollectAlignmentSummaryMetrics argument: {key}"
            ));
        }
    }

    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "ASSUME_SORTED")?;
    if optional_bool(args, "COLLECT_ALIGNMENT_INFORMATION")? == Some(false) {
        return Err(
            "unsupported CollectAlignmentSummaryMetrics COLLECT_ALIGNMENT_INFORMATION=false"
                .to_string(),
        );
    }
    if let Some(level) = optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")? {
        if level != "ALL_READS" && level != "SAMPLE" && level != "LIBRARY" && level != "READ_GROUP"
        {
            return Err(format!(
                "unsupported CollectAlignmentSummaryMetrics METRIC_ACCUMULATION_LEVEL={level}"
            ));
        }
    }
    optional_u32(args, "STOP_AFTER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectAlignmentSummaryMetrics COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_collectqualityyield_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "USE_ORIGINAL_QUALITIES",
        "INCLUDE_SECONDARY_ALIGNMENTS",
        "INCLUDE_SUPPLEMENTAL_ALIGNMENTS",
        "STOP_AFTER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported CollectQualityYieldMetrics argument: {key}"
            ));
        }
    }

    optional_bool(args, "USE_ORIGINAL_QUALITIES")?;
    optional_bool(args, "INCLUDE_SECONDARY_ALIGNMENTS")?;
    optional_bool(args, "INCLUDE_SUPPLEMENTAL_ALIGNMENTS")?;
    optional_u32(args, "STOP_AFTER")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "QUIET")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectQualityYieldMetrics COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_collectinsertsize_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "HISTOGRAM_FILE",
        "REFERENCE_SEQUENCE",
        "METRIC_ACCUMULATION_LEVEL",
        "INCLUDE_DUPLICATES",
        "ASSUME_SORTED",
        "DEVIATIONS",
        "MINIMUM_PCT",
        "STOP_AFTER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported CollectInsertSizeMetrics argument: {key}"
            ));
        }
    }
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "HISTOGRAM_FILE")?;
    if let Some(level) = optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")? {
        if level != "ALL_READS" && level != "SAMPLE" && level != "LIBRARY" && level != "READ_GROUP"
        {
            return Err(format!(
                "unsupported CollectInsertSizeMetrics METRIC_ACCUMULATION_LEVEL={level}"
            ));
        }
    }
    optional_bool(args, "INCLUDE_DUPLICATES")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_scalar(args, "DEVIATIONS")?;
    optional_scalar(args, "MINIMUM_PCT")?;
    optional_u32(args, "STOP_AFTER")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectInsertSizeMetrics COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_collectmultiplemetrics_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "PROGRAM",
        "EXTRA_ARGUMENT",
        "FILE_EXTENSION",
        "METRIC_ACCUMULATION_LEVEL",
        "ASSUME_SORTED",
        "STOP_AFTER",
        "REFERENCE_SEQUENCE",
        "SCAN_WINDOW_SIZE",
        "MINIMUM_GENOME_FRACTION",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "USE_FAST_ALGORITHM",
        "SAMPLE_SIZE",
        "INCLUDE_BQ_HISTOGRAM",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported CollectMultipleMetrics argument: {key}"
            ));
        }
    }
    optional_bool(args, "ASSUME_SORTED")?;
    for value in args.get("EXTRA_ARGUMENT").into_iter().flatten() {
        let Some((program, rest)) = value.split_once("::") else {
            return Err(format!(
                "unsupported CollectMultipleMetrics EXTRA_ARGUMENT={value}"
            ));
        };
        if !collectmultiplemetrics_programs(args)?
            .iter()
            .any(|p| p == program)
        {
            return Err(format!(
                "unsupported CollectMultipleMetrics EXTRA_ARGUMENT program: {program}"
            ));
        }
        let Some((key, _argument_value)) = rest.split_once('=') else {
            return Err(format!(
                "unsupported CollectMultipleMetrics EXTRA_ARGUMENT={value}"
            ));
        };
        match (program, key) {
            ("CollectGcBiasMetrics", "SCAN_WINDOW_SIZE")
            | ("CollectGcBiasMetrics", "MINIMUM_GENOME_FRACTION")
            | ("CollectGcBiasMetrics", "ALSO_IGNORE_DUPLICATES")
            | ("CollectInsertSizeMetrics", "INCLUDE_DUPLICATES")
            | ("CollectInsertSizeMetrics", "DEVIATIONS")
            | ("CollectInsertSizeMetrics", "MINIMUM_PCT")
            | ("QualityScoreDistribution", "ALIGNED_READS_ONLY")
            | ("QualityScoreDistribution", "PF_READS_ONLY")
            | ("QualityScoreDistribution", "INCLUDE_NO_CALLS")
            | ("MeanQualityByCycle", "ALIGNED_READS_ONLY")
            | ("MeanQualityByCycle", "PF_READS_ONLY")
            | ("CollectQualityYieldMetrics", "INCLUDE_SECONDARY_ALIGNMENTS")
            | ("CollectQualityYieldMetrics", "INCLUDE_SUPPLEMENTAL_ALIGNMENTS")
            | ("CollectQualityYieldMetrics", "USE_ORIGINAL_QUALITIES")
            | ("CollectBaseDistributionByCycle", "ALIGNED_READS_ONLY")
            | ("CollectBaseDistributionByCycle", "PF_READS_ONLY") => {}
            _ => {
                return Err(format!(
                    "unsupported CollectMultipleMetrics EXTRA_ARGUMENT={value}"
                ));
            }
        }
    }
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "FILE_EXTENSION")?;
    optional_u32(args, "SCAN_WINDOW_SIZE")?;
    optional_f64(args, "MINIMUM_GENOME_FRACTION")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    optional_bool(args, "USE_FAST_ALGORITHM")?;
    optional_u32(args, "SAMPLE_SIZE")?;
    optional_bool(args, "INCLUDE_BQ_HISTOGRAM")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    let programs = collectmultiplemetrics_programs(args)?;
    if let Some(level) = optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")? {
        if !matches!(
            level.as_str(),
            "ALL_READS" | "SAMPLE" | "LIBRARY" | "READ_GROUP"
        ) {
            return Err(format!(
                "unsupported CollectMultipleMetrics METRIC_ACCUMULATION_LEVEL={level}"
            ));
        }
        if level != "ALL_READS"
            && programs.iter().any(|program| {
                program != "CollectInsertSizeMetrics" && program != "CollectAlignmentSummaryMetrics"
            })
        {
            return Err(format!(
                "unsupported CollectMultipleMetrics METRIC_ACCUMULATION_LEVEL={level} for selected PROGRAM values"
            ));
        }
    }
    optional_u32(args, "STOP_AFTER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectMultipleMetrics COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn collectmultiplemetrics_programs(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<Vec<String>, String> {
    if !args.contains_key("PROGRAM") {
        return Ok(default_collectmultiplemetrics_programs());
    }
    let mut programs = Vec::new();
    for value in args.get("PROGRAM").into_iter().flatten() {
        if value.eq_ignore_ascii_case("null") {
            programs.clear();
            continue;
        }
        match value.as_str() {
            "CollectAlignmentSummaryMetrics"
            | "CollectBaseDistributionByCycle"
            | "CollectGcBiasMetrics"
            | "CollectInsertSizeMetrics"
            | "QualityScoreDistribution"
            | "MeanQualityByCycle"
            | "CollectQualityYieldMetrics"
            | "CollectWgsMetrics" => programs.push(value.clone()),
            _ => {
                return Err(format!(
                    "unsupported CollectMultipleMetrics PROGRAM={value}"
                ));
            }
        }
    }
    if programs.is_empty() {
        return Err("unsupported CollectMultipleMetrics empty PROGRAM set".to_string());
    }
    Ok(programs)
}

fn collectmultiplemetrics_extra_argument(
    args: &BTreeMap<String, Vec<String>>,
    program: &str,
    key: &str,
) -> Option<String> {
    let prefix = format!("{program}::{key}=");
    args.get("EXTRA_ARGUMENT")
        .into_iter()
        .flatten()
        .find_map(|value| value.strip_prefix(&prefix).map(ToString::to_string))
}

fn extend_collectmultiplemetrics_extra_arguments(
    args: &BTreeMap<String, Vec<String>>,
    program: &str,
    keys: &[&str],
    child_args: &mut Vec<String>,
) {
    for key in keys {
        if let Some(value) = collectmultiplemetrics_extra_argument(args, program, key) {
            child_args.push(format!("{key}={value}"));
        }
    }
}

fn default_collectmultiplemetrics_programs() -> Vec<String> {
    [
        "CollectAlignmentSummaryMetrics",
        "CollectBaseDistributionByCycle",
        "CollectInsertSizeMetrics",
        "MeanQualityByCycle",
        "QualityScoreDistribution",
    ]
    .into_iter()
    .map(ToString::to_string)
    .collect()
}

fn reject_unsupported_collectbasedistributionbycycle_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "CHART_OUTPUT",
        "REFERENCE_SEQUENCE",
        "ALIGNED_READS_ONLY",
        "PF_READS_ONLY",
        "ASSUME_SORTED",
        "STOP_AFTER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported CollectBaseDistributionByCycle argument: {key}"
            ));
        }
    }
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "CHART_OUTPUT")?;
    optional_bool(args, "ALIGNED_READS_ONLY")?;
    optional_bool(args, "PF_READS_ONLY")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_u32(args, "STOP_AFTER")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectBaseDistributionByCycle COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_collectgcbiasmetrics_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SUMMARY_OUTPUT",
        "CHART_OUTPUT",
        "REFERENCE_SEQUENCE",
        "SCAN_WINDOW_SIZE",
        "MINIMUM_GENOME_FRACTION",
        "ALSO_IGNORE_DUPLICATES",
        "IS_BISULFITE_SEQUENCED",
        "METRIC_ACCUMULATION_LEVEL",
        "ASSUME_SORTED",
        "STOP_AFTER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported CollectGcBiasMetrics argument: {key}"));
        }
    }
    optional_scalar(args, "CHART_OUTPUT")?;
    optional_scalar(args, "SUMMARY_OUTPUT")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_bool(args, "ALSO_IGNORE_DUPLICATES")?;
    if optional_bool(args, "IS_BISULFITE_SEQUENCED")?.unwrap_or(false) {
        return Err("unsupported CollectGcBiasMetrics IS_BISULFITE_SEQUENCED=true".to_string());
    }
    if let Some(level) = optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")? {
        if level != "ALL_READS" {
            return Err(format!(
                "unsupported CollectGcBiasMetrics METRIC_ACCUMULATION_LEVEL={level}"
            ));
        }
    }
    let window_size = optional_u32(args, "SCAN_WINDOW_SIZE")?.unwrap_or(100);
    if window_size == 0 {
        return Err("unsupported CollectGcBiasMetrics SCAN_WINDOW_SIZE=0".to_string());
    }
    optional_f64(args, "MINIMUM_GENOME_FRACTION")?;
    optional_u32(args, "STOP_AFTER")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectGcBiasMetrics COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_collecthsmetrics_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "BAIT_INTERVALS",
        "TARGET_INTERVALS",
        "REFERENCE_SEQUENCE",
        "PER_TARGET_COVERAGE",
        "PER_BASE_REPORT",
        "CLIP_OVERLAPPING_READS",
        "NEAR_DISTANCE",
        "METRIC_ACCUMULATION_LEVEL",
        "ASSUME_SORTED",
        "STOP_AFTER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported CollectHsMetrics argument: {key}"));
        }
    }
    optional_scalar(args, "PER_TARGET_COVERAGE")?;
    optional_scalar(args, "PER_BASE_REPORT")?;
    optional_bool(args, "CLIP_OVERLAPPING_READS")?;
    optional_u32(args, "NEAR_DISTANCE")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_u32(args, "STOP_AFTER")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")? {
        if level != "ALL_READS" {
            return Err(format!(
                "unsupported CollectHsMetrics METRIC_ACCUMULATION_LEVEL={level}"
            ));
        }
    }
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectHsMetrics COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_collectwgsmetrics_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "REFERENCE_SEQUENCE",
        "MINIMUM_MAPPING_QUALITY",
        "MINIMUM_BASE_QUALITY",
        "COVERAGE_CAP",
        "LOCUS_ACCUMULATION_CAP",
        "STOP_AFTER",
        "COUNT_UNPAIRED",
        "SAMPLE_SIZE",
        "INCLUDE_BQ_HISTOGRAM",
        "INTERVALS",
        "INTERVAL_MERGING_RULE",
        "USE_FAST_ALGORITHM",
        "READ_LENGTH",
        "ALLELE_FRACTION",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported CollectWgsMetrics argument: {key}"));
        }
    }
    let _ = args.get("INTERVALS");
    optional_scalar(args, "INTERVAL_MERGING_RULE")?;
    optional_bool(args, "INCLUDE_BQ_HISTOGRAM")?;
    optional_bool(args, "USE_FAST_ALGORITHM")?;
    optional_bool(args, "COUNT_UNPAIRED")?;
    optional_u32(args, "MINIMUM_MAPPING_QUALITY")?;
    optional_u32(args, "MINIMUM_BASE_QUALITY")?;
    optional_u32(args, "COVERAGE_CAP")?;
    optional_u32(args, "LOCUS_ACCUMULATION_CAP")?;
    optional_i64(args, "STOP_AFTER")?;
    optional_u32(args, "SAMPLE_SIZE")?;
    optional_scalar(args, "READ_LENGTH")?;
    optional_scalar(args, "ALLELE_FRACTION")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectWgsMetrics COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_fixmateinformation_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "ADD_MATE_CIGAR",
        "ASSUME_SORTED",
        "SORT_ORDER",
        "IGNORE_MISSING_MATES",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "CREATE_MD5_FILE",
        "CREATE_INDEX",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported FixMateInformation argument: {key}"));
        }
    }
    required_values_for(args, "INPUT", "FixMateInformation")?;
    if let Some(sort_order) = optional_scalar(args, "SORT_ORDER")? {
        if sort_order != "queryname" && sort_order != "coordinate" && sort_order != "unsorted" {
            return Err(format!(
                "unsupported FixMateInformation SORT_ORDER={sort_order}"
            ));
        }
    }
    optional_bool(args, "ADD_MATE_CIGAR")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_bool(args, "IGNORE_MISSING_MATES")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "CREATE_INDEX")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported FixMateInformation COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_qualityscoredistribution_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "CHART_OUTPUT",
        "REFERENCE_SEQUENCE",
        "ALIGNED_READS_ONLY",
        "PF_READS_ONLY",
        "INCLUDE_NO_CALLS",
        "ASSUME_SORTED",
        "STOP_AFTER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported QualityScoreDistribution argument: {key}"
            ));
        }
    }

    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "CHART_OUTPUT")?;
    optional_bool(args, "ALIGNED_READS_ONLY")?;
    optional_bool(args, "PF_READS_ONLY")?;
    optional_bool(args, "INCLUDE_NO_CALLS")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_u32(args, "STOP_AFTER")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported QualityScoreDistribution COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_meanqualitybycycle_args(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "CHART_OUTPUT",
        "REFERENCE_SEQUENCE",
        "ALIGNED_READS_ONLY",
        "PF_READS_ONLY",
        "ASSUME_SORTED",
        "STOP_AFTER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported MeanQualityByCycle argument: {key}"));
        }
    }

    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "CHART_OUTPUT")?;
    optional_bool(args, "ALIGNED_READS_ONLY")?;
    optional_bool(args, "PF_READS_ONLY")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_u32(args, "STOP_AFTER")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported MeanQualityByCycle COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct AlignmentSummarySet {
    unpaired: AlignmentSummary,
    first: AlignmentSummary,
    second: AlignmentSummary,
    pair: AlignmentSummary,
    saw_paired: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AlignmentAccumulation {
    AllReads,
    Sample,
    Library,
    ReadGroup,
}

fn alignment_accumulation_level(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<AlignmentAccumulation, String> {
    match optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")?
        .unwrap_or_else(|| "ALL_READS".to_string())
        .as_str()
    {
        "ALL_READS" => Ok(AlignmentAccumulation::AllReads),
        "SAMPLE" => Ok(AlignmentAccumulation::Sample),
        "LIBRARY" => Ok(AlignmentAccumulation::Library),
        "READ_GROUP" => Ok(AlignmentAccumulation::ReadGroup),
        value => Err(format!(
            "unsupported CollectAlignmentSummaryMetrics METRIC_ACCUMULATION_LEVEL={value}"
        )),
    }
}

#[derive(Debug)]
struct AlignmentSummaryCollection {
    accumulation: AlignmentAccumulation,
    all_reads: AlignmentSummarySet,
    samples: BTreeMap<String, AlignmentSummarySet>,
    libraries: BTreeMap<String, AlignmentSummaryLibrary>,
    read_groups: BTreeMap<String, AlignmentSummaryReadGroup>,
}

#[derive(Debug)]
struct AlignmentSummaryLibrary {
    sample: String,
    summary: AlignmentSummarySet,
}

#[derive(Debug)]
struct AlignmentSummaryReadGroup {
    sample: String,
    library: String,
    summary: AlignmentSummarySet,
}

impl AlignmentSummaryCollection {
    fn new(accumulation: AlignmentAccumulation) -> Self {
        Self {
            accumulation,
            all_reads: AlignmentSummarySet::default(),
            samples: BTreeMap::new(),
            libraries: BTreeMap::new(),
            read_groups: BTreeMap::new(),
        }
    }

    fn observe(&mut self, record: &bam::Record, read_group: Option<&InsertSizeReadGroup>) {
        self.all_reads.observe(record);
        if self.accumulation == AlignmentAccumulation::Sample {
            if let Some(read_group) = read_group {
                self.samples
                    .entry(read_group.sample.clone())
                    .or_default()
                    .observe(record);
            }
        } else if self.accumulation == AlignmentAccumulation::Library {
            if let Some(read_group) = read_group {
                self.libraries
                    .entry(read_group.library.clone())
                    .or_insert_with(|| AlignmentSummaryLibrary {
                        sample: read_group.sample.clone(),
                        summary: AlignmentSummarySet::default(),
                    })
                    .summary
                    .observe(record);
            }
        } else if self.accumulation == AlignmentAccumulation::ReadGroup {
            if let Some(read_group) = read_group {
                self.read_groups
                    .entry(read_group.platform_unit.clone())
                    .or_insert_with(|| AlignmentSummaryReadGroup {
                        sample: read_group.sample.clone(),
                        library: read_group.library.clone(),
                        summary: AlignmentSummarySet::default(),
                    })
                    .summary
                    .observe(record);
            }
        }
    }

    fn observe_sam_parts(
        &mut self,
        flags: u16,
        read_length: u64,
        sequence_bases: &[u8],
        aligned_length: u64,
        mapq: u8,
        qualities: &[u8],
        cigar: CigarSummary,
        chimeric: bool,
        read_group: Option<&InsertSizeReadGroup>,
    ) {
        self.all_reads.observe_sam_parts(
            flags,
            read_length,
            sequence_bases,
            aligned_length,
            mapq,
            qualities,
            cigar,
            chimeric,
        );
        if self.accumulation == AlignmentAccumulation::Sample {
            if let Some(read_group) = read_group {
                self.samples
                    .entry(read_group.sample.clone())
                    .or_default()
                    .observe_sam_parts(
                        flags,
                        read_length,
                        sequence_bases,
                        aligned_length,
                        mapq,
                        qualities,
                        cigar,
                        chimeric,
                    );
            }
        } else if self.accumulation == AlignmentAccumulation::Library {
            if let Some(read_group) = read_group {
                self.libraries
                    .entry(read_group.library.clone())
                    .or_insert_with(|| AlignmentSummaryLibrary {
                        sample: read_group.sample.clone(),
                        summary: AlignmentSummarySet::default(),
                    })
                    .summary
                    .observe_sam_parts(
                        flags,
                        read_length,
                        sequence_bases,
                        aligned_length,
                        mapq,
                        qualities,
                        cigar,
                        chimeric,
                    );
            }
        } else if self.accumulation == AlignmentAccumulation::ReadGroup {
            if let Some(read_group) = read_group {
                self.read_groups
                    .entry(read_group.platform_unit.clone())
                    .or_insert_with(|| AlignmentSummaryReadGroup {
                        sample: read_group.sample.clone(),
                        library: read_group.library.clone(),
                        summary: AlignmentSummarySet::default(),
                    })
                    .summary
                    .observe_sam_parts(
                        flags,
                        read_length,
                        sequence_bases,
                        aligned_length,
                        mapq,
                        qualities,
                        cigar,
                        chimeric,
                    );
            }
        }
    }

    fn to_picard_text(&self) -> String {
        let mut rows = self.all_reads.picard_rows(None, None, None);
        if self.accumulation == AlignmentAccumulation::Sample {
            for (sample, summary) in &self.samples {
                rows.extend(summary.picard_rows(Some(sample), None, None));
            }
        } else if self.accumulation == AlignmentAccumulation::Library {
            for (library, summary) in &self.libraries {
                rows.extend(summary.summary.picard_rows(
                    Some(&summary.sample),
                    Some(library),
                    None,
                ));
            }
        } else if self.accumulation == AlignmentAccumulation::ReadGroup {
            for (read_group, summary) in &self.read_groups {
                rows.extend(summary.summary.picard_rows(
                    Some(&summary.sample),
                    Some(&summary.library),
                    Some(read_group),
                ));
            }
        }
        AlignmentSummary::to_picard_text_for_rows(&rows, &self.all_reads.histogram_summary())
    }
}

impl AlignmentSummarySet {
    fn observe(&mut self, record: &bam::Record) {
        if record.is_secondary() || record.is_supplementary() {
            return;
        }
        if record.is_paired() {
            self.saw_paired = true;
            if record.is_first_in_template() {
                self.first.observe(record);
            } else if record.is_last_in_template() {
                self.second.observe(record);
            }
            self.pair.observe(record);
        } else {
            self.unpaired.observe(record);
        }
    }

    fn observe_sam_parts(
        &mut self,
        flags: u16,
        read_length: u64,
        sequence_bases: &[u8],
        aligned_length: u64,
        mapq: u8,
        qualities: &[u8],
        cigar: CigarSummary,
        chimeric: bool,
    ) {
        if flags & (0x100 | 0x800) != 0 {
            return;
        }
        if flags & 0x1 != 0 {
            self.saw_paired = true;
            if flags & 0x40 != 0 {
                self.first.observe_sam_parts(
                    flags,
                    read_length,
                    sequence_bases,
                    aligned_length,
                    mapq,
                    qualities,
                    cigar,
                    chimeric,
                );
            } else if flags & 0x80 != 0 {
                self.second.observe_sam_parts(
                    flags,
                    read_length,
                    sequence_bases,
                    aligned_length,
                    mapq,
                    qualities,
                    cigar,
                    chimeric,
                );
            }
            self.pair.observe_sam_parts(
                flags,
                read_length,
                sequence_bases,
                aligned_length,
                mapq,
                qualities,
                cigar,
                chimeric,
            );
        } else {
            self.unpaired.observe_sam_parts(
                flags,
                read_length,
                sequence_bases,
                aligned_length,
                mapq,
                qualities,
                cigar,
                chimeric,
            );
        }
    }

    fn picard_rows<'a>(
        &'a self,
        sample: Option<&'a str>,
        library: Option<&'a str>,
        read_group: Option<&'a str>,
    ) -> Vec<AlignmentSummaryRow<'a>> {
        if self.saw_paired {
            vec![
                AlignmentSummaryRow {
                    category: "FIRST_OF_PAIR",
                    summary: &self.first,
                    bad_cycles_override: None,
                    sample,
                    library,
                    read_group,
                },
                AlignmentSummaryRow {
                    category: "SECOND_OF_PAIR",
                    summary: &self.second,
                    bad_cycles_override: None,
                    sample,
                    library,
                    read_group,
                },
                AlignmentSummaryRow {
                    category: "PAIR",
                    summary: &self.pair,
                    bad_cycles_override: Some(self.first.bad_cycles() + self.second.bad_cycles()),
                    sample,
                    library,
                    read_group,
                },
            ]
        } else {
            vec![AlignmentSummaryRow {
                category: "UNPAIRED",
                summary: &self.unpaired,
                bad_cycles_override: None,
                sample,
                library,
                read_group,
            }]
        }
    }

    fn histogram_summary(&self) -> &AlignmentSummary {
        if self.saw_paired {
            &self.pair
        } else {
            &self.unpaired
        }
    }
}

struct AlignmentSummaryRow<'a> {
    category: &'static str,
    summary: &'a AlignmentSummary,
    bad_cycles_override: Option<u64>,
    sample: Option<&'a str>,
    library: Option<&'a str>,
    read_group: Option<&'a str>,
}

#[derive(Debug, Default)]
struct AlignmentSummary {
    total_reads: u64,
    pf_reads: u64,
    pf_read_bases: u64,
    pf_noise_reads: u64,
    pf_reads_aligned: u64,
    pf_aligned_bases: u64,
    pf_read_aligned_bases: u64,
    pf_hq_aligned_reads: u64,
    pf_hq_aligned_bases: u64,
    pf_hq_aligned_q20_bases: u64,
    reads_aligned_in_pairs: u64,
    pf_reads_improper_pairs: u64,
    forward_aligned_reads: u64,
    reverse_aligned_reads: u64,
    chimeras: u64,
    adapter_reads: u64,
    indel_bases: u64,
    soft_clip_bases: u64,
    hard_clip_bases: u64,
    three_prime_soft_clip_bases: u64,
    three_prime_soft_clip_reads: u64,
    cycle_bases: Vec<u64>,
    cycle_no_calls: Vec<u64>,
    total_read_lengths: Vec<u64>,
    aligned_read_lengths: Vec<u64>,
}

impl AlignmentSummary {
    fn observe(&mut self, record: &bam::Record) {
        let read_length = record.seq_len() as u64;
        let cigar = alignment_cigar_summary(record.cigar().iter(), record.is_reverse());
        let aligned_length = if record.is_unmapped() {
            0
        } else {
            cigar.aligned_length
        };
        let aligned_read_length = if record.is_unmapped() {
            0
        } else {
            cigar.read_aligned_length
        };
        self.total_reads += 1;
        ensure_histogram_len(&mut self.total_read_lengths, read_length as usize);
        self.total_read_lengths[read_length as usize] += 1;

        if record.is_quality_check_failed() {
            return;
        }

        self.pf_reads += 1;
        self.pf_read_bases += read_length;
        if is_noise_read(record) {
            self.pf_noise_reads += 1;
        }
        let sequence_bases = record.seq().as_bytes();
        self.observe_bad_cycle_bases(&sequence_bases);
        if is_adapter_read(
            &sequence_bases,
            record.is_unmapped(),
            record.mapq(),
            record.is_reverse(),
        ) {
            self.adapter_reads += 1;
        }

        let is_aligned = !record.is_unmapped();
        if is_aligned {
            self.pf_reads_aligned += 1;
            self.pf_aligned_bases += aligned_length;
            self.pf_read_aligned_bases += aligned_read_length;
            if is_hq_aligned(record) {
                self.pf_hq_aligned_reads += 1;
                self.pf_hq_aligned_bases += aligned_length;
                self.pf_hq_aligned_q20_bases +=
                    q20_match_bases(record.cigar().iter(), record.qual());
            }
            if record.is_reverse() {
                self.reverse_aligned_reads += 1;
            } else {
                self.forward_aligned_reads += 1;
            }
            if record.is_paired() {
                if record.is_mate_unmapped() {
                    self.pf_reads_improper_pairs += 1;
                    if is_chimeric_bam_record(record) {
                        self.chimeras += 1;
                    }
                } else {
                    self.reads_aligned_in_pairs += 1;
                    if !record.is_proper_pair() {
                        self.pf_reads_improper_pairs += 1;
                    }
                    if is_chimeric_bam_record(record) {
                        self.chimeras += 1;
                    }
                }
            }
            self.observe_cigar_summary(cigar);
        }

        ensure_histogram_len(&mut self.aligned_read_lengths, aligned_read_length as usize);
        self.aligned_read_lengths[aligned_read_length as usize] += 1;
    }

    fn observe_sam_parts(
        &mut self,
        flags: u16,
        read_length: u64,
        sequence_bases: &[u8],
        aligned_length: u64,
        mapq: u8,
        _qualities: &[u8],
        cigar: CigarSummary,
        chimeric: bool,
    ) {
        self.total_reads += 1;
        ensure_histogram_len(&mut self.total_read_lengths, read_length as usize);
        self.total_read_lengths[read_length as usize] += 1;

        if flags & 0x200 != 0 {
            return;
        }

        self.pf_reads += 1;
        self.pf_read_bases += read_length;
        self.observe_bad_cycle_bases(sequence_bases);
        if is_adapter_read(sequence_bases, flags & 0x4 != 0, mapq, flags & 0x10 != 0) {
            self.adapter_reads += 1;
        }
        let is_aligned = flags & 0x4 == 0;
        if is_aligned {
            self.pf_reads_aligned += 1;
            self.pf_aligned_bases += aligned_length;
            self.pf_read_aligned_bases += cigar.read_aligned_length;
            if mapq >= 20 {
                self.pf_hq_aligned_reads += 1;
                self.pf_hq_aligned_bases += aligned_length;
                self.pf_hq_aligned_q20_bases += cigar.q20_match_bases;
            }
            if flags & 0x10 != 0 {
                self.reverse_aligned_reads += 1;
            } else {
                self.forward_aligned_reads += 1;
            }
            if flags & 0x1 != 0 {
                if flags & 0x8 != 0 {
                    self.pf_reads_improper_pairs += 1;
                    if chimeric {
                        self.chimeras += 1;
                    }
                } else {
                    self.reads_aligned_in_pairs += 1;
                    if flags & 0x2 == 0 {
                        self.pf_reads_improper_pairs += 1;
                    }
                    if chimeric {
                        self.chimeras += 1;
                    }
                }
            }
            self.observe_cigar_summary(cigar);
        }

        ensure_histogram_len(
            &mut self.aligned_read_lengths,
            cigar.read_aligned_length as usize,
        );
        self.aligned_read_lengths[cigar.read_aligned_length as usize] += 1;
    }

    fn observe_cigar_summary(&mut self, cigar: CigarSummary) {
        self.indel_bases += cigar.indel_events;
        self.soft_clip_bases += cigar.soft_clip_bases;
        self.hard_clip_bases += cigar.hard_clip_bases;
        if cigar.three_prime_soft_clip_bases > 0 {
            self.three_prime_soft_clip_bases += cigar.three_prime_soft_clip_bases;
            self.three_prime_soft_clip_reads += 1;
        }
    }

    fn observe_bad_cycle_bases(&mut self, bases: &[u8]) {
        if bases.is_empty() {
            return;
        }
        let last = bases.len() - 1;
        ensure_histogram_len(&mut self.cycle_bases, last);
        ensure_histogram_len(&mut self.cycle_no_calls, last);
        for (index, base) in bases.iter().enumerate() {
            self.cycle_bases[index] += 1;
            if base.eq_ignore_ascii_case(&b'N') {
                self.cycle_no_calls[index] += 1;
            }
        }
    }

    fn bad_cycles(&self) -> u64 {
        self.cycle_bases
            .iter()
            .enumerate()
            .filter(|(index, bases)| {
                **bases > 0
                    && self.cycle_no_calls.get(*index).copied().unwrap_or_default() * 5
                        >= **bases * 4
            })
            .count() as u64
    }

    fn to_picard_row(
        &self,
        category: &str,
        bad_cycles_override: Option<u64>,
        sample: Option<&str>,
        library: Option<&str>,
        read_group: Option<&str>,
    ) -> String {
        let mean_read_length = mean_from_histogram(&self.total_read_lengths);
        let sd_read_length = if self.total_reads < 2 {
            "?".to_string()
        } else {
            format_float(standard_deviation_from_histogram(&self.total_read_lengths))
        };
        let median_read_length = median_from_histogram(&self.total_read_lengths);
        let mad_read_length = mad_from_histogram(&self.total_read_lengths, median_read_length);
        let min_read_length = min_from_histogram(&self.total_read_lengths);
        let max_read_length = max_from_histogram(&self.total_read_lengths);
        let mean_aligned_read_length = mean_from_histogram(&self.aligned_read_lengths);
        let aligned_reads = self.forward_aligned_reads + self.reverse_aligned_reads;
        let strand_balance = if aligned_reads == 0 {
            0.0
        } else {
            self.forward_aligned_reads as f64 / aligned_reads as f64
        };
        let avg_three_prime_soft_clip = if self.three_prime_soft_clip_reads == 0 {
            0.0
        } else {
            self.three_prime_soft_clip_bases as f64 / self.three_prime_soft_clip_reads as f64
        };
        let chimera_denominator = if self.chimeras > 0 && self.pf_reads_aligned > 100 {
            let adjustment = if category == "PAIR" { 2 } else { 1 };
            self.pf_reads_aligned.saturating_sub(adjustment)
        } else {
            self.pf_reads_aligned
        };

        format!(
            "{category}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t0\t0\t0\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.total_reads,
            self.pf_reads,
            format_float(ratio(self.pf_reads, self.total_reads)),
            self.pf_noise_reads,
            self.pf_reads_aligned,
            format_float(ratio(self.pf_reads_aligned, self.pf_reads)),
            self.pf_aligned_bases,
            self.pf_hq_aligned_reads,
            self.pf_hq_aligned_bases,
            self.pf_hq_aligned_q20_bases,
            format_float(ratio(self.indel_bases, self.pf_aligned_bases)),
            format_float(mean_read_length),
            sd_read_length,
            median_read_length,
            mad_read_length,
            min_read_length,
            max_read_length,
            format_float(mean_aligned_read_length),
            self.reads_aligned_in_pairs,
            format_float(ratio(self.reads_aligned_in_pairs, self.pf_reads_aligned)),
            self.pf_reads_improper_pairs,
            format_float(ratio(self.pf_reads_improper_pairs, self.pf_reads_aligned)),
            bad_cycles_override.unwrap_or_else(|| self.bad_cycles()),
            format_float(strand_balance),
            format_float(ratio(self.chimeras, chimera_denominator)),
            format_float(ratio(self.adapter_reads, self.pf_reads)),
            format_float(ratio(self.soft_clip_bases, self.pf_read_bases)),
            format_float(ratio(self.hard_clip_bases, self.pf_read_bases)),
            format_float(avg_three_prime_soft_clip),
            sample.unwrap_or_default(),
            library.unwrap_or_default(),
            read_group.unwrap_or_default(),
        )
    }

    fn to_picard_text_for_rows(
        rows: &[AlignmentSummaryRow<'_>],
        histogram_summary: &AlignmentSummary,
    ) -> String {
        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.AlignmentSummaryMetrics\n");
        output.push_str("CATEGORY\tTOTAL_READS\tPF_READS\tPCT_PF_READS\tPF_NOISE_READS\tPF_READS_ALIGNED\tPCT_PF_READS_ALIGNED\tPF_ALIGNED_BASES\tPF_HQ_ALIGNED_READS\tPF_HQ_ALIGNED_BASES\tPF_HQ_ALIGNED_Q20_BASES\tPF_HQ_MEDIAN_MISMATCHES\tPF_MISMATCH_RATE\tPF_HQ_ERROR_RATE\tPF_INDEL_RATE\tMEAN_READ_LENGTH\tSD_READ_LENGTH\tMEDIAN_READ_LENGTH\tMAD_READ_LENGTH\tMIN_READ_LENGTH\tMAX_READ_LENGTH\tMEAN_ALIGNED_READ_LENGTH\tREADS_ALIGNED_IN_PAIRS\tPCT_READS_ALIGNED_IN_PAIRS\tPF_READS_IMPROPER_PAIRS\tPCT_PF_READS_IMPROPER_PAIRS\tBAD_CYCLES\tSTRAND_BALANCE\tPCT_CHIMERAS\tPCT_ADAPTER\tPCT_SOFTCLIP\tPCT_HARDCLIP\tAVG_POS_3PRIME_SOFTCLIP_LENGTH\tSAMPLE\tLIBRARY\tREAD_GROUP\n");
        for row in rows {
            output.push_str(&row.summary.to_picard_row(
                row.category,
                row.bad_cycles_override,
                row.sample,
                row.library,
                row.read_group,
            ));
        }
        output.push('\n');
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        if rows
            .first()
            .is_some_and(|row| row.category == "FIRST_OF_PAIR")
        {
            output
                .push_str("READ_LENGTH\tPAIRED_TOTAL_LENGTH_COUNT\tPAIRED_ALIGNED_LENGTH_COUNT\n");
        } else {
            output.push_str(
                "READ_LENGTH\tUNPAIRED_TOTAL_LENGTH_COUNT\tUNPAIRED_ALIGNED_LENGTH_COUNT\n",
            );
        }
        let max_len = histogram_summary
            .total_read_lengths
            .len()
            .max(histogram_summary.aligned_read_lengths.len());
        for index in 0..max_len {
            let total = histogram_summary
                .total_read_lengths
                .get(index)
                .copied()
                .unwrap_or(0);
            let aligned = histogram_summary
                .aligned_read_lengths
                .get(index)
                .copied()
                .unwrap_or(0);
            if total != 0 || aligned != 0 {
                output.push_str(&format!("{index}\t{total}\t{aligned}\n"));
            }
        }
        output
    }
}

#[derive(Debug, Clone, Copy, Default)]
struct CigarSummary {
    aligned_length: u64,
    read_aligned_length: u64,
    indel_events: u64,
    soft_clip_bases: u64,
    hard_clip_bases: u64,
    three_prime_soft_clip_bases: u64,
    q20_match_bases: u64,
}

fn alignment_cigar_summary<'a>(
    cigars: impl Iterator<Item = &'a Cigar>,
    is_reverse: bool,
) -> CigarSummary {
    let mut summary = CigarSummary::default();
    let mut first_soft_clip = 0;
    let mut last_soft_clip = 0;
    let mut seen_operator = false;
    for cigar in cigars {
        match cigar {
            Cigar::Match(len) | Cigar::Equal(len) | Cigar::Diff(len) => {
                summary.aligned_length += u64::from(*len);
                summary.read_aligned_length += u64::from(*len);
                last_soft_clip = 0;
            }
            Cigar::Ins(len) => {
                summary.read_aligned_length += u64::from(*len);
                summary.indel_events += 1;
                last_soft_clip = 0;
            }
            Cigar::Del(len) => {
                let _ = len;
                summary.indel_events += 1;
                last_soft_clip = 0;
            }
            Cigar::SoftClip(len) => {
                summary.soft_clip_bases += u64::from(*len);
                if !seen_operator {
                    first_soft_clip = u64::from(*len);
                }
                last_soft_clip = u64::from(*len);
            }
            Cigar::HardClip(len) => {
                summary.hard_clip_bases += u64::from(*len);
                last_soft_clip = 0;
            }
            Cigar::RefSkip(_) | Cigar::Pad(_) => {
                last_soft_clip = 0;
            }
        }
        seen_operator = true;
    }
    summary.three_prime_soft_clip_bases = if is_reverse {
        first_soft_clip
    } else {
        last_soft_clip
    };
    summary
}

fn q20_match_bases<'a>(cigars: impl Iterator<Item = &'a Cigar>, qualities: &[u8]) -> u64 {
    let mut read_position = 0_usize;
    let mut q20 = 0_u64;
    for cigar in cigars {
        match cigar {
            Cigar::Match(len) | Cigar::Equal(len) | Cigar::Diff(len) => {
                let len = *len as usize;
                q20 += qualities
                    .get(read_position..read_position.saturating_add(len))
                    .unwrap_or_default()
                    .iter()
                    .filter(|quality| **quality >= 20)
                    .count() as u64;
                read_position = read_position.saturating_add(len);
            }
            Cigar::Ins(len) | Cigar::SoftClip(len) => {
                read_position = read_position.saturating_add(*len as usize);
            }
            Cigar::Del(_) | Cigar::RefSkip(_) | Cigar::HardClip(_) | Cigar::Pad(_) => {}
        }
    }
    q20
}

fn q20_match_bases_from_sam(cigar: &[u8], qualities: &[u8]) -> Result<u64, String> {
    if cigar == b"*" || qualities.is_empty() {
        return Ok(0);
    }
    let mut q20 = 0_u64;
    let mut len = 0_usize;
    let mut saw_digit = false;
    let mut read_position = 0_usize;
    for byte in cigar {
        if byte.is_ascii_digit() {
            saw_digit = true;
            len = len
                .checked_mul(10)
                .and_then(|value| value.checked_add(usize::from(byte - b'0')))
                .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
            continue;
        }
        if !saw_digit || len == 0 {
            return Err("malformed CollectAlignmentSummaryMetrics CIGAR".to_string());
        }
        match *byte {
            b'M' | b'=' | b'X' => {
                q20 += qualities
                    .get(read_position..read_position.saturating_add(len))
                    .unwrap_or_default()
                    .iter()
                    .filter(|quality| **quality >= b'5')
                    .count() as u64;
                read_position = read_position.saturating_add(len);
            }
            b'I' | b'S' => {
                read_position = read_position.saturating_add(len);
            }
            b'D' | b'N' | b'H' | b'P' => {}
            _ => return Err("malformed CollectAlignmentSummaryMetrics CIGAR".to_string()),
        }
        len = 0;
        saw_digit = false;
    }
    if saw_digit {
        return Err("malformed CollectAlignmentSummaryMetrics CIGAR".to_string());
    }
    Ok(q20)
}

fn is_chimeric_bam_record(record: &bam::Record) -> bool {
    if !record.is_paired() || record.is_unmapped() {
        return false;
    }
    if record.aux(b"SA").is_ok() {
        return true;
    }
    if record.is_mate_unmapped() {
        return false;
    }
    record.tid() != record.mtid()
        || record.insert_size().unsigned_abs() > 100_000
        || !is_expected_fr_pair(
            record.is_first_in_template(),
            record.is_reverse(),
            record.is_mate_reverse(),
            record.insert_size(),
        )
}

fn is_expected_fr_pair(
    _first_in_pair: bool,
    read_reverse: bool,
    mate_reverse: bool,
    insert_size: i64,
) -> bool {
    read_reverse != mate_reverse
        && ((!read_reverse && insert_size > 0) || (read_reverse && insert_size < 0))
}

fn is_hq_aligned(record: &bam::Record) -> bool {
    !record.is_unmapped() && record.mapq() >= 20
}

fn is_noise_read(record: &bam::Record) -> bool {
    let _ = record;
    false
}

const ADAPTER_MATCH_LENGTH: usize = 16;
const MAX_ADAPTER_ERRORS: usize = 1;
const DEFAULT_ALIGNMENT_ADAPTERS: [&[u8]; 6] = [
    b"AATGATACGGCGACCACCGAGATCTACACTCTTTCCCTACACGACGCTCTTCCGATCT",
    b"AGATCGGAAGAGCTCGTATGCCGTCTTCTGCTTG",
    b"AATGATACGGCGACCACCGAGATCTACACTCTTTCCCTACACGACGCTCTTCCGATCT",
    b"AGATCGGAAGAGCGGTTCAGCAGGAATGCCGAGACCGATCTCGTATGCCGTCTTCTGCTTG",
    b"AATGATACGGCGACCACCGAGATCTACACTCTTTCCCTACACGACGCTCTTCCGATCT",
    b"AGATCGGAAGAGCACACGTCTGAACTCCAGTCACNNNNNNNNATCTCGTATGCCGTCTTCTGCTTG",
];

fn adapter_kmers() -> &'static Vec<[u8; ADAPTER_MATCH_LENGTH]> {
    static KMERS: OnceLock<Vec<[u8; ADAPTER_MATCH_LENGTH]>> = OnceLock::new();
    KMERS.get_or_init(|| {
        let mut kmers = BTreeSet::new();
        for adapter in DEFAULT_ALIGNMENT_ADAPTERS {
            if adapter.len() < ADAPTER_MATCH_LENGTH {
                continue;
            }
            for window in adapter.windows(ADAPTER_MATCH_LENGTH) {
                if window
                    .iter()
                    .filter(|base| base.eq_ignore_ascii_case(&b'N'))
                    .count()
                    > MAX_ADAPTER_ERRORS
                {
                    continue;
                }
                let mut kmer = [0_u8; ADAPTER_MATCH_LENGTH];
                for (index, base) in window.iter().enumerate() {
                    kmer[index] = base.to_ascii_uppercase();
                }
                kmers.insert(kmer);
                kmers.insert(reverse_complement_kmer(&kmer));
            }
        }
        kmers.into_iter().collect()
    })
}

fn reverse_complement_kmer(kmer: &[u8; ADAPTER_MATCH_LENGTH]) -> [u8; ADAPTER_MATCH_LENGTH] {
    let mut reversed = [0_u8; ADAPTER_MATCH_LENGTH];
    for (index, base) in kmer.iter().rev().enumerate() {
        reversed[index] = complement_base(*base);
    }
    reversed
}

fn complement_base(base: u8) -> u8 {
    match base.to_ascii_uppercase() {
        b'A' => b'T',
        b'C' => b'G',
        b'G' => b'C',
        b'T' => b'A',
        _ => b'N',
    }
}

fn is_adapter_read(read: &[u8], unmapped: bool, mapq: u8, reverse: bool) -> bool {
    if read.len() < ADAPTER_MATCH_LENGTH || (!unmapped && mapq != 0) {
        return false;
    }
    adapter_kmers().iter().any(|adapter| {
        let mut errors = 0;
        for index in 0..ADAPTER_MATCH_LENGTH {
            let base = if reverse && !unmapped {
                complement_base(read[read.len() - index - 1])
            } else {
                read[index].to_ascii_uppercase()
            };
            if base != adapter[index] {
                errors += 1;
                if errors > MAX_ADAPTER_ERRORS {
                    return false;
                }
            }
        }
        true
    })
}

fn collect_alignment_sam_text(
    input: &str,
    stop_after: u32,
    accumulation: AlignmentAccumulation,
) -> Result<AlignmentSummaryCollection, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut metrics = AlignmentSummaryCollection::new(accumulation);
    let mut read_groups = BTreeMap::new();
    let mut observed = 0_u32;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@RG\t") {
            observe_sam_insert_size_read_group(&mut read_groups, &line);
            continue;
        }
        if line.starts_with(b"@") || line.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        observe_alignment_sam_line(&mut metrics, &line, &read_groups)?;
        observed = observed.saturating_add(1);
        if stop_after > 0 && observed >= stop_after {
            break;
        }
    }
    Ok(metrics)
}

fn observe_alignment_sam_line(
    metrics: &mut AlignmentSummaryCollection,
    line: &[u8],
    read_groups: &BTreeMap<String, InsertSizeReadGroup>,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    fields
        .next()
        .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?,
    )?;
    let reference_name = fields
        .next()
        .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    fields
        .next()
        .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    let mapq = parse_u8_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?,
    )?;
    let cigar = fields
        .next()
        .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    let mate_reference_name = fields
        .next()
        .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    fields
        .next()
        .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    let template_length = parse_i64_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?,
    )?;
    let sequence = fields
        .next()
        .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    let qualities = fields
        .next()
        .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    let read_length = if sequence == b"*" {
        0
    } else {
        sequence.len() as u64
    };
    let mut cigar_summary = cigar_summary_from_sam(cigar, flags & 0x10 != 0)?;
    let aligned_length = if flags & 0x4 != 0 {
        0
    } else {
        cigar_summary.aligned_length
    };
    let qualities = if qualities == b"*" {
        &[][..]
    } else {
        qualities
    };
    cigar_summary.q20_match_bases = q20_match_bases_from_sam(cigar, qualities)?;
    let mut read_group = None;
    let mut has_sa = false;
    for tag in fields {
        if !has_sa && tag.starts_with(b"SA:") {
            has_sa = true;
        }
        if read_group.is_none() {
            read_group = insert_size_read_group_for_sam_tags(std::iter::once(tag), read_groups);
        }
    }
    let chimeric = is_chimeric_sam_record(
        flags,
        reference_name,
        mate_reference_name,
        template_length,
        has_sa,
    );
    metrics.observe_sam_parts(
        flags,
        read_length,
        if sequence == b"*" { &[][..] } else { sequence },
        aligned_length,
        mapq,
        qualities,
        cigar_summary,
        chimeric,
        read_group.as_ref(),
    );
    Ok(())
}

fn ensure_histogram_len(histogram: &mut Vec<u64>, index: usize) {
    if histogram.len() <= index {
        histogram.resize(index + 1, 0);
    }
}

fn ratio(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

fn mean_from_histogram(histogram: &[u64]) -> f64 {
    let total_count = histogram.iter().sum::<u64>();
    if total_count == 0 {
        return 0.0;
    }
    let total_bases = histogram
        .iter()
        .enumerate()
        .map(|(length, count)| length as u64 * count)
        .sum::<u64>();
    total_bases as f64 / total_count as f64
}

fn standard_deviation_from_histogram(histogram: &[u64]) -> f64 {
    let total_count = histogram.iter().sum::<u64>();
    if total_count < 2 {
        return 0.0;
    }
    let mean = mean_from_histogram(histogram);
    let variance = histogram
        .iter()
        .enumerate()
        .map(|(length, count)| {
            let delta = length as f64 - mean;
            delta * delta * *count as f64
        })
        .sum::<f64>()
        / (total_count - 1) as f64;
    variance.sqrt()
}

fn median_from_histogram(histogram: &[u64]) -> u64 {
    let total_count = histogram.iter().sum::<u64>();
    if total_count == 0 {
        return 0;
    }
    let target = (total_count - 1) / 2;
    let mut seen = 0;
    for (length, count) in histogram.iter().enumerate() {
        seen += count;
        if seen > target {
            return length as u64;
        }
    }
    0
}

fn mad_from_histogram(histogram: &[u64], median: u64) -> u64 {
    let total_count = histogram.iter().sum::<u64>();
    if total_count == 0 {
        return 0;
    }
    let target = (total_count - 1) / 2;
    let mut deviations = BTreeMap::<u64, u64>::new();
    for (length, count) in histogram.iter().enumerate() {
        let deviation = (length as i64 - median as i64).unsigned_abs();
        *deviations.entry(deviation).or_default() += count;
    }
    let mut seen = 0;
    for (deviation, count) in deviations {
        seen += count;
        if seen > target {
            return deviation;
        }
    }
    0
}

fn min_from_histogram(histogram: &[u64]) -> u64 {
    histogram
        .iter()
        .position(|count| *count > 0)
        .unwrap_or_default() as u64
}

fn max_from_histogram(histogram: &[u64]) -> u64 {
    histogram
        .iter()
        .rposition(|count| *count > 0)
        .unwrap_or_default() as u64
}

fn format_float(value: f64) -> String {
    if value == 0.0 {
        return "0".to_string();
    }
    if (value - value.round()).abs() < 0.0000005 {
        return format!("{}", value.round() as i64);
    }
    let formatted = format!("{value:.6}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

#[derive(Debug, Default)]
struct QualityYieldSummary {
    total_reads: u64,
    pf_reads: u64,
    total_bases: u64,
    pf_bases: u64,
    q20_bases: u64,
    pf_q20_bases: u64,
    q30_bases: u64,
    pf_q30_bases: u64,
    total_quality: u64,
    pf_quality: u64,
}

impl QualityYieldSummary {
    fn observe(
        &mut self,
        record: &bam::Record,
        use_original_qualities: bool,
        include_secondary: bool,
        include_supplemental: bool,
    ) {
        if record.is_secondary() && !include_secondary {
            return;
        }
        if record.is_supplementary() && !include_supplemental {
            return;
        }

        let qualities = quality_values(record, use_original_qualities);
        let is_pf = !record.is_quality_check_failed();
        self.total_reads += 1;
        self.total_bases += qualities.len() as u64;

        if is_pf {
            self.pf_reads += 1;
            self.pf_bases += qualities.len() as u64;
        }

        for quality in qualities {
            let quality = quality as u64;
            self.total_quality += quality;
            if quality >= 20 {
                self.q20_bases += 1;
            }
            if quality >= 30 {
                self.q30_bases += 1;
            }
            if is_pf {
                self.pf_quality += quality;
                if quality >= 20 {
                    self.pf_q20_bases += 1;
                }
                if quality >= 30 {
                    self.pf_q30_bases += 1;
                }
            }
        }
    }

    fn to_picard_text(&self) -> String {
        let read_length = if self.total_reads == 0 {
            0
        } else {
            self.total_bases / self.total_reads
        };
        format!(
            "## METRICS CLASS\tpicard.analysis.CollectQualityYieldMetrics$QualityYieldMetrics\n\
             TOTAL_READS\tPF_READS\tREAD_LENGTH\tTOTAL_BASES\tPF_BASES\tQ20_BASES\tPF_Q20_BASES\tQ30_BASES\tPF_Q30_BASES\tQ20_EQUIVALENT_YIELD\tPF_Q20_EQUIVALENT_YIELD\n\
             {}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            self.total_reads,
            self.pf_reads,
            read_length,
            self.total_bases,
            self.pf_bases,
            self.q20_bases,
            self.pf_q20_bases,
            self.q30_bases,
            self.pf_q30_bases,
            self.total_quality / 20,
            self.pf_quality / 20,
        )
    }
}

fn collect_quality_yield_sam_text(
    input: &str,
    use_original_qualities: bool,
    include_secondary: bool,
    include_supplemental: bool,
    stop_after: u32,
) -> Result<QualityYieldSummary, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut metrics = QualityYieldSummary::default();
    let mut observed = 0_u32;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@") || line.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        observe_quality_yield_sam_line(
            &mut metrics,
            &line,
            use_original_qualities,
            include_secondary,
            include_supplemental,
        )?;
        observed = observed.saturating_add(1);
        if stop_after > 0 && observed >= stop_after {
            break;
        }
    }
    Ok(metrics)
}

fn observe_quality_yield_sam_line(
    metrics: &mut QualityYieldSummary,
    line: &[u8],
    use_original_qualities: bool,
    include_secondary: bool,
    include_supplemental: bool,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    fields
        .next()
        .ok_or_else(|| "malformed CollectQualityYieldMetrics SAM record".to_string())?;
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed CollectQualityYieldMetrics SAM record".to_string())?,
    )?;
    if flags & 0x100 != 0 && !include_secondary {
        return Ok(());
    }
    if flags & 0x800 != 0 && !include_supplemental {
        return Ok(());
    }
    for _ in 0..8 {
        fields
            .next()
            .ok_or_else(|| "malformed CollectQualityYieldMetrics SAM record".to_string())?;
    }
    let quality_field = fields
        .next()
        .ok_or_else(|| "malformed CollectQualityYieldMetrics SAM record".to_string())?;
    let qualities = if use_original_qualities {
        let mut preferred = quality_field;
        for field in fields {
            if let Some(original_qualities) = field.strip_prefix(b"OQ:Z:") {
                preferred = original_qualities;
                break;
            }
        }
        sam_quality_bytes(preferred)
    } else {
        sam_quality_bytes(quality_field)
    };
    let is_pf = flags & 0x200 == 0;
    metrics.total_reads += 1;
    metrics.total_bases += qualities.len() as u64;
    if is_pf {
        metrics.pf_reads += 1;
        metrics.pf_bases += qualities.len() as u64;
    }
    for quality in qualities {
        let quality = quality.saturating_sub(33) as u64;
        metrics.total_quality += quality;
        if quality >= 20 {
            metrics.q20_bases += 1;
        }
        if quality >= 30 {
            metrics.q30_bases += 1;
        }
        if is_pf {
            metrics.pf_quality += quality;
            if quality >= 20 {
                metrics.pf_q20_bases += 1;
            }
            if quality >= 30 {
                metrics.pf_q30_bases += 1;
            }
        }
    }
    Ok(())
}

fn collect_insert_size_sam_text(
    input: &str,
    include_duplicates: bool,
    stop_after: u32,
    accumulation: InsertSizeAccumulation,
) -> Result<InsertSizeCollection, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut metrics = InsertSizeCollection::new(accumulation);
    let mut read_groups = BTreeMap::new();
    let mut observed = 0_u32;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@RG\t") {
            observe_sam_insert_size_read_group(&mut read_groups, &line);
            continue;
        }
        if line.starts_with(b"@") || line.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        observe_insert_size_sam_line(&mut metrics, &line, include_duplicates, &read_groups)?;
        observed = observed.saturating_add(1);
        if stop_after > 0 && observed >= stop_after {
            break;
        }
    }
    Ok(metrics)
}

fn observe_insert_size_sam_line(
    metrics: &mut InsertSizeCollection,
    line: &[u8],
    include_duplicates: bool,
    read_groups: &BTreeMap<String, InsertSizeReadGroup>,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    fields
        .next()
        .ok_or_else(|| "malformed CollectInsertSizeMetrics SAM record".to_string())?;
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed CollectInsertSizeMetrics SAM record".to_string())?,
    )?;
    for _ in 0..6 {
        fields
            .next()
            .ok_or_else(|| "malformed CollectInsertSizeMetrics SAM record".to_string())?;
    }
    let insert_size = parse_i64_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed CollectInsertSizeMetrics SAM record".to_string())?,
    )?;
    for _ in 0..2 {
        fields
            .next()
            .ok_or_else(|| "malformed CollectInsertSizeMetrics SAM record".to_string())?;
    }
    let read_group = insert_size_read_group_for_sam_tags(fields, read_groups);
    metrics.observe_sam_parts(flags, insert_size, include_duplicates, read_group.as_ref());
    Ok(())
}

fn observe_sam_insert_size_read_group(
    read_groups: &mut BTreeMap<String, InsertSizeReadGroup>,
    line: &[u8],
) {
    let line = trim_ascii_line_end(line);
    let mut id = None;
    let mut sample = None;
    let mut library = None;
    let mut platform_unit = None;
    for field in line.split(|byte| *byte == b'\t').skip(1) {
        if let Some(value) = field.strip_prefix(b"ID:") {
            id = Some(String::from_utf8_lossy(value).to_string());
        } else if let Some(value) = field.strip_prefix(b"SM:") {
            sample = Some(String::from_utf8_lossy(value).to_string());
        } else if let Some(value) = field.strip_prefix(b"LB:") {
            library = Some(String::from_utf8_lossy(value).to_string());
        } else if let Some(value) = field.strip_prefix(b"PU:") {
            platform_unit = Some(String::from_utf8_lossy(value).to_string());
        }
    }
    if let (Some(id), Some(sample)) = (id, sample) {
        read_groups.insert(
            id,
            InsertSizeReadGroup {
                sample,
                library: library.unwrap_or_default(),
                platform_unit: platform_unit.unwrap_or_else(|| "unknown".to_string()),
            },
        );
    }
}

fn insert_size_read_group_for_sam_tags<'a>(
    tags: impl Iterator<Item = &'a [u8]>,
    read_groups: &BTreeMap<String, InsertSizeReadGroup>,
) -> Option<InsertSizeReadGroup> {
    for tag in tags {
        if let Some(read_group) = tag.strip_prefix(b"RG:Z:") {
            return read_groups
                .get(String::from_utf8_lossy(read_group).as_ref())
                .cloned();
        }
    }
    None
}

fn trim_ascii_line_end(mut line: &[u8]) -> &[u8] {
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    line
}

fn parse_u16_bytes(value: &[u8]) -> Result<u16, String> {
    let mut parsed = 0_u16;
    if value.is_empty() {
        return Err("malformed integer".to_string());
    }
    for byte in value {
        if !byte.is_ascii_digit() {
            return Err("malformed integer".to_string());
        }
        parsed = parsed
            .checked_mul(10)
            .and_then(|number| number.checked_add(u16::from(byte - b'0')))
            .ok_or_else(|| "malformed integer".to_string())?;
    }
    Ok(parsed)
}

fn parse_i64_bytes(value: &[u8]) -> Result<i64, String> {
    if value.is_empty() {
        return Err("malformed integer".to_string());
    }
    let (negative, digits) = if let Some(digits) = value.strip_prefix(b"-") {
        (true, digits)
    } else {
        (false, value)
    };
    if digits.is_empty() {
        return Err("malformed integer".to_string());
    }
    let mut parsed = 0_i64;
    for byte in digits {
        if !byte.is_ascii_digit() {
            return Err("malformed integer".to_string());
        }
        parsed = parsed
            .checked_mul(10)
            .and_then(|number| number.checked_add(i64::from(byte - b'0')))
            .ok_or_else(|| "malformed integer".to_string())?;
    }
    if negative {
        parsed
            .checked_neg()
            .ok_or_else(|| "malformed integer".to_string())
    } else {
        Ok(parsed)
    }
}

fn parse_u8_bytes(value: &[u8]) -> Result<u8, String> {
    let parsed = parse_u16_bytes(value)?;
    u8::try_from(parsed).map_err(|_| "malformed integer".to_string())
}

fn sam_sequence_bytes(value: &[u8]) -> &[u8] {
    if value == b"*" { &[] } else { value }
}

fn sam_quality_bytes(value: &[u8]) -> &[u8] {
    if value == b"*" { &[] } else { value }
}

fn cigar_summary_from_sam(cigar: &[u8], is_reverse: bool) -> Result<CigarSummary, String> {
    if cigar == b"*" {
        return Ok(CigarSummary::default());
    }
    let mut summary = CigarSummary::default();
    let mut len = 0_u64;
    let mut saw_digit = false;
    let mut first_soft_clip = 0;
    let mut last_soft_clip = 0;
    let mut seen_operator = false;
    for byte in cigar {
        if byte.is_ascii_digit() {
            saw_digit = true;
            len = len
                .checked_mul(10)
                .and_then(|value| value.checked_add(u64::from(byte - b'0')))
                .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
            continue;
        }
        if !saw_digit || len == 0 {
            return Err("malformed CollectAlignmentSummaryMetrics CIGAR".to_string());
        }
        match *byte {
            b'M' | b'=' | b'X' => {
                summary.aligned_length = summary
                    .aligned_length
                    .checked_add(len)
                    .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
                summary.read_aligned_length = summary
                    .read_aligned_length
                    .checked_add(len)
                    .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
                last_soft_clip = 0;
            }
            b'I' => {
                summary.read_aligned_length = summary
                    .read_aligned_length
                    .checked_add(len)
                    .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
                summary.indel_events = summary
                    .indel_events
                    .checked_add(1)
                    .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
                last_soft_clip = 0;
            }
            b'D' => {
                summary.indel_events = summary
                    .indel_events
                    .checked_add(1)
                    .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
                last_soft_clip = 0;
            }
            b'S' => {
                summary.soft_clip_bases = summary
                    .soft_clip_bases
                    .checked_add(len)
                    .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
                if !seen_operator {
                    first_soft_clip = len;
                }
                last_soft_clip = len;
            }
            b'H' => {
                summary.hard_clip_bases = summary
                    .hard_clip_bases
                    .checked_add(len)
                    .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
                last_soft_clip = 0;
            }
            b'N' | b'P' => {
                last_soft_clip = 0;
            }
            _ => return Err("malformed CollectAlignmentSummaryMetrics CIGAR".to_string()),
        }
        seen_operator = true;
        len = 0;
        saw_digit = false;
    }
    if saw_digit {
        return Err("malformed CollectAlignmentSummaryMetrics CIGAR".to_string());
    }
    summary.three_prime_soft_clip_bases = if is_reverse {
        first_soft_clip
    } else {
        last_soft_clip
    };
    Ok(summary)
}

fn is_chimeric_sam_record(
    flags: u16,
    reference_name: &[u8],
    mate_reference_name: &[u8],
    template_length: i64,
    has_sa_tag: bool,
) -> bool {
    if flags & 0x1 == 0 || flags & 0x4 != 0 {
        return false;
    }
    if has_sa_tag {
        return true;
    }
    if flags & 0x8 != 0 {
        return false;
    }
    let mate_on_different_reference =
        mate_reference_name != b"=" && mate_reference_name != reference_name;
    mate_on_different_reference
        || template_length.unsigned_abs() > 100_000
        || !is_expected_fr_pair(
            flags & 0x40 != 0,
            flags & 0x10 != 0,
            flags & 0x20 != 0,
            template_length,
        )
}

fn bam_cigar_reference_len(record: &bam::Record) -> usize {
    record
        .cigar()
        .iter()
        .map(|cigar| match cigar {
            Cigar::Match(len)
            | Cigar::Equal(len)
            | Cigar::Diff(len)
            | Cigar::Del(len)
            | Cigar::RefSkip(len) => *len as usize,
            _ => 0,
        })
        .sum()
}

#[derive(Debug, Default)]
struct WgsOverlapBitmap {
    words: Vec<u64>,
}

impl WgsOverlapBitmap {
    fn with_bit_len(bits: usize) -> Self {
        Self {
            words: vec![0; bits.div_ceil(64)],
        }
    }

    fn set(&mut self, index: usize) {
        if let Some(word) = self.words.get_mut(index / 64) {
            *word |= 1_u64 << (index % 64);
        }
    }

    fn get(&self, index: usize) -> bool {
        self.words
            .get(index / 64)
            .copied()
            .is_some_and(|word| word & (1_u64 << (index % 64)) != 0)
    }
}

#[derive(Debug)]
struct WgsCachedMate {
    overlap_start: u32,
    bitmap: WgsOverlapBitmap,
}

impl WgsCachedMate {
    fn covered_at(&self, reference_index: usize) -> bool {
        if reference_index < self.overlap_start as usize {
            return false;
        }
        let index = reference_index - self.overlap_start as usize;
        self.bitmap.get(index)
    }
}

#[derive(Debug)]
struct WgsMateBuffer {
    pending: FxHashMap<Vec<u8>, WgsCachedMate>,
}

enum WgsMatePeek {
    Alone,
    WouldBuffer {
        overlap_start: u32,
        overlap_len: u32,
    },
    PairWith(WgsCachedMate),
}

impl WgsMateBuffer {
    const INITIAL_CAPACITY: usize = 4096;

    fn clear(&mut self) {
        self.pending.clear();
    }

    fn probe(&mut self, record: &bam::Record) -> WgsMatePeek {
        if !record.is_paired() || record.is_unmapped() || record.is_mate_unmapped() {
            return WgsMatePeek::Alone;
        }
        let tid = record.tid();
        let mtid = record.mtid();
        if tid < 0 || mtid < 0 || tid != mtid {
            return WgsMatePeek::Alone;
        }
        let qname = record.qname();
        if qname.is_empty() {
            return WgsMatePeek::Alone;
        }
        if let Some(cached) = self.pending.remove(qname) {
            return WgsMatePeek::PairWith(cached);
        }
        let read_start = record.pos().max(0) as u32;
        let mate_start = record.mpos().max(0) as u32;
        let read_end = read_start.saturating_add(bam_cigar_reference_len(record) as u32);
        if mate_start >= read_start && mate_start < read_end {
            let overlap_len = read_end - mate_start;
            return WgsMatePeek::WouldBuffer {
                overlap_start: mate_start,
                overlap_len,
            };
        }
        WgsMatePeek::Alone
    }

    fn insert(&mut self, qname: &[u8], cached: WgsCachedMate) {
        self.pending.insert(qname.to_vec(), cached);
    }
}

impl Default for WgsMateBuffer {
    fn default() -> Self {
        Self {
            pending: FxHashMap::with_capacity_and_hasher(Self::INITIAL_CAPACITY, FxBuildHasher),
        }
    }
}

enum WgsOverlapMode<'a> {
    Buffer {
        overlap_start: u32,
        bitmap: &'a mut WgsOverlapBitmap,
    },
    Pair(&'a WgsCachedMate),
}

impl WgsOverlapMode<'_> {
    #[inline]
    fn is_mate_covered(&self, reference_index: usize) -> bool {
        match self {
            Self::Buffer { .. } => false,
            Self::Pair(cached) => cached.covered_at(reference_index),
        }
    }

    #[inline]
    fn on_depth_counted(&mut self, reference_index: usize) {
        if let Self::Buffer {
            overlap_start,
            bitmap,
        } = self
        {
            if reference_index < *overlap_start as usize {
                return;
            }
            let index = reference_index - *overlap_start as usize;
            bitmap.set(index);
        }
    }
}

fn wgs_locus_included(contig: &WgsContigMetadata, index: usize) -> bool {
    wgs_locus_included_at(contig.included.as_deref(), index, contig.length)
}

#[inline]
fn wgs_locus_included_at(mask: Option<&[bool]>, index: usize, contig_length: usize) -> bool {
    mask.map_or(true, |included| {
        included.get(index).copied().unwrap_or(false)
    }) && index < contig_length
}

fn wgs_included_loci(contig: &WgsContigMetadata) -> usize {
    contig.included.as_ref().map_or(contig.length, |mask| {
        mask.iter().filter(|included| **included).count()
    })
}

#[derive(Debug)]
struct WgsMetricsSummary {
    contigs: BTreeMap<String, WgsContigMetadata>,
    coverage_cap: u32,
    total_aligned_bases: u64,
    excluded_mapq: u64,
    excluded_duplicate: u64,
    excluded_unpaired: u64,
    excluded_baseq: u64,
    excluded_overlap: u64,
    excluded_capped: u64,
    base_quality_histogram: Vec<u64>,
    sensitivity_base_quality_histogram: Vec<u64>,
    coverage_histogram: Vec<u64>,
    active_contig: Option<String>,
    active_included: Option<Arc<[bool]>>,
    active_depths: Vec<u16>,
    processed_contigs: HashSet<String>,
    mate_buffer: WgsMateBuffer,
}

#[derive(Debug)]
struct WgsContigMetadata {
    length: usize,
    /// `None` when every position is included (avoids allocating a full-genome mask).
    included: Option<Arc<[bool]>>,
}

impl WgsMetricsSummary {
    fn new(
        reference_contigs: &[(String, usize)],
        interval_masks: Option<BTreeMap<String, Vec<bool>>>,
        coverage_cap: u32,
    ) -> Self {
        let contigs = reference_contigs
            .iter()
            .map(|(name, length)| {
                let included = interval_masks
                    .as_ref()
                    .and_then(|masks| masks.get(name).cloned())
                    .map(Arc::<[bool]>::from);
                (
                    name.clone(),
                    WgsContigMetadata {
                        length: *length,
                        included,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        let included_loci = contigs
            .values()
            .map(|contig| wgs_included_loci(contig))
            .sum::<usize>();
        let mut coverage_histogram = vec![0; coverage_cap as usize + 1];
        if included_loci > 0 {
            coverage_histogram[0] = included_loci as u64;
        }
        Self {
            contigs,
            coverage_cap,
            total_aligned_bases: 0,
            excluded_mapq: 0,
            excluded_duplicate: 0,
            excluded_unpaired: 0,
            excluded_baseq: 0,
            excluded_overlap: 0,
            excluded_capped: 0,
            base_quality_histogram: vec![0; 256.max(coverage_cap as usize + 1)],
            sensitivity_base_quality_histogram: vec![0; 256.max(coverage_cap as usize + 1)],
            coverage_histogram,
            active_contig: None,
            active_included: None,
            active_depths: Vec::new(),
            processed_contigs: HashSet::new(),
            mate_buffer: WgsMateBuffer::default(),
        }
    }

    fn finish(&mut self) {
        if let Some(contig) = self.active_contig.take() {
            self.processed_contigs.insert(contig);
        }
        self.active_included = None;
        self.mate_buffer.clear();
        self.active_depths.clear();
    }

    fn ensure_contig(&mut self, contig: &str) -> Result<(), String> {
        if self.active_contig.as_deref() == Some(contig) {
            return Ok(());
        }
        if self.processed_contigs.contains(contig) {
            return Err(format!(
                "CollectWgsMetrics alignment is not coordinate-sorted: contig {contig} was revisited"
            ));
        }
        if let Some(previous) = self.active_contig.take() {
            self.processed_contigs.insert(previous);
            self.active_depths.clear();
            self.active_included = None;
            self.mate_buffer.clear();
        }
        let Some(metadata) = self.contigs.get(contig) else {
            return Err(format!(
                "CollectWgsMetrics reference missing contig {contig}"
            ));
        };
        self.active_depths.resize(metadata.length, 0);
        self.active_contig = Some(contig.to_string());
        self.active_included = metadata.included.clone();
        Ok(())
    }

    fn adjust_coverage_histogram(
        histogram: &mut [u64],
        old_depth: u32,
        new_depth: u32,
        coverage_cap: u32,
    ) {
        let old_index = old_depth.min(coverage_cap) as usize;
        let new_index = new_depth.min(coverage_cap) as usize;
        if old_index == new_index {
            return;
        }
        histogram[old_index] = histogram[old_index].saturating_sub(1);
        histogram[new_index] += 1;
    }

    #[inline]
    fn increment_filtered_depth(&mut self, reference_index: usize) {
        let old_depth = self.active_depths[reference_index] as u32;
        let new_depth = old_depth.saturating_add(1);
        self.active_depths[reference_index] = new_depth.min(u16::MAX as u32) as u16;
        Self::adjust_coverage_histogram(
            &mut self.coverage_histogram,
            old_depth,
            new_depth,
            self.coverage_cap,
        );
    }

    fn exclude_locus_from_histograms(&mut self, depth: u32) {
        let depth_index = depth.min(self.coverage_cap) as usize;
        self.coverage_histogram[depth_index] =
            self.coverage_histogram[depth_index].saturating_sub(1);
    }

    fn observe(
        &mut self,
        record: &bam::Record,
        target_names: &[String],
        minimum_mapping_quality: u8,
        minimum_base_quality: u8,
        coverage_cap: u32,
        locus_accumulation_cap: u32,
        count_unpaired: bool,
    ) -> Result<(), String> {
        if record.is_unmapped() || record.is_secondary() || record.is_supplementary() {
            return Ok(());
        }
        let tid = record.tid();
        if tid < 0 {
            return Ok(());
        }
        let Some(contig) = target_names.get(tid as usize) else {
            return Err("CollectWgsMetrics record references unknown target".to_string());
        };
        self.ensure_contig(contig)?;
        match self.mate_buffer.probe(record) {
            WgsMatePeek::Alone => self.observe_cigar_ops(
                contig,
                record.pos().max(0) as usize,
                record.qual(),
                record.is_duplicate(),
                record.mapq(),
                record.is_paired(),
                &record.cigar(),
                minimum_mapping_quality,
                minimum_base_quality,
                coverage_cap,
                locus_accumulation_cap,
                count_unpaired,
                None,
            ),
            WgsMatePeek::WouldBuffer {
                overlap_start,
                overlap_len,
            } => {
                let mut bitmap = WgsOverlapBitmap::with_bit_len(overlap_len as usize);
                self.observe_cigar_ops(
                    contig,
                    record.pos().max(0) as usize,
                    record.qual(),
                    record.is_duplicate(),
                    record.mapq(),
                    record.is_paired(),
                    &record.cigar(),
                    minimum_mapping_quality,
                    minimum_base_quality,
                    coverage_cap,
                    locus_accumulation_cap,
                    count_unpaired,
                    Some(WgsOverlapMode::Buffer {
                        overlap_start,
                        bitmap: &mut bitmap,
                    }),
                )?;
                self.mate_buffer.insert(
                    record.qname(),
                    WgsCachedMate {
                        overlap_start,
                        bitmap,
                    },
                );
                Ok(())
            }
            WgsMatePeek::PairWith(cached) => self.observe_cigar_ops(
                contig,
                record.pos().max(0) as usize,
                record.qual(),
                record.is_duplicate(),
                record.mapq(),
                record.is_paired(),
                &record.cigar(),
                minimum_mapping_quality,
                minimum_base_quality,
                coverage_cap,
                locus_accumulation_cap,
                count_unpaired,
                Some(WgsOverlapMode::Pair(&cached)),
            ),
        }
    }

    fn observe_cigar_ops(
        &mut self,
        contig: &str,
        reference_offset_start: usize,
        qualities: &[u8],
        is_duplicate: bool,
        mapq: u8,
        is_paired: bool,
        cigars: &bam::record::CigarStringView,
        minimum_mapping_quality: u8,
        minimum_base_quality: u8,
        coverage_cap: u32,
        locus_accumulation_cap: u32,
        count_unpaired: bool,
        overlap_mode: Option<WgsOverlapMode<'_>>,
    ) -> Result<(), String> {
        self.observe_cigar_ops_iter(
            contig,
            reference_offset_start,
            qualities,
            is_duplicate,
            mapq,
            is_paired,
            cigars.iter().copied(),
            minimum_mapping_quality,
            minimum_base_quality,
            coverage_cap,
            locus_accumulation_cap,
            count_unpaired,
            overlap_mode,
        )
    }

    fn observe_cigar_ops_iter<I>(
        &mut self,
        contig: &str,
        reference_offset_start: usize,
        qualities: &[u8],
        is_duplicate: bool,
        mapq: u8,
        is_paired: bool,
        cigars: I,
        minimum_mapping_quality: u8,
        minimum_base_quality: u8,
        coverage_cap: u32,
        locus_accumulation_cap: u32,
        count_unpaired: bool,
        mut overlap_mode: Option<WgsOverlapMode<'_>>,
    ) -> Result<(), String>
    where
        I: Iterator<Item = Cigar>,
    {
        self.ensure_contig(contig)?;
        let depth_len = self.active_depths.len();
        let mut read_offset = 0usize;
        let mut reference_offset = reference_offset_start;
        let exclude_unpaired = !count_unpaired && !is_paired;
        let low_mapq = mapq < minimum_mapping_quality;
        let locus_accumulation_cap = locus_accumulation_cap.min(u16::MAX as u32) as u16;

        let locus_mask = self.active_included.clone();
        for cigar in cigars {
            let (len, op) = match bam_cigar_to_op(cigar) {
                Some(op) => op,
                None => continue,
            };
            let len = len as usize;
            match op {
                b'M' | b'=' | b'X' => {
                    for index in 0..len {
                        let read_index = read_offset + index;
                        let reference_index = reference_offset + index;
                        if reference_index >= depth_len {
                            return Err(
                                "CollectWgsMetrics alignment extends beyond reference".to_string()
                            );
                        }
                        if !wgs_locus_included_at(locus_mask.as_deref(), reference_index, depth_len)
                        {
                            continue;
                        }
                        self.total_aligned_bases += 1;
                        if is_duplicate {
                            self.excluded_duplicate += 1;
                        } else if low_mapq {
                            self.excluded_mapq += 1;
                        } else if exclude_unpaired {
                            self.excluded_unpaired += 1;
                        } else if qualities
                            .get(read_index)
                            .is_none_or(|quality| *quality < minimum_base_quality)
                        {
                            self.excluded_baseq += 1;
                        } else if overlap_mode
                            .as_ref()
                            .is_some_and(|mode| mode.is_mate_covered(reference_index))
                        {
                            self.excluded_overlap += 1;
                        } else if self.active_depths[reference_index] >= coverage_cap as u16
                            || self.active_depths[reference_index] >= locus_accumulation_cap
                        {
                            if let Some(quality) = qualities.get(read_index) {
                                let index = *quality as usize;
                                self.base_quality_histogram[index] += 1;
                                if *quality >= 30 {
                                    self.sensitivity_base_quality_histogram[index] += 1;
                                }
                            }
                            self.excluded_capped += 1;
                        } else {
                            if let Some(quality) = qualities.get(read_index) {
                                let index = *quality as usize;
                                self.base_quality_histogram[index] += 1;
                                if *quality >= 30 {
                                    self.sensitivity_base_quality_histogram[index] += 1;
                                }
                            }
                            self.increment_filtered_depth(reference_index);
                            if let Some(mode) = overlap_mode.as_mut() {
                                mode.on_depth_counted(reference_index);
                            }
                        }
                    }
                    read_offset += len;
                    reference_offset += len;
                }
                b'I' | b'S' => {
                    read_offset += len;
                }
                b'D' | b'N' => {
                    reference_offset += len;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn limit_included_loci(&mut self, limit: usize) {
        let mut remaining = limit;
        let mut excluded_loci = 0usize;
        for contig in self.contigs.values_mut() {
            for index in 0..contig.length {
                if !wgs_locus_included(contig, index) {
                    continue;
                }
                if remaining == 0 {
                    if contig.included.is_none() {
                        contig.included = Some(Arc::<[bool]>::from(vec![true; contig.length]));
                    }
                    if let Some(mask) = contig.included.as_mut() {
                        Arc::make_mut(mask)[index] = false;
                    }
                    excluded_loci += 1;
                } else {
                    remaining -= 1;
                }
            }
        }
        for _ in 0..excluded_loci {
            self.exclude_locus_from_histograms(0);
        }
    }

    fn to_picard_text(&self, sample_size: u32, include_bq_histogram: bool) -> String {
        let histogram = &self.coverage_histogram;
        let genome_territory = histogram.iter().sum::<u64>();
        let mean_coverage = mean_from_histogram_u32(&histogram);
        let sd_coverage = sample_standard_deviation_from_histogram_u32(&histogram, mean_coverage);
        let median_coverage = if genome_territory <= 1 {
            0.0
        } else {
            median_f64_from_histogram_u64(&histogram)
        };
        let mad_coverage = if genome_territory <= 1 {
            0.0
        } else {
            mad_f64_from_histogram_u64(&histogram, median_coverage)
        };
        let pct_exc_total = ratio(
            self.excluded_mapq
                + self.excluded_duplicate
                + self.excluded_unpaired
                + self.excluded_baseq
                + self.excluded_overlap
                + self.excluded_capped,
            self.total_aligned_bases,
        );
        let het_sensitivity = if sample_size > 0 && genome_territory > 0 {
            format_float(het_snp_sensitivity_from_histograms(
                &histogram,
                &self.sensitivity_base_quality_histogram,
                sample_size,
            ))
        } else {
            "0".to_string()
        };
        let het_q = het_snp_q(&het_sensitivity);

        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.WgsMetrics\n");
        output.push_str("GENOME_TERRITORY\tMEAN_COVERAGE\tSD_COVERAGE\tMEDIAN_COVERAGE\tMAD_COVERAGE\tPCT_EXC_ADAPTER\tPCT_EXC_MAPQ\tPCT_EXC_DUPE\tPCT_EXC_UNPAIRED\tPCT_EXC_BASEQ\tPCT_EXC_OVERLAP\tPCT_EXC_CAPPED\tPCT_EXC_TOTAL\tPCT_1X\tPCT_5X\tPCT_10X\tPCT_15X\tPCT_20X\tPCT_25X\tPCT_30X\tPCT_40X\tPCT_50X\tPCT_60X\tPCT_70X\tPCT_80X\tPCT_90X\tPCT_100X\tFOLD_80_BASE_PENALTY\tFOLD_90_BASE_PENALTY\tFOLD_95_BASE_PENALTY\tHET_SNP_SENSITIVITY\tHET_SNP_Q\n");
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t0\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n\n",
            genome_territory,
            format_float(mean_coverage),
            if genome_territory < 2 {
                "?".to_string()
            } else {
                format_float(sd_coverage)
            },
            format_float(median_coverage),
            format_float(mad_coverage),
            format_float(ratio(self.excluded_mapq, self.total_aligned_bases)),
            format_float(ratio(self.excluded_duplicate, self.total_aligned_bases)),
            format_float(ratio(self.excluded_unpaired, self.total_aligned_bases)),
            format_float(ratio(self.excluded_baseq, self.total_aligned_bases)),
            format_float(ratio(self.excluded_overlap, self.total_aligned_bases)),
            format_float(ratio(self.excluded_capped, self.total_aligned_bases)),
            format_float(pct_exc_total),
            format_float(pct_at_least(&histogram, 1)),
            format_float(pct_at_least(&histogram, 5)),
            format_float(pct_at_least(&histogram, 10)),
            format_float(pct_at_least(&histogram, 15)),
            format_float(pct_at_least(&histogram, 20)),
            format_float(pct_at_least(&histogram, 25)),
            format_float(pct_at_least(&histogram, 30)),
            format_float(pct_at_least(&histogram, 40)),
            format_float(pct_at_least(&histogram, 50)),
            format_float(pct_at_least(&histogram, 60)),
            format_float(pct_at_least(&histogram, 70)),
            format_float(pct_at_least(&histogram, 80)),
            format_float(pct_at_least(&histogram, 90)),
            format_float(pct_at_least(&histogram, 100)),
            fold_base_penalty(&histogram, mean_coverage, 80.0),
            fold_base_penalty(&histogram, mean_coverage, 90.0),
            fold_base_penalty(&histogram, mean_coverage, 95.0),
            het_sensitivity,
            het_q,
        ));
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        if include_bq_histogram {
            output.push_str("coverage\thigh_quality_coverage_count\tunfiltered_baseq_count\n");
            for coverage in 0..=self.coverage_cap as usize {
                output.push_str(&format!(
                    "{coverage}\t{}\t{}\n",
                    histogram.get(coverage).copied().unwrap_or(0),
                    self.base_quality_histogram
                        .get(coverage)
                        .copied()
                        .unwrap_or(0)
                ));
            }
        } else {
            output.push_str("coverage\thigh_quality_coverage_count\n");
            for coverage in 0..=self.coverage_cap as usize {
                output.push_str(&format!(
                    "{coverage}\t{}\n",
                    histogram.get(coverage).copied().unwrap_or(0)
                ));
            }
        }
        output
    }
}

fn mean_from_histogram_u32(histogram: &[u64]) -> f64 {
    let total_count = histogram.iter().sum::<u64>();
    if total_count == 0 {
        return 0.0;
    }
    let total_depth = histogram
        .iter()
        .enumerate()
        .map(|(depth, count)| depth as u64 * count)
        .sum::<u64>();
    total_depth as f64 / total_count as f64
}

fn sample_standard_deviation_from_histogram_u32(histogram: &[u64], mean: f64) -> f64 {
    let total_count = histogram.iter().sum::<u64>();
    if total_count < 2 {
        return 0.0;
    }
    let variance = histogram
        .iter()
        .enumerate()
        .map(|(depth, count)| {
            let delta = depth as f64 - mean;
            delta * delta * *count as f64
        })
        .sum::<f64>()
        / (total_count - 1) as f64;
    variance.sqrt()
}

fn median_f64_from_histogram_u64(histogram: &[u64]) -> f64 {
    let total_count = histogram.iter().sum::<u64>();
    if total_count == 0 {
        return 0.0;
    }
    if total_count % 2 == 1 {
        return histogram_value_at_rank(histogram, total_count / 2) as f64;
    }
    let left = histogram_value_at_rank(histogram, total_count / 2 - 1);
    let right = histogram_value_at_rank(histogram, total_count / 2);
    (left + right) as f64 / 2.0
}

fn histogram_value_at_rank(histogram: &[u64], rank: u64) -> u64 {
    let mut seen = 0;
    for (value, count) in histogram.iter().enumerate() {
        seen += count;
        if seen > rank {
            return value as u64;
        }
    }
    0
}

fn mad_f64_from_histogram_u64(histogram: &[u64], median: f64) -> f64 {
    let total_count = histogram.iter().sum::<u64>();
    if total_count == 0 {
        return 0.0;
    }
    let mut deviation_counts: Vec<(f64, u64)> = histogram
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(depth, count)| ((depth as f64 - median).abs(), *count))
        .collect();
    deviation_counts.sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap_or(Ordering::Equal));
    if total_count % 2 == 1 {
        weighted_histogram_value_at_rank(&deviation_counts, total_count / 2)
    } else {
        let left = weighted_histogram_value_at_rank(&deviation_counts, total_count / 2 - 1);
        let right = weighted_histogram_value_at_rank(&deviation_counts, total_count / 2);
        (left + right) / 2.0
    }
}

fn weighted_histogram_value_at_rank(bins: &[(f64, u64)], rank: u64) -> f64 {
    let mut seen = 0u64;
    for (value, count) in bins {
        seen += count;
        if seen > rank {
            return *value;
        }
    }
    0.0
}

fn pct_at_least(histogram: &[u64], depth: usize) -> f64 {
    let total = histogram.iter().sum::<u64>();
    if total == 0 {
        return 0.0;
    }
    histogram.iter().skip(depth).sum::<u64>() as f64 / total as f64
}

fn fold_base_penalty(histogram: &[u64], mean_coverage: f64, percent: f64) -> String {
    let total = histogram.iter().sum::<u64>();
    if total == 0 {
        return "?".to_string();
    }
    let target = ((percent / 100.0) * total as f64).ceil() as u64;
    let mut covered = 0u64;
    for (depth, count) in histogram.iter().enumerate().rev() {
        covered += count;
        if covered >= target {
            if depth == 0 {
                return "?".to_string();
            }
            return format_float(mean_coverage / depth as f64);
        }
    }
    "?".to_string()
}

fn het_snp_sensitivity_from_histograms(
    depth_histogram: &[u64],
    quality_histogram: &[u64],
    sample_size: u32,
) -> f64 {
    let total = depth_histogram.iter().sum::<u64>();
    if total == 0 {
        return 0.0;
    }
    let called_proportions = sampled_quality_called_proportions(
        depth_histogram.len().min(1001),
        sample_size as usize,
        quality_histogram,
    );
    depth_histogram
        .iter()
        .enumerate()
        .map(|(depth, count)| {
            let detection_probability = het_snp_detection_probability(depth, &called_proportions);
            detection_probability * *count as f64
        })
        .sum::<f64>()
        / total as f64
}

fn sampled_quality_called_proportions(
    iterations: usize,
    sample_size: usize,
    quality_histogram: &[u64],
) -> Vec<f64> {
    if sample_size == 0 || iterations == 0 {
        return vec![0.0; iterations];
    }
    let mut wheel = PicardRouletteWheel::new(quality_histogram);
    let thresholds = (0..iterations)
        .map(|depth| 10.0 * (depth as f64 * 2.0_f64.log10() + 3.0))
        .collect::<Vec<_>>();
    let mut called_counts = vec![0usize; iterations];
    for _ in 0..sample_size {
        let mut sum = 0_u32;
        for (index, threshold) in thresholds.iter().enumerate() {
            if sum as f64 >= *threshold {
                called_counts[index] += 1;
            }
            sum = sum.saturating_add(wheel.draw() as u32);
        }
    }
    called_counts
        .into_iter()
        .map(|count| count as f64 / sample_size as f64)
        .collect()
}

fn het_snp_detection_probability(depth: usize, called_proportions: &[f64]) -> f64 {
    if depth == 0 {
        return 0.0;
    }
    let mut probability = 0.0;
    let mut alt_probability = 0.5_f64.powi(depth as i32);
    for alt_depth in 0..=depth {
        let Some(called_probability) = called_proportions.get(alt_depth) else {
            probability += alt_probability;
            if alt_depth < depth {
                alt_probability *= (depth - alt_depth) as f64 / (alt_depth + 1) as f64;
            }
            continue;
        };
        probability += alt_probability * *called_probability;
        if alt_depth < depth {
            alt_probability *= (depth - alt_depth) as f64 / (alt_depth + 1) as f64;
        }
    }
    probability
}

struct PicardRouletteWheel {
    probabilities: Vec<f64>,
    count: u32,
    rng: JavaRandom,
}

impl PicardRouletteWheel {
    fn new(histogram: &[u64]) -> Self {
        let last_non_zero = histogram
            .iter()
            .rposition(|count| *count > 0)
            .map(|index| index + 1)
            .unwrap_or(1);
        let histogram = &histogram[..last_non_zero];
        let max = histogram.iter().copied().max().unwrap_or(0) as f64;
        let probabilities = if max == 0.0 {
            vec![1.0]
        } else {
            histogram.iter().map(|count| *count as f64 / max).collect()
        };
        Self {
            probabilities,
            count: 0,
            rng: JavaRandom::new(51),
        }
    }

    fn draw(&mut self) -> usize {
        loop {
            let index = (self.probabilities.len() as f64 * self.rng.next_double()) as usize;
            self.count += 1;
            if self.rng.next_double() < self.probabilities[index] {
                self.count = 0;
                return index;
            }
            if self.count >= 600 {
                self.count = 0;
                return 0;
            }
        }
    }
}

struct JavaRandom {
    seed: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5DEECE66D;
    const ADDEND: u64 = 0xB;
    const MASK: u64 = (1_u64 << 48) - 1;

    fn new(seed: u64) -> Self {
        Self {
            seed: (seed ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    fn next_bits(&mut self, bits: u32) -> u32 {
        self.seed = self
            .seed
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::ADDEND)
            & Self::MASK;
        (self.seed >> (48 - bits)) as u32
    }

    fn next_double(&mut self) -> f64 {
        let high = self.next_bits(26) as u64;
        let low = self.next_bits(27) as u64;
        ((high << 27) + low) as f64 / (1_u64 << 53) as f64
    }
}

fn het_snp_q(sensitivity_text: &str) -> String {
    let Ok(sensitivity) = sensitivity_text.parse::<f64>() else {
        return "0".to_string();
    };
    if sensitivity <= 0.0 {
        return "0".to_string();
    }
    if sensitivity >= 1.0 {
        return "?".to_string();
    }
    ((-10.0 * (1.0 - sensitivity).log10()).round() as u64).to_string()
}

fn collect_quality_score_distribution_sam_text(
    input: &str,
    aligned_reads_only: bool,
    pf_reads_only: bool,
    include_no_calls: bool,
    stop_after: u32,
) -> Result<QualityScoreDistributionSummary, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut metrics = QualityScoreDistributionSummary::default();
    let mut observed = 0_u32;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@") || line.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        observe_quality_score_distribution_sam_line(
            &mut metrics,
            &line,
            aligned_reads_only,
            pf_reads_only,
            include_no_calls,
        )?;
        observed = observed.saturating_add(1);
        if stop_after > 0 && observed >= stop_after {
            break;
        }
    }
    Ok(metrics)
}

fn observe_quality_score_distribution_sam_line(
    metrics: &mut QualityScoreDistributionSummary,
    line: &[u8],
    aligned_reads_only: bool,
    pf_reads_only: bool,
    include_no_calls: bool,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    fields
        .next()
        .ok_or_else(|| "malformed QualityScoreDistribution SAM record".to_string())?;
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed QualityScoreDistribution SAM record".to_string())?,
    )?;
    if flags & 0x100 != 0 || flags & 0x800 != 0 {
        return Ok(());
    }
    let rname = fields
        .next()
        .ok_or_else(|| "malformed QualityScoreDistribution SAM record".to_string())?;
    if aligned_reads_only && rname == b"*" {
        return Ok(());
    }
    if pf_reads_only && flags & 0x200 != 0 {
        return Ok(());
    }
    for _ in 0..6 {
        fields
            .next()
            .ok_or_else(|| "malformed QualityScoreDistribution SAM record".to_string())?;
    }
    let sequence = fields
        .next()
        .ok_or_else(|| "malformed QualityScoreDistribution SAM record".to_string())?;
    let qualities = fields
        .next()
        .ok_or_else(|| "malformed QualityScoreDistribution SAM record".to_string())?;
    let sequence = sam_sequence_bytes(sequence);
    let qualities = sam_quality_bytes(qualities);
    let original_qualities = fields
        .find(|field| field.starts_with(b"OQ:Z:"))
        .map(|field| sam_quality_bytes(&field[5..]));
    for (index, quality) in qualities.iter().copied().enumerate() {
        if !include_no_calls && sequence.get(index).is_some_and(|base| *base == b'N') {
            continue;
        }
        metrics.counts[quality.saturating_sub(33) as usize] += 1;
    }
    if let Some(original_qualities) = original_qualities {
        metrics.has_original = true;
        for (index, quality) in original_qualities.iter().copied().enumerate() {
            if !include_no_calls && sequence.get(index).is_some_and(|base| *base == b'N') {
                continue;
            }
            metrics.original_counts[quality.saturating_sub(33) as usize] += 1;
        }
    }
    Ok(())
}

fn collect_base_distribution_by_cycle_sam_text(
    input: &str,
    aligned_reads_only: bool,
    pf_reads_only: bool,
    stop_after: u32,
) -> Result<BaseDistributionByCycleSummary, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut metrics = BaseDistributionByCycleSummary::default();
    let mut observed = 0_u32;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@") || line.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        observe_base_distribution_by_cycle_sam_line(
            &mut metrics,
            &line,
            aligned_reads_only,
            pf_reads_only,
        )?;
        observed = observed.saturating_add(1);
        if stop_after > 0 && observed >= stop_after {
            break;
        }
    }
    Ok(metrics)
}

fn observe_base_distribution_by_cycle_sam_line(
    metrics: &mut BaseDistributionByCycleSummary,
    line: &[u8],
    aligned_reads_only: bool,
    pf_reads_only: bool,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    fields
        .next()
        .ok_or_else(|| "malformed CollectBaseDistributionByCycle SAM record".to_string())?;
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed CollectBaseDistributionByCycle SAM record".to_string())?,
    )?;
    if flags & 0x100 != 0 || flags & 0x800 != 0 {
        return Ok(());
    }
    let rname = fields
        .next()
        .ok_or_else(|| "malformed CollectBaseDistributionByCycle SAM record".to_string())?;
    if aligned_reads_only && rname == b"*" {
        return Ok(());
    }
    if pf_reads_only && flags & 0x200 != 0 {
        return Ok(());
    }
    for _ in 0..6 {
        fields
            .next()
            .ok_or_else(|| "malformed CollectBaseDistributionByCycle SAM record".to_string())?;
    }
    let sequence = fields
        .next()
        .ok_or_else(|| "malformed CollectBaseDistributionByCycle SAM record".to_string())?;
    let sequence = sam_sequence_bytes(sequence);
    let is_second_end = flags & 0x1 != 0 && flags & 0x80 != 0;
    let cycle_offset = if is_second_end { sequence.len() } else { 0 };
    let cycles = if is_second_end {
        &mut metrics.second
    } else {
        &mut metrics.first
    };
    ensure_base_cycle_capacity(cycles, cycle_offset + sequence.len());
    if flags & 0x10 != 0 {
        for (index, base) in sequence.iter().rev().enumerate() {
            cycles[cycle_offset + index].observe(*base);
        }
    } else {
        for (index, base) in sequence.iter().enumerate() {
            cycles[cycle_offset + index].observe(*base);
        }
    }
    Ok(())
}

#[derive(Debug)]
struct QualityScoreDistributionSummary {
    counts: [u64; 256],
    original_counts: [u64; 256],
    has_original: bool,
}

impl Default for QualityScoreDistributionSummary {
    fn default() -> Self {
        Self {
            counts: [0; 256],
            original_counts: [0; 256],
            has_original: false,
        }
    }
}

impl QualityScoreDistributionSummary {
    fn observe(
        &mut self,
        record: &bam::Record,
        aligned_reads_only: bool,
        pf_reads_only: bool,
        include_no_calls: bool,
    ) {
        if skip_quality_metric_record(record, aligned_reads_only, pf_reads_only) {
            return;
        }
        let sequence = record.seq().as_bytes();
        for (index, quality) in record.qual().iter().copied().enumerate() {
            if !include_no_calls && sequence.get(index).is_some_and(|base| *base == b'N') {
                continue;
            }
            self.counts[quality as usize] += 1;
        }
        if let Some(original_qualities) = original_quality_values(record) {
            self.has_original = true;
            for (index, quality) in original_qualities.into_iter().enumerate() {
                if !include_no_calls && sequence.get(index).is_some_and(|base| *base == b'N') {
                    continue;
                }
                self.original_counts[quality as usize] += 1;
            }
        }
    }

    fn to_picard_text(&self) -> String {
        let mut output = String::new();
        output.push_str("## HISTOGRAM\tjava.lang.Byte\n");
        if !self.has_original {
            output.push_str("QUALITY\tCOUNT_OF_Q\n");
            for quality in 0_u8..=u8::MAX {
                let count = self.counts[quality as usize];
                if count > 0 {
                    output.push_str(&format!("{quality}\t{count}\n"));
                }
            }
        } else {
            output.push_str("QUALITY\tCOUNT_OF_Q\tCOUNT_OF_OQ\n");
            for quality in 0_u8..=u8::MAX {
                let primary = self.counts[quality as usize];
                let original = self.original_counts[quality as usize];
                if primary > 0 || original > 0 {
                    output.push_str(&format!("{quality}\t{primary}\t{original}\n"));
                }
            }
        }
        output
    }
}

#[derive(Debug, Default)]
struct BaseDistributionByCycleSummary {
    first: Vec<BaseCycleCounts>,
    second: Vec<BaseCycleCounts>,
}

#[derive(Debug, Default, Clone, Copy)]
struct BaseCycleCounts {
    a: u64,
    c: u64,
    g: u64,
    t: u64,
    n: u64,
}

impl BaseCycleCounts {
    fn observe(&mut self, base: u8) {
        match base.to_ascii_uppercase() {
            b'A' => self.a += 1,
            b'C' => self.c += 1,
            b'G' => self.g += 1,
            b'T' => self.t += 1,
            _ => self.n += 1,
        }
    }

    fn total(self) -> u64 {
        self.a + self.c + self.g + self.t + self.n
    }
}

fn ensure_base_cycle_capacity(cycles: &mut Vec<BaseCycleCounts>, needed: usize) {
    if cycles.len() < needed {
        cycles.resize(needed, BaseCycleCounts::default());
    }
}

impl BaseDistributionByCycleSummary {
    fn observe(&mut self, record: &bam::Record, aligned_reads_only: bool, pf_reads_only: bool) {
        if skip_quality_metric_record(record, aligned_reads_only, pf_reads_only) {
            return;
        }
        let bases = record.seq().as_bytes();
        let is_second_end = record.is_paired() && record.is_last_in_template();
        let cycle_offset = if is_second_end { bases.len() } else { 0 };
        let cycles = if is_second_end {
            &mut self.second
        } else {
            &mut self.first
        };
        ensure_base_cycle_capacity(cycles, cycle_offset + bases.len());
        if record.is_reverse() {
            for (index, base) in bases.iter().rev().enumerate() {
                cycles[cycle_offset + index].observe(*base);
            }
        } else {
            for (index, base) in bases.iter().enumerate() {
                cycles[cycle_offset + index].observe(*base);
            }
        }
    }

    fn to_picard_text(&self) -> String {
        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.BaseDistributionByCycleMetrics\n");
        output.push_str("READ_END\tCYCLE\tPCT_A\tPCT_C\tPCT_G\tPCT_T\tPCT_N\n");
        write_base_distribution_read_end(&mut output, 1, &self.first);
        write_base_distribution_read_end(&mut output, 2, &self.second);
        output
    }
}

fn write_base_distribution_read_end(output: &mut String, read_end: u8, cycles: &[BaseCycleCounts]) {
    for (index, counts) in cycles.iter().enumerate() {
        let total = counts.total();
        if total == 0 {
            continue;
        }
        output.push_str(&format!(
            "{read_end}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            index + 1,
            format_float(percent(counts.a, total)),
            format_float(percent(counts.c, total)),
            format_float(percent(counts.g, total)),
            format_float(percent(counts.t, total)),
            format_float(percent(counts.n, total)),
        ));
    }
}

fn percent(numerator: u64, denominator: u64) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 * 100.0 / denominator as f64
    }
}

#[derive(Debug)]
struct GcBiasMetricsSummary {
    windows: [u64; 101],
    read_starts: [u64; 101],
    quality_sums: [u64; 101],
    quality_counts: [u64; 101],
    unique_read_starts: [u64; 101],
    unique_quality_sums: [u64; 101],
    unique_quality_counts: [u64; 101],
    reference_path: String,
    active_contig: Option<String>,
    active_sequence: Vec<u8>,
    total_clusters: u64,
    aligned_reads: u64,
    unique_total_clusters: u64,
    unique_aligned_reads: u64,
    emit_unique: bool,
}

impl GcBiasMetricsSummary {
    fn new(reference_path: &str, window_size: usize, emit_unique: bool) -> Result<Self, String> {
        Ok(Self {
            windows: count_gc_bias_windows(reference_path, window_size)?,
            read_starts: [0; 101],
            quality_sums: [0; 101],
            quality_counts: [0; 101],
            unique_read_starts: [0; 101],
            unique_quality_sums: [0; 101],
            unique_quality_counts: [0; 101],
            reference_path: reference_path.to_string(),
            active_contig: None,
            active_sequence: Vec::new(),
            total_clusters: 0,
            aligned_reads: 0,
            unique_total_clusters: 0,
            unique_aligned_reads: 0,
            emit_unique,
        })
    }

    fn ensure_contig(&mut self, contig: &str) -> Result<(), String> {
        if self.active_contig.as_deref() == Some(contig) {
            return Ok(());
        }
        self.active_sequence = load_fasta_contig_sequence(&self.reference_path, contig)?;
        self.active_contig = Some(contig.to_string());
        Ok(())
    }

    fn observe(
        &mut self,
        record: &bam::Record,
        target_names: &[String],
        window_size: usize,
    ) -> Result<(), String> {
        if record.is_secondary() || record.is_supplementary() {
            return Ok(());
        }
        self.total_clusters += 1;
        if self.emit_unique && !record.is_duplicate() {
            self.unique_total_clusters += 1;
        }
        if record.is_unmapped() || record.tid() < 0 {
            return Ok(());
        }
        self.aligned_reads += 1;
        if self.emit_unique && !record.is_duplicate() {
            self.unique_aligned_reads += 1;
        }
        let contig = target_names
            .get(record.tid() as usize)
            .ok_or_else(|| "CollectGcBiasMetrics record references unknown target".to_string())?;
        self.ensure_contig(contig)?;
        let reference = &self.active_sequence;
        if reference.len() < window_size {
            return Ok(());
        }
        let start = (record.pos().max(0) as usize).min(reference.len() - window_size);
        let gc = gc_percent(&reference[start..start + window_size], window_size);
        let quality_sum = record
            .qual()
            .iter()
            .map(|quality| *quality as u64)
            .sum::<u64>();
        let quality_count = record.qual().len() as u64;
        self.read_starts[gc] += 1;
        self.quality_sums[gc] += quality_sum;
        self.quality_counts[gc] += quality_count;
        if self.emit_unique && !record.is_duplicate() {
            self.unique_read_starts[gc] += 1;
            self.unique_quality_sums[gc] += quality_sum;
            self.unique_quality_counts[gc] += quality_count;
        }
        Ok(())
    }

    fn detail_text(&self, _window_size: usize, minimum_genome_fraction: f64) -> String {
        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.GcBiasDetailMetrics\n");
        output.push_str("ACCUMULATION_LEVEL\tREADS_USED\tGC\tWINDOWS\tREAD_STARTS\tMEAN_BASE_QUALITY\tNORMALIZED_COVERAGE\tERROR_BAR_WIDTH\tSAMPLE\tLIBRARY\tREAD_GROUP\n");
        self.push_detail_rows(
            &mut output,
            "ALL",
            &self.read_starts,
            &self.quality_sums,
            &self.quality_counts,
            self.aligned_reads,
            minimum_genome_fraction,
        );
        if self.emit_unique {
            self.push_detail_rows(
                &mut output,
                "UNIQUE",
                &self.unique_read_starts,
                &self.unique_quality_sums,
                &self.unique_quality_counts,
                self.unique_aligned_reads,
                minimum_genome_fraction,
            );
        }
        output
    }

    fn push_detail_rows(
        &self,
        output: &mut String,
        reads_used: &str,
        read_starts_by_gc: &[u64; 101],
        quality_sums_by_gc: &[u64; 101],
        quality_counts_by_gc: &[u64; 101],
        aligned_reads: u64,
        minimum_genome_fraction: f64,
    ) {
        let total_windows = self.total_windows();
        let mean_reads_per_window = if total_windows == 0 {
            0.0
        } else {
            aligned_reads as f64 / total_windows as f64
        };
        for gc in 0..=100 {
            let windows = self.windows[gc];
            let genome_fraction = if total_windows == 0 {
                0.0
            } else {
                windows as f64 / total_windows as f64
            };
            let read_starts = read_starts_by_gc[gc];
            let normalized_coverage = if windows == 0
                || mean_reads_per_window == 0.0
                || genome_fraction < minimum_genome_fraction
            {
                0.0
            } else {
                (read_starts as f64 / windows as f64) / mean_reads_per_window
            };
            let error_bar_width = if read_starts == 0 {
                0.0
            } else {
                normalized_coverage / (read_starts as f64).sqrt()
            };
            let mean_base_quality = ratio(quality_sums_by_gc[gc], quality_counts_by_gc[gc]);
            output.push_str(&format!(
                "All Reads\t{reads_used}\t{gc}\t{windows}\t{read_starts}\t{}\t{}\t{}\t\t\t\n",
                format_float(mean_base_quality),
                format_float(normalized_coverage),
                format_float(error_bar_width),
            ));
        }
    }

    fn summary_text(&self, window_size: usize, minimum_genome_fraction: f64) -> String {
        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.GcBiasSummaryMetrics\n");
        output.push_str("ACCUMULATION_LEVEL\tREADS_USED\tWINDOW_SIZE\tTOTAL_CLUSTERS\tALIGNED_READS\tAT_DROPOUT\tGC_DROPOUT\tGC_NC_0_19\tGC_NC_20_39\tGC_NC_40_59\tGC_NC_60_79\tGC_NC_80_100\tSAMPLE\tLIBRARY\tREAD_GROUP\n");
        self.push_summary_row(
            &mut output,
            "ALL",
            window_size,
            self.total_clusters,
            self.aligned_reads,
            &self.read_starts,
            minimum_genome_fraction,
        );
        if self.emit_unique {
            self.push_summary_row(
                &mut output,
                "UNIQUE",
                window_size,
                self.unique_total_clusters,
                self.unique_aligned_reads,
                &self.unique_read_starts,
                minimum_genome_fraction,
            );
        }
        output
    }

    fn push_summary_row(
        &self,
        output: &mut String,
        reads_used: &str,
        window_size: usize,
        total_clusters: u64,
        aligned_reads: u64,
        read_starts_by_gc: &[u64; 101],
        minimum_genome_fraction: f64,
    ) {
        let at_dropout = self.gc_dropout_slice(
            read_starts_by_gc,
            aligned_reads,
            0,
            49,
            minimum_genome_fraction,
        );
        let gc_dropout = self.gc_dropout_slice(
            read_starts_by_gc,
            aligned_reads,
            50,
            100,
            minimum_genome_fraction,
        );
        output.push_str(&format!(
            "All Reads\t{reads_used}\t{window_size}\t{total_clusters}\t{aligned_reads}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t\t\t\n",
            format_float(at_dropout),
            format_float(gc_dropout),
            format_float(self.gc_nc_slice(
                read_starts_by_gc,
                aligned_reads,
                0,
                19,
                minimum_genome_fraction
            )),
            format_float(self.gc_nc_slice(
                read_starts_by_gc,
                aligned_reads,
                20,
                39,
                minimum_genome_fraction
            )),
            format_float(self.gc_nc_slice(
                read_starts_by_gc,
                aligned_reads,
                40,
                59,
                minimum_genome_fraction
            )),
            format_float(self.gc_nc_slice(
                read_starts_by_gc,
                aligned_reads,
                60,
                79,
                minimum_genome_fraction
            )),
            format_float(self.gc_nc_slice(
                read_starts_by_gc,
                aligned_reads,
                80,
                100,
                minimum_genome_fraction
            )),
        ));
    }

    fn total_windows(&self) -> u64 {
        self.windows.iter().sum()
    }

    fn gc_nc_slice(
        &self,
        read_starts_by_gc: &[u64; 101],
        aligned_reads: u64,
        low: usize,
        high: usize,
        minimum_genome_fraction: f64,
    ) -> f64 {
        let total_windows = self.total_windows();
        if total_windows == 0 || aligned_reads == 0 {
            return 0.0;
        }
        let mut windows = 0_u64;
        let mut read_starts = 0_u64;
        for gc in low..=high {
            let genome_fraction = self.windows[gc] as f64 / total_windows as f64;
            if genome_fraction >= minimum_genome_fraction {
                windows += self.windows[gc];
                read_starts += read_starts_by_gc[gc];
            }
        }
        if windows == 0 {
            0.0
        } else {
            (read_starts as f64 / windows as f64) / (aligned_reads as f64 / total_windows as f64)
        }
    }

    fn gc_dropout_slice(
        &self,
        read_starts_by_gc: &[u64; 101],
        aligned_reads: u64,
        low: usize,
        high: usize,
        minimum_genome_fraction: f64,
    ) -> f64 {
        let total_windows = self.total_windows();
        if total_windows == 0 || aligned_reads == 0 {
            return 0.0;
        }
        let mean_reads_per_window = aligned_reads as f64 / total_windows as f64;
        let mut dropout = 0.0;
        for gc in low..=high {
            let windows = self.windows[gc];
            if windows == 0 {
                continue;
            }
            let genome_fraction = windows as f64 / total_windows as f64;
            if genome_fraction < minimum_genome_fraction {
                continue;
            }
            let normalized_coverage =
                (read_starts_by_gc[gc] as f64 / windows as f64) / mean_reads_per_window;
            if normalized_coverage < 1.0 {
                dropout += genome_fraction * (1.0 - normalized_coverage);
            }
        }
        dropout * 100.0
    }
}

fn gc_percent(window: &[u8], window_size: usize) -> usize {
    let gc = window
        .iter()
        .filter(|base| matches!(base.to_ascii_uppercase(), b'G' | b'C'))
        .count();
    ((gc * 100) + (window_size / 2)) / window_size
}

fn collect_mean_quality_by_cycle_sam_text(
    input: &str,
    aligned_reads_only: bool,
    pf_reads_only: bool,
    stop_after: u32,
) -> Result<MeanQualityByCycleSummary, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut metrics = MeanQualityByCycleSummary::default();
    let mut observed = 0_u32;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with(b"@") || line.iter().all(|byte| byte.is_ascii_whitespace()) {
            continue;
        }
        observe_mean_quality_by_cycle_sam_line(
            &mut metrics,
            &line,
            aligned_reads_only,
            pf_reads_only,
        )?;
        observed = observed.saturating_add(1);
        if stop_after > 0 && observed >= stop_after {
            break;
        }
    }
    Ok(metrics)
}

fn observe_mean_quality_by_cycle_sam_line(
    metrics: &mut MeanQualityByCycleSummary,
    line: &[u8],
    aligned_reads_only: bool,
    pf_reads_only: bool,
) -> Result<(), String> {
    let mut line = line;
    while line.ends_with(b"\n") || line.ends_with(b"\r") {
        line = &line[..line.len() - 1];
    }
    let mut fields = line.split(|byte| *byte == b'\t');
    fields
        .next()
        .ok_or_else(|| "malformed MeanQualityByCycle SAM record".to_string())?;
    let flags = parse_u16_bytes(
        fields
            .next()
            .ok_or_else(|| "malformed MeanQualityByCycle SAM record".to_string())?,
    )?;
    if flags & 0x100 != 0 || flags & 0x800 != 0 {
        return Ok(());
    }
    let rname = fields
        .next()
        .ok_or_else(|| "malformed MeanQualityByCycle SAM record".to_string())?;
    if aligned_reads_only && rname == b"*" {
        return Ok(());
    }
    if pf_reads_only && flags & 0x200 != 0 {
        return Ok(());
    }
    for _ in 0..7 {
        fields
            .next()
            .ok_or_else(|| "malformed MeanQualityByCycle SAM record".to_string())?;
    }
    let qualities = fields
        .next()
        .ok_or_else(|| "malformed MeanQualityByCycle SAM record".to_string())?;
    let qualities = sam_quality_bytes(qualities);
    let mut original_qualities = None::<&[u8]>;
    for field in fields {
        if let Some(value) = field.strip_prefix(b"OQ:Z:") {
            original_qualities = Some(sam_quality_bytes(value));
        }
    }

    metrics.records += 1;
    let cycles = if flags & 0x1 != 0 && flags & 0x80 != 0 {
        &mut metrics.second
    } else {
        &mut metrics.first
    };
    if flags & 0x10 != 0 {
        ensure_cycle_quality_capacity(cycles, qualities.len());
        for (cycle, quality) in qualities.iter().rev().copied().enumerate() {
            let quality = quality.saturating_sub(33) as u64;
            cycles[cycle].quality_sum += quality;
            cycles[cycle].count += 1;
        }
    } else {
        ensure_cycle_quality_capacity(cycles, qualities.len());
        for (cycle, quality) in qualities.iter().copied().enumerate() {
            let quality = quality.saturating_sub(33) as u64;
            cycles[cycle].quality_sum += quality;
            cycles[cycle].count += 1;
        }
    }

    if let Some(original_qualities) = original_qualities {
        metrics.original_records += 1;
        let cycles = if flags & 0x1 != 0 && flags & 0x80 != 0 {
            &mut metrics.original_second
        } else {
            &mut metrics.original_first
        };
        if flags & 0x10 != 0 {
            ensure_cycle_quality_capacity(cycles, original_qualities.len());
            for (cycle, quality) in original_qualities.iter().rev().copied().enumerate() {
                let quality = quality.saturating_sub(33) as u64;
                cycles[cycle].quality_sum += quality;
                cycles[cycle].count += 1;
            }
        } else {
            ensure_cycle_quality_capacity(cycles, original_qualities.len());
            for (cycle, quality) in original_qualities.iter().copied().enumerate() {
                let quality = quality.saturating_sub(33) as u64;
                cycles[cycle].quality_sum += quality;
                cycles[cycle].count += 1;
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
struct MeanQualityByCycleSummary {
    first: Vec<CycleQuality>,
    second: Vec<CycleQuality>,
    original_first: Vec<CycleQuality>,
    original_second: Vec<CycleQuality>,
    records: u64,
    original_records: u64,
}

#[derive(Debug, Default, Clone, Copy)]
struct CycleQuality {
    quality_sum: u64,
    count: u64,
}

fn ensure_cycle_quality_capacity(cycles: &mut Vec<CycleQuality>, needed: usize) {
    if cycles.len() < needed {
        cycles.resize(needed, CycleQuality::default());
    }
}

impl MeanQualityByCycleSummary {
    fn observe(&mut self, record: &bam::Record, aligned_reads_only: bool, pf_reads_only: bool) {
        if skip_quality_metric_record(record, aligned_reads_only, pf_reads_only) {
            return;
        }
        self.records += 1;
        let qualities = record.qual();
        let cycles = if record.is_paired() && record.is_last_in_template() {
            &mut self.second
        } else {
            &mut self.first
        };
        if !qualities.is_empty() {
            ensure_cycle_quality_capacity(cycles, qualities.len());
            for (cycle, quality) in qualities.iter().copied().enumerate() {
                let cycle = if record.is_reverse() {
                    qualities.len() - cycle - 1
                } else {
                    cycle
                };
                cycles[cycle].quality_sum += quality as u64;
                cycles[cycle].count += 1;
            }
        }
        if let Some(original_qualities) = original_quality_values(record) {
            self.original_records += 1;
            let cycles = if record.is_paired() && record.is_last_in_template() {
                &mut self.original_second
            } else {
                &mut self.original_first
            };
            if !original_qualities.is_empty() {
                ensure_cycle_quality_capacity(cycles, original_qualities.len());
                for (cycle, quality) in original_qualities.iter().copied().enumerate() {
                    let cycle = if record.is_reverse() {
                        original_qualities.len() - cycle - 1
                    } else {
                        cycle
                    };
                    cycles[cycle].quality_sum += quality as u64;
                    cycles[cycle].count += 1;
                }
            }
        }
    }

    fn to_picard_text(&self) -> String {
        let mut output = String::new();
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        if self.original_records == 0 {
            output.push_str("CYCLE\tMEAN_QUALITY\n");
            write_mean_quality_cycles(&mut output, 0, &self.first);
            write_mean_quality_cycles(&mut output, self.first.len(), &self.second);
        } else if self.original_records == self.records {
            output.push_str("CYCLE\tMEAN_ORIGINAL_QUALITY\n");
            write_mean_quality_cycles(&mut output, 0, &self.original_first);
            write_mean_quality_cycles(
                &mut output,
                self.original_first.len(),
                &self.original_second,
            );
        } else {
            output.push_str("CYCLE\tMEAN_QUALITY\tMEAN_ORIGINAL_QUALITY\n");
            write_mean_quality_combined_cycles(&mut output, 0, &self.first, &self.original_first);
            write_mean_quality_combined_cycles(
                &mut output,
                self.first.len().max(self.original_first.len()),
                &self.second,
                &self.original_second,
            );
        }
        output
    }
}

fn write_mean_quality_combined_cycles(
    output: &mut String,
    offset: usize,
    primary: &[CycleQuality],
    original: &[CycleQuality],
) {
    for index in 0..primary.len().max(original.len()) {
        let primary = primary.get(index).copied().unwrap_or_default();
        let original = original.get(index).copied().unwrap_or_default();
        if primary.count == 0 && original.count == 0 {
            continue;
        }
        output.push_str(&format!(
            "{}\t{}\t{}\n",
            offset + index + 1,
            format_cycle_quality(primary),
            format_cycle_quality(original),
        ));
    }
}

fn write_mean_quality_cycles(output: &mut String, offset: usize, cycles: &[CycleQuality]) {
    for (index, cycle) in cycles.iter().enumerate() {
        if cycle.count == 0 {
            continue;
        }
        output.push_str(&format!(
            "{}\t{}\n",
            offset + index + 1,
            format_float(cycle.quality_sum as f64 / cycle.count as f64)
        ));
    }
}

fn format_cycle_quality(cycle: CycleQuality) -> String {
    if cycle.count == 0 {
        "?".to_string()
    } else {
        format_float(cycle.quality_sum as f64 / cycle.count as f64)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InsertSizeAccumulation {
    AllReads,
    Sample,
    Library,
    ReadGroup,
}

fn insert_size_accumulation_level(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<InsertSizeAccumulation, String> {
    match optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")?
        .unwrap_or_else(|| "ALL_READS".to_string())
        .as_str()
    {
        "ALL_READS" => Ok(InsertSizeAccumulation::AllReads),
        "SAMPLE" => Ok(InsertSizeAccumulation::Sample),
        "LIBRARY" => Ok(InsertSizeAccumulation::Library),
        "READ_GROUP" => Ok(InsertSizeAccumulation::ReadGroup),
        value => Err(format!(
            "unsupported CollectInsertSizeMetrics METRIC_ACCUMULATION_LEVEL={value}"
        )),
    }
}

#[derive(Debug)]
struct InsertSizeLibrarySummary {
    sample: String,
    summary: InsertSizeSummary,
}

#[derive(Debug)]
struct InsertSizeReadGroupSummary {
    sample: String,
    library: String,
    summary: InsertSizeSummary,
}

#[derive(Clone, Debug)]
struct InsertSizeReadGroup {
    sample: String,
    library: String,
    platform_unit: String,
}

#[derive(Debug)]
struct InsertSizeCollection {
    accumulation: InsertSizeAccumulation,
    all_reads: InsertSizeSummary,
    samples: BTreeMap<String, InsertSizeSummary>,
    libraries: BTreeMap<String, InsertSizeLibrarySummary>,
    read_groups: BTreeMap<String, InsertSizeReadGroupSummary>,
}

impl InsertSizeCollection {
    fn new(accumulation: InsertSizeAccumulation) -> Self {
        Self {
            accumulation,
            all_reads: InsertSizeSummary::default(),
            samples: BTreeMap::new(),
            libraries: BTreeMap::new(),
            read_groups: BTreeMap::new(),
        }
    }

    fn observe(
        &mut self,
        record: &bam::Record,
        include_duplicates: bool,
        read_group: Option<&InsertSizeReadGroup>,
    ) {
        if self.all_reads.observe(record, include_duplicates) {
            match (self.accumulation, read_group) {
                (InsertSizeAccumulation::Sample, Some(read_group)) => {
                    self.samples
                        .entry(read_group.sample.clone())
                        .or_default()
                        .observe(record, include_duplicates);
                }
                (InsertSizeAccumulation::Library, Some(read_group)) => {
                    self.libraries
                        .entry(read_group.library.clone())
                        .or_insert_with(|| InsertSizeLibrarySummary {
                            sample: read_group.sample.clone(),
                            summary: InsertSizeSummary::default(),
                        })
                        .summary
                        .observe(record, include_duplicates);
                }
                (InsertSizeAccumulation::ReadGroup, Some(read_group)) => {
                    self.read_groups
                        .entry(read_group.platform_unit.clone())
                        .or_insert_with(|| InsertSizeReadGroupSummary {
                            sample: read_group.sample.clone(),
                            library: read_group.library.clone(),
                            summary: InsertSizeSummary::default(),
                        })
                        .summary
                        .observe(record, include_duplicates);
                }
                _ => {}
            }
        }
    }

    fn observe_sam_parts(
        &mut self,
        flags: u16,
        insert_size: i64,
        include_duplicates: bool,
        read_group: Option<&InsertSizeReadGroup>,
    ) {
        if self
            .all_reads
            .observe_sam_parts(flags, insert_size, include_duplicates)
        {
            match (self.accumulation, read_group) {
                (InsertSizeAccumulation::Sample, Some(read_group)) => {
                    self.samples
                        .entry(read_group.sample.clone())
                        .or_default()
                        .observe_sam_parts(flags, insert_size, include_duplicates);
                }
                (InsertSizeAccumulation::Library, Some(read_group)) => {
                    self.libraries
                        .entry(read_group.library.clone())
                        .or_insert_with(|| InsertSizeLibrarySummary {
                            sample: read_group.sample.clone(),
                            summary: InsertSizeSummary::default(),
                        })
                        .summary
                        .observe_sam_parts(flags, insert_size, include_duplicates);
                }
                (InsertSizeAccumulation::ReadGroup, Some(read_group)) => {
                    self.read_groups
                        .entry(read_group.platform_unit.clone())
                        .or_insert_with(|| InsertSizeReadGroupSummary {
                            sample: read_group.sample.clone(),
                            library: read_group.library.clone(),
                            summary: InsertSizeSummary::default(),
                        })
                        .summary
                        .observe_sam_parts(flags, insert_size, include_duplicates);
                }
                _ => {}
            }
        }
    }

    fn to_picard_text(&self, minimum_pct: f64, deviations: f64) -> String {
        let mut output = String::new();
        let orientations = self.reportable_orientations(minimum_pct);
        output.push_str("## METRICS CLASS\tpicard.analysis.InsertSizeMetrics\n");
        output.push_str("MEDIAN_INSERT_SIZE\tMODE_INSERT_SIZE\tMEDIAN_ABSOLUTE_DEVIATION\tMIN_INSERT_SIZE\tMAX_INSERT_SIZE\tMEAN_INSERT_SIZE\tSTANDARD_DEVIATION\tREAD_PAIRS\tPAIR_ORIENTATION\tWIDTH_OF_10_PERCENT\tWIDTH_OF_20_PERCENT\tWIDTH_OF_30_PERCENT\tWIDTH_OF_40_PERCENT\tWIDTH_OF_50_PERCENT\tWIDTH_OF_60_PERCENT\tWIDTH_OF_70_PERCENT\tWIDTH_OF_80_PERCENT\tWIDTH_OF_90_PERCENT\tWIDTH_OF_95_PERCENT\tWIDTH_OF_99_PERCENT\tSAMPLE\tLIBRARY\tREAD_GROUP\n");
        output.push_str(&self.all_reads.picard_metric_rows(
            None,
            None,
            None,
            &orientations,
            deviations,
        ));
        if self.accumulation == InsertSizeAccumulation::Sample {
            for (sample, summary) in &self.samples {
                output.push_str(&summary.picard_metric_rows(
                    Some(sample),
                    None,
                    None,
                    &orientations,
                    deviations,
                ));
            }
        } else if self.accumulation == InsertSizeAccumulation::Library {
            for (library, summary) in &self.libraries {
                output.push_str(&summary.summary.picard_metric_rows(
                    Some(&summary.sample),
                    Some(library),
                    None,
                    &orientations,
                    deviations,
                ));
            }
        } else if self.accumulation == InsertSizeAccumulation::ReadGroup {
            for (read_group, summary) in &self.read_groups {
                output.push_str(&summary.summary.picard_metric_rows(
                    Some(&summary.sample),
                    Some(&summary.library),
                    Some(read_group),
                    &orientations,
                    deviations,
                ));
            }
        }
        output.push('\n');
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        output.push_str("insert_size");
        for orientation in &orientations {
            output.push_str(&format!("\tAll_Reads.{}_count", orientation.suffix()));
        }
        if self.accumulation == InsertSizeAccumulation::Sample {
            for sample in self.samples.keys() {
                for orientation in &orientations {
                    output.push_str(&format!("\t{sample}.{}_count", orientation.suffix()));
                }
            }
        } else if self.accumulation == InsertSizeAccumulation::Library {
            for library in self.libraries.keys() {
                for orientation in &orientations {
                    output.push_str(&format!("\t{library}.{}_count", orientation.suffix()));
                }
            }
        } else if self.accumulation == InsertSizeAccumulation::ReadGroup {
            for read_group in self.read_groups.keys() {
                for orientation in &orientations {
                    output.push_str(&format!("\t{read_group}.{}_count", orientation.suffix()));
                }
            }
        }
        output.push('\n');

        let mut insert_sizes = self
            .all_reads
            .trimmed_insert_sizes(&orientations, deviations);
        for summary in self.samples.values() {
            insert_sizes.extend(summary.trimmed_insert_sizes(&orientations, deviations));
        }
        for summary in self.libraries.values() {
            insert_sizes.extend(
                summary
                    .summary
                    .trimmed_insert_sizes(&orientations, deviations),
            );
        }
        for summary in self.read_groups.values() {
            insert_sizes.extend(
                summary
                    .summary
                    .trimmed_insert_sizes(&orientations, deviations),
            );
        }
        for insert_size in insert_sizes {
            output.push_str(&format!("{insert_size}"));
            for orientation in &orientations {
                output.push_str(&format!(
                    "\t{}",
                    self.all_reads
                        .trimmed_count(*orientation, insert_size, deviations)
                ));
            }
            if self.accumulation == InsertSizeAccumulation::Sample {
                for summary in self.samples.values() {
                    for orientation in &orientations {
                        output.push_str(&format!(
                            "\t{}",
                            summary.trimmed_count(*orientation, insert_size, deviations)
                        ));
                    }
                }
            } else if self.accumulation == InsertSizeAccumulation::Library {
                for summary in self.libraries.values() {
                    for orientation in &orientations {
                        output.push_str(&format!(
                            "\t{}",
                            summary
                                .summary
                                .trimmed_count(*orientation, insert_size, deviations)
                        ));
                    }
                }
            } else if self.accumulation == InsertSizeAccumulation::ReadGroup {
                for summary in self.read_groups.values() {
                    for orientation in &orientations {
                        output.push_str(&format!(
                            "\t{}",
                            summary
                                .summary
                                .trimmed_count(*orientation, insert_size, deviations)
                        ));
                    }
                }
            }
            output.push('\n');
        }
        output
    }

    fn reportable_orientations(&self, minimum_pct: f64) -> Vec<InsertSizeOrientation> {
        let total = self.all_reads.total_count() as f64;
        let mut orientations = BTreeSet::new();
        for orientation in self.all_reads.orientations() {
            let count = self.all_reads.orientation_count(orientation) as f64;
            if total == 0.0 || count / total >= minimum_pct {
                orientations.insert(orientation);
            }
        }
        orientations.into_iter().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum InsertSizeOrientation {
    Fr,
    Rf,
    Tandem,
}

impl InsertSizeOrientation {
    fn label(self) -> &'static str {
        match self {
            InsertSizeOrientation::Fr => "FR",
            InsertSizeOrientation::Rf => "RF",
            InsertSizeOrientation::Tandem => "TANDEM",
        }
    }

    fn suffix(self) -> &'static str {
        match self {
            InsertSizeOrientation::Fr => "fr",
            InsertSizeOrientation::Rf => "rf",
            InsertSizeOrientation::Tandem => "tandem",
        }
    }
}

#[derive(Debug, Default)]
struct InsertSizeSummary {
    histograms: BTreeMap<InsertSizeOrientation, BTreeMap<u64, u64>>,
}

impl InsertSizeSummary {
    fn observe(&mut self, record: &bam::Record, include_duplicates: bool) -> bool {
        if !record.is_paired()
            || record.is_unmapped()
            || record.is_mate_unmapped()
            || record.is_secondary()
            || record.is_supplementary()
            || (record.is_duplicate() && !include_duplicates)
            || record.insert_size() == 0
            || !record.is_last_in_template()
        {
            return false;
        }
        let orientation = insert_size_orientation(
            record.is_reverse(),
            record.is_mate_reverse(),
            record.insert_size(),
        );
        *self
            .histograms
            .entry(orientation)
            .or_default()
            .entry(record.insert_size().unsigned_abs())
            .or_default() += 1;
        true
    }

    fn observe_sam_parts(
        &mut self,
        flags: u16,
        insert_size: i64,
        include_duplicates: bool,
    ) -> bool {
        if flags & 0x1 == 0
            || flags & 0x4 != 0
            || flags & 0x8 != 0
            || flags & 0x100 != 0
            || flags & 0x800 != 0
            || (flags & 0x400 != 0 && !include_duplicates)
            || flags & 0x80 == 0
            || insert_size == 0
        {
            return false;
        }
        let orientation = insert_size_orientation_from_flags(flags, insert_size);
        *self
            .histograms
            .entry(orientation)
            .or_default()
            .entry(insert_size.unsigned_abs())
            .or_default() += 1;
        true
    }

    fn picard_metric_rows(
        &self,
        sample: Option<&str>,
        library: Option<&str>,
        read_group: Option<&str>,
        orientations: &[InsertSizeOrientation],
        deviations: f64,
    ) -> String {
        let mut output = String::new();
        for orientation in orientations {
            if let Some(histogram) = self.histograms.get(orientation) {
                output.push_str(&picard_insert_size_metric_row(
                    histogram,
                    *orientation,
                    sample,
                    library,
                    read_group,
                    deviations,
                ));
            }
        }
        output
    }

    fn orientations(&self) -> BTreeSet<InsertSizeOrientation> {
        self.histograms.keys().copied().collect()
    }

    fn trimmed_insert_sizes(
        &self,
        orientations: &[InsertSizeOrientation],
        deviations: f64,
    ) -> BTreeSet<u64> {
        orientations
            .iter()
            .filter_map(|orientation| self.histograms.get(orientation))
            .flat_map(|histogram| {
                let width = insert_size_histogram_width(histogram, deviations);
                histogram.keys().copied().filter(move |size| *size <= width)
            })
            .collect()
    }

    fn trimmed_count(
        &self,
        orientation: InsertSizeOrientation,
        insert_size: u64,
        deviations: f64,
    ) -> u64 {
        let Some(histogram) = self.histograms.get(&orientation) else {
            return 0;
        };
        if insert_size > insert_size_histogram_width(histogram, deviations) {
            return 0;
        }
        histogram.get(&insert_size).copied().unwrap_or(0)
    }

    fn orientation_count(&self, orientation: InsertSizeOrientation) -> u64 {
        self.histograms
            .get(&orientation)
            .map(histogram_total_count)
            .unwrap_or(0)
    }

    fn total_count(&self) -> u64 {
        self.histograms.values().map(histogram_total_count).sum()
    }
}

fn insert_size_orientation(
    read_reverse: bool,
    mate_reverse: bool,
    insert_size: i64,
) -> InsertSizeOrientation {
    if read_reverse == mate_reverse {
        InsertSizeOrientation::Tandem
    } else if (!read_reverse && insert_size > 0) || (read_reverse && insert_size < 0) {
        InsertSizeOrientation::Fr
    } else if read_reverse {
        InsertSizeOrientation::Rf
    } else {
        InsertSizeOrientation::Rf
    }
}

fn insert_size_orientation_from_flags(flags: u16, insert_size: i64) -> InsertSizeOrientation {
    insert_size_orientation(flags & 0x10 != 0, flags & 0x20 != 0, insert_size)
}

fn picard_insert_size_metric_row(
    histogram: &BTreeMap<u64, u64>,
    orientation: InsertSizeOrientation,
    sample: Option<&str>,
    library: Option<&str>,
    read_group: Option<&str>,
    deviations: f64,
) -> String {
    let read_pairs = histogram_total_count(histogram);
    let median = histogram_median_f64(histogram);
    let mad = histogram_median_absolute_deviation(histogram, median);
    let min = histogram.keys().next().copied().unwrap_or(0);
    let max = histogram.keys().next_back().copied().unwrap_or(0);
    let trimmed = trimmed_histogram(
        histogram,
        insert_size_histogram_width(histogram, deviations),
    );
    let mean = histogram_mean(&trimmed);
    let stddev = if read_pairs < 2 {
        "?".to_string()
    } else {
        format_float(histogram_sample_standard_deviation(&trimmed, mean))
    };
    let mode = mode_from_histogram(histogram);
    let widths = insert_size_widths(histogram);

    format!(
        "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
        format_float(median),
        mode,
        format_float(mad),
        min,
        max,
        format_float(mean),
        stddev,
        read_pairs,
        orientation.label(),
        widths[0],
        widths[1],
        widths[2],
        widths[3],
        widths[4],
        widths[5],
        widths[6],
        widths[7],
        widths[8],
        widths[9],
        widths[10],
        sample.unwrap_or_default(),
        library.unwrap_or_default(),
        read_group.unwrap_or_default(),
    )
}

fn insert_size_histogram_width(histogram: &BTreeMap<u64, u64>, deviations: f64) -> u64 {
    let median = histogram_median_f64(histogram);
    let mad = histogram_median_absolute_deviation(histogram, median);
    (median + deviations * mad).max(0.0) as u64
}

fn trimmed_histogram(histogram: &BTreeMap<u64, u64>, width: u64) -> BTreeMap<u64, u64> {
    histogram
        .iter()
        .filter_map(|(insert_size, count)| {
            if *insert_size <= width {
                Some((*insert_size, *count))
            } else {
                None
            }
        })
        .collect()
}

fn histogram_total_count(histogram: &BTreeMap<u64, u64>) -> u64 {
    histogram.values().sum()
}

fn histogram_median_f64(histogram: &BTreeMap<u64, u64>) -> f64 {
    let total_count = histogram_total_count(histogram);
    if total_count == 0 {
        return 0.0;
    }
    if total_count % 2 == 1 {
        histogram_value_at_zero_based_rank(histogram, total_count / 2) as f64
    } else {
        let left = histogram_value_at_zero_based_rank(histogram, total_count / 2 - 1);
        let right = histogram_value_at_zero_based_rank(histogram, total_count / 2);
        (left + right) as f64 / 2.0
    }
}

fn histogram_value_at_zero_based_rank(histogram: &BTreeMap<u64, u64>, rank: u64) -> u64 {
    let mut cumulative = 0_u64;
    for (value, count) in histogram {
        cumulative += count;
        if cumulative > rank {
            return *value;
        }
    }
    histogram.keys().next_back().copied().unwrap_or(0)
}

fn histogram_median_absolute_deviation(histogram: &BTreeMap<u64, u64>, median: f64) -> f64 {
    let total_count = histogram_total_count(histogram);
    if total_count == 0 {
        return 0.0;
    }
    let mut deviations = histogram
        .iter()
        .map(|(value, count)| ((*value as f64 - median).abs(), *count))
        .collect::<Vec<_>>();
    deviations.sort_by(|left, right| left.0.partial_cmp(&right.0).unwrap_or(Ordering::Equal));
    if total_count % 2 == 1 {
        weighted_f64_value_at_zero_based_rank(&deviations, total_count / 2)
    } else {
        let left = weighted_f64_value_at_zero_based_rank(&deviations, total_count / 2 - 1);
        let right = weighted_f64_value_at_zero_based_rank(&deviations, total_count / 2);
        (left + right) / 2.0
    }
}

fn weighted_f64_value_at_zero_based_rank(values: &[(f64, u64)], rank: u64) -> f64 {
    let mut cumulative = 0_u64;
    for (value, count) in values {
        cumulative += count;
        if cumulative > rank {
            return *value;
        }
    }
    values.last().map(|(value, _)| *value).unwrap_or(0.0)
}

fn histogram_mean(histogram: &BTreeMap<u64, u64>) -> f64 {
    let total_count = histogram_total_count(histogram);
    if total_count == 0 {
        return 0.0;
    }
    histogram
        .iter()
        .map(|(value, count)| *value as f64 * *count as f64)
        .sum::<f64>()
        / total_count as f64
}

fn histogram_sample_standard_deviation(histogram: &BTreeMap<u64, u64>, mean: f64) -> f64 {
    let total_count = histogram_total_count(histogram);
    if total_count < 2 {
        return 0.0;
    }
    let variance = histogram
        .iter()
        .map(|(value, count)| {
            let delta = *value as f64 - mean;
            delta * delta * *count as f64
        })
        .sum::<f64>()
        / (total_count - 1) as f64;
    variance.sqrt()
}

fn mode_from_histogram(histogram: &BTreeMap<u64, u64>) -> u64 {
    histogram
        .iter()
        .max_by(|left, right| left.1.cmp(right.1).then_with(|| right.0.cmp(left.0)))
        .map(|(insert_size, _)| *insert_size)
        .unwrap_or(0)
}

fn insert_size_widths(histogram: &BTreeMap<u64, u64>) -> [u64; 11] {
    let mut widths = [0_u64; 11];
    if histogram.is_empty() {
        return widths;
    }
    let thresholds = [
        10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 95.0, 99.0,
    ];
    let total = histogram_total_count(histogram) as f64;
    let min = histogram.keys().next().copied().unwrap_or(0) as f64;
    let max = histogram.keys().next_back().copied().unwrap_or(0) as f64;
    let median = histogram_median_f64(histogram);
    let mut covered = 0.0;
    let mut low = median;
    let mut high = median;

    while low >= min || high <= max {
        if low >= 0.0 {
            covered += histogram.get(&(low as u64)).copied().unwrap_or(0) as f64;
        }
        if low != high && high >= 0.0 {
            covered += histogram.get(&(high as u64)).copied().unwrap_or(0) as f64;
        }
        let percent_covered = covered / total;
        let distance = (high - low) as u64 + 1;
        for (index, threshold) in thresholds.iter().enumerate() {
            if percent_covered >= threshold / 100.0 && widths[index] == 0 {
                widths[index] = distance;
            }
        }
        low -= 1.0;
        high += 1.0;
    }
    widths
}

fn skip_quality_metric_record(
    record: &bam::Record,
    aligned_reads_only: bool,
    pf_reads_only: bool,
) -> bool {
    record.is_secondary()
        || record.is_supplementary()
        || (aligned_reads_only && record.is_unmapped())
        || (pf_reads_only && record.is_quality_check_failed())
}

fn quality_values(record: &bam::Record, use_original_qualities: bool) -> Vec<u8> {
    if use_original_qualities {
        if let Some(qualities) = original_quality_values(record) {
            return qualities;
        }
    }
    record.qual().to_vec()
}

fn original_quality_values(record: &bam::Record) -> Option<Vec<u8>> {
    if let Ok(Aux::String(qualities)) = record.aux(b"OQ") {
        return Some(
            qualities
                .bytes()
                .map(|quality| quality.saturating_sub(33))
                .collect(),
        );
    }
    None
}

#[derive(Debug)]
struct ReadGroup {
    id: String,
    library: String,
    platform: String,
    platform_unit: String,
    sample: String,
    sequencing_center: Option<String>,
    description: Option<String>,
    run_date: Option<String>,
    predicted_insert_size: Option<String>,
    program_group: Option<String>,
    platform_model: Option<String>,
    key_sequence: Option<String>,
    flow_order: Option<String>,
}

struct FastqReadGroup {
    id: String,
    sample: String,
    library: Option<String>,
    platform: Option<String>,
    platform_unit: Option<String>,
    sequencing_center: Option<String>,
    description: Option<String>,
    run_date: Option<String>,
    predicted_insert_size: Option<String>,
    program_group: Option<String>,
    platform_model: Option<String>,
    sort_order: String,
    comments: Vec<String>,
}

#[derive(Debug, Default)]
struct FastqRecord {
    name: String,
    sequence: Vec<u8>,
    qualities: Vec<u8>,
}

struct FastqReader {
    reader: Box<dyn BufRead>,
    options: FastqToSamOptions,
    name_buf: String,
    sequence_buf: String,
    plus_buf: String,
    qualities_buf: String,
}

#[derive(Clone, Copy)]
struct FastqToSamOptions {
    quality_format: FastqQualityFormat,
    min_q: u8,
    max_q: u8,
    allow_and_ignore_empty_lines: bool,
    allow_empty_fastq: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FastqQualityFormat {
    Standard,
    Illumina,
    Solexa,
}

enum FastqToSamWriter {
    Sam(BufWriter<fs::File>),
    Bam(bam::Writer),
}

impl FastqToSamWriter {
    fn write_header(&mut self, read_group: &FastqReadGroup) -> Result<(), String> {
        match self {
            Self::Sam(writer) => writer
                .write_all(fastqtosam_header_text(read_group).as_bytes())
                .map_err(|error| error.to_string()),
            Self::Bam(_) => Ok(()),
        }
    }

    fn write_record(
        &mut self,
        read: &FastqRecord,
        flags: u16,
        read_group_id: &str,
        quality_format: FastqQualityFormat,
    ) -> Result<(), String> {
        match self {
            Self::Sam(writer) => {
                write_fastq_sam_record(writer, read, flags, read_group_id, quality_format)
            }
            Self::Bam(writer) => {
                let record = fastq_bam_record(read, flags, read_group_id, quality_format)?;
                writer.write(&record).map_err(|error| error.to_string())
            }
        }
    }
}

fn run_fastqtosam_standard_sam(
    fastq_paths: &[String],
    fastq2_paths: Option<&[String]>,
    output: &str,
    read_group: &FastqReadGroup,
    options: FastqToSamOptions,
) -> Result<(), String> {
    let mut writer = BufWriter::with_capacity(
        1024 * 1024,
        fs::File::create(output).map_err(|error| error.to_string())?,
    );
    writer
        .write_all(fastqtosam_header_text(read_group).as_bytes())
        .map_err(|error| error.to_string())?;
    let mut first_readers = fastq_paths
        .iter()
        .map(|path| FastqBytesReader::from_path(path, options))
        .collect::<Result<Vec<_>, _>>()?;
    let mut second_readers = match fastq2_paths {
        Some(paths) => Some(
            paths
                .iter()
                .map(|path| FastqBytesReader::from_path(path, options))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        None => None,
    };
    let mut first_reader_index = 0usize;
    let mut second_reader_index = 0usize;
    let mut first = FastqBytesRecord::default();
    let mut second = FastqBytesRecord::default();
    let mut output_buffer = Vec::with_capacity(8 * 1024 * 1024);
    let mut records_written = 0_u64;
    loop {
        if !next_fastq_bytes_record_from_readers(
            &mut first_readers,
            &mut first_reader_index,
            &mut first,
        )? {
            if let Some(readers) = second_readers.as_mut() {
                if next_fastq_bytes_record_from_readers(
                    readers,
                    &mut second_reader_index,
                    &mut second,
                )? {
                    return Err(
                        "malformed FastqToSam FASTQ2 has more records than FASTQ".to_string()
                    );
                }
            }
            break;
        }
        if let Some(readers) = second_readers.as_mut() {
            if !next_fastq_bytes_record_from_readers(
                readers,
                &mut second_reader_index,
                &mut second,
            )? {
                return Err("malformed FastqToSam FASTQ has more records than FASTQ2".to_string());
            }
            if first.name != second.name {
                return Err(format!(
                    "malformed FastqToSam paired read names differ: {} vs {}",
                    String::from_utf8_lossy(&first.name),
                    String::from_utf8_lossy(&second.name),
                ));
            }
            append_fastq_sam_bytes_record(&mut output_buffer, &first, 77, &read_group.id);
            append_fastq_sam_bytes_record(&mut output_buffer, &second, 141, &read_group.id);
            records_written += 2;
            flush_large_fastqtosam_buffer(&mut writer, &mut output_buffer)?;
        } else {
            append_fastq_sam_bytes_record(&mut output_buffer, &first, 4, &read_group.id);
            records_written += 1;
            flush_large_fastqtosam_buffer(&mut writer, &mut output_buffer)?;
        }
    }
    if records_written == 0 && !options.allow_empty_fastq {
        return Err("malformed FastqToSam empty FASTQ input".to_string());
    }
    if !output_buffer.is_empty() {
        writer
            .write_all(&output_buffer)
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn flush_large_fastqtosam_buffer(
    writer: &mut BufWriter<fs::File>,
    output_buffer: &mut Vec<u8>,
) -> Result<(), String> {
    if output_buffer.len() >= 8 * 1024 * 1024 {
        writer
            .write_all(output_buffer)
            .map_err(|error| error.to_string())?;
        output_buffer.clear();
    }
    Ok(())
}

#[derive(Default)]
struct FastqBytesRecord {
    name: Vec<u8>,
    sequence: Vec<u8>,
    qualities: Vec<u8>,
}

struct FastqBytesReader {
    reader: FastqBytesReaderSource,
    options: FastqToSamOptions,
    name_buf: Vec<u8>,
    plus_buf: Vec<u8>,
}

enum FastqBytesReaderSource {
    Plain(BufReader<fs::File>),
    Gzip(BufReader<GzDecoder<fs::File>>),
}

impl FastqBytesReaderSource {
    fn read_until(&mut self, byte: u8, buffer: &mut Vec<u8>) -> std::io::Result<usize> {
        match self {
            Self::Plain(reader) => reader.read_until(byte, buffer),
            Self::Gzip(reader) => reader.read_until(byte, buffer),
        }
    }
}

impl FastqBytesReader {
    fn from_path(path: &str, options: FastqToSamOptions) -> Result<Self, String> {
        let file = fs::File::open(path).map_err(|error| error.to_string())?;
        let reader = if has_gzip_extension(path) {
            FastqBytesReaderSource::Gzip(BufReader::with_capacity(
                1024 * 1024,
                GzDecoder::new(file),
            ))
        } else {
            FastqBytesReaderSource::Plain(BufReader::with_capacity(1024 * 1024, file))
        };
        Ok(Self {
            reader,
            options,
            name_buf: Vec::new(),
            plus_buf: Vec::new(),
        })
    }

    fn next_record_into(&mut self, record: &mut FastqBytesRecord) -> Result<bool, String> {
        if !read_fastq_bytes_line(
            &mut self.reader,
            &mut self.name_buf,
            self.options.allow_and_ignore_empty_lines,
        )? {
            return Ok(false);
        }
        if !read_fastq_bytes_line(
            &mut self.reader,
            &mut record.sequence,
            self.options.allow_and_ignore_empty_lines,
        )? || !read_fastq_bytes_line(
            &mut self.reader,
            &mut self.plus_buf,
            self.options.allow_and_ignore_empty_lines,
        )? || !read_fastq_bytes_line(
            &mut self.reader,
            &mut record.qualities,
            self.options.allow_and_ignore_empty_lines,
        )? {
            return Err("malformed FastqToSam FASTQ record".to_string());
        }
        if !self.name_buf.starts_with(b"@") || !self.plus_buf.starts_with(b"+") {
            return Err("malformed FastqToSam FASTQ record".to_string());
        }
        if record.sequence.len() != record.qualities.len() {
            return Err("malformed FastqToSam FASTQ sequence/quality length mismatch".to_string());
        }
        validate_fastq_qualities(&record.qualities, self.options)?;
        record.name.clear();
        push_normalized_fastq_read_name_bytes(&self.name_buf[1..], &mut record.name);
        Ok(true)
    }
}

fn next_fastq_bytes_record_from_readers(
    readers: &mut [FastqBytesReader],
    index: &mut usize,
    record: &mut FastqBytesRecord,
) -> Result<bool, String> {
    while *index < readers.len() {
        if readers[*index].next_record_into(record)? {
            return Ok(true);
        }
        *index += 1;
    }
    Ok(false)
}

fn detect_fastqtosam_quality_format(
    fastq_paths: &[String],
    fastq2_paths: Option<&[String]>,
    allow_and_ignore_empty_lines: bool,
) -> Result<FastqQualityFormat, String> {
    let mut saw_quality = false;
    let mut saw_standard_only_quality = false;
    for fastq in fastq_paths {
        scan_fastqtosam_quality_file(
            fastq,
            allow_and_ignore_empty_lines,
            &mut saw_quality,
            &mut saw_standard_only_quality,
        )?;
    }
    if let Some(fastq2_paths) = fastq2_paths {
        for fastq2 in fastq2_paths {
            scan_fastqtosam_quality_file(
                fastq2,
                allow_and_ignore_empty_lines,
                &mut saw_quality,
                &mut saw_standard_only_quality,
            )?;
        }
    }

    if !saw_quality || saw_standard_only_quality {
        Ok(FastqQualityFormat::Standard)
    } else {
        Ok(FastqQualityFormat::Illumina)
    }
}

fn scan_fastqtosam_quality_file(
    path: &str,
    allow_and_ignore_empty_lines: bool,
    saw_quality: &mut bool,
    saw_standard_only_quality: &mut bool,
) -> Result<(), String> {
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut reader = if has_gzip_extension(path) {
        FastqBytesReaderSource::Gzip(BufReader::with_capacity(1024 * 1024, GzDecoder::new(file)))
    } else {
        FastqBytesReaderSource::Plain(BufReader::with_capacity(1024 * 1024, file))
    };
    let mut name = Vec::new();
    let mut sequence = Vec::new();
    let mut plus = Vec::new();
    let mut qualities = Vec::new();

    loop {
        if !read_fastq_bytes_line(&mut reader, &mut name, allow_and_ignore_empty_lines)? {
            return Ok(());
        }
        if !read_fastq_bytes_line(&mut reader, &mut sequence, allow_and_ignore_empty_lines)?
            || !read_fastq_bytes_line(&mut reader, &mut plus, allow_and_ignore_empty_lines)?
            || !read_fastq_bytes_line(&mut reader, &mut qualities, allow_and_ignore_empty_lines)?
        {
            return Err("malformed FastqToSam FASTQ record".to_string());
        }
        if !name.starts_with(b"@") || !plus.starts_with(b"+") || sequence.len() != qualities.len() {
            return Err("malformed FastqToSam FASTQ record".to_string());
        }
        if !qualities.is_empty() {
            *saw_quality = true;
        }
        if qualities.iter().any(|quality| *quality < 64) {
            *saw_standard_only_quality = true;
        }
    }
}

fn read_fastq_bytes_line(
    reader: &mut FastqBytesReaderSource,
    buffer: &mut Vec<u8>,
    skip_empty: bool,
) -> Result<bool, String> {
    loop {
        buffer.clear();
        if reader
            .read_until(b'\n', buffer)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(false);
        }
        trim_ascii_line_end_bytes(buffer);
        if !skip_empty || !buffer.is_empty() {
            return Ok(true);
        }
    }
}

fn trim_ascii_line_end_bytes(value: &mut Vec<u8>) {
    while value.ends_with(b"\n") || value.ends_with(b"\r") {
        value.pop();
    }
}

fn append_fastq_sam_bytes_record(
    output: &mut Vec<u8>,
    read: &FastqBytesRecord,
    flags: u16,
    read_group_id: &str,
) {
    output.extend_from_slice(&read.name);
    output.extend_from_slice(fastqtosam_flag_prefix(flags));
    output.extend_from_slice(&read.sequence);
    output.extend_from_slice(b"\t");
    output.extend_from_slice(&read.qualities);
    output.extend_from_slice(b"\tRG:Z:");
    output.extend_from_slice(read_group_id.as_bytes());
    output.extend_from_slice(b"\n");
}

impl FastqReader {
    fn from_path(path: &str, options: FastqToSamOptions) -> Result<Self, String> {
        let file = fs::File::open(path).map_err(|error| error.to_string())?;
        let reader: Box<dyn BufRead> = if has_gzip_extension(path) {
            Box::new(BufReader::with_capacity(1024 * 1024, GzDecoder::new(file)))
        } else {
            Box::new(BufReader::with_capacity(1024 * 1024, file))
        };
        Ok(Self {
            reader,
            options,
            name_buf: String::new(),
            sequence_buf: String::new(),
            plus_buf: String::new(),
            qualities_buf: String::new(),
        })
    }

    fn next_record_into(&mut self, record: &mut FastqRecord) -> Result<bool, String> {
        if !read_fastq_string_line(
            self.reader.as_mut(),
            &mut self.name_buf,
            self.options.allow_and_ignore_empty_lines,
        )? {
            return Ok(false);
        }
        if !read_fastq_string_line(
            self.reader.as_mut(),
            &mut self.sequence_buf,
            self.options.allow_and_ignore_empty_lines,
        )? || !read_fastq_string_line(
            self.reader.as_mut(),
            &mut self.plus_buf,
            self.options.allow_and_ignore_empty_lines,
        )? || !read_fastq_string_line(
            self.reader.as_mut(),
            &mut self.qualities_buf,
            self.options.allow_and_ignore_empty_lines,
        )? {
            return Err("malformed FastqToSam FASTQ record".to_string());
        }
        let name = self.name_buf.as_str();
        if !name.starts_with('@') || !self.plus_buf.starts_with('+') {
            return Err("malformed FastqToSam FASTQ record".to_string());
        }
        let sequence = self.sequence_buf.as_bytes();
        let qualities = self.qualities_buf.as_bytes();
        if sequence.len() != qualities.len() {
            return Err("malformed FastqToSam FASTQ sequence/quality length mismatch".to_string());
        }
        validate_fastq_qualities(qualities, self.options)?;
        record.name.clear();
        push_normalized_fastq_read_name(&name[1..], &mut record.name);
        record.sequence.clear();
        record.sequence.extend_from_slice(sequence);
        record.qualities.clear();
        record.qualities.extend_from_slice(qualities);
        Ok(true)
    }
}

fn next_fastq_record_from_readers(
    readers: &mut [FastqReader],
    index: &mut usize,
    record: &mut FastqRecord,
) -> Result<bool, String> {
    while *index < readers.len() {
        if readers[*index].next_record_into(record)? {
            return Ok(true);
        }
        *index += 1;
    }
    Ok(false)
}

fn read_fastq_string_line(
    reader: &mut dyn BufRead,
    buffer: &mut String,
    skip_empty: bool,
) -> Result<bool, String> {
    loop {
        buffer.clear();
        if reader
            .read_line(buffer)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(false);
        }
        while buffer.ends_with('\n') || buffer.ends_with('\r') {
            buffer.pop();
        }
        if !skip_empty || !buffer.is_empty() {
            return Ok(true);
        }
    }
}

fn validate_fastq_qualities(qualities: &[u8], options: FastqToSamOptions) -> Result<(), String> {
    for quality in qualities {
        let decoded = decode_fastq_quality(*quality, options.quality_format)?;
        if decoded < options.min_q {
            return Err(format!(
                "malformed FastqToSam quality below MIN_Q: {decoded} < {}",
                options.min_q
            ));
        }
        if decoded > options.max_q {
            return Err(format!(
                "malformed FastqToSam quality above MAX_Q: {decoded} > {}",
                options.max_q
            ));
        }
    }
    Ok(())
}

fn decode_fastq_quality(quality: u8, format: FastqQualityFormat) -> Result<u8, String> {
    match format {
        FastqQualityFormat::Standard => quality
            .checked_sub(33)
            .ok_or_else(|| "malformed FastqToSam quality below encoding offset".to_string()),
        FastqQualityFormat::Illumina => quality
            .checked_sub(64)
            .ok_or_else(|| "malformed FastqToSam quality below encoding offset".to_string()),
        FastqQualityFormat::Solexa => {
            let solexa = i16::from(quality) - 64;
            let phred = 10.0 * (1.0 + 10_f64.powf(f64::from(solexa) / 10.0)).log10();
            let rounded = phred.round();
            if !(0.0..=f64::from(u8::MAX)).contains(&rounded) {
                return Err("malformed FastqToSam quality below encoding offset".to_string());
            }
            Ok(rounded as u8)
        }
    }
}

fn fastqtosam_header_text(read_group: &FastqReadGroup) -> String {
    let mut text = String::new();
    text.push_str("@HD\tVN:1.6\tSO:");
    text.push_str(&read_group.sort_order);
    text.push('\n');
    text.push_str("@RG");
    push_sam_tag(&mut text, "ID", Some(&read_group.id));
    push_sam_tag(&mut text, "SM", Some(&read_group.sample));
    push_sam_tag(&mut text, "LB", read_group.library.as_deref());
    push_sam_tag(&mut text, "PL", read_group.platform.as_deref());
    push_sam_tag(&mut text, "PU", read_group.platform_unit.as_deref());
    push_sam_tag(&mut text, "CN", read_group.sequencing_center.as_deref());
    push_sam_tag(&mut text, "DS", read_group.description.as_deref());
    push_sam_tag(&mut text, "DT", read_group.run_date.as_deref());
    push_sam_tag(&mut text, "PI", read_group.predicted_insert_size.as_deref());
    push_sam_tag(&mut text, "PG", read_group.program_group.as_deref());
    push_sam_tag(&mut text, "PM", read_group.platform_model.as_deref());
    text.push('\n');
    for comment in &read_group.comments {
        text.push_str("@CO\t");
        text.push_str(comment);
        text.push('\n');
    }
    text
}

fn push_sam_tag(text: &mut String, tag: &str, value: Option<&str>) {
    if let Some(value) = value {
        text.push('\t');
        text.push_str(tag);
        text.push(':');
        text.push_str(value);
    }
}

fn sequential_fastq_paths(base_fastq: &str) -> Result<Vec<String>, String> {
    const SUFFIXES: [(&str, &str); 4] = [
        ("_001.fastq.gz", ".fastq.gz"),
        ("_001.fq.gz", ".fq.gz"),
        ("_001.fastq", ".fastq"),
        ("_001.fq", ".fq"),
    ];
    let Some((matched_suffix, extension)) = SUFFIXES
        .iter()
        .find(|(suffix, _)| base_fastq.ends_with(*suffix))
        .copied()
    else {
        return Err(format!(
            "Could not parse the FASTQ extension (expected '_001' + ['.fastq', '.fastq.gz', '.fq', '.fq.gz']): {base_fastq}"
        ));
    };

    let mut files = vec![base_fastq.to_string()];
    let prefix = &base_fastq[..base_fastq.len() - matched_suffix.len()];
    for idx in 2.. {
        let candidate = format!("{prefix}_{idx:03}{extension}");
        if !Path::new(&candidate).is_file() {
            break;
        }
        files.push(candidate);
    }
    Ok(files)
}

fn write_fastq_sam_record(
    writer: &mut dyn Write,
    read: &FastqRecord,
    flags: u16,
    read_group_id: &str,
    quality_format: FastqQualityFormat,
) -> Result<(), String> {
    let converted_qualities;
    let qualities = if quality_format == FastqQualityFormat::Standard {
        read.qualities.as_slice()
    } else {
        converted_qualities = read
            .qualities
            .iter()
            .map(|quality| decode_fastq_quality(*quality, quality_format).map(|value| value + 33))
            .collect::<Result<Vec<_>, _>>()?;
        converted_qualities.as_slice()
    };
    writer
        .write_all(read.name.as_bytes())
        .and_then(|_| writer.write_all(fastqtosam_flag_prefix(flags)))
        .and_then(|_| writer.write_all(&read.sequence))
        .and_then(|_| writer.write_all(b"\t"))
        .and_then(|_| writer.write_all(&qualities))
        .and_then(|_| writer.write_all(b"\tRG:Z:"))
        .and_then(|_| writer.write_all(read_group_id.as_bytes()))
        .and_then(|_| writer.write_all(b"\n"))
        .map_err(|error| error.to_string())
}

fn fastqtosam_flag_prefix(flags: u16) -> &'static [u8] {
    match flags {
        4 => b"\t4\t*\t0\t0\t*\t*\t0\t0\t",
        77 => b"\t77\t*\t0\t0\t*\t*\t0\t0\t",
        141 => b"\t141\t*\t0\t0\t*\t*\t0\t0\t",
        _ => b"\t0\t*\t0\t0\t*\t*\t0\t0\t",
    }
}

fn push_normalized_fastq_read_name(name: &str, output: &mut String) {
    let name = name.split_ascii_whitespace().next().unwrap_or(name);
    output.push_str(
        name.strip_suffix("/1")
            .or_else(|| name.strip_suffix("/2"))
            .unwrap_or(name),
    );
}

fn push_normalized_fastq_read_name_bytes(name: &[u8], output: &mut Vec<u8>) {
    let end = name
        .iter()
        .position(|byte| byte.is_ascii_whitespace())
        .unwrap_or(name.len());
    let mut name = &name[..end];
    if name.ends_with(b"/1") || name.ends_with(b"/2") {
        name = &name[..name.len() - 2];
    }
    output.extend_from_slice(name);
}

fn fastq_bam_record(
    read: &FastqRecord,
    flags: u16,
    read_group_id: &str,
    quality_format: FastqQualityFormat,
) -> Result<bam::Record, String> {
    let qualities = read
        .qualities
        .iter()
        .map(|quality| decode_fastq_quality(*quality, quality_format))
        .collect::<Result<Vec<_>, _>>()?;
    let mut record = bam::Record::new();
    record.set(read.name.as_bytes(), None, &read.sequence, &qualities);
    record.set_flags(flags);
    record.set_tid(-1);
    record.set_pos(-1);
    record.set_mapq(0);
    record.set_mtid(-1);
    record.set_mpos(-1);
    record.set_insert_size(0);
    set_record_read_group(&mut record, read_group_id)?;
    Ok(record)
}

fn fastqtosam_header(read_group: &FastqReadGroup) -> bam::Header {
    let mut header = bam::Header::new();
    header.push_record(
        HeaderRecord::new(b"HD")
            .push_tag(b"VN", "1.6")
            .push_tag(b"SO", &read_group.sort_order),
    );
    let mut rg_record = HeaderRecord::new(b"RG");
    rg_record
        .push_tag(b"ID", &read_group.id)
        .push_tag(b"SM", &read_group.sample);
    push_optional_header_tag(&mut rg_record, b"LB", read_group.library.as_deref());
    push_optional_header_tag(&mut rg_record, b"PL", read_group.platform.as_deref());
    push_optional_header_tag(&mut rg_record, b"PU", read_group.platform_unit.as_deref());
    push_optional_header_tag(
        &mut rg_record,
        b"CN",
        read_group.sequencing_center.as_deref(),
    );
    push_optional_header_tag(&mut rg_record, b"DS", read_group.description.as_deref());
    push_optional_header_tag(&mut rg_record, b"DT", read_group.run_date.as_deref());
    push_optional_header_tag(
        &mut rg_record,
        b"PI",
        read_group.predicted_insert_size.as_deref(),
    );
    push_optional_header_tag(&mut rg_record, b"PG", read_group.program_group.as_deref());
    push_optional_header_tag(&mut rg_record, b"PM", read_group.platform_model.as_deref());
    header.push_record(&rg_record);
    for comment in &read_group.comments {
        header.push_comment(comment.as_bytes());
    }
    header
}

fn reject_unsupported_addorreplacereadgroups_args(
    args: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "RGID",
        "RGLB",
        "RGPL",
        "RGPU",
        "RGSM",
        "RGCN",
        "RGDS",
        "RGDT",
        "RGPI",
        "RGPG",
        "RGPM",
        "RGKS",
        "RGFO",
        "REFERENCE_SEQUENCE",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!(
                "unsupported AddOrReplaceReadGroups argument: {key}"
            ));
        }
    }

    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported AddOrReplaceReadGroups COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    Ok(())
}

fn read_group_header(source: &bam::HeaderView, read_group: &ReadGroup) -> bam::Header {
    let header_text = String::from_utf8_lossy(source.as_bytes());
    let mut header = bam::Header::new();

    for line in header_text.lines() {
        if line.is_empty() || line.starts_with("@RG\t") {
            continue;
        }
        if line.starts_with("@CO") {
            header.push_comment(line.strip_prefix("@CO\t").unwrap_or("").as_bytes());
            continue;
        }
        let Some(record_type) = line.get(1..3) else {
            continue;
        };
        let mut record = HeaderRecord::new(record_type.as_bytes());
        for field in line.split('\t').skip(1) {
            let Some((tag, value)) = field.split_once(':') else {
                continue;
            };
            record.push_tag(tag.as_bytes(), value);
        }
        header.push_record(&record);
    }

    let mut rg_record = HeaderRecord::new(b"RG");
    rg_record
        .push_tag(b"ID", &read_group.id)
        .push_tag(b"LB", &read_group.library)
        .push_tag(b"PL", &read_group.platform)
        .push_tag(b"SM", &read_group.sample)
        .push_tag(b"PU", &read_group.platform_unit);
    push_optional_header_tag(
        &mut rg_record,
        b"CN",
        read_group.sequencing_center.as_deref(),
    );
    push_optional_header_tag(&mut rg_record, b"DS", read_group.description.as_deref());
    push_optional_header_tag(&mut rg_record, b"DT", read_group.run_date.as_deref());
    push_optional_header_tag(
        &mut rg_record,
        b"PI",
        read_group.predicted_insert_size.as_deref(),
    );
    push_optional_header_tag(&mut rg_record, b"PG", read_group.program_group.as_deref());
    push_optional_header_tag(&mut rg_record, b"PM", read_group.platform_model.as_deref());
    push_optional_header_tag(&mut rg_record, b"KS", read_group.key_sequence.as_deref());
    push_optional_header_tag(&mut rg_record, b"FO", read_group.flow_order.as_deref());
    header.push_record(&rg_record);

    header
}

fn push_optional_header_tag(
    record: &mut HeaderRecord<'_>,
    tag: &'static [u8],
    value: Option<&str>,
) {
    if let Some(value) = value {
        record.push_tag(tag, value);
    }
}

fn set_record_read_group(record: &mut bam::Record, read_group_id: &str) -> Result<(), String> {
    if record.aux(b"RG").is_ok() {
        record
            .remove_aux(b"RG")
            .map_err(|error| error.to_string())?;
    }
    record
        .push_aux(b"RG", Aux::String(read_group_id))
        .map_err(|error| error.to_string())
}

fn reject_unsupported_samtofastq_args(
    args: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "FASTQ",
        "SECOND_END_FASTQ",
        "UNPAIRED_FASTQ",
        "INTERLEAVE",
        "RE_REVERSE",
        "READ1_TRIM",
        "READ2_TRIM",
        "READ1_MAX_BASES_TO_WRITE",
        "READ2_MAX_BASES_TO_WRITE",
        "QUALITY",
        "CLIPPING_ATTRIBUTE",
        "CLIPPING_ACTION",
        "CLIPPING_MIN_LENGTH",
        "OUTPUT_PER_RG",
        "COMPRESS_OUTPUTS_PER_RG",
        "RG_TAG",
        "OUTPUT_DIR",
        "INCLUDE_NON_PF_READS",
        "INCLUDE_NON_PRIMARY_ALIGNMENTS",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "CREATE_MD5_FILE",
        "CREATE_INDEX",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported SamToFastq argument: {key}"));
        }
    }

    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    let output_per_rg = optional_bool(args, "OUTPUT_PER_RG")?.unwrap_or(false);
    let compress_outputs_per_rg = optional_bool(args, "COMPRESS_OUTPUTS_PER_RG")?.unwrap_or(false);
    optional_bool(args, "INTERLEAVE")?;
    optional_bool(args, "RE_REVERSE")?;
    if output_per_rg {
        if args.contains_key("FASTQ")
            || args.contains_key("SECOND_END_FASTQ")
            || args.contains_key("UNPAIRED_FASTQ")
        {
            return Err(
                "SamToFastq OUTPUT_PER_RG cannot be combined with FASTQ, SECOND_END_FASTQ, or UNPAIRED_FASTQ"
                    .to_string(),
            );
        }
    } else if args.contains_key("RG_TAG") || args.contains_key("OUTPUT_DIR") {
        return Err("SamToFastq RG_TAG and OUTPUT_DIR require OUTPUT_PER_RG=true".to_string());
    }
    if compress_outputs_per_rg {
        if args.contains_key("FASTQ")
            || args.contains_key("SECOND_END_FASTQ")
            || args.contains_key("UNPAIRED_FASTQ")
        {
            return Err(
                "SamToFastq COMPRESS_OUTPUTS_PER_RG cannot be combined with FASTQ, SECOND_END_FASTQ, or UNPAIRED_FASTQ"
                    .to_string(),
            );
        }
        if !output_per_rg {
            return Err(
                "SamToFastq COMPRESS_OUTPUTS_PER_RG requires OUTPUT_PER_RG=true".to_string(),
            );
        }
    }
    if let Some(tag) = optional_scalar(args, "RG_TAG")? {
        match tag.as_str() {
            "PU" | "pu" | "ID" | "id" => {}
            _ => return Err(format!("unsupported SamToFastq RG_TAG: {tag}")),
        }
    }
    optional_u32(args, "READ1_TRIM")?;
    optional_u32(args, "READ2_TRIM")?;
    optional_u32(args, "READ1_MAX_BASES_TO_WRITE")?;
    optional_u32(args, "READ2_MAX_BASES_TO_WRITE")?;
    optional_u32(args, "QUALITY")?;
    optional_scalar(args, "CLIPPING_ATTRIBUTE")?;
    optional_scalar(args, "CLIPPING_ACTION")?;
    optional_u32(args, "CLIPPING_MIN_LENGTH")?;
    if args.contains_key("CLIPPING_ATTRIBUTE") != args.contains_key("CLIPPING_ACTION") {
        return Err(
            "unsupported SamToFastq clipping requires both CLIPPING_ATTRIBUTE and CLIPPING_ACTION"
                .to_string(),
        );
    }
    if let Some(action) = optional_scalar(args, "CLIPPING_ACTION")? {
        if !matches!(action.as_str(), "N" | "X") && action.parse::<i32>().is_err() {
            return Err("unsupported SamToFastq CLIPPING_ACTION".to_string());
        }
    }
    optional_bool(args, "INCLUDE_NON_PF_READS")?;
    optional_bool(args, "INCLUDE_NON_PRIMARY_ALIGNMENTS")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported SamToFastq COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn reject_unsupported_fastqtosam_args(
    args: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "FASTQ",
        "FASTQ2",
        "USE_SEQUENTIAL_FASTQS",
        "OUTPUT",
        "SAMPLE_NAME",
        "READ_GROUP_NAME",
        "LIBRARY_NAME",
        "PLATFORM",
        "PLATFORM_UNIT",
        "SEQUENCING_CENTER",
        "DESCRIPTION",
        "RUN_DATE",
        "PREDICTED_INSERT_SIZE",
        "PROGRAM_GROUP",
        "PLATFORM_MODEL",
        "QUALITY_FORMAT",
        "MIN_Q",
        "MAX_Q",
        "ALLOW_AND_IGNORE_EMPTY_LINES",
        "ALLOW_EMPTY_FASTQ",
        "SORT_ORDER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "CREATE_MD5_FILE",
        "CREATE_INDEX",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "USE_JDK_DEFLATER",
        "USE_JDK_INFLATER",
        "COMMENT",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported FastqToSam argument: {key}"));
        }
    }
    if let Some(sort_order) = optional_scalar(args, "SORT_ORDER")? {
        if sort_order != "queryname" && sort_order != "coordinate" && sort_order != "unsorted" {
            return Err(format!("unsupported FastqToSam SORT_ORDER={sort_order}"));
        }
    }
    optional_u32(args, "MIN_Q")?;
    optional_u32(args, "MAX_Q")?;
    optional_bool(args, "USE_SEQUENTIAL_FASTQS")?;
    optional_bool(args, "ALLOW_AND_IGNORE_EMPTY_LINES")?;
    optional_bool(args, "ALLOW_EMPTY_FASTQ")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    optional_bool(args, "USE_JDK_DEFLATER")?;
    optional_bool(args, "USE_JDK_INFLATER")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported FastqToSam COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn fastq_writer(path: &str, compression_level: u32) -> Result<Box<dyn Write>, String> {
    let file = fs::File::create(path).map_err(|error| error.to_string())?;
    let writer = BufWriter::with_capacity(1024 * 1024, file);
    if has_gzip_extension(path) {
        Ok(Box::new(GzEncoder::new(
            writer,
            Compression::new(compression_level),
        )))
    } else {
        Ok(Box::new(writer))
    }
}

#[allow(clippy::too_many_arguments)]
fn run_samtofastq_from_sam_text(
    input: &str,
    fastq: Option<&str>,
    second_end_fastq: Option<&str>,
    unpaired_fastq: Option<&str>,
    interleave: bool,
    re_reverse: bool,
    include_non_pf_reads: bool,
    include_non_primary_alignments: bool,
    compression_level: u32,
    create_md5_file: bool,
    transform: SamToFastqTransform,
    per_rg: Option<SamToFastqPerRgMode>,
) -> Result<(), String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut first_writer = match fastq {
        Some(path) => Some(fastq_writer(path, compression_level)?),
        None => None,
    };
    let mut second_writer = match second_end_fastq {
        Some(path) => Some(fastq_writer(path, compression_level)?),
        None => None,
    };
    let mut unpaired_writer = match unpaired_fastq {
        Some(path) => Some(fastq_writer(path, compression_level)?),
        None => None,
    };
    let mut line = String::new();
    let mut sequence = Vec::new();
    let mut qualities = Vec::new();
    let mut output = Vec::with_capacity(512);
    let mut first_seen_mates: HashMap<String, SamFastqRecord> = HashMap::new();
    let mut per_rg_outputs =
        per_rg.map(|config| SamToFastqPerRgOutputs::new(config, compression_level));

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
        }
        if line.starts_with("@RG\t") {
            if let Some(outputs) = per_rg_outputs.as_mut() {
                outputs.observe_sam_read_group_line(line.trim_end_matches(['\r', '\n']))?;
            }
            continue;
        }
        if line.starts_with('@') || line.trim().is_empty() {
            continue;
        }
        let line = line.trim_end_matches(['\r', '\n']);
        let (name, flags, sam_sequence, sam_qualities) = sam_fastq_fields(line)?;
        if flags & 0x200 != 0 && !include_non_pf_reads {
            continue;
        }
        if flags & (0x100 | 0x800) != 0 && !include_non_primary_alignments {
            continue;
        }
        let is_paired = flags & 0x1 != 0;
        if is_paired && !interleave && second_writer.is_none() && per_rg_outputs.is_none() {
            return Err(
                "SamToFastq input contains paired reads but no SECOND_END_FASTQ was specified"
                    .to_string(),
            );
        }

        let current_record = SamFastqRecord {
            name: name.to_string(),
            flags,
            sequence: sam_sequence.to_string(),
            qualities: sam_qualities.to_string(),
            clip_point: sam_clip_point(line, transform.clipping),
        };
        if is_paired {
            if let Some(first_record) = first_seen_mates.remove(name) {
                let (read1, read2) = if flags & 0x40 != 0 {
                    (&current_record, &first_record)
                } else {
                    (&first_record, &current_record)
                };
                if let Some(outputs) = per_rg_outputs.as_mut() {
                    outputs.write_sam_pair(
                        line,
                        read1,
                        read2,
                        &transform,
                        re_reverse,
                        &mut sequence,
                        &mut qualities,
                        &mut output,
                    )?;
                } else {
                    let first = first_writer
                        .as_mut()
                        .expect("first writer exists for standard SamToFastq output");
                    write_sam_fastq_record(
                        first.as_mut(),
                        read1,
                        &transform,
                        re_reverse,
                        transform.trim_for_flags(read1.flags),
                        transform.quality,
                        transform.max_bases_for_flags(read1.flags),
                        &mut sequence,
                        &mut qualities,
                        &mut output,
                    )?;
                    let writer = if interleave {
                        first.as_mut()
                    } else {
                        second_writer
                            .as_mut()
                            .expect("second writer exists for paired output")
                            .as_mut()
                    };
                    write_sam_fastq_record(
                        writer,
                        read2,
                        &transform,
                        re_reverse,
                        transform.trim_for_flags(read2.flags),
                        transform.quality,
                        transform.max_bases_for_flags(read2.flags),
                        &mut sequence,
                        &mut qualities,
                        &mut output,
                    )?;
                }
            } else {
                first_seen_mates.insert(name.to_string(), current_record);
            }
        } else {
            let writer: &mut dyn Write = if let Some(outputs) = per_rg_outputs.as_mut() {
                outputs.unpaired_writer_for_sam_record(line)?
            } else if let Some(writer) = unpaired_writer.as_mut() {
                writer.as_mut()
            } else {
                first_writer
                    .as_mut()
                    .expect("first writer exists for standard SamToFastq output")
                    .as_mut()
            };
            write_sam_fastq_record(
                writer,
                &current_record,
                &transform,
                re_reverse,
                transform.trim_for_flags(current_record.flags),
                transform.quality,
                transform.max_bases_for_flags(current_record.flags),
                &mut sequence,
                &mut qualities,
                &mut output,
            )?;
        }
    }

    if let Some(writer) = first_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    if let Some(writer) = second_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    if let Some(writer) = unpaired_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    if let Some(outputs) = per_rg_outputs.as_mut() {
        outputs.flush_all()?;
    }
    drop(first_writer);
    drop(second_writer);
    drop(unpaired_writer);
    if let Some(outputs) = per_rg_outputs {
        outputs.write_md5_sidecars(create_md5_file)
    } else {
        write_samtofastq_sidecars(
            fastq.expect("fastq path exists"),
            second_end_fastq,
            unpaired_fastq,
            create_md5_file,
        )
    }
}

#[derive(Clone)]
struct SamToFastqPerRgMode {
    tag: SamToFastqReadGroupTag,
    output_dir: Option<String>,
    compress: bool,
    interleave: bool,
}

impl SamToFastqPerRgMode {
    fn new(
        rg_tag: String,
        output_dir: Option<String>,
        compress: bool,
        interleave: bool,
    ) -> Result<Self, String> {
        Ok(Self {
            tag: SamToFastqReadGroupTag::parse(&rg_tag)?,
            output_dir,
            compress,
            interleave,
        })
    }
}

#[derive(Clone, Copy)]
enum SamToFastqReadGroupTag {
    PlatformUnit,
    Id,
}

impl SamToFastqReadGroupTag {
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "PU" | "pu" => Ok(Self::PlatformUnit),
            "ID" | "id" => Ok(Self::Id),
            _ => Err(format!("unsupported SamToFastq RG_TAG: {value}")),
        }
    }
}

struct SamToFastqPerRgOutputs {
    config: SamToFastqPerRgMode,
    compression_level: u32,
    read_groups: BTreeMap<String, SamToFastqReadGroupInfo>,
    writers: BTreeMap<String, SamToFastqReadGroupWriters>,
    output_paths: Vec<String>,
}

impl SamToFastqPerRgOutputs {
    fn new(config: SamToFastqPerRgMode, compression_level: u32) -> Self {
        Self {
            config,
            compression_level,
            read_groups: BTreeMap::new(),
            writers: BTreeMap::new(),
            output_paths: Vec::new(),
        }
    }

    fn from_bam_header(
        header: &bam::HeaderView,
        config: SamToFastqPerRgMode,
        compression_level: u32,
    ) -> Result<Self, String> {
        let header_text =
            std::str::from_utf8(header.as_bytes()).map_err(|error| error.to_string())?;
        let mut outputs = Self::new(config, compression_level);
        for line in header_text.lines().filter(|line| line.starts_with("@RG\t")) {
            outputs.observe_sam_read_group_line(line)?;
        }
        outputs.ensure_has_read_groups()?;
        Ok(outputs)
    }

    fn observe_sam_read_group_line(&mut self, line: &str) -> Result<(), String> {
        let mut id = None;
        let mut platform_unit = None;
        for field in line.split('\t').skip(1) {
            if let Some(value) = field.strip_prefix("ID:") {
                id = Some(value.to_string());
            } else if let Some(value) = field.strip_prefix("PU:") {
                platform_unit = Some(value.to_string());
            }
        }
        if let Some(id) = id {
            self.read_groups
                .insert(id, SamToFastqReadGroupInfo { platform_unit });
            Ok(())
        } else {
            Err("malformed SamToFastq @RG header".to_string())
        }
    }

    fn ensure_has_read_groups(&self) -> Result<(), String> {
        if self.read_groups.is_empty() {
            return Err(
                "SamToFastq input does not contain Read Groups, consider not using the OUTPUT_PER_RG option"
                    .to_string(),
            );
        }
        Ok(())
    }

    fn unpaired_writer_for_bam_record(
        &mut self,
        record: &bam::Record,
    ) -> Result<&mut dyn Write, String> {
        let read_group_id = bam_record_read_group_id(record)?;
        self.unpaired_writer_for_read_group(&read_group_id)
    }

    fn unpaired_writer_for_sam_record(&mut self, line: &str) -> Result<&mut dyn Write, String> {
        let read_group_id = sam_record_read_group_id(line)?;
        self.unpaired_writer_for_read_group(&read_group_id)
    }

    fn write_bam_pair(
        &mut self,
        read1: &bam::Record,
        read2: &bam::Record,
        transform: &SamToFastqTransform,
        re_reverse: bool,
    ) -> Result<(), String> {
        let read_group_id = bam_record_read_group_id(read1)?;
        self.ensure_writers_for_read_group(&read_group_id)?;
        if self.config.interleave {
            let writer = self
                .writers
                .get_mut(&read_group_id)
                .expect("writers exist after ensure")
                .first
                .as_mut();
            write_fastq_record(
                writer,
                read1,
                transform,
                re_reverse,
                fastq_name_suffix(read1),
                transform.trim_for(read1),
                transform.quality,
                transform.max_bases_for(read1),
            )?;
            write_fastq_record(
                writer,
                read2,
                transform,
                re_reverse,
                fastq_name_suffix(read2),
                transform.trim_for(read2),
                transform.quality,
                transform.max_bases_for(read2),
            )
        } else {
            self.ensure_second_writer_for_read_group(&read_group_id)?;
            let writers = self
                .writers
                .get_mut(&read_group_id)
                .expect("writers exist after ensure");
            write_fastq_record(
                writers.first.as_mut(),
                read1,
                transform,
                re_reverse,
                fastq_name_suffix(read1),
                transform.trim_for(read1),
                transform.quality,
                transform.max_bases_for(read1),
            )?;
            write_fastq_record(
                writers
                    .second
                    .as_mut()
                    .expect("second writer exists after ensure")
                    .as_mut(),
                read2,
                transform,
                re_reverse,
                fastq_name_suffix(read2),
                transform.trim_for(read2),
                transform.quality,
                transform.max_bases_for(read2),
            )
        }
    }

    fn write_sam_pair(
        &mut self,
        line: &str,
        read1: &SamFastqRecord,
        read2: &SamFastqRecord,
        transform: &SamToFastqTransform,
        re_reverse: bool,
        sequence: &mut Vec<u8>,
        qualities: &mut Vec<u8>,
        output: &mut Vec<u8>,
    ) -> Result<(), String> {
        let read_group_id = sam_record_read_group_id(line)?;
        self.ensure_writers_for_read_group(&read_group_id)?;
        if self.config.interleave {
            let writer = self
                .writers
                .get_mut(&read_group_id)
                .expect("writers exist after ensure")
                .first
                .as_mut();
            write_sam_fastq_record(
                writer,
                read1,
                transform,
                re_reverse,
                transform.trim_for_flags(read1.flags),
                transform.quality,
                transform.max_bases_for_flags(read1.flags),
                sequence,
                qualities,
                output,
            )?;
            write_sam_fastq_record(
                writer,
                read2,
                transform,
                re_reverse,
                transform.trim_for_flags(read2.flags),
                transform.quality,
                transform.max_bases_for_flags(read2.flags),
                sequence,
                qualities,
                output,
            )
        } else {
            self.ensure_second_writer_for_read_group(&read_group_id)?;
            let writers = self
                .writers
                .get_mut(&read_group_id)
                .expect("writers exist after ensure");
            write_sam_fastq_record(
                writers.first.as_mut(),
                read1,
                transform,
                re_reverse,
                transform.trim_for_flags(read1.flags),
                transform.quality,
                transform.max_bases_for_flags(read1.flags),
                sequence,
                qualities,
                output,
            )?;
            write_sam_fastq_record(
                writers
                    .second
                    .as_mut()
                    .expect("second writer exists after ensure")
                    .as_mut(),
                read2,
                transform,
                re_reverse,
                transform.trim_for_flags(read2.flags),
                transform.quality,
                transform.max_bases_for_flags(read2.flags),
                sequence,
                qualities,
                output,
            )
        }
    }

    fn unpaired_writer_for_read_group(
        &mut self,
        read_group_id: &str,
    ) -> Result<&mut dyn Write, String> {
        self.ensure_has_read_groups()?;
        self.ensure_writers_for_read_group(read_group_id)?;
        Ok(self
            .writers
            .get_mut(read_group_id)
            .expect("writers exist after ensure")
            .first
            .as_mut())
    }

    fn ensure_writers_for_read_group(&mut self, read_group_id: &str) -> Result<(), String> {
        if self.writers.contains_key(read_group_id) {
            return Ok(());
        }
        let first_path = self.read_group_output_path(read_group_id, "_1")?;
        self.output_paths.push(first_path.clone());
        let first = fastq_writer(&first_path, self.compression_level)?;
        self.writers.insert(
            read_group_id.to_string(),
            SamToFastqReadGroupWriters {
                first,
                second: None,
            },
        );
        Ok(())
    }

    fn ensure_second_writer_for_read_group(&mut self, read_group_id: &str) -> Result<(), String> {
        if self.config.interleave {
            return Ok(());
        }
        let needs_second = self
            .writers
            .get(read_group_id)
            .is_some_and(|writers| writers.second.is_none());
        if !needs_second {
            return Ok(());
        }
        let path = self.read_group_output_path(read_group_id, "_2")?;
        self.output_paths.push(path.clone());
        self.writers
            .get_mut(read_group_id)
            .expect("writers exist after ensure")
            .second = Some(fastq_writer(&path, self.compression_level)?);
        Ok(())
    }

    fn read_group_output_path(&self, read_group_id: &str, suffix: &str) -> Result<String, String> {
        let info = self.read_groups.get(read_group_id).ok_or_else(|| {
            format!("SamToFastq read group {read_group_id} is not present in the header")
        })?;
        let value = match self.config.tag {
            SamToFastqReadGroupTag::PlatformUnit => info.platform_unit.as_deref(),
            SamToFastqReadGroupTag::Id => Some(read_group_id),
        }
        .ok_or_else(|| {
            let tag = match self.config.tag {
                SamToFastqReadGroupTag::PlatformUnit => "PU",
                SamToFastqReadGroupTag::Id => "ID",
            };
            format!("The selected RG_TAG: {tag} is not present in the header.")
        })?;
        let mut file_name = samtofastq_make_file_name_safe(value);
        file_name.push_str(suffix);
        file_name.push_str(".fastq");
        if self.config.compress {
            file_name.push_str(".gz");
        }
        Ok(match self.config.output_dir.as_deref() {
            Some(dir) => Path::new(dir).join(file_name).display().to_string(),
            None => file_name,
        })
    }

    fn flush_all(&mut self) -> Result<(), String> {
        for writers in self.writers.values_mut() {
            writers.first.flush().map_err(|error| error.to_string())?;
            if let Some(writer) = writers.second.as_mut() {
                writer.flush().map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    fn write_md5_sidecars(self, create_md5_file: bool) -> Result<(), String> {
        if !create_md5_file {
            return Ok(());
        }
        for path in self.output_paths {
            write_md5_sidecar(&path)?;
        }
        Ok(())
    }
}

struct SamToFastqReadGroupInfo {
    platform_unit: Option<String>,
}

struct SamToFastqReadGroupWriters {
    first: Box<dyn Write>,
    second: Option<Box<dyn Write>>,
}

fn bam_record_read_group_id(record: &bam::Record) -> Result<String, String> {
    match record.aux(b"RG") {
        Ok(Aux::String(value)) => Ok(value.to_string()),
        _ => Err("SamToFastq record is missing RG tag".to_string()),
    }
}

fn sam_record_read_group_id(line: &str) -> Result<String, String> {
    for field in line.split('\t').skip(11) {
        if let Some(value) = field.strip_prefix("RG:Z:") {
            return Ok(value.to_string());
        }
    }
    Err("SamToFastq record is missing RG tag".to_string())
}

fn samtofastq_make_file_name_safe(value: &str) -> String {
    value
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_whitespace()
                || matches!(
                    ch,
                    '!' | '"'
                        | '#'
                        | '$'
                        | '%'
                        | '&'
                        | '\''
                        | '('
                        | ')'
                        | '*'
                        | '/'
                        | ':'
                        | ';'
                        | '<'
                        | '='
                        | '>'
                        | '?'
                        | '@'
                        | '['
                        | ']'
                        | '\\'
                        | '^'
                        | '`'
                        | '{'
                        | '|'
                        | '}'
                        | '~'
                )
            {
                '_'
            } else {
                ch
            }
        })
        .collect()
}

struct SamFastqRecord {
    name: String,
    flags: u16,
    sequence: String,
    qualities: String,
    clip_point: Option<usize>,
}

#[derive(Clone, Copy)]
struct SamToFastqTransform {
    read1_trim: usize,
    read2_trim: usize,
    read1_max_bases_to_write: Option<usize>,
    read2_max_bases_to_write: Option<usize>,
    quality: Option<u8>,
    clipping: Option<SamToFastqClipping>,
}

#[derive(Clone, Copy)]
struct SamToFastqClipping {
    tag: [u8; 2],
    action: SamToFastqClippingAction,
    minimum_length: usize,
}

#[derive(Clone, Copy)]
enum SamToFastqClippingAction {
    Trim,
    MaskBase,
    SetQuality(u8),
}

impl SamToFastqTransform {
    fn trim_for(&self, record: &bam::Record) -> usize {
        if record.is_paired() && record.is_last_in_template() {
            self.read2_trim
        } else {
            self.read1_trim
        }
    }

    fn max_bases_for(&self, record: &bam::Record) -> Option<usize> {
        if record.is_paired() && record.is_last_in_template() {
            self.read2_max_bases_to_write
        } else {
            self.read1_max_bases_to_write
        }
    }

    fn trim_for_flags(&self, flags: u16) -> usize {
        if flags & 0x1 != 0 && flags & 0x80 != 0 {
            self.read2_trim
        } else {
            self.read1_trim
        }
    }

    fn max_bases_for_flags(&self, flags: u16) -> Option<usize> {
        if flags & 0x1 != 0 && flags & 0x80 != 0 {
            self.read2_max_bases_to_write
        } else {
            self.read1_max_bases_to_write
        }
    }
}

fn samtofastq_clipping(
    args: &BTreeMap<String, Vec<String>>,
) -> Result<Option<SamToFastqClipping>, String> {
    let Some(attribute) = optional_scalar(args, "CLIPPING_ATTRIBUTE")? else {
        return Ok(None);
    };
    let action = required_scalar_for(args, "CLIPPING_ACTION", "SamToFastq")?;
    let tag = sam_tag_bytes(&attribute, "SamToFastq CLIPPING_ATTRIBUTE")?;
    let action = match action.as_str() {
        "X" => SamToFastqClippingAction::Trim,
        "N" => SamToFastqClippingAction::MaskBase,
        value => {
            let phred = value
                .parse::<u8>()
                .map_err(|_| "unsupported SamToFastq CLIPPING_ACTION".to_string())?;
            SamToFastqClippingAction::SetQuality(phred.saturating_add(33))
        }
    };
    Ok(Some(SamToFastqClipping {
        tag,
        action,
        minimum_length: optional_u32(args, "CLIPPING_MIN_LENGTH")?.unwrap_or(0) as usize,
    }))
}

fn sam_tag_bytes(value: &str, label: &str) -> Result<[u8; 2], String> {
    let bytes = value.as_bytes();
    if bytes.len() != 2 {
        return Err(format!("unsupported {label}: {value}"));
    }
    Ok([bytes[0], bytes[1]])
}

fn write_sam_fastq_record(
    writer: &mut dyn Write,
    record: &SamFastqRecord,
    transform: &SamToFastqTransform,
    re_reverse: bool,
    trim: usize,
    quality: Option<u8>,
    max_bases_to_write: Option<usize>,
    sequence: &mut Vec<u8>,
    qualities: &mut Vec<u8>,
    output: &mut Vec<u8>,
) -> Result<(), String> {
    sequence.clear();
    sequence.extend_from_slice(record.sequence.as_bytes());
    qualities.clear();
    qualities.extend_from_slice(record.qualities.as_bytes());
    if let Some(clipping) = transform.clipping {
        apply_samtofastq_clipping(
            sequence,
            qualities,
            record.clip_point,
            record.flags & 0x10 != 0,
            clipping,
        )?;
    }
    if re_reverse && record.flags & 0x10 != 0 {
        reverse_complement(sequence);
        qualities.reverse();
    }
    trim_and_cap_fastq(sequence, qualities, trim, quality, max_bases_to_write)?;
    output.clear();
    append_fastq_text_record(
        output,
        record.name.as_bytes(),
        fastq_name_suffix_from_flags(record.flags),
        sequence,
        qualities,
    );
    writer.write_all(output).map_err(|error| error.to_string())
}

fn write_samtofastq_sidecars(
    fastq: &str,
    second_end_fastq: Option<&str>,
    unpaired_fastq: Option<&str>,
    create_md5_file: bool,
) -> Result<(), String> {
    if !create_md5_file {
        return Ok(());
    }
    write_md5_sidecar(fastq)?;
    if let Some(path) = second_end_fastq {
        write_md5_sidecar(path)?;
    }
    if let Some(path) = unpaired_fastq {
        write_md5_sidecar(path)?;
    }
    Ok(())
}

fn sam_fastq_fields(line: &str) -> Result<(&str, u16, &str, &str), String> {
    let mut fields = line.split('\t');
    let name = fields
        .next()
        .ok_or_else(|| "malformed SamToFastq SAM record".to_string())?;
    let flags = fields
        .next()
        .ok_or_else(|| "malformed SamToFastq SAM record".to_string())?
        .parse::<u16>()
        .map_err(|_| "malformed SamToFastq SAM flag".to_string())?;
    for _ in 0..7 {
        fields
            .next()
            .ok_or_else(|| "malformed SamToFastq SAM record".to_string())?;
    }
    let sequence = fields
        .next()
        .ok_or_else(|| "malformed SamToFastq SAM record".to_string())?;
    let qualities = fields
        .next()
        .ok_or_else(|| "malformed SamToFastq SAM record".to_string())?;
    Ok((name, flags, sequence, qualities))
}

fn append_fastq_text_record(
    output: &mut Vec<u8>,
    name: &[u8],
    name_suffix: Option<&'static [u8]>,
    sequence: &[u8],
    qualities: &[u8],
) {
    output.extend_from_slice(b"@");
    output.extend_from_slice(name);
    output.extend_from_slice(name_suffix.unwrap_or_default());
    output.extend_from_slice(b"\n");
    output.extend_from_slice(sequence);
    output.extend_from_slice(b"\n+\n");
    output.extend_from_slice(qualities);
    output.extend_from_slice(b"\n");
}

fn write_fastq_record(
    writer: &mut dyn Write,
    record: &bam::Record,
    transform: &SamToFastqTransform,
    re_reverse: bool,
    name_suffix: Option<&'static str>,
    trim: usize,
    quality: Option<u8>,
    max_bases_to_write: Option<usize>,
) -> Result<(), String> {
    let name = String::from_utf8_lossy(record.qname());
    let mut sequence = record.seq().as_bytes();
    let mut qualities = record
        .qual()
        .iter()
        .map(|quality| quality.saturating_add(33))
        .collect::<Vec<_>>();

    if let Some(clipping) = transform.clipping {
        apply_samtofastq_clipping(
            &mut sequence,
            &mut qualities,
            bam_clip_point(record, clipping.tag),
            record.is_reverse(),
            clipping,
        )?;
    }
    if re_reverse && record.is_reverse() {
        reverse_complement(&mut sequence);
        qualities.reverse();
    }
    trim_and_cap_fastq(
        &mut sequence,
        &mut qualities,
        trim,
        quality,
        max_bases_to_write,
    )?;

    writer
        .write_all(b"@")
        .and_then(|_| writer.write_all(name.as_bytes()))
        .and_then(|_| writer.write_all(name_suffix.unwrap_or_default().as_bytes()))
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.write_all(&sequence))
        .and_then(|_| writer.write_all(b"\n+\n"))
        .and_then(|_| writer.write_all(&qualities))
        .and_then(|_| writer.write_all(b"\n"))
        .map_err(|error| error.to_string())
}

fn trim_and_cap_fastq(
    sequence: &mut Vec<u8>,
    qualities: &mut Vec<u8>,
    trim: usize,
    quality: Option<u8>,
    max_bases_to_write: Option<usize>,
) -> Result<(), String> {
    if trim > sequence.len() || trim > qualities.len() {
        return Err("SamToFastq trim exceeds read length".to_string());
    }
    if trim > 0 {
        sequence.drain(..trim);
        qualities.drain(..trim);
    }
    if let Some(quality) = quality {
        let trim_point = find_quality_trim_point(qualities, quality).max(1);
        if trim_point < qualities.len() {
            sequence.truncate(trim_point);
            qualities.truncate(trim_point);
        }
    }
    if let Some(max_bases) = max_bases_to_write {
        sequence.truncate(max_bases);
        qualities.truncate(max_bases);
    }
    Ok(())
}

fn apply_samtofastq_clipping(
    sequence: &mut Vec<u8>,
    qualities: &mut Vec<u8>,
    clip_point: Option<usize>,
    reverse: bool,
    clipping: SamToFastqClipping,
) -> Result<(), String> {
    let Some(mut point) = clip_point else {
        return Ok(());
    };
    if point < clipping.minimum_length {
        point = sequence.len().min(clipping.minimum_length);
    }
    if point == 0 || point > sequence.len() || point > qualities.len() {
        return Ok(());
    }
    let positive_strand = !reverse;
    match clipping.action {
        SamToFastqClippingAction::Trim => {
            clip_fastq_component(sequence, point, None, positive_strand);
            clip_fastq_component(qualities, point, None, positive_strand);
        }
        SamToFastqClippingAction::MaskBase => {
            clip_fastq_component(sequence, point, Some(b'N'), positive_strand);
        }
        SamToFastqClippingAction::SetQuality(quality) => {
            clip_fastq_component(qualities, point, Some(quality), positive_strand);
        }
    }
    Ok(())
}

fn clip_fastq_component(
    component: &mut Vec<u8>,
    point: usize,
    replacement: Option<u8>,
    positive_strand: bool,
) {
    let len = component.len();
    let mut result = if positive_strand {
        component[..point - 1].to_vec()
    } else {
        component[len - point + 1..].to_vec()
    };
    if let Some(replacement) = replacement {
        let replacement_count = len - point + 1;
        if positive_strand {
            result.extend(std::iter::repeat(replacement).take(replacement_count));
        } else {
            let mut prefixed = vec![replacement; replacement_count];
            prefixed.extend_from_slice(&result);
            result = prefixed;
        }
    }
    *component = result;
}

fn bam_clip_point(record: &bam::Record, tag: [u8; 2]) -> Option<usize> {
    match record.aux(&tag) {
        Ok(Aux::I8(value)) => usize::try_from(value).ok(),
        Ok(Aux::U8(value)) => Some(value as usize),
        Ok(Aux::I16(value)) => usize::try_from(value).ok(),
        Ok(Aux::U16(value)) => Some(value as usize),
        Ok(Aux::I32(value)) => usize::try_from(value).ok(),
        Ok(Aux::U32(value)) => usize::try_from(value).ok(),
        _ => None,
    }
}

fn sam_clip_point(line: &str, clipping: Option<SamToFastqClipping>) -> Option<usize> {
    let clipping = clipping?;
    let tag = std::str::from_utf8(&clipping.tag).ok()?;
    for field in line.split('\t').skip(11) {
        let mut parts = field.splitn(3, ':');
        let Some(field_tag) = parts.next() else {
            continue;
        };
        if field_tag != tag {
            continue;
        }
        let Some(field_type) = parts.next() else {
            continue;
        };
        let Some(value) = parts.next() else {
            continue;
        };
        if !matches!(field_type, "i" | "I" | "c" | "C" | "s" | "S") {
            return None;
        }
        return value.parse::<usize>().ok();
    }
    None
}

fn find_quality_trim_point(qualities: &[u8], trim_quality: u8) -> usize {
    let length = qualities.len();
    if trim_quality < 1 || length == 0 {
        return 0;
    }
    let mut score = 0i32;
    let mut max_score = 0i32;
    let mut trim_point = length;
    for index in (0..length).rev() {
        let phred = qualities[index].saturating_sub(33) as i32;
        score += trim_quality as i32 - phred;
        if score < 0 {
            break;
        }
        if score > max_score {
            max_score = score;
            trim_point = index;
        }
    }
    trim_point
}

fn has_sam_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("sam"))
        .unwrap_or(false)
}

fn has_gzip_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "gz" | "gzip"))
        .unwrap_or(false)
}

fn fastq_name_suffix_from_flags(flags: u16) -> Option<&'static [u8]> {
    if flags & 0x1 == 0 {
        None
    } else if flags & 0x40 != 0 {
        Some(b"/1")
    } else if flags & 0x80 != 0 {
        Some(b"/2")
    } else {
        None
    }
}

fn fastq_name_suffix(record: &bam::Record) -> Option<&'static str> {
    if !record.is_paired() {
        None
    } else if record.is_first_in_template() {
        Some("/1")
    } else if record.is_last_in_template() {
        Some("/2")
    } else {
        None
    }
}

fn reverse_complement(sequence: &mut [u8]) {
    sequence.reverse();
    for base in sequence {
        *base = match *base {
            b'A' | b'a' => b'T',
            b'C' | b'c' => b'G',
            b'G' | b'g' => b'C',
            b'T' | b't' => b'A',
            b'N' | b'n' => b'N',
            other => other,
        };
    }
}

fn reject_unsupported_sortsam_args(
    args: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SORT_ORDER",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "MAX_RECORDS_IN_RAM",
        "COMPRESSION_LEVEL",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported SortSam argument: {key}"));
        }
    }

    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported SortSam COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn reject_unsupported_cleansam_args(args: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "TMP_DIR",
        "REFERENCE_SEQUENCE",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "MAX_RECORDS_IN_RAM",
        "COMPRESSION_LEVEL",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported CleanSam argument: {key}"));
        }
    }

    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_scalar(args, "TMP_DIR")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported CleanSam COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn reject_unsupported_mergesamfiles_args(
    args: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "SORT_ORDER",
        "COMMENT",
        "REFERENCE_SEQUENCE",
        "TMP_DIR",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "CREATE_INDEX",
        "CREATE_MD5_FILE",
        "MAX_RECORDS_IN_RAM",
        "COMPRESSION_LEVEL",
        "MERGE_SEQUENCE_DICTIONARIES",
        "ASSUME_SORTED",
        "INTERVALS",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported MergeSamFiles argument: {key}"));
        }
    }

    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_INDEX")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_bool(args, "ASSUME_SORTED")?;
    optional_bool(args, "MERGE_SEQUENCE_DICTIONARIES")?;
    let _ = args.get("INTERVALS");
    optional_scalar(args, "TMP_DIR")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported MergeSamFiles COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
}

fn reject_unsupported_buildbamindex_args(
    args: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "REFERENCE_SEQUENCE",
        "CREATE_MD5_FILE",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported BuildBamIndex argument: {key}"));
        }
    }

    optional_scalar(args, "OUTPUT")?;
    optional_scalar(args, "REFERENCE_SEQUENCE")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    Ok(())
}

fn required_scalar(
    args: &std::collections::BTreeMap<String, Vec<String>>,
    key: &'static str,
) -> Result<String, String> {
    required_scalar_for(args, key, "SortSam")
}

fn required_scalar_for(
    args: &std::collections::BTreeMap<String, Vec<String>>,
    key: &'static str,
    command: &'static str,
) -> Result<String, String> {
    let values = args
        .get(key)
        .ok_or_else(|| format!("missing required {command} argument: {key}"))?;
    scalar_value(values, key)
}

fn required_values_for(
    args: &std::collections::BTreeMap<String, Vec<String>>,
    key: &'static str,
    command: &'static str,
) -> Result<Vec<String>, String> {
    let values = args
        .get(key)
        .ok_or_else(|| format!("missing required {command} argument: {key}"))?;
    if values.is_empty() {
        return Err(format!("missing required {command} argument: {key}"));
    }
    Ok(values.clone())
}

fn optional_scalar(
    args: &std::collections::BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<String>, String> {
    let Some(values) = args.get(key) else {
        return Ok(None);
    };
    scalar_value(values, key).map(Some)
}

fn optional_bool(
    args: &std::collections::BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<bool>, String> {
    let Some(value) = optional_scalar(args, key)? else {
        return Ok(None);
    };
    match value.to_ascii_lowercase().as_str() {
        "true" => Ok(Some(true)),
        "false" => Ok(Some(false)),
        _ => Err(format!(
            "invalid boolean for SortSam argument {key}: {value}"
        )),
    }
}

fn optional_u32(
    args: &std::collections::BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<u32>, String> {
    let Some(value) = optional_scalar(args, key)? else {
        return Ok(None);
    };
    value
        .parse::<u32>()
        .map(Some)
        .map_err(|_| format!("unsupported SortSam argument {key}={value}"))
}

fn optional_i64(
    args: &std::collections::BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<i64>, String> {
    let Some(value) = optional_scalar(args, key)? else {
        return Ok(None);
    };
    value
        .parse::<i64>()
        .map(Some)
        .map_err(|_| format!("unsupported SortSam argument {key}={value}"))
}

fn optional_f64(
    args: &std::collections::BTreeMap<String, Vec<String>>,
    key: &str,
) -> Result<Option<f64>, String> {
    let Some(value) = optional_scalar(args, key)? else {
        return Ok(None);
    };
    value
        .parse::<f64>()
        .map(Some)
        .map_err(|_| format!("unsupported SortSam argument {key}={value}"))
}

fn scalar_value(values: &[String], key: &str) -> Result<String, String> {
    if values.len() != 1 {
        return Err(format!("duplicate scalar SortSam argument: {key}"));
    }
    Ok(values[0].clone())
}

fn limited_records<'a>(
    reader: &'a mut bam::Reader,
    stop_after: u32,
) -> Box<dyn Iterator<Item = Result<bam::Record, rust_htslib::errors::Error>> + 'a> {
    let records = reader.records();
    if stop_after == 0 {
        Box::new(records)
    } else {
        Box::new(records.take(stop_after as usize))
    }
}

fn output_format(output: &str) -> Result<bam::Format, String> {
    hts_io::output_format_for_path(output, "SortSam")
}

fn output_format_for(output: &str, command: &str) -> Result<bam::Format, String> {
    hts_io::output_format_for_path(output, command)
}

fn clean_sam_record(record: &mut bam::Record, target_lengths: &[u64]) -> Result<(), String> {
    if record.is_unmapped() {
        record.set_mapq(0);
        return Ok(());
    }
    if record.tid() < 0 || record.pos() < 0 {
        return Ok(());
    }
    let tid = record.tid() as usize;
    let Some(target_len) = target_lengths.get(tid).copied() else {
        return Ok(());
    };
    if target_len == 0 {
        return Ok(());
    }
    let start = record.pos() as u64;
    if start >= target_len {
        return Err("unsupported CleanSam alignment starting beyond reference end".to_string());
    }
    let reference_end =
        start.saturating_add(record.cigar().end_pos().saturating_sub(record.pos()) as u64);
    let read_len = record.seq_len() as u64;
    if reference_end > target_len && read_len > 0 {
        let overhang = reference_end - target_len;
        if overhang >= read_len {
            let cigar = CigarString(vec![Cigar::SoftClip(overhang as u32)]);
            record.set_cigar(Some(&cigar));
            return Ok(());
        }
    }

    let mut ref_pos = start;
    let mut changed = false;
    let mut cleaned = Vec::<Cigar>::new();
    for cigar in record.cigar().iter().copied() {
        let len = cigar.len() as u64;
        match cigar {
            Cigar::Match(_) | Cigar::Equal(_) | Cigar::Diff(_) => {
                if ref_pos >= target_len {
                    push_merged_cigar(&mut cleaned, Cigar::SoftClip(len as u32));
                    changed = true;
                } else if ref_pos + len > target_len {
                    let keep = (target_len - ref_pos) as u32;
                    let clip = (len - u64::from(keep)) as u32;
                    push_merged_cigar(&mut cleaned, cigar_with_len(cigar, keep));
                    push_merged_cigar(&mut cleaned, Cigar::SoftClip(clip));
                    ref_pos += len;
                    changed = true;
                } else {
                    push_merged_cigar(&mut cleaned, cigar);
                    ref_pos += len;
                }
            }
            Cigar::Del(_) | Cigar::RefSkip(_) => {
                if ref_pos >= target_len {
                    changed = true;
                } else if ref_pos + len > target_len {
                    let keep = (target_len - ref_pos) as u32;
                    push_merged_cigar(&mut cleaned, cigar_with_len(cigar, keep));
                    ref_pos += len;
                    changed = true;
                } else {
                    push_merged_cigar(&mut cleaned, cigar);
                    ref_pos += len;
                }
            }
            Cigar::Ins(_) => {
                if ref_pos >= target_len {
                    push_merged_cigar(&mut cleaned, Cigar::SoftClip(len as u32));
                    changed = true;
                } else {
                    push_merged_cigar(&mut cleaned, cigar);
                }
            }
            Cigar::SoftClip(_) | Cigar::HardClip(_) | Cigar::Pad(_) => {
                push_merged_cigar(&mut cleaned, cigar);
            }
        }
    }

    if changed {
        let cigar = CigarString(cleaned);
        record.set_cigar(Some(&cigar));
    }
    Ok(())
}

fn cigar_with_len(cigar: Cigar, len: u32) -> Cigar {
    match cigar {
        Cigar::Match(_) => Cigar::Match(len),
        Cigar::Ins(_) => Cigar::Ins(len),
        Cigar::Del(_) => Cigar::Del(len),
        Cigar::RefSkip(_) => Cigar::RefSkip(len),
        Cigar::SoftClip(_) => Cigar::SoftClip(len),
        Cigar::HardClip(_) => Cigar::HardClip(len),
        Cigar::Pad(_) => Cigar::Pad(len),
        Cigar::Equal(_) => Cigar::Equal(len),
        Cigar::Diff(_) => Cigar::Diff(len),
    }
}

fn push_merged_cigar(cigars: &mut Vec<Cigar>, cigar: Cigar) {
    if cigar.is_empty() {
        return;
    }
    if let Some(last) = cigars.last_mut() {
        if last.char() == cigar.char() {
            *last = cigar_with_len(*last, last.len() + cigar.len());
            return;
        }
    }
    cigars.push(cigar);
}

fn picard_reference(args: &BTreeMap<String, Vec<String>>) -> Result<Option<String>, String> {
    optional_scalar(args, "REFERENCE_SEQUENCE")
}

fn open_bam_reader(path: impl AsRef<Path>) -> Result<bam::Reader, String> {
    hts_io::open_reader(path, None)
}

fn open_bam_reader_with_reference(
    path: impl AsRef<Path>,
    reference: Option<&str>,
) -> Result<bam::Reader, String> {
    hts_io::open_reader(path, reference)
}

fn open_bam_reader_for_args(
    path: impl AsRef<Path>,
    args: &BTreeMap<String, Vec<String>>,
) -> Result<bam::Reader, String> {
    let reference = picard_reference(args)?;
    open_bam_reader_with_reference(path, reference.as_deref())
}

fn bam_writer_for_path(
    output: &str,
    header: &bam::Header,
    format: bam::Format,
    compression_level: Option<u32>,
) -> Result<bam::Writer, String> {
    hts_io::open_writer(output, header, format, None, compression_level)
}

fn bam_writer_for_path_with_reference(
    output: &str,
    header: &bam::Header,
    format: bam::Format,
    reference: Option<&str>,
    compression_level: Option<u32>,
) -> Result<bam::Writer, String> {
    hts_io::open_writer(output, header, format, reference, compression_level)
}

fn revert_record(
    record: &mut bam::Record,
    restore_original_qualities: bool,
    remove_alignment_information: bool,
    remove_duplicate_information: bool,
    restore_hardclips: bool,
    attributes_to_clear: &[[u8; 2]],
    attributes_to_reverse: &[[u8; 2]],
    attributes_to_reverse_complement: &[[u8; 2]],
) -> Result<(), String> {
    if restore_original_qualities
        && remove_alignment_information
        && attributes_to_clear.is_empty()
        && attributes_to_reverse.is_empty()
        && attributes_to_reverse_complement.is_empty()
    {
        return revert_record_default_unmapped_fast(
            record,
            remove_duplicate_information,
            restore_hardclips,
        );
    }

    let mut restored_qualities = None;
    if restore_original_qualities {
        if let Ok(Aux::String(qualities)) = record.aux(b"OQ") {
            let restored = qualities
                .bytes()
                .map(|quality| quality.saturating_sub(33))
                .collect::<Vec<_>>();
            if restored.len() != record.seq_len() {
                return Err("malformed RevertSam OQ length does not match read length".to_string());
            }
            let qname = record.qname().to_vec();
            let sequence = record.seq().as_bytes();
            if remove_alignment_information {
                restored_qualities = Some(restored);
            } else {
                let cigar = CigarString(record.cigar().iter().copied().collect());
                record.set(&qname, Some(&cigar), &sequence, &restored);
            }
        }
        remove_aux_if_present(record, b"OQ")?;
    }

    if remove_alignment_information {
        let qname = record.qname().to_vec();
        let mut sequence = record.seq().as_bytes();
        let mut qualities = restored_qualities.unwrap_or_else(|| record.qual().to_vec());
        let hardclips = if restore_hardclips {
            hardclip_restoration_for_revertsam(record)?
        } else {
            None
        };
        if record.is_reverse() {
            reverse_complement(&mut sequence);
            qualities.reverse();
            reverse_aux_strings(record, attributes_to_reverse, false)?;
            reverse_aux_strings(record, attributes_to_reverse_complement, true)?;
        }
        if let Some((hardclip_bases, hardclip_qualities)) = hardclips {
            sequence.extend(hardclip_bases);
            qualities.extend(hardclip_qualities);
            remove_aux_if_present(record, b"XB")?;
            remove_aux_if_present(record, b"XQ")?;
        }
        record.set(&qname, None, &sequence, &qualities);
        record.set_tid(-1);
        record.set_pos(-1);
        record.set_mapq(0);
        record.set_mtid(-1);
        record.set_mpos(-1);
        record.set_insert_size(0);
        let mut flags = record.flags();
        flags |= 0x4;
        if flags & 0x1 != 0 {
            flags |= 0x8;
        } else {
            flags &= !0x8;
        }
        flags &= !(0x2 | 0x10 | 0x20 | 0x100 | 0x800);
        if remove_duplicate_information {
            flags &= !0x400;
        }
        record.set_flags(flags);
        for tag in [b"NM", b"UQ", b"PG", b"MD", b"MQ", b"SA", b"MC", b"AS"] {
            remove_aux_if_present(record, tag)?;
        }
    } else if remove_duplicate_information {
        record.set_flags(record.flags() & !0x400);
        sort_aux_tags_for_kept_alignment(record)?;
    }
    if remove_alignment_information {
        for tag in attributes_to_clear {
            remove_aux_if_present(record, tag)?;
        }
        sort_aux_tags_lexicographically(record)?;
    }
    Ok(())
}

fn revert_record_default_unmapped_fast(
    record: &mut bam::Record,
    remove_duplicate_information: bool,
    restore_hardclips: bool,
) -> Result<(), String> {
    if !restore_hardclips && record.aux(b"XB").is_err() && record.aux(b"XQ").is_err() {
        return revert_record_default_unmapped_in_place(record, remove_duplicate_information);
    }

    let mut restored_qualities = None;
    let mut hardclip_bases = None;
    let mut hardclip_qualities = None;
    let mut kept_aux = Vec::<(Vec<u8>, OwnedAux)>::new();

    for entry in record.aux_iter() {
        let (tag, value) = entry.map_err(|error| error.to_string())?;
        if tag == b"OQ" {
            let Aux::String(qualities) = value else {
                continue;
            };
            let restored = qualities
                .bytes()
                .map(|quality| quality.saturating_sub(33))
                .collect::<Vec<_>>();
            if restored.len() != record.seq_len() {
                return Err("malformed RevertSam OQ length does not match read length".to_string());
            }
            restored_qualities = Some(restored);
        } else if restore_hardclips && tag == b"XB" {
            if let Aux::String(bases) = value {
                hardclip_bases = Some(bases.as_bytes().to_vec());
            } else {
                kept_aux.push((tag.to_vec(), owned_aux(value)));
            }
        } else if restore_hardclips && tag == b"XQ" {
            if let Aux::String(qualities) = value {
                hardclip_qualities = Some(
                    qualities
                        .bytes()
                        .map(|quality| quality.saturating_sub(33))
                        .collect::<Vec<_>>(),
                );
            } else {
                kept_aux.push((tag.to_vec(), owned_aux(value)));
            }
        } else if !revertsam_default_removed_alignment_tag(tag) {
            kept_aux.push((tag.to_vec(), owned_aux(value)));
        }
    }

    let qname = record.qname().to_vec();
    let mut sequence = record.seq().as_bytes();
    let mut qualities = restored_qualities.unwrap_or_else(|| record.qual().to_vec());
    if record.is_reverse() {
        reverse_complement(&mut sequence);
        qualities.reverse();
    }
    if let (Some(hardclip_bases), Some(hardclip_qualities)) = (hardclip_bases, hardclip_qualities) {
        if hardclip_bases.len() != hardclip_qualities.len() {
            return Err("malformed RevertSam XB/XQ lengths differ".to_string());
        }
        sequence.extend(hardclip_bases);
        qualities.extend(hardclip_qualities);
        kept_aux.retain(|(tag, _)| tag.as_slice() != b"XB" && tag.as_slice() != b"XQ");
    }

    let mut flags = record.flags();
    flags |= 0x4;
    if flags & 0x1 != 0 {
        flags |= 0x8;
    } else {
        flags &= !0x8;
    }
    flags &= !(0x2 | 0x10 | 0x20 | 0x100 | 0x800);
    if remove_duplicate_information {
        flags &= !0x400;
    }

    kept_aux.sort_by(|left, right| left.0.cmp(&right.0));

    let mut reverted = bam::Record::new();
    reverted.set(&qname, None, &sequence, &qualities);
    reverted.set_tid(-1);
    reverted.set_pos(-1);
    reverted.set_mapq(0);
    reverted.set_mtid(-1);
    reverted.set_mpos(-1);
    reverted.set_insert_size(0);
    reverted.set_flags(flags);
    for (tag, value) in &kept_aux {
        push_owned_aux(&mut reverted, tag, value)?;
    }
    *record = reverted;
    Ok(())
}

fn revert_record_default_unmapped_in_place(
    record: &mut bam::Record,
    remove_duplicate_information: bool,
) -> Result<(), String> {
    let mut restored_qualities = None;
    if let Ok(Aux::String(qualities)) = record.aux(b"OQ") {
        let restored = qualities
            .bytes()
            .map(|quality| quality.saturating_sub(33))
            .collect::<Vec<_>>();
        if restored.len() != record.seq_len() {
            return Err("malformed RevertSam OQ length does not match read length".to_string());
        }
        restored_qualities = Some(restored);
        remove_aux_if_present(record, b"OQ")?;
    }

    let qname = record.qname().to_vec();
    let mut sequence = record.seq().as_bytes();
    let mut qualities = restored_qualities.unwrap_or_else(|| record.qual().to_vec());
    if record.is_reverse() {
        reverse_complement(&mut sequence);
        qualities.reverse();
    }

    record.set(&qname, None, &sequence, &qualities);
    record.set_tid(-1);
    record.set_pos(-1);
    record.set_mapq(0);
    record.set_mtid(-1);
    record.set_mpos(-1);
    record.set_insert_size(0);

    let mut flags = record.flags();
    flags |= 0x4;
    if flags & 0x1 != 0 {
        flags |= 0x8;
    } else {
        flags &= !0x8;
    }
    flags &= !(0x2 | 0x10 | 0x20 | 0x100 | 0x800);
    if remove_duplicate_information {
        flags &= !0x400;
    }
    record.set_flags(flags);

    for tag in [b"NM", b"UQ", b"PG", b"MD", b"MQ", b"SA", b"MC", b"AS"] {
        remove_aux_if_present(record, tag)?;
    }
    sort_aux_tags_lexicographically(record)?;
    Ok(())
}

fn revertsam_default_removed_alignment_tag(tag: &[u8]) -> bool {
    matches!(
        tag,
        b"NM" | b"UQ" | b"PG" | b"MD" | b"MQ" | b"SA" | b"MC" | b"AS"
    )
}

fn hardclip_restoration_for_revertsam(
    record: &bam::Record,
) -> Result<Option<(Vec<u8>, Vec<u8>)>, String> {
    let Ok(Aux::String(bases)) = record.aux(b"XB") else {
        return Ok(None);
    };
    let Ok(Aux::String(qualities)) = record.aux(b"XQ") else {
        return Ok(None);
    };
    let bases = bases.as_bytes().to_vec();
    let qualities = qualities
        .bytes()
        .map(|quality| quality.saturating_sub(33))
        .collect::<Vec<_>>();
    if bases.len() != qualities.len() {
        return Err("malformed RevertSam XB/XQ lengths differ".to_string());
    }
    Ok(Some((bases, qualities)))
}

enum OwnedAux {
    Char(u8),
    I8(i8),
    U8(u8),
    I16(i16),
    U16(u16),
    I32(i32),
    U32(u32),
    Float(f32),
    Double(f64),
    String(String),
    HexByteArray(String),
    ArrayI8(Vec<i8>),
    ArrayU8(Vec<u8>),
    ArrayI16(Vec<i16>),
    ArrayU16(Vec<u16>),
    ArrayI32(Vec<i32>),
    ArrayU32(Vec<u32>),
    ArrayFloat(Vec<f32>),
}

fn sort_aux_tags_lexicographically(record: &mut bam::Record) -> Result<(), String> {
    sort_aux_tags_by_rank(record, |_| 0)
}

fn sort_aux_tags_for_kept_alignment(record: &mut bam::Record) -> Result<(), String> {
    sort_aux_tags_by_rank(record, revertsam_kept_alignment_aux_rank)
}

fn sort_aux_tags_by_rank(record: &mut bam::Record, rank: fn(&[u8]) -> usize) -> Result<(), String> {
    let mut aux_values = {
        let mut aux_iter = record.aux_iter();
        let Some(first) = aux_iter.next() else {
            return Ok(());
        };
        let (first_tag, first_value) = first.map_err(|error| error.to_string())?;
        let Some(second) = aux_iter.next() else {
            return Ok(());
        };
        let (second_tag, second_value) = second.map_err(|error| error.to_string())?;
        let mut aux_values = vec![
            (first_tag.to_vec(), owned_aux(first_value)),
            (second_tag.to_vec(), owned_aux(second_value)),
        ];
        for entry in aux_iter {
            let (tag, value) = entry.map_err(|error| error.to_string())?;
            aux_values.push((tag.to_vec(), owned_aux(value)));
        }
        aux_values
    };
    aux_values.sort_by(|left, right| {
        rank(&left.0)
            .cmp(&rank(&right.0))
            .then_with(|| left.0.cmp(&right.0))
    });

    for (tag, _) in &aux_values {
        remove_aux_if_present(record, tag)?;
    }
    for (tag, value) in &aux_values {
        push_owned_aux(record, tag, value)?;
    }
    Ok(())
}

fn revertsam_kept_alignment_aux_rank(tag: &[u8]) -> usize {
    match tag {
        b"MC" => 0,
        b"MD" => 1,
        b"NM" => 2,
        b"MQ" => 3,
        _ => 4,
    }
}

fn reverse_aux_strings(
    record: &mut bam::Record,
    tags: &[[u8; 2]],
    complement: bool,
) -> Result<(), String> {
    for tag in tags {
        let tag = tag.as_slice();
        let Ok(Aux::String(value)) = record.aux(tag) else {
            continue;
        };
        let mut value = value.as_bytes().to_vec();
        if complement {
            reverse_complement(&mut value);
        } else {
            value.reverse();
        }
        let value = String::from_utf8(value).map_err(|error| error.to_string())?;
        record.remove_aux(tag).map_err(|error| error.to_string())?;
        record
            .push_aux(tag, Aux::String(&value))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn owned_aux(value: Aux<'_>) -> OwnedAux {
    match value {
        Aux::Char(value) => OwnedAux::Char(value),
        Aux::I8(value) => OwnedAux::I8(value),
        Aux::U8(value) => OwnedAux::U8(value),
        Aux::I16(value) => OwnedAux::I16(value),
        Aux::U16(value) => OwnedAux::U16(value),
        Aux::I32(value) => OwnedAux::I32(value),
        Aux::U32(value) => OwnedAux::U32(value),
        Aux::Float(value) => OwnedAux::Float(value),
        Aux::Double(value) => OwnedAux::Double(value),
        Aux::String(value) => OwnedAux::String(value.to_string()),
        Aux::HexByteArray(value) => OwnedAux::HexByteArray(value.to_string()),
        Aux::ArrayI8(value) => OwnedAux::ArrayI8(value.iter().collect()),
        Aux::ArrayU8(value) => OwnedAux::ArrayU8(value.iter().collect()),
        Aux::ArrayI16(value) => OwnedAux::ArrayI16(value.iter().collect()),
        Aux::ArrayU16(value) => OwnedAux::ArrayU16(value.iter().collect()),
        Aux::ArrayI32(value) => OwnedAux::ArrayI32(value.iter().collect()),
        Aux::ArrayU32(value) => OwnedAux::ArrayU32(value.iter().collect()),
        Aux::ArrayFloat(value) => OwnedAux::ArrayFloat(value.iter().collect()),
    }
}

fn push_owned_aux(record: &mut bam::Record, tag: &[u8], value: &OwnedAux) -> Result<(), String> {
    let value = match value {
        OwnedAux::Char(value) => Aux::Char(*value),
        OwnedAux::I8(value) => Aux::I8(*value),
        OwnedAux::U8(value) => Aux::U8(*value),
        OwnedAux::I16(value) => Aux::I16(*value),
        OwnedAux::U16(value) => Aux::U16(*value),
        OwnedAux::I32(value) => Aux::I32(*value),
        OwnedAux::U32(value) => Aux::U32(*value),
        OwnedAux::Float(value) => Aux::Float(*value),
        OwnedAux::Double(value) => Aux::Double(*value),
        OwnedAux::String(value) => Aux::String(value),
        OwnedAux::HexByteArray(value) => Aux::HexByteArray(value),
        OwnedAux::ArrayI8(value) => Aux::ArrayI8(value.into()),
        OwnedAux::ArrayU8(value) => Aux::ArrayU8(value.into()),
        OwnedAux::ArrayI16(value) => Aux::ArrayI16(value.into()),
        OwnedAux::ArrayU16(value) => Aux::ArrayU16(value.into()),
        OwnedAux::ArrayI32(value) => Aux::ArrayI32(value.into()),
        OwnedAux::ArrayU32(value) => Aux::ArrayU32(value.into()),
        OwnedAux::ArrayFloat(value) => Aux::ArrayFloat(value.into()),
    };
    record
        .push_aux(tag, value)
        .map_err(|error| error.to_string())
}

fn remove_aux_if_present(record: &mut bam::Record, tag: &[u8]) -> Result<(), String> {
    if record.aux(tag).is_ok() {
        record.remove_aux(tag).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn set_nm_md_uq_tags(
    record: &mut bam::Record,
    references_by_tid: &[Option<&[u8]>],
    set_only_uq: bool,
) -> Result<(), String> {
    if record.is_unmapped() || record.is_secondary() || record.is_supplementary() {
        return Ok(());
    }
    if record.tid() < 0 || record.pos() < 0 {
        return Ok(());
    }
    let reference = references_by_tid
        .get(record.tid() as usize)
        .copied()
        .flatten()
        .ok_or_else(|| format!("SetNmMdAndUqTags reference missing target {}", record.tid()))?;
    let read_bases = record.seq();
    let qualities = record.qual();
    let mut read_offset = 0usize;
    let mut ref_offset = record.pos() as usize;
    let mut nm = 0i32;
    let mut uq = 0i32;
    let mut md = String::with_capacity(read_bases.len());
    let mut matches = 0usize;
    let md_present = record.aux(b"MD").is_ok();
    let nm_present = record.aux(b"NM").is_ok();
    let uq_present = record.aux(b"UQ").is_ok();

    for cigar in &record.cigar() {
        match *cigar {
            Cigar::Match(length) | Cigar::Equal(length) | Cigar::Diff(length) => {
                for _ in 0..length {
                    if read_offset >= read_bases.len() {
                        return Err("SetNmMdAndUqTags read sequence shorter than CIGAR".to_string());
                    }
                    let read_base = read_bases[read_offset];
                    let ref_base = *reference.get(ref_offset).ok_or_else(|| {
                        "SetNmMdAndUqTags alignment extends beyond reference".to_string()
                    })?;
                    if dna_bases_equal(read_base, ref_base) {
                        matches += 1;
                    } else {
                        push_usize_decimal(&mut md, matches);
                        md.push(ref_base as char);
                        matches = 0;
                        nm += 1;
                        uq += qualities.get(read_offset).copied().unwrap_or(0) as i32;
                    }
                    read_offset += 1;
                    ref_offset += 1;
                }
            }
            Cigar::Ins(length) => {
                read_offset += length as usize;
                nm += length as i32;
            }
            Cigar::Del(length) => {
                push_usize_decimal(&mut md, matches);
                md.push('^');
                matches = 0;
                for _ in 0..length {
                    let ref_base = *reference.get(ref_offset).ok_or_else(|| {
                        "SetNmMdAndUqTags deletion extends beyond reference".to_string()
                    })?;
                    md.push(ref_base as char);
                    ref_offset += 1;
                }
                nm += length as i32;
            }
            Cigar::SoftClip(length) => {
                read_offset += length as usize;
            }
            Cigar::HardClip(_) | Cigar::Pad(_) => {}
            Cigar::RefSkip(_) => {
                return Err("unsupported SetNmMdAndUqTags CIGAR N/ref-skip".to_string());
            }
        }
    }
    push_usize_decimal(&mut md, matches);

    if !set_only_uq {
        if md_present {
            replace_aux_string(record, b"MD", &md)?;
        } else {
            record
                .push_aux_unchecked(b"MD", Aux::String(&md))
                .map_err(|error| error.to_string())?;
        }
        if nm_present {
            replace_aux_i32(record, b"NM", nm)?;
        } else {
            record
                .push_aux_unchecked(b"NM", Aux::I32(nm))
                .map_err(|error| error.to_string())?;
        }
    }
    if uq_present {
        replace_aux_i32(record, b"UQ", uq)?;
    } else {
        record
            .push_aux_unchecked(b"UQ", Aux::I32(uq))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn dna_bases_equal(read_base: u8, ref_base: u8) -> bool {
    if read_base == ref_base {
        return true;
    }
    match read_base.to_ascii_uppercase() {
        b'A' => ref_base.to_ascii_uppercase() == b'A',
        b'C' => ref_base.to_ascii_uppercase() == b'C',
        b'G' => ref_base.to_ascii_uppercase() == b'G',
        b'T' => ref_base.to_ascii_uppercase() == b'T',
        b'N' => ref_base.to_ascii_uppercase() == b'N',
        _ => false,
    }
}

fn push_usize_decimal(output: &mut String, value: usize) {
    if value < 10 {
        output.push((b'0' + value as u8) as char);
        return;
    }

    let mut buffer = [0u8; 20];
    let mut cursor = buffer.len();
    let mut remaining = value;
    while remaining > 0 {
        cursor -= 1;
        buffer[cursor] = b'0' + (remaining % 10) as u8;
        remaining /= 10;
    }
    output.push_str(std::str::from_utf8(&buffer[cursor..]).expect("decimal digits are UTF-8"));
}

fn write_fixed_mate_group(
    writer: &mut bam::Writer,
    records: &mut Vec<bam::Record>,
    add_mate_cigar: bool,
    ignore_missing_mates: bool,
) -> Result<(), String> {
    for record in drain_fixed_mate_group(records, add_mate_cigar, ignore_missing_mates)? {
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn drain_fixed_mate_group(
    records: &mut Vec<bam::Record>,
    add_mate_cigar: bool,
    ignore_missing_mates: bool,
) -> Result<Vec<bam::Record>, String> {
    if records.is_empty() {
        return Ok(Vec::new());
    }

    if records.len() == 1 {
        if !ignore_missing_mates && records[0].is_paired() {
            let name = String::from_utf8_lossy(records[0].qname());
            return Err(format!("Missing second read of pair: {name}"));
        }
        return Ok(records.drain(..).collect());
    }

    if records.iter().any(|record| record.is_secondary()) {
        return Ok(records.drain(..).collect());
    }

    let primary_indices = records
        .iter()
        .enumerate()
        .filter_map(|(index, record)| (!record.is_supplementary()).then_some(index))
        .collect::<Vec<_>>();
    if primary_indices.len() != 2 {
        return Err(
            "unsupported FixMateInformation read group without exactly two primary records"
                .to_string(),
        );
    }
    if records
        .iter()
        .any(|record| !record.is_paired() && !record.is_supplementary())
    {
        return Ok(records.drain(..).collect());
    }

    let mut fixed = records.drain(..).collect::<Vec<_>>();
    let first_index = primary_indices[0];
    let second_index = primary_indices[1];
    fix_mate_pair_by_index(&mut fixed, first_index, second_index, add_mate_cigar)?;
    let first_primary = fixed[first_index].clone();
    let second_primary = fixed[second_index].clone();
    let first_template_length = first_primary.insert_size();
    let second_template_length = second_primary.insert_size();

    for record in &mut fixed {
        if !record.is_supplementary() {
            continue;
        }
        if record.is_first_in_template() {
            set_mate_fields(record, &second_primary, add_mate_cigar)?;
            record.set_insert_size(first_template_length);
        } else if record.is_last_in_template() {
            set_mate_fields(record, &first_primary, add_mate_cigar)?;
            record.set_insert_size(second_template_length);
        } else {
            return Err(
                "unsupported FixMateInformation supplementary alignment without pair side"
                    .to_string(),
            );
        }
    }

    Ok(fixed)
}

fn fix_mate_pair_by_index(
    records: &mut [bam::Record],
    first_index: usize,
    second_index: usize,
    add_mate_cigar: bool,
) -> Result<(), String> {
    debug_assert_ne!(first_index, second_index);
    if first_index < second_index {
        let (left, right) = records.split_at_mut(second_index);
        fix_mate_pair(&mut left[first_index], &mut right[0], add_mate_cigar)
    } else {
        let (left, right) = records.split_at_mut(first_index);
        fix_mate_pair(&mut right[0], &mut left[second_index], add_mate_cigar)
    }
}

fn fix_mate_pair(
    first: &mut bam::Record,
    second: &mut bam::Record,
    add_mate_cigar: bool,
) -> Result<(), String> {
    set_mate_fields(first, second, add_mate_cigar)?;
    set_mate_fields(second, first, add_mate_cigar)?;
    let insert_size = template_length(first, second);
    first.set_insert_size(insert_size);
    second.set_insert_size(-insert_size);
    Ok(())
}

fn set_mate_fields(
    record: &mut bam::Record,
    mate: &bam::Record,
    add_mate_cigar: bool,
) -> Result<(), String> {
    record.set_mtid(mate.tid());
    record.set_mpos(mate.pos());

    let mut flags = record.flags();
    if mate.is_unmapped() {
        flags |= 0x8;
    } else {
        flags &= !0x8;
    }
    if mate.is_reverse() {
        flags |= 0x20;
    } else {
        flags &= !0x20;
    }
    record.set_flags(flags);

    if add_mate_cigar {
        replace_aux_string(record, b"MC", &mate.cigar().to_string())?;
    } else if record.aux(b"MC").is_ok() {
        record
            .remove_aux(b"MC")
            .map_err(|error| error.to_string())?;
    }
    replace_aux_i32(record, b"MQ", mate.mapq() as i32)?;
    Ok(())
}

fn replace_aux_i32(record: &mut bam::Record, tag: &[u8], value: i32) -> Result<(), String> {
    if record.aux(tag).is_ok() {
        record.remove_aux(tag).map_err(|error| error.to_string())?;
    }
    record
        .push_aux_unchecked(tag, Aux::I32(value))
        .map_err(|error| error.to_string())
}

fn replace_aux_string(record: &mut bam::Record, tag: &[u8], value: &str) -> Result<(), String> {
    if record.aux(tag).is_ok() {
        record.remove_aux(tag).map_err(|error| error.to_string())?;
    }
    record
        .push_aux_unchecked(tag, Aux::String(value))
        .map_err(|error| error.to_string())
}

fn template_length(first: &bam::Record, second: &bam::Record) -> i64 {
    if first.is_unmapped() || second.is_unmapped() || first.tid() != second.tid() {
        return 0;
    }
    let start = first.pos().min(second.pos());
    let end = first.cigar().end_pos().max(second.cigar().end_pos());
    let length = end - start;
    if first.pos() <= second.pos() {
        length
    } else {
        -length
    }
}

fn has_extension(path: &str, extension: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.eq_ignore_ascii_case(extension))
        .unwrap_or(false)
}

fn header_sort_order(header: &bam::HeaderView) -> Option<String> {
    let header_text = String::from_utf8_lossy(header.as_bytes());
    header_text
        .lines()
        .find(|line| line.starts_with("@HD\t"))
        .and_then(|line| {
            line.split('\t')
                .skip(1)
                .find_map(|field| field.strip_prefix("SO:"))
        })
        .map(ToString::to_string)
}

fn sorted_header(source: &bam::HeaderView, sort_order: SortOrder) -> bam::Header {
    sorted_header_with_group_order(source, sort_order, None)
}

fn sorted_header_with_group_order(
    source: &bam::HeaderView,
    sort_order: SortOrder,
    group_order: Option<&str>,
) -> bam::Header {
    let sort_order = match sort_order {
        SortOrder::Coordinate => "coordinate",
        SortOrder::QueryName => "queryname",
        SortOrder::Unsorted => "unsorted",
    };
    let header_text = String::from_utf8_lossy(source.as_bytes());
    let mut header = bam::Header::new();
    let mut saw_hd = false;

    for line in header_text.lines() {
        if line.is_empty() {
            continue;
        }
        if line.starts_with("@CO") {
            header.push_comment(line.strip_prefix("@CO\t").unwrap_or("").as_bytes());
            continue;
        }
        let Some(record_type) = line.get(1..3) else {
            continue;
        };
        let mut record = HeaderRecord::new(record_type.as_bytes());
        let is_hd = record_type == "HD";
        saw_hd |= is_hd;
        let mut saw_so = false;
        let mut saw_go = false;
        for field in line.split('\t').skip(1) {
            let Some((tag, value)) = field.split_once(':') else {
                continue;
            };
            if is_hd && tag == "SO" {
                saw_so = true;
                if group_order.is_none() {
                    record.push_tag(b"SO", sort_order);
                }
            } else if is_hd && tag == "GO" {
                if let Some(group_order) = group_order {
                    record.push_tag(b"GO", group_order);
                } else {
                    record.push_tag(tag.as_bytes(), value);
                }
                saw_go = true;
            } else {
                record.push_tag(tag.as_bytes(), value);
            }
        }
        if is_hd {
            if let Some(group_order) = group_order {
                if !saw_go {
                    record.push_tag(b"GO", group_order);
                }
                record.push_tag(b"SO", sort_order);
            } else if !saw_so {
                record.push_tag(b"SO", sort_order);
            }
        }
        header.push_record(&record);
    }

    if !saw_hd {
        let mut record = HeaderRecord::new(b"HD");
        record.push_tag(b"VN", "1.6").push_tag(b"SO", sort_order);
        if let Some(group_order) = group_order {
            record.push_tag(b"GO", group_order);
        }
        header.push_record(&record);
    }

    header
}

fn reverted_header(
    source: &bam::HeaderView,
    remove_alignment_information: bool,
    sort_order: SortOrder,
) -> bam::Header {
    let sort_order = match sort_order {
        SortOrder::Coordinate => "coordinate",
        SortOrder::QueryName => "queryname",
        SortOrder::Unsorted => "unsorted",
    };
    let header_text = String::from_utf8_lossy(source.as_bytes());
    let mut header = bam::Header::new();
    let mut saw_hd = false;

    for line in header_text.lines() {
        if line.is_empty() || (remove_alignment_information && line.starts_with("@PG\t")) {
            continue;
        }
        if remove_alignment_information && line.starts_with("@SQ\t") {
            continue;
        }
        if line.starts_with("@CO") {
            header.push_comment(line.strip_prefix("@CO\t").unwrap_or("").as_bytes());
            continue;
        }
        let Some(record_type) = line.get(1..3) else {
            continue;
        };
        let mut record = HeaderRecord::new(record_type.as_bytes());
        let is_hd = record_type == "HD";
        saw_hd |= is_hd;
        let mut saw_so = false;
        for field in line.split('\t').skip(1) {
            let Some((tag, value)) = field.split_once(':') else {
                continue;
            };
            if is_hd && tag == "SO" {
                record.push_tag(b"SO", sort_order);
                saw_so = true;
            } else {
                record.push_tag(tag.as_bytes(), value);
            }
        }
        if is_hd && !saw_so {
            record.push_tag(b"SO", sort_order);
        }
        header.push_record(&record);
    }
    if !saw_hd {
        header.push_record(
            HeaderRecord::new(b"HD")
                .push_tag(b"VN", "1.6")
                .push_tag(b"SO", sort_order),
        );
    }
    header
}

struct MergePlan {
    header_builder: MergeHeaderBuilder,
    inputs: Vec<MergeInputPlan>,
    target_names: Vec<String>,
}

struct MergeInputPlan {
    path: String,
    read_group_renames: BTreeMap<String, String>,
    is_sorted: bool,
}

fn build_merge_plan(
    inputs: &[String],
    sort_order: SortOrder,
    assume_sorted: bool,
    reference: Option<&str>,
) -> Result<MergePlan, String> {
    let first_reader =
        open_bam_reader_with_reference(&inputs[0], reference).map_err(|error| error.to_string())?;
    let first_header_text = String::from_utf8_lossy(first_reader.header().as_bytes()).into_owned();
    let target_names = first_reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let first_sequence_dictionary = sequence_dictionary_lines(&first_header_text);
    let mut header_builder = MergeHeaderBuilder::new(&first_header_text, sort_order)?;
    drop(first_reader);

    let mut input_plans = Vec::with_capacity(inputs.len());
    for input in inputs {
        let mut reader =
            open_bam_reader_with_reference(input, reference).map_err(|error| error.to_string())?;
        let header_text = String::from_utf8_lossy(reader.header().as_bytes()).into_owned();
        if sequence_dictionary_lines(&header_text) != first_sequence_dictionary {
            return Err(
                "unsupported MergeSamFiles input with different sequence dictionary".to_string(),
            );
        }
        let read_group_renames = header_builder.observe_input_header(&header_text)?;
        let is_sorted = if assume_sorted {
            true
        } else if header_declares_sort_order(reader.header(), sort_order) {
            true
        } else {
            input_reader_is_sorted(&mut reader, sort_order)?
        };
        input_plans.push(MergeInputPlan {
            path: input.clone(),
            read_group_renames,
            is_sorted,
        });
    }

    Ok(MergePlan {
        header_builder,
        inputs: input_plans,
        target_names,
    })
}

fn collect_merge_records(
    input_plans: &[MergeInputPlan],
    reference: Option<&str>,
    interval_filter: Option<&BTreeMap<i32, Vec<(u64, u64)>>>,
) -> Result<Vec<bam::Record>, String> {
    let mut records = Vec::new();
    for input in input_plans {
        let mut reader = open_bam_reader_with_reference(&input.path, reference)
            .map_err(|error| error.to_string())?;
        for record in reader.records() {
            let mut record = record.map_err(|error| error.to_string())?;
            if !record_overlaps_intervals(&record, interval_filter) {
                continue;
            }
            rewrite_record_read_group(&mut record, &input.read_group_renames)?;
            records.push(record);
        }
    }
    Ok(records)
}

fn header_declares_sort_order(header: &bam::HeaderView, sort_order: SortOrder) -> bool {
    matches!(
        (header_sort_order(header).as_deref(), sort_order),
        (Some("coordinate"), SortOrder::Coordinate) | (Some("queryname"), SortOrder::QueryName)
    )
}

fn input_is_sorted(
    path: &str,
    sort_order: SortOrder,
    reference: Option<&str>,
) -> Result<bool, String> {
    let mut reader =
        open_bam_reader_with_reference(path, reference).map_err(|error| error.to_string())?;
    if header_declares_sort_order(reader.header(), sort_order) {
        return Ok(true);
    }
    input_reader_is_sorted(&mut reader, sort_order)
}

fn input_reader_is_sorted(reader: &mut bam::Reader, sort_order: SortOrder) -> Result<bool, String> {
    if sort_order == SortOrder::Unsorted {
        return Ok(true);
    }

    let mut previous: Option<bam::Record> = None;
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        if let Some(previous) = previous.as_ref() {
            let ordering = compare_for_sort_order(previous, &record, sort_order);
            if ordering == Ordering::Greater {
                return Ok(false);
            }
        }
        previous = Some(record);
    }
    Ok(true)
}

fn write_kway_merged_records(
    writer: &mut bam::Writer,
    input_plans: &[MergeInputPlan],
    sort_order: SortOrder,
    reference: Option<&str>,
    interval_filter: Option<&BTreeMap<i32, Vec<(u64, u64)>>>,
) -> Result<(), String> {
    let mut readers = input_plans
        .iter()
        .map(|input| {
            open_bam_reader_with_reference(&input.path, reference)
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut heap = BinaryHeap::new();
    let mut serial = 0u64;

    for input_index in 0..readers.len() {
        if let Some(record) = read_next_merge_record(
            &mut readers[input_index],
            &input_plans[input_index].read_group_renames,
            interval_filter,
        )? {
            heap.push(HeapRecord {
                record,
                input_index,
                serial,
                sort_order,
            });
            serial += 1;
        }
    }

    while let Some(item) = heap.pop() {
        let input_index = item.input_index;
        writer
            .write(&item.record)
            .map_err(|error| error.to_string())?;
        if let Some(record) = read_next_merge_record(
            &mut readers[input_index],
            &input_plans[input_index].read_group_renames,
            interval_filter,
        )? {
            heap.push(HeapRecord {
                record,
                input_index,
                serial,
                sort_order,
            });
            serial += 1;
        }
    }

    Ok(())
}

fn read_next_merge_record(
    reader: &mut bam::Reader,
    read_group_renames: &BTreeMap<String, String>,
    interval_filter: Option<&BTreeMap<i32, Vec<(u64, u64)>>>,
) -> Result<Option<bam::Record>, String> {
    loop {
        let mut record = bam::Record::new();
        match reader.read(&mut record) {
            Some(Ok(())) => {
                if !record_overlaps_intervals(&record, interval_filter) {
                    continue;
                }
                rewrite_record_read_group(&mut record, read_group_renames)?;
                return Ok(Some(record));
            }
            Some(Err(error)) => return Err(error.to_string()),
            None => return Ok(None),
        }
    }
}

struct HeapRecord {
    record: bam::Record,
    input_index: usize,
    serial: u64,
    sort_order: SortOrder,
}

impl Ord for HeapRecord {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_for_sort_order(&other.record, &self.record, self.sort_order)
            .then_with(|| other.serial.cmp(&self.serial))
    }
}

impl PartialOrd for HeapRecord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for HeapRecord {
    fn eq(&self, other: &Self) -> bool {
        compare_for_sort_order(&self.record, &other.record, self.sort_order) == Ordering::Equal
            && self.serial == other.serial
    }
}

impl Eq for HeapRecord {}

struct MergeHeaderBuilder {
    lines: Vec<String>,
    seen_read_groups: BTreeMap<String, String>,
}

impl MergeHeaderBuilder {
    fn new(first_header_text: &str, sort_order: SortOrder) -> Result<Self, String> {
        let mut lines = Vec::new();
        let mut seen_hd = false;
        let mut seen_read_groups = BTreeMap::new();

        for line in first_header_text.lines() {
            if line.is_empty() {
                continue;
            }
            if line.starts_with("@HD\t") {
                lines.push(header_line_with_sort_order(line, sort_order));
                seen_hd = true;
            } else {
                if line.starts_with("@RG\t") {
                    if let Some(id) = read_group_id(line) {
                        seen_read_groups.insert(id, line.to_string());
                    }
                }
                lines.push(line.to_string());
            }
        }

        if !seen_hd {
            lines.insert(
                0,
                format!("@HD\tVN:1.6\tSO:{}", sort_order.as_picard_value()),
            );
        }

        Ok(Self {
            lines,
            seen_read_groups,
        })
    }

    fn observe_input_header(
        &mut self,
        header_text: &str,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut renames = BTreeMap::new();
        for line in header_text.lines().filter(|line| line.starts_with("@RG\t")) {
            let Some(id) = read_group_id(line) else {
                continue;
            };
            if let Some(existing) = self.seen_read_groups.get(&id) {
                if existing == line {
                    continue;
                }
                let new_id = self.unique_read_group_id(&id);
                let renamed_line = replace_read_group_id(line, &new_id);
                self.lines.push(renamed_line.clone());
                self.seen_read_groups.insert(new_id.clone(), renamed_line);
                renames.insert(id, new_id);
            } else {
                self.lines.push(line.to_string());
                self.seen_read_groups.insert(id, line.to_string());
            }
        }
        Ok(renames)
    }

    fn unique_read_group_id(&self, id: &str) -> String {
        for suffix in 1u64.. {
            let candidate = format!("{id}.{suffix}");
            if !self.seen_read_groups.contains_key(&candidate) {
                return candidate;
            }
        }
        unreachable!("suffix loop is unbounded")
    }

    fn push_comment(&mut self, comment: &str) {
        self.lines.push(format!("@CO\t{comment}"));
    }

    fn into_header(self) -> bam::Header {
        header_from_text(&self.lines.join("\n"))
    }
}

impl SortOrder {
    fn as_picard_value(self) -> &'static str {
        match self {
            SortOrder::Coordinate => "coordinate",
            SortOrder::QueryName => "queryname",
            SortOrder::Unsorted => "unsorted",
        }
    }
}

fn header_line_with_sort_order(line: &str, sort_order: SortOrder) -> String {
    let mut fields = Vec::new();
    let mut saw_sort_order = false;
    for field in line.split('\t') {
        if field.starts_with("SO:") {
            fields.push(format!("SO:{}", sort_order.as_picard_value()));
            saw_sort_order = true;
        } else {
            fields.push(field.to_string());
        }
    }
    if !saw_sort_order {
        fields.push(format!("SO:{}", sort_order.as_picard_value()));
    }
    fields.join("\t")
}

fn header_from_text(text: &str) -> bam::Header {
    let mut header = bam::Header::new();
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        if line.starts_with("@CO") {
            header.push_comment(line.strip_prefix("@CO\t").unwrap_or("").as_bytes());
            continue;
        }
        let Some(record_type) = line.get(1..3) else {
            continue;
        };
        let mut record = HeaderRecord::new(record_type.as_bytes());
        for field in line.split('\t').skip(1) {
            let Some((tag, value)) = field.split_once(':') else {
                continue;
            };
            record.push_tag(tag.as_bytes(), value);
        }
        header.push_record(&record);
    }
    header
}

fn sequence_dictionary_lines(header_text: &str) -> Vec<String> {
    header_text
        .lines()
        .filter(|line| line.starts_with("@SQ\t"))
        .map(ToString::to_string)
        .collect()
}

fn read_group_id(line: &str) -> Option<String> {
    line.split('\t')
        .skip(1)
        .find_map(|field| field.strip_prefix("ID:").map(ToString::to_string))
}

fn replace_read_group_id(line: &str, new_id: &str) -> String {
    line.split('\t')
        .map(|field| {
            if field.starts_with("ID:") {
                format!("ID:{new_id}")
            } else {
                field.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\t")
}

fn rewrite_record_read_group(
    record: &mut bam::Record,
    renames: &BTreeMap<String, String>,
) -> Result<(), String> {
    let Ok(Aux::String(old_id)) = record.aux(b"RG") else {
        return Ok(());
    };
    let Some(new_id) = renames.get(old_id) else {
        return Ok(());
    };
    record
        .remove_aux(b"RG")
        .map_err(|error| error.to_string())?;
    record
        .push_aux(b"RG", Aux::String(new_id))
        .map_err(|error| error.to_string())
}

fn compare_for_sort_order(
    left: &bam::Record,
    right: &bam::Record,
    sort_order: SortOrder,
) -> Ordering {
    match sort_order {
        SortOrder::Coordinate => compare_coordinate(left, right),
        SortOrder::QueryName => compare_queryname(left, right),
        SortOrder::Unsorted => Ordering::Equal,
    }
}

fn compare_coordinate(left: &bam::Record, right: &bam::Record) -> Ordering {
    coordinate_tid(left)
        .cmp(&coordinate_tid(right))
        .then_with(|| left.pos().cmp(&right.pos()))
        .then_with(|| left.qname().cmp(right.qname()))
        .then_with(|| left.flags().cmp(&right.flags()))
}

fn coordinate_tid(record: &bam::Record) -> i32 {
    if record.tid() < 0 {
        i32::MAX
    } else {
        record.tid()
    }
}

fn compare_queryname(left: &bam::Record, right: &bam::Record) -> Ordering {
    left.qname()
        .cmp(right.qname())
        .then_with(|| compare_coordinate(left, right))
}

fn queryname_records_are_monotonic(records: &[bam::Record]) -> bool {
    records
        .windows(2)
        .all(|window| compare_queryname(&window[0], &window[1]) != Ordering::Greater)
}

fn picard_bai_path(output: &str) -> String {
    Path::new(output)
        .with_extension("bai")
        .display()
        .to_string()
}

fn write_requested_sidecars(
    output: &str,
    create_md5_file: bool,
    create_index: bool,
) -> Result<(), String> {
    if create_md5_file {
        write_md5_sidecar(output)?;
    }
    if create_index {
        index::build(
            output,
            Some(&picard_bai_path(output)),
            index::Type::Bai,
            turbo_picard_core::bgzf_threads::htslib_worker_threads(),
        )
        .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use turbo_picard_core::picard_args::normalize_picard_args;

    fn record_with_qname(qname: &[u8]) -> bam::Record {
        let mut record = bam::Record::new();
        record.set(qname, None, b"ACGT", b"FFFF");
        record
    }

    #[test]
    fn queryname_records_are_monotonic_for_sorted_input() {
        let records = vec![
            record_with_qname(b"read0001"),
            record_with_qname(b"read0001"),
            record_with_qname(b"read0002"),
        ];

        assert!(queryname_records_are_monotonic(&records));
    }

    #[test]
    fn queryname_records_are_not_monotonic_for_unsorted_input() {
        let records = vec![
            record_with_qname(b"read0002"),
            record_with_qname(b"read0001"),
        ];

        assert!(!queryname_records_are_monotonic(&records));
    }

    #[test]
    fn collectmultiplemetrics_single_pass_requires_hts_container_and_multiple_programs() {
        assert!(!collectmultiplemetrics_can_single_pass(
            "reads.sam",
            &["CollectAlignmentSummaryMetrics".to_string()]
        ));
        assert!(!collectmultiplemetrics_can_single_pass(
            "reads.bam",
            &["CollectAlignmentSummaryMetrics".to_string()]
        ));
        assert!(collectmultiplemetrics_can_single_pass(
            "reads.bam",
            &[
                "CollectAlignmentSummaryMetrics".to_string(),
                "CollectInsertSizeMetrics".to_string(),
            ]
        ));
        assert!(collectmultiplemetrics_can_single_pass(
            "reads.bam",
            &[
                "CollectQualityYieldMetrics".to_string(),
                "CollectAlignmentSummaryMetrics".to_string(),
            ]
        ));
    }

    #[test]
    fn collectmultiplemetrics_rejects_unsupported_arguments() {
        let args = normalize_picard_args(&[
            "PROGRAM=CollectWgsMetrics".to_string(),
            "UNKNOWN=value".to_string(),
        ])
        .expect("args parse");
        let err =
            reject_unsupported_collectmultiplemetrics_args(&args).expect_err("unsupported key");
        assert_eq!(err, "unsupported CollectMultipleMetrics argument: UNKNOWN");
    }

    #[test]
    fn collectmultiplemetrics_accepts_wgs_sampling_arguments() {
        let args = normalize_picard_args(&[
            "I=in.bam".to_string(),
            "O=out.txt".to_string(),
            "PROGRAM=CollectWgsMetrics".to_string(),
            "REFERENCE_SEQUENCE=ref.fa".to_string(),
            "SAMPLE_SIZE=12345".to_string(),
            "INCLUDE_BQ_HISTOGRAM=true".to_string(),
        ])
        .expect("args parse");
        let rejection = reject_unsupported_collectmultiplemetrics_args(&args);
        assert!(rejection.is_ok());
    }

    #[test]
    fn collectmultiplemetrics_accepts_base_distribution_alignment_filters() {
        let args = normalize_picard_args(&[
            "I=in.bam".to_string(),
            "O=out".to_string(),
            "PROGRAM=CollectBaseDistributionByCycle".to_string(),
            "EXTRA_ARGUMENT=CollectBaseDistributionByCycle::ALIGNED_READS_ONLY=true".to_string(),
            "EXTRA_ARGUMENT=CollectBaseDistributionByCycle::PF_READS_ONLY=true".to_string(),
        ])
        .expect("args parse");
        let rejection = reject_unsupported_collectmultiplemetrics_args(&args);
        assert!(rejection.is_ok());
    }

    #[test]
    fn collectmultiplemetrics_accepts_collect_quality_yield_original_quality_argument() {
        let args = normalize_picard_args(&[
            "I=in.bam".to_string(),
            "O=out".to_string(),
            "PROGRAM=CollectQualityYieldMetrics".to_string(),
            "EXTRA_ARGUMENT=CollectQualityYieldMetrics::USE_ORIGINAL_QUALITIES=false".to_string(),
        ])
        .expect("args parse");
        let rejection = reject_unsupported_collectmultiplemetrics_args(&args);
        assert!(rejection.is_ok());
    }

    #[test]
    fn mad_from_histogram_matches_naive_weighted_median() {
        let histogram = [0_u64, 5, 10, 3, 2, 0];
        let median = median_f64_from_histogram_u64(&histogram);
        let mad = mad_f64_from_histogram_u64(&histogram, median);

        let mut deviations = Vec::new();
        for (depth, count) in histogram.iter().enumerate() {
            deviations.extend(std::iter::repeat_n(
                (depth as f64 - median).abs(),
                *count as usize,
            ));
        }
        deviations.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
        let middle = deviations.len() / 2;
        let expected = if deviations.len() % 2 == 1 {
            deviations[middle]
        } else {
            (deviations[middle - 1] + deviations[middle]) / 2.0
        };
        assert!((mad - expected).abs() < 1e-9);
    }

    #[test]
    fn wgs_coverage_histogram_matches_included_loci() {
        let reference_contigs = vec![("chr1".to_string(), 12usize)];
        let mut summary = WgsMetricsSummary::new(&reference_contigs, None, 250);
        assert_eq!(summary.coverage_histogram[0], 12);

        let qualities = vec![30_u8; 12];
        summary
            .observe_cigar_ops_iter(
                "chr1",
                0,
                &qualities,
                false,
                30,
                true,
                std::iter::once(Cigar::Match(12)),
                20,
                20,
                250,
                100_000,
                false,
                None,
            )
            .expect("fixture alignment is valid");

        let scanned = summary
            .active_depths
            .iter()
            .map(|depth| (*depth as u32).min(250))
            .fold(vec![0u64; 251], |mut histogram, depth| {
                histogram[depth as usize] += 1;
                histogram
            });
        assert_eq!(summary.coverage_histogram, scanned);
        assert_eq!(summary.coverage_histogram[1], 12);
        assert_eq!(summary.coverage_histogram[0], 0);
    }

    #[test]
    fn wgs_overlap_bitmap_get_checks_exact_bit() {
        let mut bitmap = WgsOverlapBitmap::with_bit_len(130);
        bitmap.set(0);
        bitmap.set(65);

        assert!(bitmap.get(0));
        assert!(!bitmap.get(1));
        assert!(!bitmap.get(64));
        assert!(bitmap.get(65));
        assert!(!bitmap.get(66));
    }

    #[test]
    fn quality_yield_sam_missing_quality_has_zero_bases() {
        let mut summary = QualityYieldSummary::default();
        observe_quality_yield_sam_line(
            &mut summary,
            b"read\t4\t*\t0\t0\t*\t*\t0\t0\t*\t*\n",
            false,
            false,
            false,
        )
        .expect("SAM line parses");

        assert_eq!(summary.total_reads, 1);
        assert_eq!(summary.total_bases, 0);
        assert_eq!(summary.total_quality, 0);
    }

    #[test]
    fn quality_score_distribution_sam_missing_sequence_and_quality_are_empty() {
        let mut summary = QualityScoreDistributionSummary::default();
        observe_quality_score_distribution_sam_line(
            &mut summary,
            b"read\t4\t*\t0\t0\t*\t*\t0\t0\t*\t*\n",
            false,
            false,
            false,
        )
        .expect("SAM line parses");

        assert_eq!(summary.counts.iter().sum::<u64>(), 0);
    }

    #[test]
    fn base_distribution_sam_missing_sequence_is_empty() {
        let mut summary = BaseDistributionByCycleSummary::default();
        observe_base_distribution_by_cycle_sam_line(
            &mut summary,
            b"read\t4\t*\t0\t0\t*\t*\t0\t0\t*\t*\n",
            false,
            false,
        )
        .expect("SAM line parses");

        assert!(summary.first.is_empty());
        assert!(summary.second.is_empty());
    }

    #[test]
    fn mean_quality_by_cycle_sam_missing_quality_is_empty() {
        let mut summary = MeanQualityByCycleSummary::default();
        observe_mean_quality_by_cycle_sam_line(
            &mut summary,
            b"read\t4\t*\t0\t0\t*\t*\t0\t0\t*\t*\n",
            false,
            false,
        )
        .expect("SAM line parses");

        assert_eq!(summary.records, 1);
        assert!(summary.first.is_empty());
        assert!(summary.second.is_empty());
    }

    #[test]
    fn gc_bias_detail_reports_mean_base_quality() {
        let summary = GcBiasMetricsSummary {
            windows: {
                let mut windows = [0_u64; 101];
                windows[50] = 2;
                windows
            },
            read_starts: {
                let mut read_starts = [0_u64; 101];
                read_starts[50] = 1;
                read_starts
            },
            quality_sums: {
                let mut quality_sums = [0_u64; 101];
                quality_sums[50] = 120;
                quality_sums
            },
            quality_counts: {
                let mut quality_counts = [0_u64; 101];
                quality_counts[50] = 4;
                quality_counts
            },
            unique_read_starts: [0; 101],
            unique_quality_sums: [0; 101],
            unique_quality_counts: [0; 101],
            reference_path: String::new(),
            active_contig: None,
            active_sequence: Vec::new(),
            total_clusters: 1,
            aligned_reads: 1,
            unique_total_clusters: 0,
            unique_aligned_reads: 0,
            emit_unique: false,
        };

        let text = summary.detail_text(100, 0.0);
        assert!(text.contains("All Reads\tALL\t50\t2\t1\t30\t1\t1"));
    }

    #[test]
    fn expected_nm_honors_equal_and_diff_cigar_operators() {
        let mut equal_record = bam::Record::new();
        equal_record.set(
            b"eq",
            Some(&CigarString(vec![Cigar::Equal(4)])),
            b"TTTT",
            b"FFFF",
        );
        equal_record.set_tid(0);
        equal_record.set_pos(0);
        equal_record.set_flags(0);

        let mut diff_record = bam::Record::new();
        diff_record.set(
            b"diff",
            Some(&CigarString(vec![Cigar::Diff(4)])),
            b"AAAA",
            b"FFFF",
        );
        diff_record.set_tid(0);
        diff_record.set_pos(0);
        diff_record.set_flags(0);

        let reference = [Some(&b"AAAA"[..])];
        assert_eq!(
            expected_record_nm(&equal_record, &reference).expect("NM computes"),
            Some(0)
        );
        assert_eq!(
            expected_record_nm(&diff_record, &reference).expect("NM computes"),
            Some(4)
        );
    }

    #[test]
    fn validatesam_sam_text_reports_empty_platform_value() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let path = dir.path().join("empty-pl.sam");
        fs::write(
            &path,
            "@HD\tVN:1.6\tSO:unknown\n@SQ\tSN:chr1\tLN:10\n@RG\tID:rg1\tSM:s1\tPL:\n",
        )
        .expect("fixture is written");

        let report = validate_sam_summary_sam_text(path.to_str().unwrap(), true)
            .expect("validation completes");

        assert_eq!(
            report.counts.get("ERROR:MISSING_PLATFORM_VALUE").copied(),
            Some(1)
        );
    }

    #[test]
    fn header_declares_sort_order_reads_hd_so_field() {
        let dir = tempfile::tempdir().expect("tempdir exists");
        let path = dir.path().join("coordinate.sam");
        fs::write(&path, "@HD\tVN:1.6\tSO:coordinate\n@SQ\tSN:chr1\tLN:1000\n")
            .expect("fixture is written");
        let reader = bam::Reader::from_path(&path).expect("SAM opens");
        assert!(header_declares_sort_order(
            reader.header(),
            SortOrder::Coordinate
        ));
        assert!(!header_declares_sort_order(
            reader.header(),
            SortOrder::QueryName
        ));
    }

    #[test]
    fn split_fallback_command_handles_quoted_executable() {
        let parts =
            split_fallback_command(r#""/tmp/my tool/picard.jar" -jar input.jar"#).expect("parses");
        assert_eq!(
            parts,
            vec![
                "/tmp/my tool/picard.jar".to_string(),
                "-jar".to_string(),
                "input.jar".to_string()
            ]
        );
    }

    #[test]
    fn split_fallback_command_preserves_backslashes() {
        let parts = split_fallback_command(r#"C:\tools\picard.jar -jar C:\tools\runner.jar"#)
            .expect("parses");
        assert_eq!(
            parts,
            vec![
                r"C:\tools\picard.jar".to_string(),
                "-jar".to_string(),
                r"C:\tools\runner.jar".to_string()
            ]
        );
    }

    #[test]
    fn split_fallback_command_handles_escaped_spaces() {
        let parts =
            split_fallback_command(r#"C:\Program\ Files\My\ Tool\picard.jar -jar input.jar"#)
                .expect("parses");
        assert_eq!(
            parts,
            vec![
                r"C:\Program Files\My Tool\picard.jar".to_string(),
                "-jar".to_string(),
                "input.jar".to_string(),
            ]
        );
    }

    #[test]
    fn split_fallback_command_preserves_unc_paths() {
        let parts =
            split_fallback_command(r#"\\server\share\picard.jar -jar \\server\share\runner.jar"#)
                .expect("parses");
        assert_eq!(
            parts,
            vec![
                r"\\server\share\picard.jar".to_string(),
                "-jar".to_string(),
                r"\\server\share\runner.jar".to_string()
            ]
        );
    }

    #[test]
    fn split_fallback_command_rejects_unmatched_quote() {
        let err = split_fallback_command(r#""/tmp/missing.jar -jar input.jar"#).unwrap_err();
        assert_eq!(err, "invalid fallback command: unmatched quote");
    }

    #[test]
    fn split_fallback_command_rejects_unmatched_single_quote() {
        let err = split_fallback_command(r"'/tmp/missing.jar -jar input.jar").unwrap_err();
        assert_eq!(err, "invalid fallback command: unmatched quote");
    }

    #[test]
    fn split_fallback_command_handles_single_quoted_executable() {
        let parts =
            split_fallback_command("'/tmp/my tool/picard.jar' -jar input.jar").expect("parses");
        assert_eq!(
            parts,
            vec![
                "/tmp/my tool/picard.jar".to_string(),
                "-jar".to_string(),
                "input.jar".to_string(),
            ]
        );
    }

    #[test]
    fn fallback_command_quoting_handles_space_in_jar_path() {
        let quoted = quote_fallback_command_arg("/tmp/my tool/picard.jar");
        assert_eq!(quoted, "'/tmp/my tool/picard.jar'");
        let parts = split_fallback_command(&format!("java -jar {quoted}")).expect("parses");
        assert_eq!(
            parts,
            vec![
                "java".to_string(),
                "-jar".to_string(),
                "/tmp/my tool/picard.jar".to_string(),
            ]
        );
    }

    #[test]
    fn fallback_command_quoting_handles_single_quote_in_jar_path() {
        let quoted = quote_fallback_command_arg("/tmp/o'reilly/picard.jar");
        assert!(quoted.starts_with('"') && quoted.ends_with('"'));
        let parts = split_fallback_command(&format!("java -jar {quoted}")).expect("parses");
        assert_eq!(
            parts,
            vec![
                "java".to_string(),
                "-jar".to_string(),
                "/tmp/o'reilly/picard.jar".to_string(),
            ]
        );
    }

    #[test]
    fn fallback_command_quoting_escapes_double_quotes() {
        let quoted = quote_fallback_command_arg("C:/tools/picard \"beta\".jar");
        assert!(quoted.starts_with('\'') && quoted.ends_with('\''));
        let parts = split_fallback_command(&format!("java -jar {quoted}")).expect("parses");
        assert_eq!(
            parts,
            vec![
                "java".to_string(),
                "-jar".to_string(),
                "C:/tools/picard \"beta\".jar".to_string(),
            ]
        );
    }

    #[test]
    fn split_fallback_command_rejects_empty_command() {
        let err = split_fallback_command("   ").unwrap_err();
        assert_eq!(err, "fallback command is empty");
    }

    #[test]
    fn split_fallback_command_fast_path_for_simple_command() {
        assert_eq!(
            split_fallback_command("java").expect("parses"),
            vec!["java".to_string()]
        );
    }

    #[test]
    fn split_fallback_command_rejects_empty_quoted_program() {
        let err = split_fallback_command(r#""""#).unwrap_err();
        assert_eq!(err, "fallback command is empty");
    }

    #[test]
    fn split_fallback_command_handles_leading_and_trailing_whitespace() {
        let parts = split_fallback_command(r#"  "/tmp/my tool/picard.jar" -jar input.jar  "#)
            .expect("parses");
        assert_eq!(
            parts,
            vec![
                "/tmp/my tool/picard.jar".to_string(),
                "-jar".to_string(),
                "input.jar".to_string()
            ]
        );
    }

    #[test]
    fn split_fallback_command_parses_tab_separated_parts() {
        let parts = split_fallback_command("java\t-jar\tinput.jar").expect("parses");
        assert_eq!(
            parts,
            vec![
                "java".to_string(),
                "-jar".to_string(),
                "input.jar".to_string()
            ]
        );
    }

    #[test]
    fn split_fallback_command_parses_empty_quoted_argument() {
        let parts = split_fallback_command(r#"java -jar "" -Dfoo=bar"#).expect("parses");
        assert_eq!(
            parts,
            vec![
                "java".to_string(),
                "-jar".to_string(),
                String::new(),
                "-Dfoo=bar".to_string(),
            ]
        );
    }

    #[test]
    fn quote_fallback_command_uses_single_quote_without_single_quotes_inside() {
        let quoted = quote_fallback_command_arg("/tmp/with spaces/picard.jar");
        assert_eq!(quoted, "'/tmp/with spaces/picard.jar'");
        let parts = split_fallback_command(&format!("java -jar {quoted}")).expect("parses");
        assert_eq!(
            parts,
            vec![
                "java".to_string(),
                "-jar".to_string(),
                "/tmp/with spaces/picard.jar".to_string(),
            ]
        );
    }

    #[test]
    fn quote_fallback_command_uses_double_quotes_when_single_quote_present() {
        let quoted = quote_fallback_command_arg("/tmp/o'reilly/picard.jar");
        assert_eq!(quoted, "\"/tmp/o'reilly/picard.jar\"");
        let parts = split_fallback_command(&format!("java -jar {quoted}")).expect("parses");
        assert_eq!(parts[2], "/tmp/o'reilly/picard.jar");
    }

    #[test]
    fn fallback_command_with_both_quote_styles_parses_to_original_path() {
        let quoted = quote_fallback_command_arg("C:/tools/picard \"be'ta\".jar");
        assert_eq!(quoted, "\"C:/tools/picard \\\"be'ta\\\".jar\"");
        let parts = split_fallback_command(&format!("java -jar {quoted}")).expect("parses");
        assert_eq!(
            parts,
            vec![
                "java".to_string(),
                "-jar".to_string(),
                "C:/tools/picard \"be'ta\".jar".to_string(),
            ]
        );
    }
}

fn write_md5_sidecar(output: &str) -> Result<(), String> {
    let bytes = fs::read(output).map_err(|error| error.to_string())?;
    let digest = md5::compute(bytes);
    fs::write(format!("{output}.md5"), format!("{digest:x}")).map_err(|error| error.to_string())
}

fn write_vcf_idx_sidecar(output: &str, text: &str) -> Result<(), String> {
    let mut offset = 0usize;
    let mut index = String::from("# turbo-picard VCF record offsets\n");
    for line in text.split_inclusive('\n') {
        if !line.starts_with('#') {
            index.push_str(&offset.to_string());
            index.push('\n');
        }
        offset += line.len();
    }
    fs::write(format!("{output}.idx"), index).map_err(|error| error.to_string())
}

fn picard_reference_command_names() -> &'static [&'static str] {
    static COMMANDS: OnceLock<Vec<&'static str>> = OnceLock::new();
    COMMANDS
        .get_or_init(|| {
            PICARD_REFERENCE_COMMANDS
                .lines()
                .filter(|line| !line.trim().is_empty())
                .collect()
        })
        .as_slice()
}

fn is_picard_reference_command(command: &str) -> bool {
    picard_reference_command_names()
        .iter()
        .any(|name| name == &command)
}

fn print_picard_command_list() {
    for command in picard_reference_command_names() {
        println!("{command}");
    }
}

fn resolve_fallback_command() -> Option<String> {
    if let Ok(command) = env::var("TURBO_PICARD_FALLBACK_COMMAND") {
        let trimmed = command.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }
    discover_upstream_picard_command()
}

fn discover_upstream_picard_command() -> Option<String> {
    if env::var("TURBO_PICARD_DISABLE_AUTO_FALLBACK").is_ok() {
        return None;
    }
    if let Ok(jar) = env::var("PICARD_JAR") {
        let trimmed = jar.trim();
        if !trimmed.is_empty() && Path::new(trimmed).is_file() {
            return Some(format!("java -jar {}", quote_fallback_command_arg(trimmed)));
        }
    }
    if let Ok(prefix) = env::var("CONDA_PREFIX") {
        if let Some(command) = discover_picard_in_prefix(&prefix) {
            return Some(command);
        }
    }
    discover_picard_on_path()
}

fn discover_picard_in_prefix(prefix: &str) -> Option<String> {
    let share = Path::new(prefix).join("share");
    let entries = fs::read_dir(&share).ok()?;
    let mut jars = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let jar = path.join("picard.jar");
        if jar.is_file() {
            jars.push(jar);
        }
    }
    jars.sort();
    jars.into_iter().next().map(|jar| {
        format!(
            "java -jar {}",
            quote_fallback_command_arg(&jar.display().to_string())
        )
    })
}

fn discover_picard_on_path() -> Option<String> {
    let current_exe = env::current_exe().ok();
    let path_var = env::var_os("PATH")?;
    let mut candidates = Vec::new();
    for dir in env::split_paths(&path_var) {
        let candidate = dir.join("picard");
        if !is_executable_file(&candidate) {
            continue;
        }
        if current_exe.as_ref().is_some_and(|exe| exe == &candidate) {
            continue;
        }
        if is_turbo_picard_binary(&candidate) {
            continue;
        }
        candidates.push(candidate);
    }
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .map(|path| quote_fallback_command_arg(&path.to_string_lossy()))
}

fn quote_fallback_command_arg(value: &str) -> String {
    if !value.contains(&['"', '\''][..]) && !value.contains(char::is_whitespace) {
        return value.to_string();
    }
    if !value.contains('\'') {
        let mut quoted = String::with_capacity(value.len() + 2);
        quoted.push('\'');
        quoted.push_str(value);
        quoted.push('\'');
        return quoted;
    }

    let mut quoted = String::new();
    quoted.push('"');
    for ch in value.chars() {
        if ch == '"' {
            quoted.push('\\');
        }
        quoted.push(ch);
    }
    quoted.push('"');
    quoted
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && fs::metadata(path)
            .map(|meta| meta.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

fn is_turbo_picard_binary(path: &Path) -> bool {
    Command::new(path)
        .arg("--version")
        .output()
        .ok()
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .is_some_and(|text| {
            let normalized = text.trim().to_ascii_lowercase();
            normalized.contains("turbo-picard")
                || normalized.starts_with(
                    &format!("picard {}", env!("CARGO_PKG_VERSION")).to_ascii_lowercase(),
                )
        })
}

fn try_run_fallback(args: &[String]) -> Option<i32> {
    let fallback_command = resolve_fallback_command()?;

    match fallback_status(&fallback_command, args) {
        Ok(exit_code) => Some(exit_code),
        Err(error) => {
            eprintln!("{error}");
            Some(2)
        }
    }
}

fn try_run_fallback_for_native_error(error: &str, args: &[String]) -> Option<i32> {
    if should_delegate_to_picard(error) {
        try_run_fallback(args)
    } else {
        None
    }
}

fn should_delegate_to_picard(error: &str) -> bool {
    error.starts_with("unsupported ")
        || error.contains("not implemented yet")
        || error.contains("should use upstream Picard")
}

fn fallback_status(fallback_command: &str, args: &[String]) -> Result<i32, String> {
    let mut command_parts = split_fallback_command(fallback_command)?;
    let program = command_parts.remove(0);
    let mut command = Command::new(program);
    command.args(command_parts).args(args);

    let status = command
        .env_remove("TURBO_PICARD_FALLBACK_COMMAND")
        .status()
        .map_err(|error| format!("failed to run Picard fallback command: {error}"))?;

    Ok(status.code().unwrap_or(1))
}

fn split_fallback_command(command: &str) -> Result<Vec<String>, String> {
    if command.trim().is_empty() {
        return Err("fallback command is empty".to_string());
    }
    if !command.contains(char::is_whitespace) && !command.contains(&['"', '\''][..]) {
        return Ok(vec![command.to_string()]);
    }

    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quoted: Option<char> = None;
    let mut chars = command.chars().peekable();

    let mut in_token = false;

    while let Some(ch) = chars.next() {
        if ch == '\\' && quoted != Some('\'') {
            if let Some(&next) = chars.peek() {
                if next.is_whitespace() || matches!(next, '"' | '\'') {
                    current.push(next);
                    chars.next();
                    in_token = true;
                    continue;
                }
            }
            current.push('\\');
            in_token = true;
            continue;
        }

        match ch {
            '\'' if quoted.is_none() => {
                quoted = Some('\'');
                in_token = true;
            }
            '\'' if quoted == Some('\'') => {
                quoted = None;
            }
            '"' if quoted.is_none() => {
                quoted = Some('"');
                in_token = true;
            }
            '"' if quoted == Some('"') => {
                quoted = None;
            }
            ch if ch.is_whitespace() && quoted.is_none() => {
                if in_token {
                    if current.is_empty() {
                        parts.push(String::new());
                    } else {
                        parts.push(std::mem::take(&mut current));
                    }
                    in_token = false;
                }
            }
            _ => {
                in_token = true;
                current.push(ch);
            }
        }
    }

    if quoted.is_some() {
        return Err("invalid fallback command: unmatched quote".to_string());
    }
    if in_token || !current.is_empty() {
        parts.push(current);
    }
    if parts.first().is_none_or(|part| part.trim().is_empty()) {
        return Err("fallback command is empty".to_string());
    }
    Ok(parts)
}
