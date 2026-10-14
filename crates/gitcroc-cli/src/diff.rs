//! `gitcroc-cli diff`: a commit's header, its changed files and one file's
//! diff as JSON (UI-14, ARCH-8).

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitCode;

use gitcroc_core::{
    Context, DiffOptions, DiffSource, GitRepo, changed_files, commit_details, file_diff,
};
use serde_json::json;

pub const USAGE: &str = "diff <repo-path> <hash> [--parent <n>] [--file <id>] \
     [--ignore-whitespace] [--context <3|10|all>]";

#[derive(Debug, PartialEq, Eq)]
pub struct Options {
    repo: PathBuf,
    hash: String,
    parent: usize,
    file: usize,
    diff: DiffOptions,
}

/// Parses the arguments after `diff`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let [repo, hash, rest @ ..] = args else {
        return None;
    };
    let mut options = Options {
        repo: PathBuf::from(repo),
        hash: hash.to_str()?.to_owned(),
        parent: 0,
        file: 0,
        diff: DiffOptions::default(),
    };
    let mut rest = rest.iter();
    while let Some(arg) = rest.next() {
        match arg.to_str()? {
            "--parent" => options.parent = rest.next()?.to_str()?.parse().ok()?,
            "--file" => options.file = rest.next()?.to_str()?.parse().ok()?,
            "--ignore-whitespace" => options.diff.ignore_whitespace = true,
            "--context" => {
                options.diff.context = match rest.next()?.to_str()? {
                    "3" => Context::Three,
                    "10" => Context::Ten,
                    "all" => Context::WholeFile,
                    _ => return None,
                }
            }
            _ => return None,
        }
    }
    Some(options)
}

/// Prints `{commit, files, diff}`; `diff` is the diff of file `--file` (0 by
/// default), `null` when the commit changed no file. Exits 0 on success and
/// 2 when the repo cannot be read or the file id is out of range.
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
    let text = |error: gitcroc_core::DiffReadError| error.to_string();
    let commit = runtime
        .block_on(commit_details(repo.clone(), options.hash.clone()))
        .map_err(text)?;
    let files = runtime
        .block_on(changed_files(
            repo.clone(),
            options.hash.clone(),
            options.parent,
        ))
        .map_err(text)?;
    let diff = match files.get(options.file) {
        Some(file) => Some(
            runtime
                .block_on(file_diff(
                    repo,
                    DiffSource::Commit {
                        hash: options.hash,
                        parent: options.parent,
                    },
                    file.clone(),
                    options.diff,
                ))
                .map_err(text)?,
        ),
        None if files.is_empty() => None,
        None => {
            return Err(format!(
                "no file {}; the commit changed {} files",
                options.file,
                files.len()
            ));
        }
    };
    serde_json::to_string_pretty(&json!({ "commit": commit, "files": files, "diff": diff }))
        .map_err(|error| format!("cannot write the result as JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_8_diff_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        let options = parse_args(&args(&[
            "repo",
            "HEAD",
            "--parent",
            "1",
            "--file",
            "4",
            "--ignore-whitespace",
            "--context",
            "all",
        ]))
        .unwrap();
        assert_eq!(
            options,
            Options {
                repo: PathBuf::from("repo"),
                hash: "HEAD".into(),
                parent: 1,
                file: 4,
                diff: DiffOptions {
                    ignore_whitespace: true,
                    context: Context::WholeFile,
                    ..DiffOptions::default()
                },
            }
        );
        let options = parse_args(&args(&["repo", "c0ffee", "--context", "10"])).unwrap();
        assert_eq!(options.diff.context, Context::Ten);
        assert!(parse_args(&args(&["repo"])).is_none());
        assert!(parse_args(&args(&["repo", "HEAD", "--context", "5"])).is_none());
        assert!(parse_args(&args(&["repo", "HEAD", "--file"])).is_none());
    }
}
