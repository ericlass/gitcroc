//! `gitcroc-cli conflict`: one conflicted file as the conflict editor reads
//! it, labels included, as JSON (OPS-4, OPS-6, OPS-8). `conflict
//! ai-context` prints what would be sent to AI for it instead, or why it
//! may not go (AI-4, AI-5), for tests and for debugging a prompt; `conflict
//! ai-resolve` resolves it with AI and prints the proposal (M4-08).

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use gitcroc_core::{GitRepo, PlatformConfigDir, SettingsStore, ai_file_context};

use crate::ai::is_variable_name;
use crate::ai_resolve::{ResolveOptions, Resolved, resolve};
use crate::workspace::{GivenConfigDir, runtime_and_git};

pub const USAGE: &str = "conflict <repo-path> <path> | conflict ai-context <repo-path> <path> [--hunks 1,3] [--config-dir <dir>] | conflict ai-resolve <repo-path> <path> [--hunks 1,3] [--config-dir <dir>] [--mock <replies.json> | --key-env <NAME>]";

/// The repo and the file's path in its working tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub repo: PathBuf,
    pub path: OsString,
    /// `Some` for `ai-context`.
    pub ai: Option<AiOptions>,
    /// `Some` for `ai-resolve`.
    pub resolve: Option<ResolveOptions>,
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
            resolve: None,
        }),
        [command, repo, path, rest @ ..] if command == "ai-context" => Some(Options {
            repo: PathBuf::from(repo),
            path: path.clone(),
            ai: Some(parse_ai(rest)?),
            resolve: None,
        }),
        [command, repo, path, rest @ ..] if command == "ai-resolve" => Some(Options {
            repo: PathBuf::from(repo),
            path: path.clone(),
            ai: None,
            resolve: Some(parse_resolve(rest)?),
        }),
        _ => None,
    }
}

/// The hunk ids of `--hunks`: a comma-separated list, each from 1.
fn parse_hunks(value: &OsStr) -> Option<Vec<u32>> {
    value
        .to_str()?
        .split(',')
        .map(|id| id.trim().parse::<u32>().ok().filter(|&id| id > 0))
        .collect()
}

fn parse_resolve(args: &[OsString]) -> Option<ResolveOptions> {
    let mut options = ResolveOptions::default();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next()?;
        match flag.to_str()? {
            "--hunks" if options.hunks.is_none() => options.hunks = Some(parse_hunks(value)?),
            "--config-dir" if options.config_dir.is_none() => {
                options.config_dir = Some(PathBuf::from(value));
            }
            "--mock" if options.mock.is_none() && options.key_env.is_none() => {
                options.mock = Some(PathBuf::from(value));
            }
            "--key-env" if options.key_env.is_none() && options.mock.is_none() => {
                let name = value.to_str()?;
                is_variable_name(name).then_some(())?;
                options.key_env = Some(name.to_owned());
            }
            _ => return None,
        }
    }
    Some(options)
}

fn parse_ai(args: &[OsString]) -> Option<AiOptions> {
    let mut options = AiOptions::default();
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let value = rest.next()?;
        match flag.to_str()? {
            "--hunks" if options.hunks.is_none() => options.hunks = Some(parse_hunks(value)?),
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
/// not in conflict and 2 when the repo or the conflict cannot be read;
/// `ai-resolve` exits 3, its JSON printed, when it was refused or AI
/// failed.
pub fn run(options: &Options) -> ExitCode {
    let read = match (&options.ai, &options.resolve) {
        (_, Some(resolve)) => return run_resolve(options, resolve),
        (Some(ai), None) => read_ai(&options.repo, &options.path, ai),
        (None, None) => read(&options.repo, &options.path),
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

fn run_resolve(options: &Options, resolve_options: &ResolveOptions) -> ExitCode {
    match resolve(&options.repo, git_path(&options.path), resolve_options) {
        Ok(Resolved::Proposal(json)) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Ok(Resolved::NotProposed(json)) => {
            println!("{json}");
            ExitCode::from(3)
        }
        Ok(Resolved::NotInConflict) => {
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
                resolve: None,
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
                resolve: None,
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
            parse_args(&args(&["ai-resolve", "a.txt"])).map(|options| options.resolve),
            Some(None)
        );
        assert_eq!(
            parse_args(&args(&["ai-context", "a.txt"])).map(|options| options.ai),
            Some(None)
        );
    }

    #[test]
    fn ai_3_ai_resolve_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        assert_eq!(
            parse_args(&args(&[
                "ai-resolve",
                "repo",
                "a.txt",
                "--mock",
                "replies.json",
                "--hunks",
                "2",
                "--config-dir",
                "c",
            ]))
            .and_then(|options| options.resolve),
            Some(ResolveOptions {
                hunks: Some(vec![2]),
                config_dir: Some(PathBuf::from("c")),
                mock: Some(PathBuf::from("replies.json")),
                key_env: None,
            })
        );
        assert_eq!(
            parse_args(&args(&[
                "ai-resolve",
                "repo",
                "a.txt",
                "--key-env",
                "MY_KEY"
            ]))
            .and_then(|options| options.resolve)
            .and_then(|resolve| resolve.key_env),
            Some("MY_KEY".to_owned())
        );
        for bad in [
            &["ai-resolve", "repo", "a.txt", "--key-env", "sk-live-abc"][..],
            &[
                "ai-resolve",
                "repo",
                "a.txt",
                "--mock",
                "m",
                "--key-env",
                "K",
            ],
            &["ai-resolve", "repo", "a.txt", "--key", "sk"],
            &["ai-resolve", "repo", "a.txt", "--hunks", "0"],
            &["ai-resolve", "repo", "a.txt", "--mock"],
        ] {
            assert_eq!(parse_args(&args(bad)), None, "{bad:?}");
        }
    }
}
