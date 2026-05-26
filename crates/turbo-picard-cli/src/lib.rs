#![forbid(unsafe_code)]

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
  MarkDuplicates    Identifies duplicate reads in SAM or BAM files"
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

fn run_markduplicates(args: &[String]) -> Result<(), String> {
    let picard_args = normalize_picard_args(args).map_err(|error| error.to_string())?;
    let config =
        MarkDuplicatesConfig::try_from_args(&picard_args).map_err(|error| error.to_string())?;

    turbo_picard_markdup::run(&config).map_err(|error| error.to_string())?;
    Ok(())
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
