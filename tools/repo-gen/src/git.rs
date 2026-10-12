//! Running `git` for the fixtures and the generator. Always an argument array,
//! never a shell (ARCH-3).

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::Error;

/// The environment of every git process started here. It matches the product
/// runner's (`LC_ALL=C`, no prompts) and also hides the user's and the
/// system's git config, so that settings such as `commit.gpgsign` or a hooks
/// path cannot change or break the repos. A missing global config file is
/// treated by git as empty.
pub(crate) fn isolated(command: &mut Command, home: &Path) {
    command
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", home.join("no-global-gitconfig"))
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE");
}

/// A git invocation in a directory with the isolated environment.
pub(crate) fn command<I, S>(cwd: &Path, args: I) -> Command
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let mut command = Command::new("git");
    command.args(args).current_dir(cwd).stdin(Stdio::null());
    isolated(&mut command, cwd);
    command
}

/// Runs a command to completion and fails unless it exits with 0.
pub(crate) fn run(mut command: Command) -> Result<Output, Error> {
    let output = command.output()?;
    if output.status.success() {
        Ok(output)
    } else {
        Err(failed(&command, &output))
    }
}

pub(crate) fn failed(command: &Command, output: &Output) -> Error {
    Error::Git {
        args: describe(command),
        code: output.status.code(),
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_owned(),
    }
}

pub(crate) fn describe(command: &Command) -> String {
    let dir = command
        .get_current_dir()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    format!("git {} (in {})", args.join(" "), dir.display())
}
