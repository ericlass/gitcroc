//! Starting the file manager or a terminal: the only programs other than
//! git the app starts. Each gets its arguments as an array (ARCH-3) and is
//! not waited for; a thread reaps it when it exits.

use std::ffi::OsString;
use std::io;
use std::path::PathBuf;
use std::process::Command;

/// A program to start in folder `cwd`, with `args`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: OsString,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
}

impl Launch {
    pub fn new(program: &str, args: impl IntoIterator<Item = OsString>, cwd: PathBuf) -> Self {
        Self {
            program: program.into(),
            args: args.into_iter().collect(),
            cwd,
        }
    }
}

/// Why nothing was started.
#[derive(Debug, thiserror::Error)]
pub enum LaunchError {
    #[error("none of {} was found", tried.join(", "))]
    NotFound { tried: Vec<String> },
    #[error("cannot start {program}: {source}")]
    Start { program: String, source: io::Error },
    #[error("the folder {} does not exist", path.display())]
    NoFolder { path: PathBuf },
}

/// Starts the first of `candidates` that is installed. A folder to start in
/// that is gone is reported as such, not as a missing program, which is how
/// the spawn would report it.
pub fn start(candidates: &[Launch]) -> Result<(), LaunchError> {
    let mut tried = Vec::new();
    for launch in candidates {
        if !launch.cwd.is_dir() {
            return Err(LaunchError::NoFolder {
                path: launch.cwd.clone(),
            });
        }
        let program = launch.program.to_string_lossy().into_owned();
        let started = Command::new(&launch.program)
            .args(&launch.args)
            .current_dir(&launch.cwd)
            .spawn();
        match started {
            Ok(mut child) => {
                std::thread::spawn(move || child.wait());
                return Ok(());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => tried.push(program),
            Err(source) => return Err(LaunchError::Start { program, source }),
        }
    }
    Err(LaunchError::NotFound { tried })
}
