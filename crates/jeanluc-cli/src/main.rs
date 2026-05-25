#![forbid(unsafe_code)]

use jeanluc_core::markdup_config::MarkDuplicatesConfig;
use jeanluc_core::picard_args::normalize_picard_args;

fn main() {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("MarkDuplicates") => {
            let command_args = args.collect::<Vec<_>>();
            if let Err(error) = run_markduplicates(&command_args) {
                eprintln!("{error}");
                std::process::exit(2);
            }
        }
        Some(command) => {
            eprintln!("unsupported Picard command: {command}");
            std::process::exit(2);
        }
        None => {
            eprintln!("usage: jeanluc <PicardCommand> [KEY=VALUE ...]");
            std::process::exit(2);
        }
    }
}

fn run_markduplicates(args: &[String]) -> Result<(), String> {
    let picard_args = normalize_picard_args(args).map_err(|error| error.to_string())?;
    let config =
        MarkDuplicatesConfig::try_from_args(&picard_args).map_err(|error| error.to_string())?;

    jeanluc_markdup::run(&config).map_err(|error| error.to_string())?;
    Ok(())
}
