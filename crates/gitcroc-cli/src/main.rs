//! Headless binary exposing core operations as JSON, used by tests and benchmarks.

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--version" => {
            println!("gitcroc-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("usage: gitcroc-cli --version");
            ExitCode::from(2)
        }
    }
}
