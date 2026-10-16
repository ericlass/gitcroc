//! `gitcroc-cli conflict`: one conflicted file as the conflict editor reads
//! it, labels included, as JSON (OPS-4, OPS-6, OPS-8). `conflict
//! ai-context` prints what would be sent to AI for it instead, or why it
//! may not go (AI-4, AI-5), for tests and for debugging a prompt.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitcroc_core::{GitRepo, PlatformConfigDir, SettingsStore, ai_file_context};

use crate::workspace::{GivenConfigDir, runtime_and_git};

pub const USAGE: &str = "conflict <repo-path> <path> | conflict ai-context <repo-path> <path> [--hunks 1,3] [--config-dir <dir>]";

/// The repo and the file's path in its working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub repo: PathBuf,
    pub path: OsString,
    /// `Some` for `ai-context`.
    pub ai: Option<AiOptions>,
}

/// What `ai-context` reads besides the file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AiOptions {
    /// The hunks to include, numbered from 1; all when `None`.
    pub hunks: Option<Vec<u32>>,
    /// Where `settings.toml` with the exclude list is; the platform config
    /// dir when `None`.
    pub config_dir: Option<PathBuf>,
}

/// Parses the arguments after `conflict`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    match args {
        [repo, path] => Some(Options {
            repo: PathBuf::from(repo),
            path: path.clone(),
            ai: None,
        }),
        [command, repo, path, rest @ ..] if command == "ai-context" => Some(Options {
            repo: PathBuf::from(repo),
            path: path.clone(),
            ai: Some(parse_ai(rest)?),
        }),
        _ => None,
    }
}

fn parse_ai(args: &[OsString]) -> Option<AiOptions> {
    let mut options = AiOptions::default();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next()?;
        match flag.to_str()? {
            "--hunks" if options.hunks.is_none() => {
                let ids = value
                    .to_str()?
                    .split(',')
                    .map(|id| id.trim().parse::<u32>().ok().filter(|&id| id > 0))
                    .collect::<Option<Vec<u32>>>()?;
                options.hunks = Some(ids);
            }
            "--config-dir" if options.config_dir.is_none() => {
                options.config_dir = Some(PathBuf::from(value));
            }
            _ => return None,
        }
    }
    Some(options)
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
    let read = match &options.ai {
        Some(ai) => read_ai(&options.repo, &options.path, ai),
        None => read(&options.repo, &options.path),
    };
    match read {
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

/// What would be sent for the file, or why it may not go to AI, as JSON.
fn read_ai(repo: &Path, path: &OsStr, options: &AiOptions) -> Result<Option<String>, String> {
    let settings = match &options.config_dir {
        Some(dir) => SettingsStore::load(&GivenConfigDir(dir.clone())),
        None => SettingsStore::load(&PlatformConfigDir),
    };
    let exclude = settings.ai().exclude;
    let repo = GitRepo::open(repo).map_err(|error| error.to_string())?;
    let (_runtime, git) = runtime_and_git()?;
    let Some(read) = ai_file_context(
        &repo,
        &git,
        &git_path(path),
        &exclude,
        options.hunks.as_deref(),
    )
    .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    serde_json::to_string_pretty(&read)
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
                ai: None,
            })
        );
        assert_eq!(parse_args(&args(&["repo"])), None);
        assert_eq!(parse_args(&args(&["repo", "a", "b"])), None);
    }

    #[test]
    fn ai_5_ai_context_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        assert_eq!(
            parse_args(&args(&["ai-context", "repo", "a.txt", "--hunks", "1, 3"])),
            Some(Options {
                repo: PathBuf::from("repo"),
                path: OsString::from("a.txt"),
                ai: Some(AiOptions {
                    hunks: Some(vec![1, 3]),
                    config_dir: None,
                }),
            })
        );
        assert_eq!(
            parse_args(&args(&["ai-context", "repo", "a.txt"])).and_then(|options| options.ai),
            Some(AiOptions::default())
        );
        for bad in [
            &["ai-context", "repo", "a.txt", "--hunks", "0"][..],
            &["ai-context", "repo", "a.txt", "--hunks", "x"],
            &["ai-context", "repo", "a.txt", "--hunks"],
            &["ai-context", "repo", "a.txt", "--other", "1"],
            &[
                "ai-context",
                "repo",
                "a.txt",
                "--hunks",
                "1",
                "--hunks",
                "2",
            ],
        ] {
            assert_eq!(parse_args(&args(bad)), None, "{bad:?}");
        }
        // Two arguments are still a repo and a path.
        assert_eq!(
            parse_args(&args(&["ai-context", "a.txt"])).map(|options| options.ai),
            Some(None)
        );
    }
}
