//! The git CLI runner: one process per call. Every repo write and network
//! operation goes through it (ARCH-1); writes are started only from
//! [`crate::ops`]. One write per repo at a time (ARCH-4) is the job
//! engine's part, in `gitcroc-core`. Every process it starts is reported to
//! the command log (section 10).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;

/// Set on every git process (ARCH-3): messages stay in English so they can be
/// parsed, and git never waits on a terminal prompt nobody can answer.
pub const GIT_ENV: [(&str, &str); 2] = [("LC_ALL", "C"), ("GIT_TERMINAL_PROMPT", "0")];

/// Set on every write besides [`GIT_ENV`]: git never waits on an editor
/// nobody sees. An operation that needs a message passes it explicitly.
pub const WRITE_ENV: [(&str, &str); 1] = [("GIT_EDITOR", "true")];

/// The variables that send git's and ssh's prompts to a program (D10). Only
/// a network write with [`Write::askpass`] gets them; every other process
/// has them removed, so a value inherited from the app's environment never
/// reaches git.
pub const ASKPASS_VARS: [&str; 3] = ["GIT_ASKPASS", "SSH_ASKPASS", "SSH_ASKPASS_REQUIRE"];

/// Where a network write sends the prompts git and ssh cannot answer
/// silently (D10): the helper `program`, and the variables it needs to
/// reach the app, such as its socket and the job's token. Credential
/// helpers and SSH agents still answer first.
#[derive(Clone, PartialEq, Eq)]
pub struct Askpass {
    pub program: PathBuf,
    pub env: Vec<(String, String)>,
}

impl std::fmt::Debug for Askpass {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The variables carry the job's token.
        f.debug_struct("Askpass")
            .field("program", &self.program)
            .finish_non_exhaustive()
    }
}

/// Windows: start git without opening a console window.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// What a successful git process printed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Output {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl Output {
    /// Adds what `other`, a later process, printed.
    pub fn append(&mut self, other: Output) {
        self.stdout.extend(other.stdout);
        self.stderr.extend(other.stderr);
    }
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
    Failed {
        code: Option<i32>,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
    },
    /// The process was killed on request (GRP-10), or not started because
    /// the request came first.
    #[error("git was stopped")]
    Killed,
    /// The resolved file of a conflict could not be written (OPS-5), the
    /// one write the app makes itself (ARCH-1).
    #[error("cannot write {}: {source}", path.display())]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

fn exit_code_text(code: Option<i32>) -> String {
    code.map_or_else(|| "a signal".to_owned(), |code| format!("code {code}"))
}

/// Asks a running write to stop: its git process is killed (GRP-10). Clones
/// share the request.
#[derive(Debug, Clone, Default)]
pub struct Kill(Arc<KillState>);

#[derive(Debug, Default)]
struct KillState {
    requested: AtomicBool,
    notify: Notify,
}

impl Kill {
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests the kill; a process not started yet is not started.
    pub fn kill(&self) {
        self.0.requested.store(true, Ordering::SeqCst);
        self.0.notify.notify_waiters();
    }

    pub fn is_requested(&self) -> bool {
        self.0.requested.load(Ordering::SeqCst)
    }

    /// Resolves once the kill is requested.
    pub async fn requested(&self) {
        let notified = self.0.notify.notified();
        tokio::pin!(notified);
        // Registered before the check, so a request in between still wakes.
        notified.as_mut().enable();
        if self.is_requested() {
            return;
        }
        notified.await;
    }
}

/// How a write runs besides its arguments.
#[derive(Default, Clone, Copy)]
pub struct Write<'a> {
    /// Kills the process when requested (fetch and clone, GRP-10).
    pub kill: Option<&'a Kill>,
    /// Gets each stderr line while git runs, for `--progress` output. A
    /// carriage return ends a line like a newline.
    pub on_line: Option<&'a (dyn Fn(&str) + Send + Sync)>,
    /// Set on network writes only: fetch, pull, push, clone, remote branch
    /// delete and tag push (D10).
    pub askpass: Option<&'a Askpass>,
    /// What git reads on stdin; nothing when `None`.
    pub stdin: Option<&'a [u8]>,
}

impl std::fmt::Debug for Write<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Write")
            .field("kill", &self.kill)
            .field("on_line", &self.on_line.is_some())
            .field("askpass", &self.askpass)
            .field("stdin", &self.stdin.map(<[u8]>::len))
            .finish()
    }
}

/// How a git process ended, for the command log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    /// Ended by a signal.
    Signal,
    /// Killed on request.
    Killed,
    /// Could not be started, or a kill request came first.
    NotStarted,
}

/// One git process, as the command log records it: no stdin, stdout or
/// stderr, and no file content (section 10).
#[derive(Debug, Clone, Copy)]
pub struct CommandRecord<'a> {
    pub args: &'a [OsString],
    pub cwd: Option<&'a Path>,
    pub duration: Duration,
    pub exit: Exit,
}

type CommandLog = dyn Fn(&CommandRecord<'_>) + Send + Sync;

/// Where every runner of the process reports the git processes it ran.
static COMMAND_LOG: OnceLock<Box<CommandLog>> = OnceLock::new();

/// Sends a record of every git process any runner starts from now on to
/// `log`. Set once per process; returns `false` when it was set already.
pub fn log_commands_to(log: impl Fn(&CommandRecord<'_>) + Send + Sync + 'static) -> bool {
    COMMAND_LOG.set(Box::new(log)).is_ok()
}

fn log(args: &[OsString], cwd: Option<&Path>, started: Instant, exit: Exit) {
    if let Some(log) = COMMAND_LOG.get() {
        log(&CommandRecord {
            args,
            cwd,
            duration: started.elapsed(),
            exit,
        });
    }
}

fn exit_of(status: ExitStatus) -> Exit {
    status.code().map_or(Exit::Signal, Exit::Code)
}

/// Runs one git executable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitRunner {
    git: PathBuf,
    /// Set on every process after [`GIT_ENV`].
    env: Arc<[(OsString, OsString)]>,
}

impl GitRunner {
    pub fn new(git: PathBuf) -> Self {
        Self {
            git,
            env: Arc::new([]),
        }
    }

    /// Sets `vars` on every process this runner starts, such as a `HOME`
    /// with no git config for tests.
    pub fn with_env<K, V>(self, vars: impl IntoIterator<Item = (K, V)>) -> Self
    where
        K: Into<OsString>,
        V: Into<OsString>,
    {
        let env: Vec<(OsString, OsString)> = vars
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        Self {
            env: env.into(),
            ..self
        }
    }

    /// The git executable this runner starts.
    pub fn git(&self) -> &Path {
        &self.git
    }

    /// The command [`run`](Self::run) spawns: as
    /// [`blocking_command`](Self::blocking_command), for tokio, and killed
    /// when dropped.
    pub fn command(&self, args: &[OsString], cwd: Option<&Path>) -> tokio::process::Command {
        let mut command = tokio::process::Command::from(self.blocking_command(args, cwd));
        command.kill_on_drop(true);
        command
    }

    /// The command [`run_write`](Self::run_write) spawns: as
    /// [`command`](Self::command), with [`WRITE_ENV`] set too, the askpass
    /// helper when `write` has one (D10), and stdin piped when `write`
    /// gives it.
    pub fn write_command(
        &self,
        args: &[OsString],
        cwd: &Path,
        write: &Write<'_>,
    ) -> tokio::process::Command {
        let mut command = self.command(args, Some(cwd));
        command.envs(WRITE_ENV);
        if let Some(askpass) = write.askpass {
            command
                .env("GIT_ASKPASS", &askpass.program)
                .env("SSH_ASKPASS", &askpass.program)
                .env("SSH_ASKPASS_REQUIRE", "prefer")
                .envs(askpass.env.iter().map(|(key, value)| (key, value)));
        }
        if write.stdin.is_some() {
            command.stdin(Stdio::piped());
        }
        command
    }

    /// Like [`run`](Self::run), for a thread that may block: the read API
    /// runs on blocking threads, and its git fallback (ARCH-2) waits here.
    pub fn run_blocking(&self, args: &[OsString], cwd: Option<&Path>) -> Result<Output, GitError> {
        let mut command = self.blocking_command(args, cwd);
        let started = Instant::now();
        let output = command.output().map_err(|source| {
            log(args, cwd, started, Exit::NotStarted);
            GitError::Spawn {
                path: self.git.clone(),
                source,
            }
        })?;
        log(args, cwd, started, exit_of(output.status));
        finish(output.status, output.stdout, output.stderr)
    }

    /// The command [`run_blocking`](Self::run_blocking) spawns: `git` with
    /// `args` as an argument array, never through a shell (ARCH-3), and
    /// [`GIT_ENV`] set.
    pub fn blocking_command(&self, args: &[OsString], cwd: Option<&Path>) -> std::process::Command {
        let mut command = std::process::Command::new(&self.git);
        command.args(args).envs(GIT_ENV);
        for var in ASKPASS_VARS {
            command.env_remove(var);
        }
        command.envs(self.env.iter().map(|(key, value)| (key, value)));
        command
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        #[cfg(windows)]
        std::os::windows::process::CommandExt::creation_flags(&mut command, CREATE_NO_WINDOW);
        command
    }

    /// Runs git with `args` in `cwd` (or the current directory) and captures
    /// stdout and stderr as bytes.
    pub async fn run(&self, args: &[OsString], cwd: Option<&Path>) -> Result<Output, GitError> {
        let started = Instant::now();
        let output = self.command(args, cwd).output().await.map_err(|source| {
            log(args, cwd, started, Exit::NotStarted);
            GitError::Spawn {
                path: self.git.clone(),
                source,
            }
        })?;
        log(args, cwd, started, exit_of(output.status));
        finish(output.status, output.stdout, output.stderr)
    }

    /// Runs a write in repo `cwd`: with [`WRITE_ENV`] set, stderr lines sent
    /// to `write.on_line` as they come, and the process killed when
    /// `write.kill` is requested. Only [`crate::ops`] calls it, which
    /// `arch_1_writes_go_through_the_runner` checks.
    pub async fn run_write(
        &self,
        args: &[OsString],
        cwd: &Path,
        write: Write<'_>,
    ) -> Result<Output, GitError> {
        let started = Instant::now();
        if write.kill.is_some_and(Kill::is_requested) {
            log(args, Some(cwd), started, Exit::NotStarted);
            return Err(GitError::Killed);
        }
        let mut child = self
            .write_command(args, cwd, &write)
            .spawn()
            .map_err(|source| {
                log(args, Some(cwd), started, Exit::NotStarted);
                GitError::Spawn {
                    path: self.git.clone(),
                    source,
                }
            })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let read = async {
            let ((), stdout, stderr) = tokio::join!(
                write_all(stdin, write.stdin),
                read_all(stdout),
                read_lines(stderr, write.on_line)
            );
            (stdout, stderr)
        };
        let killed = async {
            match write.kill {
                Some(kill) => kill.requested().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            (stdout, stderr) = read => {
                let status = child.wait().await;
                let status = match status {
                    Ok(status) => status,
                    Err(source) => {
                        log(args, Some(cwd), started, Exit::Signal);
                        return Err(GitError::Spawn { path: self.git.clone(), source });
                    }
                };
                log(args, Some(cwd), started, exit_of(status));
                finish(status, stdout, stderr)
            }
            () = killed => {
                // Already ended when this fails; either way it is waited for.
                let _ = child.start_kill();
                let _ = child.wait().await;
                log(args, Some(cwd), started, Exit::Killed);
                Err(GitError::Killed)
            }
        }
    }
}

/// Writes `bytes` to `pipe` and closes it. A write error ends it early:
/// git stopped reading, and its exit code tells why.
async fn write_all(pipe: Option<tokio::process::ChildStdin>, bytes: Option<&[u8]>) {
    if let (Some(mut pipe), Some(bytes)) = (pipe, bytes) {
        let _ = pipe.write_all(bytes).await;
    }
}

/// Everything `pipe` gives until it closes. A read error ends it early:
/// the process's exit code tells what happened.
async fn read_all(pipe: Option<impl AsyncRead + Unpin>) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut bytes).await;
    }
    bytes
}

/// Like [`read_all`], sending each line to `on_line` as it ends: at a
/// newline or a carriage return. A last line without either is sent at the
/// end.
async fn read_lines(
    pipe: Option<impl AsyncRead + Unpin>,
    on_line: Option<&(dyn Fn(&str) + Send + Sync)>,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    let Some(mut pipe) = pipe else {
        return bytes;
    };
    let mut buffer = [0u8; 4096];
    let mut line_start = 0;
    loop {
        let read = match pipe.read(&mut buffer).await {
            Ok(0) | Err(_) => break,
            Ok(read) => read,
        };
        bytes.extend_from_slice(buffer.get(..read).unwrap_or_default());
        if let Some(on_line) = on_line {
            line_start = send_lines(&bytes, line_start, on_line);
        }
    }
    if let Some(on_line) = on_line
        && let Some(rest) = bytes.get(line_start..)
        && !rest.is_empty()
    {
        on_line(&String::from_utf8_lossy(rest));
    }
    bytes
}

/// Sends the lines of `bytes` from `start` that have ended, and returns
/// where the next one starts. Empty lines, as between `\r\n`, are not sent.
fn send_lines(bytes: &[u8], start: usize, on_line: &(dyn Fn(&str) + Send + Sync)) -> usize {
    let mut next = start;
    for (at, byte) in bytes.iter().enumerate().skip(start) {
        if *byte == b'\n' || *byte == b'\r' {
            if let Some(line) = bytes.get(next..at)
                && !line.is_empty()
            {
                on_line(&String::from_utf8_lossy(line));
            }
            next = at + 1;
        }
    }
    next
}

fn finish(status: ExitStatus, stdout: Vec<u8>, stderr: Vec<u8>) -> Result<Output, GitError> {
    if status.success() {
        Ok(Output { stdout, stderr })
    } else {
        Err(GitError::Failed {
            code: status.code(),
            stdout,
            stderr,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    fn lines(chunks: &[&[u8]]) -> Vec<String> {
        let sent = Mutex::new(Vec::new());
        let on_line = |line: &str| sent.lock().unwrap().push(line.to_owned());
        let mut bytes = Vec::new();
        let mut start = 0;
        for chunk in chunks {
            bytes.extend_from_slice(chunk);
            start = send_lines(&bytes, start, &on_line);
        }
        assert!(start <= bytes.len());
        sent.into_inner().unwrap()
    }

    #[test]
    fn a_carriage_return_ends_a_progress_line() {
        assert_eq!(
            lines(&[
                b"Counting: 1%\rCount",
                b"ing: 50%\rCounting: 100%, done.\r\n",
                b"Done\n"
            ]),
            [
                "Counting: 1%",
                "Counting: 50%",
                "Counting: 100%, done.",
                "Done"
            ]
        );
    }
}
