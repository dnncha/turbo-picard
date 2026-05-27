#![forbid(unsafe_code)]

use flate2::Compression;
use flate2::write::GzEncoder;
use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::index;
use rust_htslib::bam::record::{Aux, Cigar};
use rust_htslib::bam::{self, Read};
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::process::Command;
use turbo_picard_core::markdup_config::MarkDuplicatesConfig;
use turbo_picard_core::picard_args::normalize_picard_args;

pub fn run_cli(program_name: &str, raw_args: impl IntoIterator<Item = String>) -> i32 {
    let raw_args = raw_args.into_iter().collect::<Vec<_>>();
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
                if let Some(exit_code) = try_run_fallback(&raw_args) {
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
                if let Some(exit_code) = try_run_fallback(&raw_args) {
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
                if let Some(exit_code) = try_run_fallback(&raw_args) {
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
                if let Some(exit_code) = try_run_fallback(&raw_args) {
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
                if let Some(exit_code) = try_run_fallback(&raw_args) {
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

fn print_top_level_help(program_name: &str) {
    println!(
        "\
Usage: {program_name} <PicardCommand> [KEY=VALUE ...]

Available commands:
  AddOrReplaceReadGroups
                    Adds or replaces a single read group in SAM or BAM files
  CollectAlignmentSummaryMetrics
                    Writes basic alignment summary metrics for SAM or BAM files
  MarkDuplicates    Identifies duplicate reads in SAM or BAM files
  SamToFastq        Converts SAM or BAM records to FASTQ
  SortSam           Sorts SAM or BAM files by coordinate or query name"
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
  RE_REVERSE            Reverse-complement reverse-strand reads"
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

fn run_markduplicates(args: &[String]) -> Result<(), String> {
    let picard_args = normalize_picard_args(args).map_err(|error| error.to_string())?;
    let config =
        MarkDuplicatesConfig::try_from_args(&picard_args).map_err(|error| error.to_string())?;

    turbo_picard_markdup::run(&config).map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortOrder {
    Coordinate,
    QueryName,
}

fn run_sortsam(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args(args).map_err(|error| error.to_string())?;
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

    let mut reader = bam::Reader::from_path(&input).map_err(|error| error.to_string())?;
    let header = sorted_header(reader.header(), sort_order);
    let format = output_format(&output)?;
    if create_index && format != bam::Format::Bam {
        return Err("SortSam CREATE_INDEX=true requires BAM output".to_string());
    }
    let mut records = reader
        .records()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;

    match sort_order {
        SortOrder::Coordinate => records.sort_by(compare_coordinate),
        SortOrder::QueryName => records.sort_by(compare_queryname),
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

    if create_md5_file {
        write_md5_sidecar(&output)?;
    }
    if create_index {
        index::build(
            &output,
            Some(&picard_bai_path(&output)),
            index::Type::Bai,
            1,
        )
        .map_err(|error| error.to_string())?;
    }

    Ok(())
}

fn run_samtofastq(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args(args).map_err(|error| error.to_string())?;
    reject_unsupported_samtofastq_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "SamToFastq")?;
    let fastq = required_scalar_for(&args, "FASTQ", "SamToFastq")?;
    let second_end_fastq = optional_scalar(&args, "SECOND_END_FASTQ")?;
    let unpaired_fastq = optional_scalar(&args, "UNPAIRED_FASTQ")?;
    let interleave = optional_bool(&args, "INTERLEAVE")?.unwrap_or(false);
    let re_reverse = optional_bool(&args, "RE_REVERSE")?.unwrap_or(true);
    let compression_level = optional_u32(&args, "COMPRESSION_LEVEL")?.unwrap_or(5);

    if interleave && second_end_fastq.is_some() {
        return Err("SamToFastq INTERLEAVE=true cannot be used with SECOND_END_FASTQ".to_string());
    }

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let mut first_writer = fastq_writer(&fastq, compression_level)?;
    let mut second_writer = match second_end_fastq {
        Some(path) => Some(fastq_writer(&path, compression_level)?),
        None => None,
    };
    let mut unpaired_writer = match unpaired_fastq {
        Some(path) => Some(fastq_writer(&path, compression_level)?),
        None => None,
    };

    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
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

    Ok(())
}

fn run_addorreplacereadgroups(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args(args).map_err(|error| error.to_string())?;
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

fn run_collectalignmentsummarymetrics(args: &[String]) -> Result<(), String> {
    let args = normalize_picard_args(args).map_err(|error| error.to_string())?;
    reject_unsupported_collectalignment_args(&args)?;
    let input = required_scalar_for(&args, "INPUT", "CollectAlignmentSummaryMetrics")?;
    let output = required_scalar_for(&args, "OUTPUT", "CollectAlignmentSummaryMetrics")?;

    let mut reader = bam::Reader::from_path(input).map_err(|error| error.to_string())?;
    let mut metrics = AlignmentSummary::default();
    for record in reader.records() {
        let record = record.map_err(|error| error.to_string())?;
        metrics.observe(&record);
    }

    fs::write(output, metrics.to_picard_text()).map_err(|error| error.to_string())
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
    if optional_u32(args, "STOP_AFTER")?.unwrap_or(0) != 0 {
        return Err("unsupported CollectAlignmentSummaryMetrics STOP_AFTER".to_string());
    }
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!(
                "unsupported CollectAlignmentSummaryMetrics COMPRESSION_LEVEL: {level}"
            ));
        }
    }
    Ok(())
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

    fn to_picard_text(&self) -> String {
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

        let mut output = String::new();
        output.push_str("## METRICS CLASS\tpicard.analysis.AlignmentSummaryMetrics\n");
        output.push_str("CATEGORY\tTOTAL_READS\tPF_READS\tPCT_PF_READS\tPF_NOISE_READS\tPF_READS_ALIGNED\tPCT_PF_READS_ALIGNED\tPF_ALIGNED_BASES\tPF_HQ_ALIGNED_READS\tPF_HQ_ALIGNED_BASES\tPF_HQ_ALIGNED_Q20_BASES\tPF_HQ_MEDIAN_MISMATCHES\tPF_MISMATCH_RATE\tPF_HQ_ERROR_RATE\tPF_INDEL_RATE\tMEAN_READ_LENGTH\tSD_READ_LENGTH\tMEDIAN_READ_LENGTH\tMAD_READ_LENGTH\tMIN_READ_LENGTH\tMAX_READ_LENGTH\tMEAN_ALIGNED_READ_LENGTH\tREADS_ALIGNED_IN_PAIRS\tPCT_READS_ALIGNED_IN_PAIRS\tPF_READS_IMPROPER_PAIRS\tPCT_PF_READS_IMPROPER_PAIRS\tBAD_CYCLES\tSTRAND_BALANCE\tPCT_CHIMERAS\tPCT_ADAPTER\tPCT_SOFTCLIP\tPCT_HARDCLIP\tAVG_POS_3PRIME_SOFTCLIP_LENGTH\tSAMPLE\tLIBRARY\tREAD_GROUP\n");
        output.push_str(&format!(
            "UNPAIRED\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t0\t0\t0\t0\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t0\t0\t{}\t{}\t0\t\t\t\n\n",
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
        ));
        output.push_str("## HISTOGRAM\tjava.lang.Integer\n");
        output
            .push_str("READ_LENGTH\tUNPAIRED_TOTAL_LENGTH_COUNT\tUNPAIRED_ALIGNED_LENGTH_COUNT\n");
        let max_len = self
            .total_read_lengths
            .len()
            .max(self.aligned_read_lengths.len());
        for index in 0..max_len {
            let total = self.total_read_lengths.get(index).copied().unwrap_or(0);
            let aligned = self.aligned_read_lengths.get(index).copied().unwrap_or(0);
            if total != 0 || aligned != 0 {
                output.push_str(&format!("{index}\t{total}\t{aligned}\n"));
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
        "VALIDATION_STRINGENCY",
        "QUIET",
        "VERBOSITY",
        "COMPRESSION_LEVEL",
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
    if let Some(level) = optional_u32(args, "COMPRESSION_LEVEL")? {
        if level > 9 {
            return Err(format!("unsupported SamToFastq COMPRESSION_LEVEL: {level}"));
        }
    }
    Ok(())
}

fn fastq_writer(path: &str, compression_level: u32) -> Result<Box<dyn Write>, String> {
    let file = fs::File::create(path).map_err(|error| error.to_string())?;
    let writer = BufWriter::new(file);
    if has_gzip_extension(path) {
        Ok(Box::new(GzEncoder::new(
            writer,
            Compression::new(compression_level),
        )))
    } else {
        Ok(Box::new(writer))
    }
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

fn has_gzip_extension(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| matches!(extension.to_ascii_lowercase().as_str(), "gz" | "gzip"))
        .unwrap_or(false)
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

fn scalar_value(values: &[String], key: &str) -> Result<String, String> {
    if values.len() != 1 {
        return Err(format!("duplicate scalar SortSam argument: {key}"));
    }
    Ok(values[0].clone())
}

fn output_format(output: &str) -> Result<bam::Format, String> {
    match Path::new(output)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("sam") => Ok(bam::Format::Sam),
        Some("bam") => Ok(bam::Format::Bam),
        _ => Err(format!(
            "unsupported SortSam output format for {output}; expected .sam or .bam"
        )),
    }
}

fn sorted_header(source: &bam::HeaderView, sort_order: SortOrder) -> bam::Header {
    let sort_order = match sort_order {
        SortOrder::Coordinate => "coordinate",
        SortOrder::QueryName => "queryname",
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
