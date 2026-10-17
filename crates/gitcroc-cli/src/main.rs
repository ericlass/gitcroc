//! Headless binary exposing core operations as JSON, used by tests and benchmarks.

use std::process::ExitCode;

use gitcroc_core::GitSetup;

mod ai;
mod ai_resolve;
mod bench;
mod conflict;
mod diff;
mod gate;
mod group;
mod group_run;
mod history;
mod proposer;
mod status;
mod summaries;
mod workspace;

fn usage() -> ExitCode {
    eprintln!(
        "usage: gitcroc-cli --version | git-info | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {}",
        history::USAGE,
        diff::USAGE,
        conflict::USAGE,
        status::STATUS_USAGE,
        status::SUMMARY_USAGE,
        workspace::USAGE,
        group::USAGE,
        group::TREE_USAGE,
        summaries::USAGE,
        bench::USAGE,
        gate::COMPARE_USAGE,
        gate::BASELINE_USAGE,
        ai::USAGE
    );
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
        [command, rest @ ..] if command == "history" => match history::parse_args(rest) {
            Some(options) => history::run(options),
            None => usage(),
        },
        [command, rest @ ..] if command == "diff" => match diff::parse_args(rest) {
            Some(options) => diff::run(options),
            None => usage(),
        },
        [command, rest @ ..] if command == "conflict" => match conflict::parse_args(rest) {
            Some(options) => conflict::run(&options),
            None => usage(),
        },
        [command, rest @ ..] if command == "status" => match status::parse_args(rest) {
            Some(repo) => status::run(status::Report::Status, &repo),
            None => usage(),
        },
        [command, rest @ ..] if command == "summary" => match status::parse_args(rest) {
            Some(repo) => status::run(status::Report::Summary, &repo),
            None => usage(),
        },
        [command, rest @ ..] if command == "workspace" => match workspace::parse_args(rest) {
            Some(options) => workspace::run(options),
            None => usage(),
        },
        [command, rest @ ..] if command == "group" => match group::parse_args(rest) {
            Some(options) => group::run(options),
            None => usage(),
        },
        [command, rest @ ..] if command == "tree" => match group::parse_tree_args(rest) {
            Some(options) => group::run_tree(&options),
            None => usage(),
        },
        [command, rest @ ..] if command == "summaries" => match summaries::parse_args(rest) {
            Some(options) => summaries::run(options),
            None => usage(),
        },
        [command, rest @ ..] if command == "ai" => match ai::parse_args(rest) {
            Some(options) => ai::run(&options),
            None => usage(),
        },
        [command, rest @ ..] if command == "bench" => match bench::parse_args(rest) {
            Some(options) => bench::run(&options),
            None => usage(),
        },
        [command, rest @ ..] if command == "bench-compare" => {
            match gate::parse_files(rest).as_deref() {
                Some([results, baseline]) => gate::run_compare(results, baseline),
                _ => usage(),
            }
        }
        [command, rest @ ..] if command == "bench-baseline" => match gate::parse_files(rest) {
            Some(files) => gate::run_baseline(&files),
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
