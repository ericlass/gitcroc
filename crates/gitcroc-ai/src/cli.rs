//! What the CLI providers share (AI-2): finding the programs, running one in
//! a new empty folder with the prompt on stdin, killing it with every
//! process it started, and the self-test.

use std::ffi::OsString;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{ChildWrapper, CommandWrap, KillOnDrop};
#[cfg(windows)]
use process_wrap::tokio::{CreationFlags, JobObject};
use tempfile::TempDir;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};

use crate::http::{REQUEST_TIMEOUT, cut};
use crate::provider::{ProviderError, ProviderRequest, Role};

/// How often, and how far apart, removing a CLI's folder is tried: on
/// Windows a folder cannot go while a process that just ended still holds
/// it.
const REMOVE_TRIES: u32 = 10;
const REMOVE_PAUSE: Duration = Duration::from_millis(100);

/// Why a CLI failed its self-test (AI-2).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SelfTestError {
    /// It could not be run, or a query failed.
    #[error("{0}")]
    Run(ProviderError),
    /// The installed version does not list every flag the provider uses.
    #[error("{version} lacks {}", missing.join(", "))]
    MissingFlags {
        version: String,
        missing: Vec<String>,
    },
}

/// The one text a CLI gets on stdin: the system prompt, the user message,
/// then a retry's earlier reply and the validation error (AI-9), each under
/// a heading, since a CLI takes no conversation.
pub(crate) fn prompt_text(request: &ProviderRequest) -> String {
    let mut text = format!("{}\n\n{}", request.system, request.user);
    for message in &request.follow_up {
        let heading = match message.role {
            Role::Assistant => "Your previous reply",
            Role::User => "Reply from the user",
        };
        let _ = write!(text, "\n\n# {heading}\n\n{}", message.text);
    }
    text
}

/// What a CLI printed before it exited.
#[derive(Debug)]
pub(crate) struct Finished {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// The error of a CLI that exited unsuccessfully: its stderr, then the
/// reason it gave on stdout, if any.
pub(crate) fn exit_error(
    program: &str,
    finished: &Finished,
    reason: Option<&str>,
) -> ProviderError {
    let parts: Vec<&str> = [Some(finished.stderr.trim()), reason.map(str::trim)]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect();
    ProviderError::Exit {
        program: program.to_owned(),
        code: finished.status.code(),
        stderr: cut(&parts.join("\n")),
    }
}

/// The error of output that is not what the CLI should print, with its
/// stderr.
pub(crate) fn malformed(program: &str, what: &str, finished: &Finished) -> ProviderError {
    let stderr = finished.stderr.trim();
    let mut message = format!("{program} {what}");
    if !stderr.is_empty() {
        let _ = write!(message, "; it said: {stderr}");
    }
    ProviderError::Malformed(cut(&message))
}

/// Runs `path` with `args` in a new empty folder, `input` on its stdin,
/// and waits until it exits or `REQUEST_TIMEOUT` passes. The folder is
/// removed afterwards. Dropping the future kills the program and every
/// process it started, then removes the folder.
pub(crate) async fn run(
    program: &str,
    path: &Path,
    args: &[String],
    input: &str,
) -> Result<Finished, ProviderError> {
    tokio::time::timeout(REQUEST_TIMEOUT, run_in_folder(program, path, args, input))
        .await
        .unwrap_or(Err(ProviderError::Timeout))
}

async fn run_in_folder(
    program: &str,
    path: &Path,
    args: &[String],
    input: &str,
) -> Result<Finished, ProviderError> {
    let failed = |error: std::io::Error| ProviderError::Start {
        program: program.to_owned(),
        message: error.to_string(),
    };
    let folder = tempfile::Builder::new()
        .prefix("gitcroc-ai-")
        .tempdir()
        .map_err(failed)?;
    // An argument array, never a shell (ARCH-3). The environment is the
    // app's, so the CLI finds its own login.
    let mut command = CommandWrap::with_new(path, |command| {
        command
            .args(args)
            .current_dir(folder.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
    });
    #[cfg(unix)]
    command.wrap(ProcessGroup::leader());
    #[cfg(windows)]
    command
        .wrap(CreationFlags(
            windows::Win32::System::Threading::CREATE_NO_WINDOW,
        ))
        .wrap(JobObject);
    command.wrap(KillOnDrop);
    let mut running = Running {
        child: None,
        folder: Some(folder),
    };
    let child = running.child.insert(command.spawn().map_err(failed)?);

    let stdin = child.stdin().take();
    let stdout = child.stdout().take();
    let stderr = child.stderr().take();
    let write = async move {
        // A CLI that exits without reading its input says why by its exit.
        if let Some(mut stdin) = stdin {
            let _ = stdin.write_all(input.as_bytes()).await;
            let _ = stdin.shutdown().await;
        }
    };
    let exited = async {
        // The program itself; what it left running is killed, so that
        // nothing it started holds its output open or outlives the call.
        let status = child.inner_mut().wait().await;
        let _ = child.start_kill();
        status
    };
    let ((), status, stdout, stderr) = tokio::join!(write, exited, read(stdout), read(stderr));
    let status = status.map_err(failed)?;
    running.child = None;
    if let Some(folder) = running.folder.take() {
        remove(folder).await;
    }
    Ok(Finished {
        status,
        stdout,
        stderr,
    })
}

/// A pipe's text, read to its end.
async fn read(pipe: Option<impl AsyncRead + Unpin>) -> String {
    let mut bytes = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut bytes).await;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Removes a CLI's folder, trying again for a while if it is still held.
async fn remove(folder: TempDir) {
    let path = folder.keep();
    for _ in 0..REMOVE_TRIES {
        if std::fs::remove_dir_all(&path).is_ok() || !path.exists() {
            return;
        }
        tokio::time::sleep(REMOVE_PAUSE).await;
    }
}

/// A running CLI and its folder. Dropped before the CLI exited, as when the
/// call is cancelled, it kills the CLI with its process group (Unix) or job
/// object (Windows), then removes the folder once the CLI is gone.
struct Running {
    child: Option<Box<dyn ChildWrapper>>,
    folder: Option<TempDir>,
}

impl Drop for Running {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = child.start_kill();
        let folder = self.folder.take();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = child.inner_mut().wait().await;
                drop(child);
                if let Some(folder) = folder {
                    remove(folder).await;
                }
            });
        }
    }
}

/// Runs a query such as `--version` and returns its stdout.
pub(crate) async fn query(
    program: &str,
    path: &Path,
    args: &[&str],
) -> Result<String, SelfTestError> {
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
    let finished = run(program, path, &args, "")
        .await
        .map_err(SelfTestError::Run)?;
    if !finished.status.success() {
        return Err(SelfTestError::Run(exit_error(program, &finished, None)));
    }
    Ok(finished.stdout)
}

/// The first line `--version` printed.
pub(crate) async fn version(program: &str, path: &Path) -> Result<String, SelfTestError> {
    let output = query(program, path, &["--version"]).await?;
    Ok(output
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or_default()
        .to_owned())
}

/// The version, or what it lacks.
pub(crate) fn verdict(version: String, missing: Vec<String>) -> Result<String, SelfTestError> {
    if missing.is_empty() {
        Ok(version)
    } else {
        Err(SelfTestError::MissingFlags { version, missing })
    }
}

/// The `flags` a help text does not list. A flag counts only as a whole
/// word: `-p` is not found in `--print`, nor `--tools` in `--toolsets`.
pub(crate) fn missing_flags(help: &str, flags: &[&str]) -> Vec<String> {
    let part =
        |c: Option<char>| c.is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    flags
        .iter()
        .filter(|flag| {
            !help.match_indices(**flag).any(|(at, _)| {
                !part(help[..at].chars().next_back())
                    && !part(help[at + flag.len()..].chars().next())
            })
        })
        .map(|flag| (*flag).to_owned())
        .collect()
}

/// Where CLIs are looked for.
#[derive(Debug, Default)]
pub(crate) struct Places {
    pub path: Option<OsString>,
    pub home: Option<PathBuf>,
    /// `%APPDATA%`, where npm puts its programs on Windows.
    pub app_data: Option<PathBuf>,
    /// npm's global prefix, when set by `NPM_CONFIG_PREFIX`.
    pub npm_prefix: Option<PathBuf>,
}

impl Places {
    pub(crate) fn from_env() -> Self {
        Self {
            path: std::env::var_os("PATH"),
            home: std::env::home_dir(),
            app_data: std::env::var_os("APPDATA").map(PathBuf::from),
            npm_prefix: std::env::var_os("NPM_CONFIG_PREFIX")
                .or_else(|| std::env::var_os("npm_config_prefix"))
                .map(PathBuf::from),
        }
    }
}

/// The program files `name` may be: npm installs a `.cmd` on Windows.
fn file_names(name: &str) -> Vec<String> {
    if cfg!(windows) {
        vec![format!("{name}.exe"), format!("{name}.cmd")]
    } else {
        vec![name.to_owned()]
    }
}

/// Every `name` program on `PATH` and in the usual install folders: the
/// native installers' `~/.local/bin`, Claude Code's older `~/.claude/local`,
/// and npm's global folder. In that order, without duplicates.
pub(crate) fn find(name: &str, places: &Places) -> Vec<PathBuf> {
    let mut folders: Vec<PathBuf> = places.path.iter().flat_map(std::env::split_paths).collect();
    if let Some(home) = &places.home {
        folders.push(home.join(".local").join("bin"));
        folders.push(home.join(".claude").join("local"));
        folders.push(home.join(".npm-global").join("bin"));
    }
    if let Some(prefix) = &places.npm_prefix {
        // npm puts programs in the prefix itself on Windows, in `bin` elsewhere.
        folders.push(if cfg!(windows) {
            prefix.clone()
        } else {
            prefix.join("bin")
        });
    }
    if let Some(app_data) = &places.app_data {
        folders.push(app_data.join("npm"));
    }
    if cfg!(unix) {
        folders.push(PathBuf::from("/usr/local/bin"));
        folders.push(PathBuf::from("/opt/homebrew/bin"));
    }
    let mut found: Vec<PathBuf> = Vec::new();
    for folder in folders.iter().filter(|folder| folder.is_absolute()) {
        for file in file_names(name) {
            let candidate = folder.join(file);
            if is_program(&candidate) && !found.contains(&candidate) {
                found.push(candidate);
            }
        }
    }
    found
}

fn is_program(path: &Path) -> bool {
    let Ok(metadata) = std::fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(folder: &Path, name: &str) -> PathBuf {
        std::fs::create_dir_all(folder).unwrap();
        let path = folder.join(&file_names(name)[0]);
        std::fs::write(&path, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        path
    }

    #[test]
    fn ai_2_detect_finds_the_cli_on_path_and_in_install_folders() {
        let root = tempfile::tempdir().unwrap();
        let on_path = program(&root.path().join("bin"), "claude");
        let home = root.path().join("home");
        let local = program(&home.join(".local").join("bin"), "claude");
        let app_data = root.path().join("appdata");
        let npm = program(&app_data.join("npm"), "claude");
        // Not a program: a folder of that name, and another CLI.
        std::fs::create_dir_all(
            home.join(".claude")
                .join("local")
                .join(&file_names("claude")[0]),
        )
        .unwrap();
        program(&root.path().join("bin"), "codex");
        let places = Places {
            // The same folder twice, and a relative one, which is skipped.
            path: Some(
                std::env::join_paths([
                    root.path().join("bin"),
                    root.path().join("bin"),
                    PathBuf::from("bin"),
                ])
                .unwrap(),
            ),
            home: Some(home),
            app_data: Some(app_data),
            npm_prefix: None,
        };
        let found = find("claude", &places);
        let found: Vec<&PathBuf> = found
            .iter()
            .filter(|path| path.starts_with(root.path()))
            .collect();
        assert_eq!(found, [&on_path, &local, &npm]);
    }

    #[cfg(unix)]
    #[test]
    fn ai_2_detect_skips_files_that_are_not_executable() {
        let root = tempfile::tempdir().unwrap();
        let path = program(root.path(), "codex");
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        let places = Places {
            path: Some(root.path().as_os_str().to_owned()),
            ..Places::default()
        };
        assert!(
            !find("codex", &places)
                .iter()
                .any(|found| found.starts_with(root.path()))
        );
    }

    #[test]
    fn ai_2_flags_are_found_as_whole_words() {
        let help = "  -p, --print   Print\n  --tools <tools...>\n  --model=<m>\n";
        assert!(missing_flags(help, &["-p", "--print", "--tools", "--model"]).is_empty());
        assert_eq!(
            missing_flags(help, &["--tool", "-m", "--pri"]),
            ["--tool", "-m", "--pri"]
        );
    }
}
