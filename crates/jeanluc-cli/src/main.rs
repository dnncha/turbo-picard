#![forbid(unsafe_code)]

fn main() {
    std::process::exit(jeanluc_cli::run_cli("jeanluc", std::env::args().skip(1)));
}
