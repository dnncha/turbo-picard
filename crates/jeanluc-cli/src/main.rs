#![forbid(unsafe_code)]

fn main() {
    let mut args = std::env::args().skip(1);

    match args.next().as_deref() {
        Some("MarkDuplicates") => {
            eprintln!("MarkDuplicates is recognized but not implemented yet");
            std::process::exit(2);
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
