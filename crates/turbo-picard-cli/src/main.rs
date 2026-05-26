#![forbid(unsafe_code)]

fn main() {
    std::process::exit(turbo_picard_cli::run_cli(
        "turbo-picard",
        std::env::args().skip(1),
    ));
}
