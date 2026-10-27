//! `req-coverage <milestone>|spec [--report-only] [--root <dir>]`
//!
//! Lists the requirement IDs on the "Implements:" lines of
//! `docs/tasks/<milestone>-*.md` that no test under `crates/` or `ui/`
//! mentions (QA-3). For M6, also every spec ID that a test under `ui/`
//! mentions and no test under `crates/gitcroc-ui/` does: what the GPUI UI
//! has still to port. With `spec` instead of a milestone, lists every ID defined
//! in `docs/spec/` that no test mentions and no task marks "No test needed:".
//! Exits 1 if there are any, unless `--report-only` is given, and 2 on a usage
//! or read error.

use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage: req-coverage <milestone>|spec [--report-only] [--root <dir>]";

struct Args {
    milestone: String,
    report_only: bool,
    root: PathBuf,
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Option<Args> {
    let mut milestone = None;
    let mut report_only = false;
    let mut root = PathBuf::from(".");
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--report-only" => report_only = true,
            "--root" => root = PathBuf::from(args.next()?),
            _ if arg.starts_with('-') || milestone.is_some() => return None,
            _ => milestone = Some(arg),
        }
    }
    Some(Args {
        milestone: milestone?,
        report_only,
        root,
    })
}

fn main() -> ExitCode {
    let Some(args) = parse_args(std::env::args().skip(1)) else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let result = if args.milestone == "spec" {
        req_coverage::run_spec(&args.root)
    } else {
        req_coverage::run(&args.root, &args.milestone)
    };
    let report = match result {
        Ok(report) => report,
        Err(err) => {
            eprintln!("req-coverage: {err}");
            return ExitCode::from(2);
        }
    };

    for (id, task, reason) in &report.no_test_needed {
        println!("no test needed: {id} ({task}): {reason}");
    }
    for id in &report.missing {
        println!("missing: {id}");
    }
    println!(
        "{}: {} covered, {} no test needed, {} missing",
        args.milestone,
        report.covered.len(),
        report.no_test_needed.len(),
        report.missing.len()
    );

    if report.missing.is_empty() || args.report_only {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
