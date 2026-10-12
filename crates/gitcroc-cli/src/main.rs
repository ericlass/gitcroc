//! Headless binary exposing core operations as JSON, used by tests and benchmarks.

use std::process::ExitCode;

use gitcroc_core::GitSetup;

mod bench;

fn usage() -> ExitCode {
    eprintln!("usage: gitcroc-cli --version | git-info | {}", bench::USAGE);
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [flag] if flag == "--version" => {
            println!("gitcroc-cli {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        [command] if command == "git-info" => git_info(),
        [command, rest @ ..] if command == "bench" => match bench::parse_args(rest) {
            Some(options) => bench::run(&options),
            None => usage(),
        },
        _ => usage(),
    }
}

/// Prints the git detection result (PLAT-3) as JSON. Exits 0 when the git is
/// usable, 1 when it is not, 2 when something else went wrong.
fn git_info() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            eprintln!("cannot start the async runtime: {error}");
            return ExitCode::from(2);
        }
    };
    let status = runtime.block_on(GitSetup::new().detect());
    match serde_json::to_string_pretty(&status) {
        Ok(json) => println!("{json}"),
        Err(error) => {
            eprintln!("cannot write the result as JSON: {error}");
            return ExitCode::from(2);
        }
    }
    if status.is_usable() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
