//! `gitcroc-cli history`: pages of a repo's history as JSON (PERF-2).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use gitcroc_core::{Filter, GitRepo, HistoryStart, history_page};
use serde_json::json;

pub const USAGE: &str = "history <repo-path> [--ref <name>] [--filter <text>] [--pages <n>]";

pub struct Options {
    repo: PathBuf,
    start: HistoryStart,
    filter: Option<Filter>,
    pages: usize,
}

/// Parses the arguments after `history`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let (repo, rest) = args.split_first()?;
    let mut options = Options {
        repo: PathBuf::from(repo),
        start: HistoryStart::All,
        filter: None,
        pages: 1,
    };
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        match arg.to_str()? {
            "--ref" => options.start = HistoryStart::Ref(rest.next()?.to_str()?.to_owned()),
            "--filter" => options.filter = Some(Filter::new(rest.next()?.to_str()?)),
            "--pages" => {
                options.pages = rest.next()?.to_str()?.parse().ok().filter(|&n| n > 0)?;
            }
            _ => return None,
        }
    }
    Some(options)
}

/// Prints a JSON array with one object per page. Exits 0 on success and 2
/// when the repo cannot be read.
pub fn run(options: Options) -> ExitCode {
    match read(options) {
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

fn read(options: Options) -> Result<String, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))?;
    let repo = GitRepo::open(&options.repo).map_err(|error| error.to_string())?;
    let mut pages = Vec::new();
    let mut cursor = None;
    for number in 1..=options.pages {
        let page = runtime
            .block_on(history_page(
                repo.clone(),
                options.start.clone(),
                cursor,
                options.filter.clone(),
            ))
            .map_err(|error| error.to_string())?;
        cursor = page.next.clone();
        let more = cursor.is_some();
        pages.push(json!({ "page": number, "more": more, "rows": page.rows }));
        if !more {
            break;
        }
    }
    serde_json::to_string_pretty(&pages)
        .map_err(|error| format!("cannot write the result as JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_2_history_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        let options = parse_args(&args(&["repo", "--ref", "main", "--pages", "3"])).unwrap();
        assert_eq!(options.repo, PathBuf::from("repo"));
        assert_eq!(options.start, HistoryStart::Ref("main".into()));
        assert_eq!((options.pages, options.filter), (3, None));
        let options = parse_args(&args(&["repo", "--filter", "Fix"])).unwrap();
        assert_eq!(options.filter, Some(Filter::new("fix")));
        assert!(parse_args(&args(&[])).is_none());
        assert!(parse_args(&args(&["repo", "--pages", "0"])).is_none());
        assert!(parse_args(&args(&["repo", "--ref"])).is_none());
    }
}
