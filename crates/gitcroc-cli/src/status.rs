//! `gitcroc-cli status` and `gitcroc-cli summary`: a repo's working copy,
//! and what its row in the tree shows, as JSON (UI-3, UI-15, OPS-1).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitcroc_core::{GitRepo, GitRunner, GitSetup, StatusReader, repo_summary, working_copy};

pub const STATUS_USAGE: &str = "status <repo-path>";
pub const SUMMARY_USAGE: &str = "summary <repo-path>";

/// What to print.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Report {
    /// The staged, unstaged and unmerged lists.
    Status,
    /// Branch, upstream, dirty flag and conflict state.
    Summary,
}

/// Parses the arguments after `status` or `summary`: one repo path.
pub fn parse_args(args: &[OsString]) -> Option<PathBuf> {
    match args {
        [repo] => Some(PathBuf::from(repo)),
        _ => None,
    }
}

/// Prints the report as JSON. Exits 0 on success and 2 when no usable git
/// is found or the repo cannot be read; git's own refusals, such as
/// `safe.directory`, are printed as git wrote them (SEC-6).
pub fn run(report: Report, repo: &Path) -> ExitCode {
    match read(report, repo) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn read(report: Report, path: &Path) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))?;
    let setup = GitSetup::new();
    let status = runtime.block_on(setup.detect());
    let git = runtime
        .block_on(setup.git())
        .ok_or_else(|| format!("no usable git: {status:?}"))?;
    let repo = GitRepo::open(path).map_err(|error| error.to_string())?;
    let reader = StatusReader::new(repo, GitRunner::new(git.path));
    let value = match report {
        Report::Status => runtime
            .block_on(working_copy(reader))
            .map(serde_json::to_value),
        Report::Summary => runtime
            .block_on(repo_summary(reader))
            .map(serde_json::to_value),
    }
    .map_err(|error| error.to_string())?;
    value
        .and_then(|value| serde_json::to_string_pretty(&value))
        .map_err(|error| format!("cannot write the result as JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_5_status_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        assert_eq!(parse_args(&args(&["repo"])), Some(PathBuf::from("repo")));
        assert_eq!(parse_args(&args(&[])), None);
        assert_eq!(parse_args(&args(&["repo", "extra"])), None);
    }
}
