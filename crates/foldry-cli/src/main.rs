#![forbid(unsafe_code)]

fn main() {
    std::process::exit(foldry_cli::run(std::env::args_os()));
}
