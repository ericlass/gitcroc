//! The git CLI runner, minimal form: one process per call, no per-repo locking
//! yet (ARCH-4 arrives in M2). Every repo write and network operation goes
//! through it (ARCH-1).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// Set on every git process (ARCH-3): messages stay in English so they can be
/// parsed, and git never waits on a terminal prompt nobody can answer.
pub const GIT_ENV: [(&str, &str); 2] = [("LC_ALL", "C"), ("GIT_TERMINAL_PROMPT", "0")];

/// Windows: start git without opening a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What a successful git process printed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

/// Why a git process failed.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// The process could not be started.
    #[error("could not run {}: {source}", path.display())]
    Spawn {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    /// git ran and exited unsuccessfully. `code` is `None` when it was killed
    /// by a signal.
    #[error("git exited with {}: {}", exit_code_text(*code), String::from_utf8_lossy(stderr).trim_end())]
    Failed { code: Option<i32>, stderr: Vec<u8> },
}

fn exit_code_text(code: Option<i32>) -> String {
    code.map_or_else(|| "a signal".to_owned(), |code| format!("code {code}"))
}

/// Runs one git executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRunner {
    git: PathBuf,
}

impl GitRunner {
    pub fn new(git: PathBuf) -> Self {
        Self { git }
    }

    /// The git executable this runner starts.
    pub fn git(&self) -> &Path {
        &self.git
    }

    /// The command [`run`](Self::run) spawns: `git` with `args` as an argument
    /// array, never through a shell (ARCH-3), and [`GIT_ENV`] set.
    pub fn command(&self, args: &[OsString], cwd: Option<&Path>) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(&self.git);
        command
            .args(args)
            .envs(GIT_ENV)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);
        command
    }

    /// Like [`run`](Self::run), for a thread that may block: the read API
    /// runs on blocking threads, and its git fallback (ARCH-2) waits here.
    pub fn run_blocking(&self, args: &[OsString], cwd: Option<&Path>) -> Result<Output, GitError> {
        let mut command = std::process::Command::new(&self.git);
        command
            .args(args)
            .envs(GIT_ENV)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
        let output = command.output().map_err(|source| GitError::Spawn {
            path: self.git.clone(),
            source,
        })?;
        finish(output)
    }

    /// Runs git with `args` in `cwd` (or the current directory) and captures
    /// stdout and stderr as bytes.
    pub async fn run(&self, args: &[OsString], cwd: Option<&Path>) -> Result<Output, GitError> {
        let output = self
            .command(args, cwd)
            .output()
            .await
            .map_err(|source| GitError::Spawn {
                path: self.git.clone(),
                source,
            })?;
        finish(output)
    }
}

fn finish(output: std::process::Output) -> Result<Output, GitError> {
    if output.status.success() {
        Ok(Output {
            stdout: output.stdout,
            stderr: output.stderr,
        })
    } else {
        Err(GitError::Failed {
            code: output.status.code(),
            stderr: output.stderr,
        })
    }
}
