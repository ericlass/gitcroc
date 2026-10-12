//! `repo-gen <set> [--out <dir>]`
//!
//! Builds a reference data set of spec section 9 (`target-project`,
//! `big-tree`, `many-repos`, `big-history` or `all`) in `<dir>/<set>`. The
//! directory defaults to `$GITCROC_BENCH_DATA`, or `target/bench-data` when
//! that is not set. A set that is already up to date is left alone.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use repo_gen::{Error, Outcome, SET_NAMES};

const USAGE: &str =
    "usage: repo-gen <target-project|big-tree|many-repos|big-history|all> [--out <dir>]";

fn parse_args(mut args: impl Iterator<Item = String>) -> Option<(Vec<&'static str>, PathBuf)> {
    let mut sets = None;
    let mut out = std::env::var_os("GITCROC_BENCH_DATA")
        .filter(|dir| !dir.is_empty())
        .map_or_else(|| PathBuf::from("target/bench-data"), PathBuf::from);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = PathBuf::from(args.next()?),
            "all" if sets.is_none() => sets = Some(SET_NAMES.to_vec()),
            name if sets.is_none() => {
                sets = Some(vec![*SET_NAMES.iter().find(|set| **set == name)?]);
            }
            _ => return None,
        }
    }
    Some((sets?, out))
}

fn build(set: &str, out: &std::path::Path) -> Result<Outcome, Error> {
    let progress = |repo: &str| eprintln!("  {set}: {repo} done");
    match set {
        "target-project" => repo_gen::generate_set(&repo_gen::target_project(), out, &progress),
        "big-tree" => repo_gen::generate_set(&repo_gen::big_tree(), out, &progress),
        "many-repos" => repo_gen::generate_set(&repo_gen::many_repos(), out, &progress),
        _ => repo_gen::clone_big_history(out),
    }
}

fn main() -> ExitCode {
    let Some((sets, out)) = parse_args(std::env::args().skip(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    for set in sets {
        eprintln!("{set}: building in {}", out.join(set).display());
        let started = Instant::now();
        match build(set, &out) {
            Ok(Outcome::UpToDate) => eprintln!("{set}: up to date"),
            Ok(Outcome::Built) => {
                eprintln!("{set}: built in {:.0} s", started.elapsed().as_secs_f64());
            }
            Err(error) => {
                eprintln!("{set}: {error}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
