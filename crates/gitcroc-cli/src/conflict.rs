//! `gitcroc-cli conflict`: one conflicted file as the conflict editor reads
//! it, labels included, as JSON (OPS-4, OPS-6, OPS-8).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitcroc_core::GitRepo;

pub const USAGE: &str = "conflict <repo-path> <path>";

/// The repo and the file's path in its working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub repo: PathBuf,
    pub path: OsString,
}

/// Parses the arguments after `conflict`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    match args {
        [repo, path] => Some(Options {
            repo: PathBuf::from(repo),
            path: path.clone(),
        }),
        _ => None,
    }
}

/// `path` as git stores it: its bytes, with `/` between folders.
fn git_path(path: &OsStr) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_bytes().to_vec()
    }
    #[cfg(not(unix))]
    {
        path.to_string_lossy().replace('\\', "/").into_bytes()
    }
}

/// Prints the conflict as JSON. Exits 0 on success, 1 when the file is
/// not in conflict and 2 when the repo or the conflict cannot be read.
pub fn run(options: &Options) -> ExitCode {
    match read(&options.repo, &options.path) {
        Ok(Some(json)) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Ok(None) => {
            eprintln!("{} is not in conflict", Path::new(&options.path).display());
            ExitCode::from(1)
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn read(repo: &Path, path: &OsStr) -> Result<Option<String>, String> {
    let repo = GitRepo::open(repo).map_err(|error| error.to_string())?;
    let Some(conflict) = repo
        .conflict(&git_path(path))
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    serde_json::to_string_pretty(&conflict)
        .map(Some)
        .map_err(|error| format!("cannot write the result as JSON: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_8_conflict_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        assert_eq!(
            parse_args(&args(&["repo", "a.txt"])),
            Some(Options {
                repo: PathBuf::from("repo"),
                path: OsString::from("a.txt"),
            })
        );
        assert_eq!(parse_args(&args(&["repo"])), None);
        assert_eq!(parse_args(&args(&["repo", "a", "b"])), None);
    }
}
