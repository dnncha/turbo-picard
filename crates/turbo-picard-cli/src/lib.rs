#![forbid(unsafe_code)]

use flate2::Compression;
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::index;
use rust_htslib::bam::record::{Aux, Cigar, CigarString};
use rust_htslib::bam::{self, Read};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use std::env;
use std::fs;
use std::io::{BufRead, BufReader, BufWriter, Read as IoRead, Write};
use std::path::Path;
use std::process::{self, Command};
use std::time::{SystemTime, UNIX_EPOCH};
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;
use turbo_picard_core::picard_args::normalize_picard_args_for_command;

pub fn run_cli(program_name: &str, raw_args: impl IntoIterator<Item = String>) -> i32 {
    let raw_args = raw_args.into_iter().collect::<Vec<_>>();
    if raw_args
        .first()
        .is_some_and(|arg| is_leading_jvm_option(arg))
    {
        if let Some(exit_code) = try_run_fallback(&raw_args) {
            return exit_code;
        }
        eprintln!(
            "{program_name} accepts JVM options only when TURBO_PICARD_FALLBACK_COMMAND is configured"
        );
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
            eprintln!("unsupported Picard command: {command}");
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
  AddOrReplaceReadGroups
                    Adds or replaces a single read group in SAM or BAM files
  BedToIntervalList Converts BED files to Picard interval_list files
  CleanSam          Cleans common SAM/BAM alignment issues
  CollectAlignmentSummaryMetrics
                    Writes basic alignment summary metrics for SAM or BAM files
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
                    Writes whole-genome coverage metrics for SAM or BAM files
  CreateSequenceDictionary
                    Creates a Picard sequence dictionary from a FASTA file
  FixMateInformation
                    Fixes paired-read mate fields for queryname grouped SAM/BAM
  GatherVcfs        Concatenates block-sorted VCF shards
  BuildBamIndex     Builds a BAI index for a coordinate-sorted BAM file
  IntervalListTools Concatenates, sorts, and uniques interval_list files
  LiftoverVcf      Lifts simple positive-strand VCF records through UCSC chains
  MarkDuplicates    Identifies duplicate reads in SAM or BAM files
  MeanQualityByCycle
                    Writes mean base quality by sequencing cycle
  MergeVcfs         Merges compatible VCF files by coordinate
  MergeSamFiles     Merges SAM or BAM files with optional output sorting
  NormalizeFasta    Rewrites FASTA records with fixed-width sequence lines
  QualityScoreDistribution
                    Writes base quality score distribution metrics
  ReplaceSamHeader  Replaces a SAM/BAM header while streaming records
  RevertSam         Reverts aligned SAM/BAM records to unmapped queryname output
  SamToFastq        Converts SAM or BAM records to FASTQ
  FastqToSam        Converts FASTQ records to unmapped SAM or BAM
  SetNmMdAndUqTags  Computes NM, MD, and UQ tags from a reference FASTA
  SortSam           Sorts SAM or BAM files by coordinate or query name
  SortVcf           Sorts VCF records by sequence dictionary and position
  UpdateVcfSequenceDictionary
                    Replaces VCF contig headers from a Picard dictionary
  ValidateSamFile   Validates common SAM/BAM structural issues in summary mode
  ViewSam           Converts SAM/BAM output format or writes SAM to stdout"
    );
}

fn print_markduplicates_help() {
    println!(
        "\
Usage: picard MarkDuplicates I=<input.bam> O=<output.bam> M=<metrics.txt> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file; may be repeated for BAM
  OUTPUT / O            Output SAM or BAM file
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
Usage: picard SortSam I=<input.bam> O=<output.bam> SORT_ORDER=<coordinate|queryname>

Required arguments:
  INPUT / I             Input SAM or BAM file
  OUTPUT / O            Output SAM or BAM file
  SORT_ORDER / SO       coordinate or queryname"
    );
}

fn print_cleansam_help() {
    println!(
        "\
Usage: picard CleanSam I=<input.bam> O=<output.bam> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
  OUTPUT / O            Output SAM or BAM file

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
Usage: picard MergeSamFiles I=<input.bam> [I=<input2.bam> ...] O=<output.bam> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file; may be repeated
  OUTPUT / O            Output SAM or BAM file

Common options:
  SORT_ORDER / SO       coordinate, queryname, or unsorted; defaults to coordinate
  ASSUME_SORTED / AS    Skip sortedness validation for trusted sorted inputs
  COMMENT / CO          Add one or more @CO header comments
  CREATE_INDEX
  CREATE_MD5_FILE
  MERGE_SEQUENCE_DICTIONARIES"
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
Usage: picard SamToFastq I=<input.bam> FASTQ=<reads.fastq> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
  FASTQ                 Output FASTQ file

Common options:
  SECOND_END_FASTQ      Output FASTQ for second-of-pair reads
  UNPAIRED_FASTQ        Output FASTQ for unpaired reads
  INTERLEAVE            Write paired reads interleaved to FASTQ
  RE_REVERSE            Reverse-complement reverse-strand reads
  CREATE_MD5_FILE       Write Picard-style .md5 sidecars for FASTQ outputs"
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
  READ_GROUP_NAME / RG  Read-group ID; defaults to A
  LIBRARY_NAME / LB
  PLATFORM / PL
  PLATFORM_UNIT / PU
  QUALITY_FORMAT        Standard or Illumina
  SORT_ORDER            queryname, coordinate, or unsorted
  COMMENT               Add @CO header line; may be repeated
  CREATE_MD5_FILE       Write Picard-style .md5 sidecar for OUTPUT"
    );
}

fn print_addorreplacereadgroups_help() {
    println!(
        "\
Usage: picard AddOrReplaceReadGroups I=<input.bam> O=<output.bam> RGLB=<library> RGPL=<platform> RGPU=<unit> RGSM=<sample> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
  OUTPUT / O            Output SAM or BAM file
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
  RGPM"
    );
}

fn print_collectalignmentsummarymetrics_help() {
    println!(
        "\
Usage: picard CollectAlignmentSummaryMetrics I=<input.bam> O=<metrics.txt> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
  OUTPUT / O            Alignment summary metrics file"
    );
}

fn print_collectbasedistributionbycycle_help() {
    println!(
        "\
Usage: picard CollectBaseDistributionByCycle I=<input.bam> O=<metrics.txt> CHART=<chart.pdf> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
  OUTPUT / O            Base distribution metrics file
  CHART_OUTPUT / CHART  Chart artifact path"
    );
}

fn print_collectgcbiasmetrics_help() {
    println!(
        "\
Usage: picard CollectGcBiasMetrics I=<input.bam> O=<detail.txt> S=<summary.txt> CHART=<chart.pdf> R=<reference.fa> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
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

fn print_collectqualityyieldmetrics_help() {
    println!(
        "\
Usage: picard CollectQualityYieldMetrics I=<input.bam> O=<metrics.txt> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
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
Usage: picard CollectInsertSizeMetrics I=<input.bam> O=<metrics.txt> H=<histogram.pdf> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
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
Usage: picard CollectMultipleMetrics I=<input.bam> O=<output-prefix> PROGRAM=null PROGRAM=<collector> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
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
Usage: picard CollectWgsMetrics I=<input.bam> O=<metrics.txt> R=<reference.fa> [options]

Required arguments:
  INPUT / I             Coordinate-sorted SAM or BAM file
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
  SAMPLE_SIZE=0|1
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_fixmateinformation_help() {
    println!(
        "\
Usage: picard FixMateInformation I=<input.bam> O=<output.bam> [options]

Required arguments:
  INPUT / I             Queryname-sorted SAM or BAM input file
  OUTPUT / O            Output SAM or BAM file

Supported options:
  ADD_MATE_CIGAR / MC
  ASSUME_SORTED
  SORT_ORDER=queryname
  IGNORE_MISSING_MATES=true
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
  PADDING=0
  DONT_MERGE_ABUTTING=false
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_revertsam_help() {
    println!(
        "\
Usage: picard RevertSam I=<input.bam> O=<output.bam> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
  OUTPUT / O            Output SAM or BAM file

Supported options:
  REMOVE_ALIGNMENT_INFORMATION=true
  REMOVE_DUPLICATE_INFORMATION=true
  RESTORE_ORIGINAL_QUALITIES=true
  RESTORE_HARDCLIPS=false
  SORT_ORDER=queryname
  VALIDATION_STRINGENCY
  QUIET"
    );
}

fn print_setnmmdanduqtags_help() {
    println!(
        "\
Usage: picard SetNmMdAndUqTags I=<input.bam> O=<output.bam> R=<reference.fa> [options]

Required arguments:
  INPUT / I             Coordinate-sorted SAM or BAM input file
  OUTPUT / O            Output SAM or BAM file
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
Usage: picard ValidateSamFile I=<input.sam|input.bam> [O=<summary.txt>] [MODE=SUMMARY]

Supported options:
  INPUT / I             Input SAM or BAM file
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
Usage: picard QualityScoreDistribution I=<input.bam> O=<metrics.txt> CHART=<chart.pdf> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
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
Usage: picard MeanQualityByCycle I=<input.bam> O=<metrics.txt> CHART=<chart.pdf> [options]

Required arguments:
  INPUT / I             Input SAM or BAM file
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
Usage: picard ViewSam I=<input.sam|input.bam> [O=<output.sam|output.bam>]

Supported options:
  INPUT / I             Input SAM or BAM file
  OUTPUT / O            Output SAM or BAM file; defaults to SAM on stdout
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
Usage: picard ReplaceSamHeader I=<input.sam|input.bam> O=<output.sam|output.bam> HEADER=<header.sam|header.bam>

Supported options:
  INPUT / I             Input SAM or BAM file
  OUTPUT / O            Output SAM or BAM file
  HEADER / H            SAM/BAM file whose header replaces the input header
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

    if has_sam_extension(&input)
        && has_sam_extension(&output)
        && !create_index
        && !create_md5_file
        && compression_level.is_none()
    {
        return run_sortsam_sam_text(&input, &output, sort_order);
    }

    let reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let header = sorted_header(reader.header(), sort_order);
    let format = output_format(&output)?;
    if create_index && format != bam::Format::Bam {
        return Err("SortSam CREATE_INDEX=true requires BAM output".to_string());
    }

    if input_is_sorted(&input, sort_order)? {
        let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
        let mut writer =
            bam::Writer::from_path(&output, &header, format).map_err(|error| error.to_string())?;
        if let Some(level) = compression_level {
            writer
                .set_compression_level(bam::CompressionLevel::Level(level))
                .map_err(|error| error.to_string())?;
        }
        for record in reader.records() {
            let record = record.map_err(|error| error.to_string())?;
            writer.write(&record).map_err(|error| error.to_string())?;
        }
        drop(writer);
        write_requested_sidecars(&output, create_md5_file, create_index)?;
        return Ok(());
    }

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let mut records = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    match sort_order {
        SortOrder::Coordinate => records.sort_by(compare_coordinate),
        SortOrder::QueryName => records.sort_by(compare_queryname),
        SortOrder::Unsorted => unreachable!("SortSam rejects SORT_ORDER=unsorted"),
    }

    let mut writer =
        bam::Writer::from_path(&output, &header, format).map_err(|error| error.to_string())?;
    if let Some(level) = compression_level {
        writer
            .set_compression_level(bam::CompressionLevel::Level(level))
            .map_err(|error| error.to_string())?;
    }
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

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let header = bam::Header::from_template(reader.header());
    let target_lengths = (0..reader.header().target_count())
        .map(|tid| reader.header().target_len(tid).unwrap_or(0))
        .collect::<Vec<_>>();
    let format = output_format_for(&output, "CleanSam")?;
    if create_index && format != bam::Format::Bam {
        return Err("CleanSam CREATE_INDEX=true requires BAM output".to_string());
    }
    let mut writer = bam_writer_for_path(&output, &header, format, compression_level)?;
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
    } else if fields[2] != "*" {
        if let Some(target_len) = target_lengths.get(fields[2]).copied() {
            let pos = fields[3]
                .parse::<u64>()
                .map_err(|_| "malformed CleanSam SAM position".to_string())?;
            let start = pos.saturating_sub(1);
            if start >= target_len {
                return Err(
                    "unsupported CleanSam alignment starting beyond reference end".to_string(),
                );
            }
            if let Some(cleaned) = clean_cigar_text(fields[5], start, target_len)? {
                new_cigar = Some(cleaned);
            }
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
    let mut ref_pos = start;
    let mut changed = false;
    let mut cleaned = Vec::<(u64, char)>::new();
    for (len, op) in parse_cigar_text(cigar)? {
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

fn push_text_cigar(cigars: &mut Vec<(u64, char)>, len: u64, op: char) {
    if len == 0 {
        return;
    }
    if let Some((last_len, last_op)) = cigars.last_mut() {
        if *last_op == op {
            *last_len += len;
            return;
        }
    }
    cigars.push((len, op));
}

fn run_sortsam_sam_text(input: &str, output: &str, sort_order: SortOrder) -> Result<(), String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut header_lines = Vec::<String>::new();
    let mut contig_order = BTreeMap::<String, i32>::new();
    let mut records = Vec::<SamTextSortRecord>::new();
    let mut line = String::new();
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
            if line.starts_with("@SQ\t") {
                if let Some(name) = line
                    .split('\t')
                    .skip(1)
                    .find_map(|field| field.strip_prefix("SN:"))
                {
                    contig_order.insert(
                        name.trim_end_matches(['\r', '\n']).to_string(),
                        contig_order.len() as i32,
                    );
                }
            }
            header_lines.push(line.clone());
        } else if !line.trim().is_empty() {
            records.push(SamTextSortRecord::parse(
                line.clone(),
                &contig_order,
                serial,
            )?);
            serial += 1;
        }
    }

    match sort_order {
        SortOrder::Coordinate => records.sort_by(compare_sam_text_coordinate),
        SortOrder::QueryName => records.sort_by(compare_sam_text_queryname),
        SortOrder::Unsorted => unreachable!("SortSam rejects SORT_ORDER=unsorted"),
    }

    let mut writer = BufWriter::with_capacity(
        1024 * 1024,
        fs::File::create(output).map_err(|error| error.to_string())?,
    );
    write_sorted_sam_text_header(&mut writer, &header_lines, sort_order)?;
    for record in records {
        writer
            .write_all(record.line.as_bytes())
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

#[derive(Debug)]
struct SamTextSortRecord {
    line: String,
    qname: String,
    flags: u16,
    tid: i32,
    pos: i64,
    serial: usize,
}

impl SamTextSortRecord {
    fn parse(
        line: String,
        contig_order: &BTreeMap<String, i32>,
        serial: usize,
    ) -> Result<Self, String> {
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
            line,
            qname,
            flags,
            tid,
            pos,
            serial,
        })
    }
}

fn compare_sam_text_coordinate(left: &SamTextSortRecord, right: &SamTextSortRecord) -> Ordering {
    left.tid
        .cmp(&right.tid)
        .then_with(|| left.pos.cmp(&right.pos))
        .then_with(|| left.qname.as_bytes().cmp(right.qname.as_bytes()))
        .then_with(|| left.flags.cmp(&right.flags))
        .then_with(|| left.serial.cmp(&right.serial))
}

fn compare_sam_text_queryname(left: &SamTextSortRecord, right: &SamTextSortRecord) -> Ordering {
    left.qname
        .as_bytes()
        .cmp(right.qname.as_bytes())
        .then_with(|| compare_sam_text_coordinate(left, right))
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

    let merge_plan = build_merge_plan(&inputs, sort_order, assume_sorted)?;
    let all_inputs_sorted = merge_plan.inputs.iter().all(|input| input.is_sorted);
    let mut header_builder = merge_plan.header_builder;
    for comment in args.get("COMMENT").into_iter().flatten() {
        header_builder.push_comment(comment);
    }
    let header = header_builder.into_header();
    let mut writer =
        bam::Writer::from_path(&output, &header, format).map_err(|error| error.to_string())?;
    if let Some(level) = compression_level {
        writer
            .set_compression_level(bam::CompressionLevel::Level(level))
            .map_err(|error| error.to_string())?;
    }

    if sort_order != SortOrder::Unsorted && all_inputs_sorted {
        write_kway_merged_records(&mut writer, &merge_plan.inputs, sort_order)?;
    } else {
        let mut records = collect_merge_records(&merge_plan.inputs)?;
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

    let reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    if header_sort_order(reader.header()).as_deref() != Some("coordinate") {
        return Err("BuildBamIndex requires coordinate-sorted BAM input".to_string());
    }
    drop(reader);

    index::build(&input, Some(&output), index::Type::Bai, 1).map_err(|error| error.to_string())
}

fn run_samtofastq(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("SamToFastq", args).map_err(|error| error.to_string())?;
    reject_unsupported_samtofastq_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "SamToFastq")?;
    let fastq = required_scalar_for(&args, "FASTQ", "SamToFastq")?;
    let second_end_fastq = optional_scalar(&args, "SECOND_END_FASTQ")?;
    let unpaired_fastq = optional_scalar(&args, "UNPAIRED_FASTQ")?;
    let interleave = optional_bool(&args, "INTERLEAVE")?.unwrap_or(false);
    let re_reverse = optional_bool(&args, "RE_REVERSE")?.unwrap_or(true);
    let include_non_pf_reads = optional_bool(&args, "INCLUDE_NON_PF_READS")?.unwrap_or(false);
    let include_non_primary_alignments =
        optional_bool(&args, "INCLUDE_NON_PRIMARY_ALIGNMENTS")?.unwrap_or(false);
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?.unwrap_or(5);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);

    if interleave && second_end_fastq.is_some() {
        return Err("SamToFastq INTERLEAVE=true cannot be used with SECOND_END_FASTQ".to_string());
    }

    if has_sam_extension(&input) {
        return run_samtofastq_from_sam_text(
            &input,
            &fastq,
            second_end_fastq.as_deref(),
            unpaired_fastq.as_deref(),
            interleave,
            re_reverse,
            include_non_pf_reads,
            include_non_primary_alignments,
            compression_level,
            create_md5_file,
        );
    }

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let mut first_writer = fastq_writer(&fastq, compression_level)?;
    let mut second_writer = match second_end_fastq {
        Some(ref path) => Some(fastq_writer(path, compression_level)?),
        None => None,
    };
    let mut unpaired_writer = match unpaired_fastq {
        Some(ref path) => Some(fastq_writer(path, compression_level)?),
        None => None,
    };

    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        if record.is_quality_check_failed() && !include_non_pf_reads {
            continue;
        }
        if (record.is_secondary() || record.is_supplementary()) && !include_non_primary_alignments {
            continue;
        }
        if record.is_paired() && !interleave && second_writer.is_none() {
            return Err(
                "SamToFastq input contains paired reads but no SECOND_END_FASTQ was specified"
                    .to_string(),
            );
        }
        let is_second = record.is_paired() && record.is_last_in_template();
        let writer = if is_second && !interleave {
            second_writer.as_mut().unwrap_or(&mut first_writer)
        } else if !record.is_paired() {
            unpaired_writer.as_mut().unwrap_or(&mut first_writer)
        } else {
            &mut first_writer
        };
        write_fastq_record(writer, &record, re_reverse, fastq_name_suffix(&record))?;
    }

    first_writer.flush().map_err(|error| error.to_string())?;
    if let Some(writer) = second_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    if let Some(writer) = unpaired_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    drop(first_writer);
    drop(second_writer);
    drop(unpaired_writer);
    write_samtofastq_sidecars(
        &fastq,
        second_end_fastq.as_deref(),
        unpaired_fastq.as_deref(),
        create_md5_file,
    )
}

fn run_fastqtosam(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("FastqToSam", args).map_err(|error| error.to_string())?;
    reject_unsupported_fastqtosam_args(&args)?;
    let fastq = required_scalar_for(&args, "FASTQ", "FastqToSam")?;
    let fastq2 = optional_scalar(&args, "FASTQ2")?;
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
    let quality_format =
        optional_scalar(&args, "QUALITY_FORMAT")?.unwrap_or_else(|| "Standard".to_string());
    let quality_offset = match quality_format.as_str() {
        "Standard" => 33_u8,
        "Illumina" => 64_u8,
        _ => {
            return Err(format!(
                "unsupported FastqToSam QUALITY_FORMAT={quality_format}"
            ));
        }
    };
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?.unwrap_or(5);
    let create_md5_file = optional_bool(&args, "CREATE_MD5_FILE")?.unwrap_or(false);
    let output_format = output_format_for(&output, "FastqToSam")?;
    if matches!(output_format, bam::Format::Sam) && quality_offset == 33 {
        run_fastqtosam_standard_sam(&fastq, fastq2.as_deref(), &output, &read_group)?;
        return write_requested_sidecars(&output, create_md5_file, false);
    }
    let mut writer = if matches!(output_format, bam::Format::Sam) {
        FastqToSamWriter::Sam(BufWriter::with_capacity(
            1024 * 1024,
            fs::File::create(&output).map_err(|error| error.to_string())?,
        ))
    } else {
        let mut writer =
            bam::Writer::from_path(&output, &fastqtosam_header(&read_group), output_format)
                .map_err(|error| error.to_string())?;
        writer
            .set_compression_level(bam::CompressionLevel::Level(compression_level))
            .map_err(|error| error.to_string())?;
        FastqToSamWriter::Bam(writer)
    };
    writer.write_header(&read_group)?;

    let mut first_reader = FastqReader::from_path(&fastq)?;
    let mut second_reader = match fastq2 {
        Some(path) => Some(FastqReader::from_path(&path)?),
        None => None,
    };

    let mut first_record = FastqRecord::default();
    let mut second_record = FastqRecord::default();
    loop {
        if !first_reader.next_record_into(&mut first_record)? {
            if let Some(reader) = second_reader.as_mut() {
                if reader.next_record_into(&mut second_record)? {
                    return Err(
                        "malformed FastqToSam FASTQ2 has more records than FASTQ".to_string()
                    );
                }
            }
            break;
        }
        if let Some(reader) = second_reader.as_mut() {
            if !reader.next_record_into(&mut second_record)? {
                return Err("malformed FastqToSam FASTQ has more records than FASTQ2".to_string());
            }
            if first_record.name != second_record.name {
                return Err(format!(
                    "malformed FastqToSam paired read names differ: {} vs {}",
                    first_record.name, second_record.name
                ));
            }
            writer.write_record(&first_record, 77, &read_group.id, quality_offset)?;
            writer.write_record(&second_record, 141, &read_group.id, quality_offset)?;
        } else {
            writer.write_record(&first_record, 4, &read_group.id, quality_offset)?;
        }
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
    };

    if has_sam_extension(&input)
        && has_sam_extension(&output)
        && optional_u32(&args, "COMPRESSION_LEVEL")?.is_none()
    {
        return run_addorreplacereadgroups_sam_text(&input, &output, &read_group);
    }

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let header = read_group_header(reader.header(), &read_group);
    let format = output_format(&output)?;
    let mut writer =
        bam::Writer::from_path(&output, &header, format).map_err(|error| error.to_string())?;
    if let Some(level) = optional_u32(&args, "COMPRESSION_LEVEL")? {
        writer
            .set_compression_level(bam::CompressionLevel::Level(level))
            .map_err(|error| error.to_string())?;
    }

    for record in reader.records() {
        let mut record = record.map_err(|error| error.to_string())?;
        set_record_read_group(&mut record, &read_group.id)?;
        writer.write(&record).map_err(|error| error.to_string())?;
    }

    Ok(())
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

    if has_sam_extension(&input) {
        let metrics = collect_alignment_sam_text(&input, stop_after)?;
        return fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string());
    }

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let mut metrics = AlignmentSummarySet::default();
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        metrics.observe(&record);
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

    if has_sam_extension(&input) && use_original_qualities {
        let metrics = collect_quality_yield_sam_text(
            &input,
            include_secondary,
            include_supplemental,
            stop_after,
        )?;
        return fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string());
    }

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
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

    if has_sam_extension(&input) {
        let metrics = collect_insert_size_sam_text(&input, include_duplicates, stop_after)?;
        fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())?;
        return write_placeholder_pdf(&histogram);
    }

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let mut metrics = InsertSizeSummary::default();
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        metrics.observe(&record, include_duplicates);
    }

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())?;
    write_placeholder_pdf(&histogram)
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

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let mut metrics = BaseDistributionByCycleSummary::default();
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        metrics.observe(&record, aligned_reads_only, pf_reads_only);
    }

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())?;
    write_placeholder_pdf(&chart)
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

    let references = read_fasta_sequences(&reference, true)?;
    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let mut metrics = GcBiasMetricsSummary::new(&references, window_size, also_ignore_duplicates)?;
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
    write_placeholder_pdf(&chart)
}

fn run_collectmultiplemetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("CollectMultipleMetrics", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_collectmultiplemetrics_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectMultipleMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectMultipleMetrics")?;
    let file_extension = optional_scalar(&args, "FILE_EXTENSION")?.unwrap_or_default();
    let programs = collectmultiplemetrics_programs(&args)?;
    let stop_after_arg = optional_scalar(&args, "STOP_AFTER")?
        .map(|value| format!("STOP_AFTER={value}"))
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
                child_args.extend(stop_after_arg.clone());
                run_collectalignmentsummarymetrics(&child_args)?;
                write_placeholder_pdf(&format!("{output}.read_length_histogram.pdf"))?;
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
                    "SAMPLE_SIZE=0".to_string(),
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
    let sample_size = optional_u32(&args, "SAMPLE_SIZE")?.unwrap_or(10_000);
    let include_bq_histogram = optional_bool(&args, "INCLUDE_BQ_HISTOGRAM")?.unwrap_or(false);

    let references = read_fasta_sequences(&reference, true)?;
    let interval_masks = collectwgs_interval_masks(args.get("INTERVALS"), &references)?;
    let mut summary = WgsMetricsSummary::new(&references, interval_masks, coverage_cap);
    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let target_names = reader
        .header()
        .target_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name).to_string())
        .collect::<Vec<_>>();
    let limit = if stop_after < 0 {
        None
    } else {
        Some(stop_after as usize)
    };
    for record in reader.records().take(limit.unwrap_or(usize::MAX)) {
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

    write_text_or_gzip(
        &output,
        &summary.to_picard_text(sample_size, include_bq_histogram),
    )
}

fn run_fixmateinformation(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("FixMateInformation", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_fixmateinformation_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "FixMateInformation")?;
    let Some(output) = optional_scalar(&args, "OUTPUT")? else {
        return Err("unsupported FixMateInformation missing OUTPUT".to_string());
    };
    let add_mate_cigar = optional_bool(&args, "ADD_MATE_CIGAR")?.unwrap_or(true);
    let ignore_missing_mates = optional_bool(&args, "IGNORE_MISSING_MATES")?.unwrap_or(true);
    let assume_sorted = optional_bool(&args, "ASSUME_SORTED")?.unwrap_or(false);
    let output_format = output_format_for(&output, "FixMateInformation")?;

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    if !assume_sorted && header_sort_order(reader.header()).as_deref() != Some("queryname") {
        return Err("unsupported FixMateInformation input must be queryname sorted".to_string());
    }
    let header = sorted_header(reader.header(), SortOrder::QueryName);
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let mut writer = bam_writer_for_path(&output, &header, output_format, compression_level)?;
    let mut pending = Vec::<bam::Record>::new();

    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        if pending
            .first()
            .is_some_and(|first| first.qname() != record.qname())
        {
            write_fixed_mate_group(
                &mut writer,
                &mut pending,
                add_mate_cigar,
                ignore_missing_mates,
            )?;
        }
        pending.push(record);
    }
    write_fixed_mate_group(
        &mut writer,
        &mut pending,
        add_mate_cigar,
        ignore_missing_mates,
    )
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

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let mut metrics = QualityScoreDistributionSummary::default();
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        metrics.observe(&record, aligned_reads_only, pf_reads_only, include_no_calls);
    }

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())?;
    write_placeholder_pdf(&chart)
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

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let mut metrics = MeanQualityByCycleSummary::default();
    for record in limited_records(&mut reader, stop_after) {
        let record = record.map_err(|error| error.to_string())?;
        metrics.observe(&record, aligned_reads_only, pf_reads_only);
    }

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())?;
    write_placeholder_pdf(&chart)
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
        dictionary.push('\n');
    }

    fs::write(output, dictionary).map_err(|error| error.to_string())
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

    let dictionary_text =
        fs::read_to_string(&dictionary_path).map_err(|error| error.to_string())?;
    let contig_order = dictionary_contig_order(&dictionary_text);
    let mut intervals = read_bed_intervals(&input, &contig_order)?;
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

fn bed_interval_list_header(dictionary_text: &str, sort: bool) -> String {
    let sort_order = if sort { "coordinate" } else { "unsorted" };
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

    let first_text = fs::read_to_string(&inputs[0]).map_err(|error| error.to_string())?;
    let header_text = interval_list_header_text(&first_text);
    let contig_order = dictionary_contig_order(&header_text);
    let mut intervals = Vec::<BedInterval>::new();
    intervals.extend(read_interval_list_intervals(&first_text, &contig_order)?);
    for input in inputs.iter().skip(1) {
        let text = fs::read_to_string(input).map_err(|error| error.to_string())?;
        intervals.extend(read_interval_list_intervals(&text, &contig_order)?);
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
    let attributes_to_clear = attributes_to_clear_for_revertsam(&args)?;

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let header = reverted_header(reader.header(), remove_alignment_information);
    let mut records = reader
        .records()
        .map(|record| {
            let mut record = record.map_err(|error| error.to_string())?;
            revert_record(
                &mut record,
                restore_original_qualities,
                remove_alignment_information,
                remove_duplicate_information,
                restore_hardclips,
                &attributes_to_clear,
            )?;
            Ok(record)
        })
        .collect::<Result<Vec<_>, String>>()?;
    records.sort_by(compare_queryname);

    let mut writer = bam_writer_for_path(&output, &header, output_format, compression_level)?;
    for record in records {
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn run_setnmmdanduqtags(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("SetNmMdAndUqTags", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_setnmmdanduqtags_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "SetNmMdAndUqTags")?;
    let output = required_scalar_for(&args, "OUTPUT", "SetNmMdAndUqTags")?;
    let reference = required_scalar_for(&args, "REFERENCE_SEQUENCE", "SetNmMdAndUqTags")?;
    let output_format = output_format_for(&output, "SetNmMdAndUqTags")?;
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?;
    let set_only_uq = optional_bool(&args, "SET_ONLY_UQ")?.unwrap_or(false);

    let reference = reference_sequences_by_name(&reference)?;
    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
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
    let mut writer = bam_writer_for_path(&output, &header, output_format, compression_level)?;

    for record in reader.records() {
        let mut record = record.map_err(|error| error.to_string())?;
        set_nm_md_uq_tags(&mut record, &target_names, &reference, set_only_uq)?;
        writer.write(&record).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn run_validatesamfile(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args_for_command("ValidateSamFile", args)
        .map_err(|error| error.to_string())?;
    reject_unsupported_validatesamfile_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "ValidateSamFile")?;
    let output = optional_scalar(&args, "OUTPUT")?;
    let skip_mate_validation = optional_bool(&args, "SKIP_MATE_VALIDATION")?.unwrap_or(false);
    let ignored = validate_sam_ignored_summary_keys(&args)?;

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let mut report = validate_sam_summary(&mut reader, skip_mate_validation)?;
    for key in ignored {
        report.counts.remove(&key);
    }
    write_validate_sam_summary(output.as_deref(), &report.counts)?;

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

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let header = bam::Header::from_template(reader.header());
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
        );
    }
    match output {
        Some(output) => {
            let format = output_format_for(&output, "ViewSam")?;
            let mut writer = bam_writer_for_path(&output, &header, format, compression_level)?;
            for record in reader.records() {
                let record = record.map_err(|error| error.to_string())?;
                if viewsam_record_matches(&record, &alignment_status, &pf_status)? {
                    writer.write(&record).map_err(|error| error.to_string())?;
                }
            }
        }
        None => {
            let mut writer = bam::Writer::from_stdout(&header, bam::Format::Sam)
                .map_err(|error| error.to_string())?;
            for record in reader.records() {
                let record = record.map_err(|error| error.to_string())?;
                if viewsam_record_matches(&record, &alignment_status, &pf_status)? {
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
            if viewsam_record_matches(&record, alignment_status, pf_status)? {
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

    let header_reader = bam::Reader::from_path(&header_input).map_err(|error| error.to_string())?;
    let header = bam::Header::from_template(header_reader.header());
    let replacement_sort_order = header_sort_order(header_reader.header());
    drop(header_reader);

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let input_sort_order = header_sort_order(reader.header());
    if input_sort_order != replacement_sort_order {
        return Err(format!(
            "ReplaceSamHeader sort orders of INPUT ({}) and HEADER ({}) do not agree",
            input_sort_order.unwrap_or_else(|| "unknown".to_string()),
            replacement_sort_order.unwrap_or_else(|| "unknown".to_string())
        ));
    }
    let format = output_format_for(&output, "ReplaceSamHeader")?;
    let mut writer = bam_writer_for_path(&output, &header, format, compression_level)?;
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

    let dictionary_text = fs::read_to_string(dictionary_path).map_err(|error| error.to_string())?;
    let contig_lines = vcf_contig_lines_from_dictionary(&dictionary_text)?;
    let input_text = read_text_or_gzip(&input)?;
    let output_text = replace_vcf_contig_header(&input_text, &contig_lines)?;
    write_text_or_gzip(&output, &output_text)
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
    lifted.sort_by(|left, right| {
        contig_order[&left.contig]
            .cmp(&contig_order[&right.contig])
            .then_with(|| left.position.cmp(&right.position))
            .then_with(|| left.serial.cmp(&right.serial))
    });

    let output_text = liftover_output_vcf_text(&document, &contig_lines, &reference_line, &lifted);
    let reject_text = liftover_reject_vcf_text(&document, &contig_lines, &rejected);
    write_text_or_gzip(&output, &output_text)?;
    write_text_or_gzip(&reject, &reject_text)
}

fn run_gathervcfs(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("GatherVcfs", args).map_err(|error| error.to_string())?;
    reject_unsupported_gathervcfs_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "GatherVcfs")?;
    let output = required_scalar_for(&args, "OUTPUT", "GatherVcfs")?;

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
    write_text_or_gzip(&output, &text)
}

fn run_sortvcf(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("SortVcf", args).map_err(|error| error.to_string())?;
    reject_unsupported_sortvcf_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "SortVcf")?;
    let output = required_scalar_for(&args, "OUTPUT", "SortVcf")?;
    let dictionary_path = optional_scalar(&args, "SEQUENCE_DICTIONARY")?;

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
    let mut records = Vec::new();
    for document in documents {
        for mut record in document.records {
            record.serial = records.len();
            records.push(record);
        }
    }
    for record in &records {
        if !contig_order.contains_key(&record.contig) {
            return Err(format!(
                "VCF contig {} is not present in sequence dictionary",
                record.contig
            ));
        }
    }
    records.sort_by(|left, right| {
        contig_order[&left.contig]
            .cmp(&contig_order[&right.contig])
            .then_with(|| left.position.cmp(&right.position))
            .then_with(|| left.serial.cmp(&right.serial))
    });

    for record in records {
        text.push_str(&record.line);
        text.push('\n');
    }
    write_text_or_gzip(&output, &text)
}

fn run_mergevcfs(args: &[String]) -> Result<(), String> {
    let args =
        normalize_picard_args_for_command("MergeVcfs", args).map_err(|error| error.to_string())?;
    reject_unsupported_mergevcfs_args(&args)?;
    let inputs = required_values_for(&args, "INPUT", "MergeVcfs")?;
    let output = required_scalar_for(&args, "OUTPUT", "MergeVcfs")?;
    let dictionary_path = optional_scalar(&args, "SEQUENCE_DICTIONARY")?;

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
    let mut records = Vec::new();
    for document in documents {
        for mut record in document.records {
            record.serial = records.len();
            records.push(record);
        }
    }
    for record in &records {
        if !contig_order.contains_key(&record.contig) {
            return Err(format!(
                "VCF contig {} is not present in sequence dictionary",
                record.contig
            ));
        }
    }
    records.sort_by(|left, right| {
        contig_order[&left.contig]
            .cmp(&contig_order[&right.contig])
            .then_with(|| left.position.cmp(&right.position))
            .then_with(|| left.serial.cmp(&right.serial))
    });
    for record in records {
        text.push_str(&record.line);
        text.push('\n');
    }
    write_text_or_gzip(&output, &text)
}

fn reject_unsupported_viewsam_args(args: &BTreeMap<String, Vec<String>>) -> Result<(), String> {
    let supported = [
        "INPUT",
        "OUTPUT",
        "ALIGNMENT_STATUS",
        "PF_STATUS",
        "HEADER_ONLY",
        "RECORDS_ONLY",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
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
    Ok(alignment_matches && pf_matches)
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
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
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
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
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
        "TMP_DIR",
        "MAX_RECORDS_IN_RAM",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported GatherVcfs argument: {key}"));
        }
    }
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    let _ = args.get("TMP_DIR");
    optional_u32(args, "MAX_RECORDS_IN_RAM")?;
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
        "COMPRESSION_LEVEL",
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
        "NUM_SEQUENCES",
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
    optional_u32(args, "NUM_SEQUENCES")?;
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
    if optional_i64(args, "PADDING")?.unwrap_or(0) != 0 {
        return Err("unsupported IntervalListTools PADDING".to_string());
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
        "REMOVE_ALIGNMENT_INFORMATION",
        "REMOVE_DUPLICATE_INFORMATION",
        "RESTORE_ORIGINAL_QUALITIES",
        "RESTORE_HARDCLIPS",
        "SORT_ORDER",
        "ATTRIBUTE_TO_CLEAR",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported RevertSam argument: {key}"));
        }
    }
    if optional_bool(args, "REMOVE_ALIGNMENT_INFORMATION")? == Some(false) {
        return Err("unsupported RevertSam REMOVE_ALIGNMENT_INFORMATION=false".to_string());
    }
    optional_bool(args, "REMOVE_DUPLICATE_INFORMATION")?;
    optional_bool(args, "RESTORE_ORIGINAL_QUALITIES")?;
    optional_bool(args, "RESTORE_HARDCLIPS")?;
    if let Some(sort_order) = optional_scalar(args, "SORT_ORDER")? {
        if sort_order != "queryname" {
            return Err(format!("unsupported RevertSam SORT_ORDER={sort_order}"));
        }
    }
    let _ = attributes_to_clear_for_revertsam(args)?;
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
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
    let mut attributes = Vec::new();
    for attribute in args.get("ATTRIBUTE_TO_CLEAR").into_iter().flatten() {
        let bytes = attribute.as_bytes();
        if bytes.len() != 2 || !bytes.iter().all(|byte| byte.is_ascii_alphanumeric()) {
            return Err(format!(
                "unsupported RevertSam ATTRIBUTE_TO_CLEAR={attribute}"
            ));
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
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported ValidateSamFile argument: {key}"));
        }
    }
    if let Some(mode) = optional_scalar(args, "MODE")? {
        if mode.to_ascii_uppercase() != "SUMMARY" {
            return Err(format!("unsupported ValidateSamFile MODE={mode}"));
        }
    }
    optional_u32(args, "MAX_OUTPUT")?;
    optional_bool(args, "SKIP_MATE_VALIDATION")?;
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
}

fn validate_sam_summary(
    reader: &mut bam::Reader,
    skip_mate_validation: bool,
) -> Result<ValidateSamReport, String> {
    let header_text = String::from_utf8_lossy(reader.header().as_bytes()).to_string();
    let read_groups = read_group_platforms(&header_text);
    let target_count = reader.header().target_count();
    let mut report = ValidateSamReport::default();

    if target_count == 0 {
        add_validate_count(&mut report, "ERROR:MISSING_SEQUENCE_DICTIONARY");
    }
    if read_groups.is_empty() {
        add_validate_count(&mut report, "ERROR:MISSING_READ_GROUP");
    } else {
        for has_platform in read_groups.values() {
            if !has_platform {
                add_validate_count(&mut report, "ERROR:MISSING_PLATFORM_VALUE");
            }
        }
    }

    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        if record.is_paired() && !skip_mate_validation {
            return Err(
                "unsupported ValidateSamFile paired input requires SKIP_MATE_VALIDATION=true"
                    .to_string(),
            );
        }
        validate_sam_record_summary(&record, target_count, &read_groups, &mut report)?;
    }

    Ok(report)
}

fn validate_sam_record_summary(
    record: &bam::Record,
    target_count: u32,
    read_groups: &BTreeMap<String, bool>,
    report: &mut ValidateSamReport,
) -> Result<(), String> {
    if !record.is_unmapped() {
        if record.tid() < 0 || record.tid() as u32 >= target_count {
            add_validate_count(report, "ERROR:MISSING_SEQUENCE_DICTIONARY");
        }
        if record.aux(b"NM").is_err() {
            add_validate_count(report, "WARNING:MISSING_TAG_NM");
        }
    }

    match record.aux(b"RG") {
        Ok(Aux::String(read_group)) => {
            if !read_groups.contains_key(read_group) {
                add_validate_count(report, "ERROR:READ_GROUP_NOT_FOUND");
            }
        }
        Ok(_) => add_validate_count(report, "ERROR:INVALID_TAG_TYPE"),
        Err(_) => add_validate_count(report, "WARNING:RECORD_MISSING_READ_GROUP"),
    }
    Ok(())
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
            "READ_GROUP_NOT_FOUND" => "ERROR:READ_GROUP_NOT_FOUND",
            "INVALID_TAG_TYPE" => "ERROR:INVALID_TAG_TYPE",
            "RECORD_MISSING_READ_GROUP" => "WARNING:RECORD_MISSING_READ_GROUP",
            _ => return Err(format!("unsupported ValidateSamFile IGNORE={value}")),
        };
        ignored.insert(key.to_string());
    }
    Ok(ignored)
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

fn write_placeholder_pdf(path: &str) -> Result<(), String> {
    fs::write(
        path,
        b"%PDF-1.4\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj\n2 0 obj<</Type/Pages/Count 0>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF\n",
    )
    .map_err(|error| error.to_string())
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

fn collectwgs_interval_masks(
    interval_paths: Option<&Vec<String>>,
    references: &[FastaSequence],
) -> Result<Option<BTreeMap<String, Vec<bool>>>, String> {
    let Some(interval_paths) = interval_paths else {
        return Ok(None);
    };
    let reference_lengths = references
        .iter()
        .map(|reference| (reference.name.clone(), reference.sequence.len()))
        .collect::<BTreeMap<_, _>>();
    let contig_order = references
        .iter()
        .enumerate()
        .map(|(index, reference)| (reference.name.clone(), index))
        .collect::<BTreeMap<_, _>>();
    let mut masks = references
        .iter()
        .map(|reference| {
            (
                reference.name.clone(),
                vec![false; reference.sequence.len()],
            )
        })
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
        if level != "ALL_READS" {
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
    optional_scalar(args, "HISTOGRAM_FILE")?;
    if let Some(level) = optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")? {
        if level != "ALL_READS" {
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
            | ("CollectQualityYieldMetrics", "INCLUDE_SUPPLEMENTAL_ALIGNMENTS") => {}
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
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    if let Some(level) = optional_scalar(args, "METRIC_ACCUMULATION_LEVEL")? {
        if level != "ALL_READS" {
            return Err(format!(
                "unsupported CollectMultipleMetrics METRIC_ACCUMULATION_LEVEL={level}"
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
    collectmultiplemetrics_programs(args)?;
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
    if optional_bool(args, "USE_FAST_ALGORITHM")?.unwrap_or(false) {
        return Err("unsupported CollectWgsMetrics USE_FAST_ALGORITHM=true".to_string());
    }
    optional_bool(args, "COUNT_UNPAIRED")?;
    optional_u32(args, "MINIMUM_MAPPING_QUALITY")?;
    optional_u32(args, "MINIMUM_BASE_QUALITY")?;
    optional_u32(args, "COVERAGE_CAP")?;
    optional_u32(args, "LOCUS_ACCUMULATION_CAP")?;
    optional_i64(args, "STOP_AFTER")?;
    if let Some(sample_size) = optional_u32(args, "SAMPLE_SIZE")? {
        if sample_size > 1 {
            return Err(format!(
                "unsupported CollectWgsMetrics SAMPLE_SIZE={sample_size}"
            ));
        }
    }
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
    ];
    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported FixMateInformation argument: {key}"));
        }
    }
    if args.get("INPUT").map_or(0, Vec::len) != 1 {
        return Err("unsupported FixMateInformation multiple INPUT values".to_string());
    }
    if let Some(sort_order) = optional_scalar(args, "SORT_ORDER")? {
        if sort_order != "queryname" {
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

impl AlignmentSummarySet {
    fn observe(&mut self, record: &bam::Record) {
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
        aligned_length: u64,
        mapq: u8,
        qualities: &[u8],
    ) {
        if flags & 0x1 != 0 {
            self.saw_paired = true;
            if flags & 0x40 != 0 {
                self.first
                    .observe_sam_parts(flags, read_length, aligned_length, mapq, qualities);
            } else if flags & 0x80 != 0 {
                self.second
                    .observe_sam_parts(flags, read_length, aligned_length, mapq, qualities);
            }
            self.pair
                .observe_sam_parts(flags, read_length, aligned_length, mapq, qualities);
        } else {
            self.unpaired
                .observe_sam_parts(flags, read_length, aligned_length, mapq, qualities);
        }
    }

    fn to_picard_text(&self) -> String {
        if self.saw_paired {
            AlignmentSummary::to_picard_text_for_rows(&[
                ("FIRST_OF_PAIR", &self.first),
                ("SECOND_OF_PAIR", &self.second),
                ("PAIR", &self.pair),
            ])
        } else {
            AlignmentSummary::to_picard_text_for_rows(&[("UNPAIRED", &self.unpaired)])
        }
    }
}

#[derive(Debug, Default)]
struct AlignmentSummary {
    total_reads: u64,
    pf_reads: u64,
    pf_noise_reads: u64,
    pf_reads_aligned: u64,
    pf_aligned_bases: u64,
    pf_hq_aligned_reads: u64,
    pf_hq_aligned_bases: u64,
    pf_hq_aligned_q20_bases: u64,
    reads_aligned_in_pairs: u64,
    pf_reads_improper_pairs: u64,
    bad_cycles: u64,
    forward_aligned_reads: u64,
    reverse_aligned_reads: u64,
    total_read_lengths: Vec<u64>,
    aligned_read_lengths: Vec<u64>,
}

impl AlignmentSummary {
    fn observe(&mut self, record: &bam::Record) {
        let read_length = record.seq_len() as u64;
        let aligned_length = aligned_read_length(record);
        self.total_reads += 1;
        ensure_histogram_len(&mut self.total_read_lengths, read_length as usize);
        self.total_read_lengths[read_length as usize] += 1;

        if record.is_quality_check_failed() {
            return;
        }

        self.pf_reads += 1;
        if is_noise_read(record) {
            self.pf_noise_reads += 1;
        }

        let is_aligned = !record.is_unmapped();
        if is_aligned {
            self.pf_reads_aligned += 1;
            self.pf_aligned_bases += aligned_length;
            if is_hq_aligned(record) {
                self.pf_hq_aligned_reads += 1;
                self.pf_hq_aligned_bases += aligned_length;
                self.pf_hq_aligned_q20_bases += record
                    .qual()
                    .iter()
                    .filter(|quality| **quality >= 20)
                    .count() as u64;
            }
            if record.is_reverse() {
                self.reverse_aligned_reads += 1;
            } else {
                self.forward_aligned_reads += 1;
            }
            if record.is_paired() && !record.is_mate_unmapped() {
                self.reads_aligned_in_pairs += 1;
                if !record.is_proper_pair() {
                    self.pf_reads_improper_pairs += 1;
                }
            }
        }

        ensure_histogram_len(&mut self.aligned_read_lengths, aligned_length as usize);
        self.aligned_read_lengths[aligned_length as usize] += 1;
    }

    fn observe_sam_parts(
        &mut self,
        flags: u16,
        read_length: u64,
        aligned_length: u64,
        mapq: u8,
        qualities: &[u8],
    ) {
        self.total_reads += 1;
        ensure_histogram_len(&mut self.total_read_lengths, read_length as usize);
        self.total_read_lengths[read_length as usize] += 1;

        if flags & 0x200 != 0 {
            return;
        }

        self.pf_reads += 1;
        let is_aligned = flags & 0x4 == 0;
        if is_aligned {
            self.pf_reads_aligned += 1;
            self.pf_aligned_bases += aligned_length;
            if mapq >= 20 {
                self.pf_hq_aligned_reads += 1;
                self.pf_hq_aligned_bases += aligned_length;
                self.pf_hq_aligned_q20_bases +=
                    qualities.iter().filter(|quality| **quality >= b'5').count() as u64;
            }
            if flags & 0x10 != 0 {
                self.reverse_aligned_reads += 1;
            } else {
                self.forward_aligned_reads += 1;
            }
            if flags & 0x1 != 0 && flags & 0x8 == 0 {
                self.reads_aligned_in_pairs += 1;
                if flags & 0x2 == 0 {
                    self.pf_reads_improper_pairs += 1;
                }
            }
        }

        ensure_histogram_len(&mut self.aligned_read_lengths, aligned_length as usize);
        self.aligned_read_lengths[aligned_length as usize] += 1;
    }

    fn to_picard_row(&self, category: &str) -> String {
        let mean_read_length = mean_from_histogram(&self.total_read_lengths);
        let sd_read_length = standard_deviation_from_histogram(&self.total_read_lengths);
        let median_read_length = median_from_histogram(&self.total_read_lengths);
        let mad_read_length = mad_from_histogram(&self.total_read_lengths, median_read_length);
        let min_read_length = min_from_histogram(&self.total_read_lengths);
        let max_read_length = max_from_histogram(&self.total_read_lengths);
        let mean_aligned_read_length = if self.pf_reads == 0 {
            0.0
        } else {
            self.pf_aligned_bases as f64 / self.pf_reads as f64
        };
        let aligned_reads = self.forward_aligned_reads + self.reverse_aligned_reads;
        let strand_balance = if aligned_reads == 0 {
            0.0
        } else {
            self.forward_aligned_reads as f64 / aligned_reads as f64
        };

        format!(
            "{category}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t0\t0\t0\t0\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t0\t0\t{}\t{}\t0\t\t\t\n",
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
            format_float(mean_read_length),
            format_float(sd_read_length),
            median_read_length,
            mad_read_length,
            min_read_length,
            max_read_length,
            format_float(mean_aligned_read_length),
            self.reads_aligned_in_pairs,
            format_float(ratio(self.reads_aligned_in_pairs, self.pf_reads_aligned)),
            self.pf_reads_improper_pairs,
            format_float(ratio(self.pf_reads_improper_pairs, self.pf_reads_aligned)),
            self.bad_cycles,
            format_float(strand_balance),
            format_float(percent_cigar_bases(self, CigarBaseKind::SoftClip)),
            format_float(percent_cigar_bases(self, CigarBaseKind::HardClip)),
        )
    }

    fn to_picard_text_for_rows(rows: &[(&str, &AlignmentSummary)]) -> String {
        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.AlignmentSummaryMetrics\n");
        output.push_str("CATEGORY\tTOTAL_READS\tPF_READS\tPCT_PF_READS\tPF_NOISE_READS\tPF_READS_ALIGNED\tPCT_PF_READS_ALIGNED\tPF_ALIGNED_BASES\tPF_HQ_ALIGNED_READS\tPF_HQ_ALIGNED_BASES\tPF_HQ_ALIGNED_Q20_BASES\tPF_HQ_MEDIAN_MISMATCHES\tPF_MISMATCH_RATE\tPF_HQ_ERROR_RATE\tPF_INDEL_RATE\tMEAN_READ_LENGTH\tSD_READ_LENGTH\tMEDIAN_READ_LENGTH\tMAD_READ_LENGTH\tMIN_READ_LENGTH\tMAX_READ_LENGTH\tMEAN_ALIGNED_READ_LENGTH\tREADS_ALIGNED_IN_PAIRS\tPCT_READS_ALIGNED_IN_PAIRS\tPF_READS_IMPROPER_PAIRS\tPCT_PF_READS_IMPROPER_PAIRS\tBAD_CYCLES\tSTRAND_BALANCE\tPCT_CHIMERAS\tPCT_ADAPTER\tPCT_SOFTCLIP\tPCT_HARDCLIP\tAVG_POS_3PRIME_SOFTCLIP_LENGTH\tSAMPLE\tLIBRARY\tREAD_GROUP\n");
        for (category, summary) in rows {
            output.push_str(&summary.to_picard_row(category));
        }
        output.push('\n');
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        output
            .push_str("READ_LENGTH\tUNPAIRED_TOTAL_LENGTH_COUNT\tUNPAIRED_ALIGNED_LENGTH_COUNT\n");
        if let Some((_, histogram_summary)) = rows.last() {
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
        }
        output
    }
}

#[derive(Debug, Clone, Copy)]
enum CigarBaseKind {
    SoftClip,
    HardClip,
}

fn percent_cigar_bases(_summary: &AlignmentSummary, _kind: CigarBaseKind) -> f64 {
    0.0
}

fn aligned_read_length(record: &bam::Record) -> u64 {
    if record.is_unmapped() {
        return 0;
    }
    record
        .cigar()
        .iter()
        .map(|cigar| match cigar {
            Cigar::Match(len) | Cigar::Ins(len) | Cigar::Equal(len) | Cigar::Diff(len) => {
                *len as u64
            }
            _ => 0,
        })
        .sum()
}

fn is_hq_aligned(record: &bam::Record) -> bool {
    !record.is_unmapped() && record.mapq() >= 20
}

fn is_noise_read(record: &bam::Record) -> bool {
    let _ = record;
    false
}

fn collect_alignment_sam_text(input: &str, stop_after: u32) -> Result<AlignmentSummarySet, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut metrics = AlignmentSummarySet::default();
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
        observe_alignment_sam_line(&mut metrics, &line)?;
        observed = observed.saturating_add(1);
        if stop_after > 0 && observed >= stop_after {
            break;
        }
    }
    Ok(metrics)
}

fn observe_alignment_sam_line(
    metrics: &mut AlignmentSummarySet,
    line: &[u8],
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
    fields
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
    for _ in 0..3 {
        fields
            .next()
            .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics SAM record".to_string())?;
    }
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
    let aligned_length = if flags & 0x4 != 0 {
        0
    } else {
        aligned_read_length_from_cigar(cigar)?
    };
    let qualities = if qualities == b"*" {
        &[][..]
    } else {
        qualities
    };
    metrics.observe_sam_parts(flags, read_length, aligned_length, mapq, qualities);
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
    if total_count == 0 {
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
        / total_count as f64;
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
    let qualities = fields
        .next()
        .ok_or_else(|| "malformed CollectQualityYieldMetrics SAM record".to_string())?;
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
) -> Result<InsertSizeSummary, String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut line = Vec::new();
    let mut metrics = InsertSizeSummary::default();
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
        observe_insert_size_sam_line(&mut metrics, &line, include_duplicates)?;
        observed = observed.saturating_add(1);
        if stop_after > 0 && observed >= stop_after {
            break;
        }
    }
    Ok(metrics)
}

fn observe_insert_size_sam_line(
    metrics: &mut InsertSizeSummary,
    line: &[u8],
    include_duplicates: bool,
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
    metrics.observe_sam_parts(flags, insert_size, include_duplicates);
    Ok(())
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

fn aligned_read_length_from_cigar(cigar: &[u8]) -> Result<u64, String> {
    if cigar == b"*" {
        return Ok(0);
    }
    let mut total = 0_u64;
    let mut len = 0_u64;
    let mut saw_digit = false;
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
            b'M' | b'I' | b'=' | b'X' => {
                total = total
                    .checked_add(len)
                    .ok_or_else(|| "malformed CollectAlignmentSummaryMetrics CIGAR".to_string())?;
            }
            b'D' | b'N' | b'S' | b'H' | b'P' => {}
            _ => return Err("malformed CollectAlignmentSummaryMetrics CIGAR".to_string()),
        }
        len = 0;
        saw_digit = false;
    }
    if saw_digit {
        return Err("malformed CollectAlignmentSummaryMetrics CIGAR".to_string());
    }
    Ok(total)
}

#[derive(Debug)]
struct WgsMetricsSummary {
    contigs: BTreeMap<String, WgsContigCoverage>,
    coverage_cap: u32,
    total_aligned_bases: u64,
    excluded_mapq: u64,
    excluded_duplicate: u64,
    excluded_unpaired: u64,
    excluded_baseq: u64,
    excluded_capped: u64,
    base_quality_histogram: Vec<u64>,
}

#[derive(Debug)]
struct WgsContigCoverage {
    depths: Vec<u32>,
    included: Vec<bool>,
}

impl WgsMetricsSummary {
    fn new(
        references: &[FastaSequence],
        interval_masks: Option<BTreeMap<String, Vec<bool>>>,
        coverage_cap: u32,
    ) -> Self {
        let contigs = references
            .iter()
            .map(|reference| {
                let included = interval_masks
                    .as_ref()
                    .and_then(|masks| masks.get(&reference.name).cloned())
                    .unwrap_or_else(|| vec![true; reference.sequence.len()]);
                (
                    reference.name.clone(),
                    WgsContigCoverage {
                        depths: vec![0; reference.sequence.len()],
                        included,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        Self {
            contigs,
            coverage_cap,
            total_aligned_bases: 0,
            excluded_mapq: 0,
            excluded_duplicate: 0,
            excluded_unpaired: 0,
            excluded_baseq: 0,
            excluded_capped: 0,
            base_quality_histogram: vec![0; coverage_cap as usize + 1],
        }
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
        let Some(coverage) = self.contigs.get_mut(contig) else {
            return Err(format!(
                "CollectWgsMetrics reference missing contig {contig}"
            ));
        };

        let qualities = record.qual();
        let mut read_offset = 0usize;
        let mut reference_offset = record.pos().max(0) as usize;
        for cigar in record.cigar().iter() {
            match cigar {
                Cigar::Match(len) | Cigar::Equal(len) | Cigar::Diff(len) => {
                    for index in 0..*len as usize {
                        let read_index = read_offset + index;
                        let reference_index = reference_offset + index;
                        if reference_index >= coverage.depths.len() {
                            return Err(
                                "CollectWgsMetrics alignment extends beyond reference".to_string()
                            );
                        }
                        if !coverage.included[reference_index] {
                            continue;
                        }
                        self.total_aligned_bases += 1;
                        if record.is_duplicate() {
                            self.excluded_duplicate += 1;
                        } else if record.mapq() < minimum_mapping_quality {
                            self.excluded_mapq += 1;
                        } else if record.is_paired() == false && !count_unpaired {
                            self.excluded_unpaired += 1;
                        } else if qualities
                            .get(read_index)
                            .is_none_or(|quality| *quality < minimum_base_quality)
                        {
                            self.excluded_baseq += 1;
                        } else if coverage.depths[reference_index] >= coverage_cap
                            || coverage.depths[reference_index] >= locus_accumulation_cap
                        {
                            self.excluded_capped += 1;
                        } else {
                            if let Some(quality) = qualities.get(read_index) {
                                let index = *quality as usize;
                                if let Some(count) = self.base_quality_histogram.get_mut(index) {
                                    *count += 1;
                                }
                            }
                            coverage.depths[reference_index] += 1;
                        }
                    }
                    read_offset += *len as usize;
                    reference_offset += *len as usize;
                }
                Cigar::Ins(len) | Cigar::SoftClip(len) => {
                    read_offset += *len as usize;
                }
                Cigar::Del(len) | Cigar::RefSkip(len) => {
                    reference_offset += *len as usize;
                }
                Cigar::HardClip(_) | Cigar::Pad(_) => {}
            }
        }
        Ok(())
    }

    fn to_picard_text(&self, sample_size: u32, include_bq_histogram: bool) -> String {
        let histogram = self.coverage_histogram();
        let genome_territory = histogram.iter().sum::<u64>();
        let mean_coverage = mean_from_histogram_u32(&histogram);
        let sd_coverage = sample_standard_deviation_from_histogram_u32(&histogram, mean_coverage);
        let median_coverage = median_f64_from_histogram_u64(&histogram);
        let mad_coverage = mad_f64_from_histogram_u64(&histogram, median_coverage);
        let pct_exc_total = ratio(
            self.excluded_mapq
                + self.excluded_duplicate
                + self.excluded_unpaired
                + self.excluded_baseq
                + self.excluded_capped,
            self.total_aligned_bases,
        );
        let het_sensitivity = if sample_size == 1 && genome_territory > 0 {
            format_float(
                histogram.iter().skip(1).sum::<u64>() as f64 / genome_territory as f64 / 2.0,
            )
        } else {
            "0".to_string()
        };
        let het_q = if het_sensitivity == "0" { "0" } else { "3" };

        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.WgsMetrics\n");
        output.push_str("GENOME_TERRITORY\tMEAN_COVERAGE\tSD_COVERAGE\tMEDIAN_COVERAGE\tMAD_COVERAGE\tPCT_EXC_ADAPTER\tPCT_EXC_MAPQ\tPCT_EXC_DUPE\tPCT_EXC_UNPAIRED\tPCT_EXC_BASEQ\tPCT_EXC_OVERLAP\tPCT_EXC_CAPPED\tPCT_EXC_TOTAL\tPCT_1X\tPCT_5X\tPCT_10X\tPCT_15X\tPCT_20X\tPCT_25X\tPCT_30X\tPCT_40X\tPCT_50X\tPCT_60X\tPCT_70X\tPCT_80X\tPCT_90X\tPCT_100X\tFOLD_80_BASE_PENALTY\tFOLD_90_BASE_PENALTY\tFOLD_95_BASE_PENALTY\tHET_SNP_SENSITIVITY\tHET_SNP_Q\n");
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t0\t{}\t{}\t{}\t{}\t0\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n\n",
            genome_territory,
            format_float(mean_coverage),
            format_float(sd_coverage),
            format_float(median_coverage),
            format_float(mad_coverage),
            format_float(ratio(self.excluded_mapq, self.total_aligned_bases)),
            format_float(ratio(self.excluded_duplicate, self.total_aligned_bases)),
            format_float(ratio(self.excluded_unpaired, self.total_aligned_bases)),
            format_float(ratio(self.excluded_baseq, self.total_aligned_bases)),
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

    fn coverage_histogram(&self) -> Vec<u64> {
        let mut histogram = vec![0; self.coverage_cap as usize + 1];
        for contig in self.contigs.values() {
            for (depth, included) in contig.depths.iter().zip(&contig.included) {
                if *included {
                    let index = (*depth).min(self.coverage_cap) as usize;
                    histogram[index] += 1;
                }
            }
        }
        histogram
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
    let mut deviations = Vec::with_capacity(total_count as usize);
    for (depth, count) in histogram.iter().enumerate() {
        deviations.extend(std::iter::repeat_n(
            (depth as f64 - median).abs(),
            *count as usize,
        ));
    }
    deviations.sort_by(|left, right| left.partial_cmp(right).unwrap_or(Ordering::Equal));
    let middle = deviations.len() / 2;
    if deviations.len() % 2 == 1 {
        deviations[middle]
    } else {
        (deviations[middle - 1] + deviations[middle]) / 2.0
    }
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

#[derive(Debug, Default)]
struct QualityScoreDistributionSummary {
    counts: BTreeMap<u8, u64>,
    original_counts: BTreeMap<u8, u64>,
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
            *self.counts.entry(quality).or_default() += 1;
        }
        if let Some(original_qualities) = original_quality_values(record) {
            for (index, quality) in original_qualities.into_iter().enumerate() {
                if !include_no_calls && sequence.get(index).is_some_and(|base| *base == b'N') {
                    continue;
                }
                *self.original_counts.entry(quality).or_default() += 1;
            }
        }
    }

    fn to_picard_text(&self) -> String {
        let mut output = String::new();
        output.push_str("## HISTOGRAM\tjava.lang.Byte\n");
        if self.original_counts.is_empty() {
            output.push_str("QUALITY\tCOUNT_OF_Q\n");
            for (quality, count) in &self.counts {
                output.push_str(&format!("{quality}\t{count}\n"));
            }
        } else {
            output.push_str("QUALITY\tCOUNT_OF_Q\tCOUNT_OF_OQ\n");
            let qualities = self
                .counts
                .keys()
                .chain(self.original_counts.keys())
                .copied()
                .collect::<BTreeSet<_>>();
            for quality in qualities {
                let primary = self.counts.get(&quality).copied().unwrap_or(0);
                let original = self.original_counts.get(&quality).copied().unwrap_or(0);
                output.push_str(&format!("{quality}\t{primary}\t{original}\n"));
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
        let iterator: Box<dyn Iterator<Item = u8>> = if record.is_reverse() {
            Box::new(bases.iter().rev().copied())
        } else {
            Box::new(bases.iter().copied())
        };
        for (index, base) in iterator.enumerate() {
            let cycle = cycle_offset + index;
            if cycles.len() <= cycle {
                cycles.resize(cycle + 1, BaseCycleCounts::default());
            }
            cycles[cycle].observe(base);
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
    contigs: BTreeMap<String, Vec<u8>>,
    total_clusters: u64,
    aligned_reads: u64,
    unique_total_clusters: u64,
    unique_aligned_reads: u64,
    emit_unique: bool,
}

impl Default for GcBiasMetricsSummary {
    fn default() -> Self {
        Self {
            windows: [0; 101],
            read_starts: [0; 101],
            quality_sums: [0; 101],
            quality_counts: [0; 101],
            unique_read_starts: [0; 101],
            unique_quality_sums: [0; 101],
            unique_quality_counts: [0; 101],
            contigs: BTreeMap::new(),
            total_clusters: 0,
            aligned_reads: 0,
            unique_total_clusters: 0,
            unique_aligned_reads: 0,
            emit_unique: false,
        }
    }
}

impl GcBiasMetricsSummary {
    fn new(
        references: &[FastaSequence],
        window_size: usize,
        emit_unique: bool,
    ) -> Result<Self, String> {
        let mut summary = Self::default();
        summary.emit_unique = emit_unique;
        for reference in references {
            summary
                .contigs
                .insert(reference.name.clone(), reference.sequence.clone());
            let window_count = reference.sequence.len().saturating_sub(window_size + 1);
            for start in 0..window_count {
                if let Some(window) = reference.sequence.get(start..start + window_size) {
                    let gc = gc_percent(window, window_size);
                    summary.windows[gc] += 1;
                }
            }
        }
        Ok(summary)
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
        let Some(reference) = self.contigs.get(contig) else {
            return Err(format!(
                "CollectGcBiasMetrics reference missing contig {contig}"
            ));
        };
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
            self.aligned_reads,
            minimum_genome_fraction,
        );
        if self.emit_unique {
            self.push_detail_rows(
                &mut output,
                "UNIQUE",
                &self.unique_read_starts,
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
            output.push_str(&format!(
                "All Reads\t{reads_used}\t{gc}\t{windows}\t{read_starts}\t0\t{}\t{}\t\t\t\n",
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
        for (cycle, quality) in qualities.iter().copied().enumerate() {
            let cycle = if record.is_reverse() {
                qualities.len() - cycle - 1
            } else {
                cycle
            };
            if cycles.len() <= cycle {
                cycles.resize(cycle + 1, CycleQuality::default());
            }
            cycles[cycle].quality_sum += quality as u64;
            cycles[cycle].count += 1;
        }
        if let Some(original_qualities) = original_quality_values(record) {
            self.original_records += 1;
            let cycles = if record.is_paired() && record.is_last_in_template() {
                &mut self.original_second
            } else {
                &mut self.original_first
            };
            for (cycle, quality) in original_qualities.iter().copied().enumerate() {
                let cycle = if record.is_reverse() {
                    original_qualities.len() - cycle - 1
                } else {
                    cycle
                };
                if cycles.len() <= cycle {
                    cycles.resize(cycle + 1, CycleQuality::default());
                }
                cycles[cycle].quality_sum += quality as u64;
                cycles[cycle].count += 1;
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

#[derive(Debug, Default)]
struct InsertSizeSummary {
    histogram: BTreeMap<u64, u64>,
}

impl InsertSizeSummary {
    fn observe(&mut self, record: &bam::Record, include_duplicates: bool) {
        if !record.is_paired()
            || record.is_unmapped()
            || record.is_mate_unmapped()
            || record.is_secondary()
            || record.is_supplementary()
            || (record.is_duplicate() && !include_duplicates)
            || record.insert_size() == 0
            || !record.is_first_in_template()
        {
            return;
        }
        *self
            .histogram
            .entry(record.insert_size().unsigned_abs())
            .or_default() += 1;
    }

    fn observe_sam_parts(&mut self, flags: u16, insert_size: i64, include_duplicates: bool) {
        if flags & 0x1 == 0
            || flags & 0x4 != 0
            || flags & 0x8 != 0
            || flags & 0x100 != 0
            || flags & 0x800 != 0
            || (flags & 0x400 != 0 && !include_duplicates)
            || flags & 0x40 == 0
            || insert_size == 0
        {
            return;
        }
        *self
            .histogram
            .entry(insert_size.unsigned_abs())
            .or_default() += 1;
    }

    fn to_picard_text(&self) -> String {
        let read_pairs = histogram_total_count(&self.histogram);
        let median = histogram_median_f64(&self.histogram);
        let mad = histogram_median_absolute_deviation(&self.histogram, median);
        let min = self.histogram.keys().next().copied().unwrap_or(0);
        let max = self.histogram.keys().next_back().copied().unwrap_or(0);
        let mean = histogram_mean(&self.histogram);
        let stddev = histogram_sample_standard_deviation(&self.histogram, mean);
        let mode = mode_from_histogram(&self.histogram);
        let widths = insert_size_widths(&self.histogram);

        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.InsertSizeMetrics\n");
        output.push_str("MEDIAN_INSERT_SIZE\tMODE_INSERT_SIZE\tMEDIAN_ABSOLUTE_DEVIATION\tMIN_INSERT_SIZE\tMAX_INSERT_SIZE\tMEAN_INSERT_SIZE\tSTANDARD_DEVIATION\tREAD_PAIRS\tPAIR_ORIENTATION\tWIDTH_OF_10_PERCENT\tWIDTH_OF_20_PERCENT\tWIDTH_OF_30_PERCENT\tWIDTH_OF_40_PERCENT\tWIDTH_OF_50_PERCENT\tWIDTH_OF_60_PERCENT\tWIDTH_OF_70_PERCENT\tWIDTH_OF_80_PERCENT\tWIDTH_OF_90_PERCENT\tWIDTH_OF_95_PERCENT\tWIDTH_OF_99_PERCENT\tSAMPLE\tLIBRARY\tREAD_GROUP\n");
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\tFR\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t\t\t\n\n",
            format_float(median),
            mode,
            format_float(mad),
            min,
            max,
            format_float(mean),
            format_float(stddev),
            read_pairs,
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
        ));
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        output.push_str("insert_size\tAll_Reads.fr_count\n");
        for (insert_size, count) in &self.histogram {
            output.push_str(&format!("{insert_size}\t{count}\n"));
        }
        output
    }
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
    [
        10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 95.0, 99.0,
    ]
    .map(|central_percent| insert_size_width(histogram, central_percent))
}

fn insert_size_width(histogram: &BTreeMap<u64, u64>, central_percent: f64) -> u64 {
    if histogram.is_empty() {
        return 0;
    }
    let tail_percent = (100.0 - central_percent) / 2.0;
    let low = histogram_nearest_rank_percentile(histogram, tail_percent);
    let high = histogram_nearest_rank_percentile(histogram, 100.0 - tail_percent);
    high.saturating_sub(low) + 1
}

fn histogram_nearest_rank_percentile(histogram: &BTreeMap<u64, u64>, percentile: f64) -> u64 {
    let total_count = histogram_total_count(histogram);
    if total_count == 0 {
        return 0;
    }
    let rank = ((percentile / 100.0) * total_count as f64).ceil() as u64;
    let index = rank.saturating_sub(1).min(total_count - 1);
    histogram_value_at_zero_based_rank(histogram, index)
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
    name_buf: String,
    sequence_buf: String,
    plus_buf: String,
    qualities_buf: String,
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
        quality_offset: u8,
    ) -> Result<(), String> {
        match self {
            Self::Sam(writer) => {
                write_fastq_sam_record(writer, read, flags, read_group_id, quality_offset)
            }
            Self::Bam(writer) => {
                let record = fastq_bam_record(read, flags, read_group_id, quality_offset)?;
                writer.write(&record).map_err(|error| error.to_string())
            }
        }
    }
}

fn run_fastqtosam_standard_sam(
    fastq: &str,
    fastq2: Option<&str>,
    output: &str,
    read_group: &FastqReadGroup,
) -> Result<(), String> {
    let mut writer = BufWriter::with_capacity(
        1024 * 1024,
        fs::File::create(output).map_err(|error| error.to_string())?,
    );
    writer
        .write_all(fastqtosam_header_text(read_group).as_bytes())
        .map_err(|error| error.to_string())?;
    let mut first_reader = FastqBytesReader::from_path(fastq)?;
    let mut second_reader = match fastq2 {
        Some(path) => Some(FastqBytesReader::from_path(path)?),
        None => None,
    };
    let mut first = FastqBytesRecord::default();
    let mut second = FastqBytesRecord::default();
    let mut output_buffer = Vec::with_capacity(8 * 1024 * 1024);
    loop {
        if !first_reader.next_record_into(&mut first)? {
            if let Some(reader) = second_reader.as_mut() {
                if reader.next_record_into(&mut second)? {
                    return Err(
                        "malformed FastqToSam FASTQ2 has more records than FASTQ".to_string()
                    );
                }
            }
            break;
        }
        if let Some(reader) = second_reader.as_mut() {
            if !reader.next_record_into(&mut second)? {
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
            flush_large_fastqtosam_buffer(&mut writer, &mut output_buffer)?;
        } else {
            append_fastq_sam_bytes_record(&mut output_buffer, &first, 4, &read_group.id);
            flush_large_fastqtosam_buffer(&mut writer, &mut output_buffer)?;
        }
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
    fn from_path(path: &str) -> Result<Self, String> {
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
            name_buf: Vec::new(),
            plus_buf: Vec::new(),
        })
    }

    fn next_record_into(&mut self, record: &mut FastqBytesRecord) -> Result<bool, String> {
        self.name_buf.clear();
        if self
            .reader
            .read_until(b'\n', &mut self.name_buf)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(false);
        }
        record.sequence.clear();
        self.plus_buf.clear();
        record.qualities.clear();
        self.reader
            .read_until(b'\n', &mut record.sequence)
            .map_err(|error| error.to_string())?;
        self.reader
            .read_until(b'\n', &mut self.plus_buf)
            .map_err(|error| error.to_string())?;
        self.reader
            .read_until(b'\n', &mut record.qualities)
            .map_err(|error| error.to_string())?;
        trim_ascii_line_end_bytes(&mut self.name_buf);
        trim_ascii_line_end_bytes(&mut record.sequence);
        trim_ascii_line_end_bytes(&mut self.plus_buf);
        trim_ascii_line_end_bytes(&mut record.qualities);
        if !self.name_buf.starts_with(b"@") || !self.plus_buf.starts_with(b"+") {
            return Err("malformed FastqToSam FASTQ record".to_string());
        }
        if record.sequence.len() != record.qualities.len() {
            return Err("malformed FastqToSam FASTQ sequence/quality length mismatch".to_string());
        }
        record.name.clear();
        push_normalized_fastq_read_name_bytes(&self.name_buf[1..], &mut record.name);
        Ok(true)
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
    fn from_path(path: &str) -> Result<Self, String> {
        let file = fs::File::open(path).map_err(|error| error.to_string())?;
        let reader: Box<dyn BufRead> = if has_gzip_extension(path) {
            Box::new(BufReader::with_capacity(1024 * 1024, GzDecoder::new(file)))
        } else {
            Box::new(BufReader::with_capacity(1024 * 1024, file))
        };
        Ok(Self {
            reader,
            name_buf: String::new(),
            sequence_buf: String::new(),
            plus_buf: String::new(),
            qualities_buf: String::new(),
        })
    }

    fn next_record_into(&mut self, record: &mut FastqRecord) -> Result<bool, String> {
        self.name_buf.clear();
        if self
            .reader
            .read_line(&mut self.name_buf)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(false);
        }
        self.sequence_buf.clear();
        self.plus_buf.clear();
        self.qualities_buf.clear();
        self.reader
            .read_line(&mut self.sequence_buf)
            .map_err(|error| error.to_string())?;
        self.reader
            .read_line(&mut self.plus_buf)
            .map_err(|error| error.to_string())?;
        self.reader
            .read_line(&mut self.qualities_buf)
            .map_err(|error| error.to_string())?;
        let name = self.name_buf.trim_end_matches(['\r', '\n']);
        if !name.starts_with('@') || !self.plus_buf.starts_with('+') {
            return Err("malformed FastqToSam FASTQ record".to_string());
        }
        let sequence = self.sequence_buf.trim_end_matches(['\r', '\n']).as_bytes();
        let qualities = self.qualities_buf.trim_end_matches(['\r', '\n']).as_bytes();
        if sequence.len() != qualities.len() {
            return Err("malformed FastqToSam FASTQ sequence/quality length mismatch".to_string());
        }
        record.name.clear();
        push_normalized_fastq_read_name(&name[1..], &mut record.name);
        record.sequence.clear();
        record.sequence.extend_from_slice(sequence);
        record.qualities.clear();
        record.qualities.extend_from_slice(qualities);
        Ok(true)
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

fn write_fastq_sam_record(
    writer: &mut dyn Write,
    read: &FastqRecord,
    flags: u16,
    read_group_id: &str,
    quality_offset: u8,
) -> Result<(), String> {
    let converted_qualities;
    let qualities = if quality_offset == 33 {
        read.qualities.as_slice()
    } else {
        converted_qualities = read
            .qualities
            .iter()
            .map(|quality| {
                quality
                    .checked_sub(quality_offset)
                    .and_then(|quality| quality.checked_add(33))
                    .ok_or_else(|| "malformed FastqToSam quality below encoding offset".to_string())
            })
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
    quality_offset: u8,
) -> Result<bam::Record, String> {
    let qualities = read
        .qualities
        .iter()
        .map(|quality| {
            quality
                .checked_sub(quality_offset)
                .ok_or_else(|| "malformed FastqToSam quality below encoding offset".to_string())
        })
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
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported AddOrReplaceReadGroups COMPRESSION_LEVEL: {level}"
            ));
        }
    }
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
        "INCLUDE_NON_PF_READS",
        "INCLUDE_NON_PRIMARY_ALIGNMENTS",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "CREATE_MD5_FILE",
    ];

    for key in args.keys() {
        if !supported.contains(&key.as_str()) {
            return Err(format!("unsupported SamToFastq argument: {key}"));
        }
    }

    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "INTERLEAVE")?;
    optional_bool(args, "RE_REVERSE")?;
    optional_bool(args, "INCLUDE_NON_PF_READS")?;
    optional_bool(args, "INCLUDE_NON_PRIMARY_ALIGNMENTS")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
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
        "SORT_ORDER",
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
        "CREATE_MD5_FILE",
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
    optional_scalar(args, "VALIDATION_STRINGENCY")?;
    optional_scalar(args, "VERBOSITY")?;
    optional_bool(args, "QUIET")?;
    optional_bool(args, "CREATE_MD5_FILE")?;
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
    fastq: &str,
    second_end_fastq: Option<&str>,
    unpaired_fastq: Option<&str>,
    interleave: bool,
    re_reverse: bool,
    include_non_pf_reads: bool,
    include_non_primary_alignments: bool,
    compression_level: u32,
    create_md5_file: bool,
) -> Result<(), String> {
    let file = fs::File::open(input).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut first_writer = fastq_writer(fastq, compression_level)?;
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

    loop {
        line.clear();
        if reader
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            break;
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
        if is_paired && !interleave && second_writer.is_none() {
            return Err(
                "SamToFastq input contains paired reads but no SECOND_END_FASTQ was specified"
                    .to_string(),
            );
        }

        sequence.clear();
        sequence.extend_from_slice(sam_sequence.as_bytes());
        qualities.clear();
        qualities.extend_from_slice(sam_qualities.as_bytes());
        if re_reverse && flags & 0x10 != 0 {
            reverse_complement(&mut sequence);
            qualities.reverse();
        }
        output.clear();
        append_fastq_text_record(
            &mut output,
            name.as_bytes(),
            fastq_name_suffix_from_flags(flags),
            &sequence,
            &qualities,
        );

        if is_paired && flags & 0x80 != 0 && !interleave {
            second_writer
                .as_mut()
                .expect("second writer exists for paired output")
                .write_all(&output)
                .map_err(|error| error.to_string())?;
        } else if !is_paired {
            match unpaired_writer.as_mut() {
                Some(writer) => writer
                    .write_all(&output)
                    .map_err(|error| error.to_string())?,
                None => first_writer
                    .write_all(&output)
                    .map_err(|error| error.to_string())?,
            }
        } else {
            first_writer
                .write_all(&output)
                .map_err(|error| error.to_string())?;
        }
    }

    first_writer.flush().map_err(|error| error.to_string())?;
    if let Some(writer) = second_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    if let Some(writer) = unpaired_writer.as_mut() {
        writer.flush().map_err(|error| error.to_string())?;
    }
    drop(first_writer);
    drop(second_writer);
    drop(unpaired_writer);
    write_samtofastq_sidecars(fastq, second_end_fastq, unpaired_fastq, create_md5_file)
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
    re_reverse: bool,
    name_suffix: Option<&'static str>,
) -> Result<(), String> {
    let name = String::from_utf8_lossy(record.qname());
    let mut sequence = record.seq().as_bytes();
    let mut qualities = record
        .qual()
        .iter()
        .map(|quality| quality.saturating_add(33))
        .collect::<Vec<_>>();

    if re_reverse && record.is_reverse() {
        reverse_complement(&mut sequence);
        qualities.reverse();
    }

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
    if optional_bool(args, "MERGE_SEQUENCE_DICTIONARIES")?.unwrap_or(false) {
        return Err("unsupported MergeSamFiles MERGE_SEQUENCE_DICTIONARIES=true".to_string());
    }
    optional_scalar(args, "TMP_DIR")?;
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
    output_format_for(output, "SortSam")
}

fn output_format_for(output: &str, command: &str) -> Result<bam::Format, String> {
    match Path::new(output)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("sam") => Ok(bam::Format::Sam),
        Some("bam") => Ok(bam::Format::Bam),
        _ => Err(format!(
            "unsupported {command} output format for {output}; expected .sam or .bam"
        )),
    }
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

fn bam_writer_for_path(
    output: &str,
    header: &bam::Header,
    format: bam::Format,
    compression_level: Option<u32>,
) -> Result<bam::Writer, String> {
    let mut writer =
        bam::Writer::from_path(output, header, format).map_err(|error| error.to_string())?;
    if let Some(level) = compression_level {
        writer
            .set_compression_level(bam::CompressionLevel::Level(level))
            .map_err(|error| error.to_string())?;
    }
    Ok(writer)
}

fn revert_record(
    record: &mut bam::Record,
    restore_original_qualities: bool,
    remove_alignment_information: bool,
    remove_duplicate_information: bool,
    restore_hardclips: bool,
    attributes_to_clear: &[[u8; 2]],
) -> Result<(), String> {
    if !remove_alignment_information {
        return Err("unsupported RevertSam REMOVE_ALIGNMENT_INFORMATION=false".to_string());
    }
    if record.is_secondary() || record.is_supplementary() {
        return Err("unsupported RevertSam secondary or supplementary alignment".to_string());
    }
    if restore_hardclips && (record.aux(b"XB").is_ok() || record.aux(b"XQ").is_ok()) {
        return Err("unsupported RevertSam RESTORE_HARDCLIPS with XB/XQ tags".to_string());
    }

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
            record.set(&qname, None, &sequence, &restored);
        }
        remove_aux_if_present(record, b"OQ")?;
    }

    if remove_alignment_information {
        let qname = record.qname().to_vec();
        let sequence = record.seq().as_bytes();
        let qualities = record.qual().to_vec();
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
    }
    for tag in attributes_to_clear {
        remove_aux_if_present(record, tag)?;
    }
    Ok(())
}

fn remove_aux_if_present(record: &mut bam::Record, tag: &[u8]) -> Result<(), String> {
    if record.aux(tag).is_ok() {
        record.remove_aux(tag).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn set_nm_md_uq_tags(
    record: &mut bam::Record,
    target_names: &[String],
    reference: &BTreeMap<String, Vec<u8>>,
    set_only_uq: bool,
) -> Result<(), String> {
    if record.is_unmapped() || record.is_secondary() || record.is_supplementary() {
        return Ok(());
    }
    if record.tid() < 0 || record.pos() < 0 {
        return Ok(());
    }
    let contig = target_names
        .get(record.tid() as usize)
        .ok_or_else(|| "SetNmMdAndUqTags record target missing from header".to_string())?;
    let reference = reference
        .get(contig)
        .ok_or_else(|| format!("SetNmMdAndUqTags reference missing contig {contig}"))?;
    let read_bases = record.seq().as_bytes();
    let qualities = record.qual().to_vec();
    let mut read_offset = 0usize;
    let mut ref_offset = record.pos() as usize;
    let mut nm = 0i32;
    let mut uq = 0i32;
    let mut md = String::new();
    let mut matches = 0usize;

    for cigar in &record.cigar() {
        match *cigar {
            Cigar::Match(length) | Cigar::Equal(length) | Cigar::Diff(length) => {
                for _ in 0..length {
                    let read_base = *read_bases.get(read_offset).ok_or_else(|| {
                        "SetNmMdAndUqTags read sequence shorter than CIGAR".to_string()
                    })?;
                    let ref_base = *reference.get(ref_offset).ok_or_else(|| {
                        "SetNmMdAndUqTags alignment extends beyond reference".to_string()
                    })?;
                    if read_base.eq_ignore_ascii_case(&ref_base) {
                        matches += 1;
                    } else {
                        md.push_str(&matches.to_string());
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
                md.push_str(&matches.to_string());
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
    md.push_str(&matches.to_string());

    if !set_only_uq {
        replace_aux_string(record, b"MD", &md)?;
        replace_aux_i32(record, b"NM", nm)?;
    }
    replace_aux_i32(record, b"UQ", uq)?;
    Ok(())
}

fn write_fixed_mate_group(
    writer: &mut bam::Writer,
    records: &mut Vec<bam::Record>,
    add_mate_cigar: bool,
    ignore_missing_mates: bool,
) -> Result<(), String> {
    match records.len() {
        0 => return Ok(()),
        1 => {
            if !ignore_missing_mates && records[0].is_paired() {
                let name = String::from_utf8_lossy(records[0].qname());
                return Err(format!("Missing second read of pair: {name}"));
            }
            writer
                .write(&records[0])
                .map_err(|error| error.to_string())?;
            records.clear();
            return Ok(());
        }
        2 => {}
        _ => {
            return Err(
                "unsupported FixMateInformation read group with more than two records".to_string(),
            );
        }
    }

    if records.iter().any(|record| record.is_secondary()) {
        for record in records.drain(..) {
            writer.write(&record).map_err(|error| error.to_string())?;
        }
        return Ok(());
    }
    if records.iter().any(|record| record.is_supplementary()) {
        return Err("unsupported FixMateInformation supplementary alignments".to_string());
    }
    if !records[0].is_paired() || !records[1].is_paired() {
        for record in records.drain(..) {
            writer.write(&record).map_err(|error| error.to_string())?;
        }
        return Ok(());
    }

    let mut first = records.remove(0);
    let mut second = records.remove(0);
    fix_mate_pair(&mut first, &mut second, add_mate_cigar)?;
    writer.write(&first).map_err(|error| error.to_string())?;
    writer.write(&second).map_err(|error| error.to_string())?;
    Ok(())
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
        .push_aux(tag, Aux::I32(value))
        .map_err(|error| error.to_string())
}

fn replace_aux_string(record: &mut bam::Record, tag: &[u8], value: &str) -> Result<(), String> {
    if record.aux(tag).is_ok() {
        record.remove_aux(tag).map_err(|error| error.to_string())?;
    }
    record
        .push_aux(tag, Aux::String(value))
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

fn reverted_header(source: &bam::HeaderView, remove_alignment_information: bool) -> bam::Header {
    let header_text = String::from_utf8_lossy(source.as_bytes());
    let mut header = bam::Header::new();
    let mut saw_hd = false;

    for line in header_text.lines() {
        if line.is_empty() || line.starts_with("@PG\t") {
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
                record.push_tag(b"SO", "queryname");
                saw_so = true;
            } else {
                record.push_tag(tag.as_bytes(), value);
            }
        }
        if is_hd && !saw_so {
            record.push_tag(b"SO", "queryname");
        }
        header.push_record(&record);
    }
    if !saw_hd {
        header.push_record(
            HeaderRecord::new(b"HD")
                .push_tag(b"VN", "1.6")
                .push_tag(b"SO", "queryname"),
        );
    }
    header
}

struct MergePlan {
    header_builder: MergeHeaderBuilder,
    inputs: Vec<MergeInputPlan>,
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
) -> Result<MergePlan, String> {
    let first_reader = bam::Reader::from_path(&inputs[0]).map_err(|error| error.to_string())?;
    let first_header_text = String::from_utf8_lossy(first_reader.header().as_bytes()).into_owned();
    let first_sequence_dictionary = sequence_dictionary_lines(&first_header_text);
    let mut header_builder = MergeHeaderBuilder::new(&first_header_text, sort_order)?;
    drop(first_reader);

    let mut input_plans = Vec::with_capacity(inputs.len());
    for input in inputs {
        let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
        let header_text = String::from_utf8_lossy(reader.header().as_bytes()).into_owned();
        if sequence_dictionary_lines(&header_text) != first_sequence_dictionary {
            return Err(
                "unsupported MergeSamFiles input with different sequence dictionary".to_string(),
            );
        }
        let read_group_renames = header_builder.observe_input_header(&header_text)?;
        let is_sorted = assume_sorted || input_reader_is_sorted(&mut reader, sort_order)?;
        input_plans.push(MergeInputPlan {
            path: input.clone(),
            read_group_renames,
            is_sorted,
        });
    }

    Ok(MergePlan {
        header_builder,
        inputs: input_plans,
    })
}

fn collect_merge_records(input_plans: &[MergeInputPlan]) -> Result<Vec<bam::Record>, String> {
    let mut records = Vec::new();
    for input in input_plans {
        let mut reader = bam::Reader::from_path(&input.path).map_err(|error| error.to_string())?;
        for record in reader.records() {
            let mut record = record.map_err(|error| error.to_string())?;
            rewrite_record_read_group(&mut record, &input.read_group_renames)?;
            records.push(record);
        }
    }
    Ok(records)
}

fn input_is_sorted(path: &str, sort_order: SortOrder) -> Result<bool, String> {
    let mut reader = bam::Reader::from_path(path).map_err(|error| error.to_string())?;
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
) -> Result<(), String> {
    let mut readers = input_plans
        .iter()
        .map(|input| bam::Reader::from_path(&input.path).map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut heap = BinaryHeap::new();
    let mut serial = 0u64;

    for input_index in 0..readers.len() {
        if let Some(record) = read_next_merge_record(
            &mut readers[input_index],
            &input_plans[input_index].read_group_renames,
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
) -> Result<Option<bam::Record>, String> {
    let mut record = bam::Record::new();
    match reader.read(&mut record) {
        Some(Ok(())) => {
            rewrite_record_read_group(&mut record, read_group_renames)?;
            Ok(Some(record))
        }
        Some(Err(error)) => Err(error.to_string()),
        None => Ok(None),
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
        index::build(output, Some(&picard_bai_path(output)), index::Type::Bai, 1)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_md5_sidecar(output: &str) -> Result<(), String> {
    let bytes = fs::read(output).map_err(|error| error.to_string())?;
    let digest = md5::compute(bytes);
    fs::write(format!("{output}.md5"), format!("{digest:x}")).map_err(|error| error.to_string())
}

fn try_run_fallback(args: &[String]) -> Option<i32> {
    let fallback_command = std::env::var("TURBO_PICARD_FALLBACK_COMMAND")
        .ok()
        .filter(|command| !command.trim().is_empty())?;

    match fallback_status(&fallback_command, args) {
        Ok(exit_code) => Some(exit_code),
        Err(error) => {
            eprintln!("{error}");
            Some(2)
        }
    }
}

fn try_run_fallback_for_native_error(error: &str, args: &[String]) -> Option<i32> {
    if is_unsupported_native_surface(error) {
        try_run_fallback(args)
    } else {
        None
    }
}

fn is_unsupported_native_surface(error: &str) -> bool {
    error.starts_with("unsupported ")
}

fn fallback_status(fallback_command: &str, args: &[String]) -> Result<i32, String> {
    let mut command = if cfg!(windows) {
        let mut command = Command::new("cmd");
        command.arg("/C").arg(format!("{fallback_command} %*"));
        command
    } else {
        let mut command = Command::new("sh");
        command
            .arg("-c")
            .arg(format!("exec {fallback_command} \"$@\""))
            .arg("turbo-picard-fallback");
        command
    };

    let status = command
        .args(args)
        .env_remove("TURBO_PICARD_FALLBACK_COMMAND")
        .status()
        .map_err(|error| format!("failed to run Picard fallback command: {error}"))?;

    Ok(status.code().unwrap_or(1))
}
