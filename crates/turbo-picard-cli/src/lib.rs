#![forbid(unsafe_code)]

use rust_htslib::bam::header::HeaderRecord;
use rust_htslib::bam::index;
use rust_htslib::bam::{self, Read};
use std::cmp::Ordering;
use std::fs;
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
  MarkDuplicates    Identifies duplicate reads in SAM or BAM files
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
    let values = args
        .get(key)
        .ok_or_else(|| format!("missing required SortSam argument: {key}"))?;
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
