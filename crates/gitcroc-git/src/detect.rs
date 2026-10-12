//! Finding the system git and checking its version (PLAT-3, D13).

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use crate::runner::GitRunner;

/// A git version as (major, minor, patch).
pub type GitVersion = (u32, u32, u32);

/// The oldest git gitcroc works with (D13).
pub const MIN_GIT: GitVersion = (2, 40, 0);

/// macOS: Homebrew's prefixes on Apple silicon and on Intel. An app started
/// from Finder gets a `PATH` without them, which leaves only Apple's git.
const HOMEBREW_PREFIXES: [&str; 2] = ["/opt/homebrew", "/usr/local"];

#[cfg(windows)]
const GIT_EXE: &str = "git.exe";
#[cfg(not(windows))]
const GIT_EXE: &str = "git";

/// A usable git: where it is and which version it reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInstall {
    pub path: PathBuf,
    pub version: GitVersion,
}

/// Why no usable git was found.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DetectError {
    /// No git executable exists at any of the `searched` paths.
    #[error("git was not found")]
    NotFound { searched: Vec<PathBuf> },
    /// The git at `path` is older than `required`.
    #[error("git at {} is version {}.{}.{}; {}.{}.{} or newer is required",
        path.display(), found.0, found.1, found.2, required.0, required.1, required.2)]
    TooOld {
        path: PathBuf,
        found: GitVersion,
        required: GitVersion,
    },
    /// The git at `path` did not run, or `git --version` printed something
    /// that is not a version. `output` is what it printed or the error.
    #[error("could not read the version of git at {}: {output}", path.display())]
    Unparseable { path: PathBuf, output: String },
}

/// The environment detection searches. [`SearchEnv::from_process`] reads it
/// from the process; tests build their own.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchEnv {
    /// `PATH`.
    pub path: Option<OsString>,
    /// Windows only: `%ProgramFiles%`, `%ProgramFiles(x86)%`, `%LocalAppData%`.
    pub program_files: Option<OsString>,
    pub program_files_x86: Option<OsString>,
    pub local_app_data: Option<OsString>,
    /// Homebrew prefixes whose `bin/git` is searched after `PATH`; filled on
    /// macOS only.
    pub homebrew_prefixes: Vec<PathBuf>,
}

impl SearchEnv {
    pub fn from_process() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            program_files: std::env::var_os("ProgramFiles"),
            program_files_x86: std::env::var_os("ProgramFiles(x86)"),
            local_app_data: std::env::var_os("LocalAppData"),
            homebrew_prefixes: if cfg!(target_os = "macos") {
                HOMEBREW_PREFIXES.iter().map(PathBuf::from).collect()
            } else {
                Vec::new()
            },
        }
    }
}

/// Where to look for git, in order: the explicit path from settings, every
/// `PATH` entry, then on Windows the standard Git for Windows locations, then
/// on macOS Homebrew's `bin/git`.
pub fn candidates(explicit: Option<&Path>, env: &SearchEnv) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = explicit.map(Path::to_path_buf).into_iter().collect();
    if let Some(path) = &env.path {
        found.extend(
            std::env::split_paths(path)
                .filter(|dir| !dir.as_os_str().is_empty())
                .map(|dir| dir.join(GIT_EXE)),
        );
    }
    if cfg!(windows) {
        let under = |root: &Option<OsString>, rest: &[&str]| {
            root.as_deref()
                .filter(|root| !root.is_empty())
                .map(|root| rest.iter().fold(PathBuf::from(root), |p, s| p.join(s)))
        };
        found.extend(
            [
                under(&env.program_files, &["Git", "cmd", "git.exe"]),
                under(&env.program_files_x86, &["Git", "cmd", "git.exe"]),
                under(&env.local_app_data, &["Programs", "Git", "cmd", "git.exe"]),
            ]
            .into_iter()
            .flatten(),
        );
    }
    found.extend(
        env.homebrew_prefixes
            .iter()
            .map(|prefix| prefix.join("bin").join("git")),
    );
    found
}

/// Finds the first git among [`candidates`] and checks its version against
/// [`MIN_GIT`]. Later candidates are not tried once one exists.
pub async fn detect(explicit: Option<&Path>, env: &SearchEnv) -> Result<GitInstall, DetectError> {
    let searched = candidates(explicit, env);
    let Some(path) = searched.iter().find(|p| is_executable(p)).cloned() else {
        return Err(DetectError::NotFound { searched });
    };
    let runner = GitRunner::new(path.clone());
    let output = match runner.run(&[OsString::from("--version")], None).await {
        Ok(output) => String::from_utf8_lossy(&output.stdout).trim().to_owned(),
        Err(error) => {
            return Err(DetectError::Unparseable {
                path,
                output: error.to_string(),
            });
        }
    };
    match parse_version(&output) {
        None => Err(DetectError::Unparseable { path, output }),
        Some(found) if found < MIN_GIT => Err(DetectError::TooOld {
            path,
            found,
            required: MIN_GIT,
        }),
        Some(version) => Ok(GitInstall { path, version }),
    }
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.is_file() && meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        meta.is_file()
    }
}

/// Reads the version from `git --version` output, such as
/// `git version 2.47.1.windows.2` or `git version 2.39.5 (Apple Git-154)`.
/// Anything after the third number (`.windows.2`, `.rc1`) is ignored.
pub fn parse_version(output: &str) -> Option<GitVersion> {
    let version = output.trim().strip_prefix("git version ")?;
    let mut parts = version.split_whitespace().next()?.split('.');
    let mut number = || -> Option<u32> {
        let part = parts.next()?;
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    };
    Some((number()?, number()?, number()?))
}
