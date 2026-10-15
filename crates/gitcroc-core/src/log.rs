//! `gitcroc.log` (section 10): every git process the app runs, with its
//! arguments, working directory, duration and exit code, and the app's
//! warnings. Never stdin, stdout, stderr or file content. Rotating: when a
//! line would take the file over [`LOG_FILE_SIZE`], it becomes
//! `gitcroc.1.log`, the older ones move up, and the oldest of
//! [`LOG_FILES`] is deleted.

use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

use gitcroc_git::{CommandRecord, Exit};

use crate::lock;

/// The current log file, in the log dir.
pub const LOG_FILE: &str = "gitcroc.log";

/// How many files are kept, the current one included.
pub const LOG_FILES: usize = 5;

/// The size a file may reach before it is rotated, in bytes.
pub const LOG_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Where the app keeps its log. Tests point it at a temporary dir.
pub trait LogDir: Send + Sync {
    /// The app's own log dir, or `None` when the platform has none.
    fn log_dir(&self) -> Option<PathBuf>;
}

/// The platform log dir: `$XDG_STATE_HOME/gitcroc` (`~/.local/state`) on
/// Linux, `~/Library/Logs/gitcroc` on macOS, `gitcroc\logs` in the local
/// app data dir on Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct PlatformLogDir;

impl LogDir for PlatformLogDir {
    fn log_dir(&self) -> Option<PathBuf> {
        let dirs = directories::BaseDirs::new()?;
        if cfg!(target_os = "macos") {
            Some(dirs.home_dir().join("Library").join("Logs").join("gitcroc"))
        } else if cfg!(windows) {
            Some(dirs.data_local_dir().join("gitcroc").join("logs"))
        } else {
            let state = dirs.state_dir().map_or_else(
                || dirs.home_dir().join(".local").join("state"),
                Path::to_owned,
            );
            Some(state.join("gitcroc"))
        }
    }
}

/// The open log file and its size.
#[derive(Debug)]
struct Current {
    file: File,
    size: u64,
}

/// The app's log. A log that cannot be written is skipped quietly: the app
/// works the same without it.
#[derive(Debug)]
pub struct AppLog {
    /// `None` when the platform has no log dir.
    dir: Option<PathBuf>,
    size_limit: u64,
    current: Mutex<Option<Current>>,
}

impl AppLog {
    /// The log in `dirs`; the file is created with the first line.
    pub fn open(dirs: &dyn LogDir) -> Self {
        Self {
            dir: dirs.log_dir(),
            size_limit: LOG_FILE_SIZE,
            current: Mutex::new(None),
        }
    }

    /// Rotates at `bytes` instead of [`LOG_FILE_SIZE`].
    pub fn with_size_limit(mut self, bytes: u64) -> Self {
        self.size_limit = bytes;
        self
    }

    /// Logs one git process.
    pub fn command(&self, record: &CommandRecord<'_>) {
        self.line(&command_line(record));
    }

    /// Logs a warning, such as a file watch that failed.
    pub fn warn(&self, message: &str) {
        self.line(&format!("warning: {}", one_line(message)));
    }

    fn lock(&self) -> MutexGuard<'_, Option<Current>> {
        // A panic mid-write leaves at worst a partial line.
        lock(&self.current)
    }

    fn line(&self, text: &str) {
        let Some(dir) = &self.dir else {
            return;
        };
        let line = format!("{} {text}\n", timestamp(SystemTime::now()));
        let length = u64::try_from(line.len()).unwrap_or(u64::MAX);
        let mut current = self.lock();
        if current.is_none() {
            *current = open_current(dir);
        }
        let full =
            |open: &Current| open.size > 0 && open.size.saturating_add(length) > self.size_limit;
        if current.as_ref().is_some_and(full) {
            *current = None;
            rotate(dir);
            *current = open_current(dir);
        }
        if let Some(open) = current.as_mut() {
            if open.file.write_all(line.as_bytes()).is_ok() {
                open.size = open.size.saturating_add(length);
            } else {
                // Opened again at the next line.
                *current = None;
            }
        }
    }
}

/// The path of file `index`: 0 is the current one.
fn file_path(dir: &Path, index: usize) -> PathBuf {
    if index == 0 {
        dir.join(LOG_FILE)
    } else {
        dir.join(format!("gitcroc.{index}.log"))
    }
}

fn open_current(dir: &Path) -> Option<Current> {
    std::fs::create_dir_all(dir).ok()?;
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path(dir, 0))
        .ok()?;
    let size = file.metadata().map_or(0, |metadata| metadata.len());
    Some(Current { file, size })
}

/// Moves each file one up, the current one to `gitcroc.1.log`, and deletes
/// the oldest.
fn rotate(dir: &Path) {
    // Missing files are skipped; a rename that fails leaves that file be.
    let _ = std::fs::remove_file(file_path(dir, LOG_FILES - 1));
    for index in (0..LOG_FILES - 1).rev() {
        let _ = std::fs::rename(file_path(dir, index), file_path(dir, index + 1));
    }
}

/// The record as one line: `git` and its arguments quoted, where it ran,
/// how long it took and how it ended.
fn command_line(record: &CommandRecord<'_>) -> String {
    let args: Vec<String> = record
        .args
        .iter()
        .map(|arg| format!("{:?}", arg.to_string_lossy()))
        .collect();
    let cwd = record.cwd.map_or_else(
        || "the app's directory".to_owned(),
        |cwd| format!("{:?}", cwd.to_string_lossy()),
    );
    let exit = match record.exit {
        Exit::Code(code) => format!("exit {code}"),
        Exit::Signal => "ended by a signal".to_owned(),
        Exit::Killed => "killed".to_owned(),
        Exit::NotStarted => "not started".to_owned(),
    };
    format!(
        "git {} in {cwd}, {} ms, {exit}",
        args.join(" "),
        record.duration.as_millis()
    )
}

fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

/// `time` in UTC as `2026-10-13T12:34:56.789Z`.
fn timestamp(time: SystemTime) -> String {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = since.as_secs();
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let of_day = seconds % 86_400;
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        of_day / 3600,
        of_day / 60 % 60,
        of_day % 60,
        since.subsec_millis()
    )
}

/// The date `days` after 1970-01-01, by Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;
    use std::time::Duration;

    use super::*;

    struct Dir(PathBuf);

    impl LogDir for Dir {
        fn log_dir(&self) -> Option<PathBuf> {
            Some(self.0.clone())
        }
    }

    fn record<'a>(args: &'a [OsString], cwd: &'a Path) -> CommandRecord<'a> {
        CommandRecord {
            args,
            cwd: Some(cwd),
            duration: Duration::from_millis(12),
            exit: Exit::Code(1),
        }
    }

    #[test]
    fn timestamps_are_utc_dates() {
        let at = |seconds| timestamp(UNIX_EPOCH + Duration::from_millis(seconds));
        assert_eq!(at(0), "1970-01-01T00:00:00.000Z");
        assert_eq!(at(1_791_888_896_789), "2026-10-13T10:54:56.789Z");
        assert_eq!(at(951_782_400_000), "2000-02-29T00:00:00.000Z");
    }

    #[test]
    fn a_command_is_one_line_with_its_arguments_place_time_and_exit() {
        let args: Vec<OsString> = ["commit", "-m", "two\nlines"].map(Into::into).into();
        let line = command_line(&record(&args, Path::new("/repo")));
        assert_eq!(
            line,
            r#"git "commit" "-m" "two\nlines" in "/repo", 12 ms, exit 1"#
        );
    }

    #[test]
    fn section_10_the_log_rotates_at_its_size_limit_and_keeps_five_files() {
        let dir = tempfile::tempdir().unwrap();
        let log = AppLog::open(&Dir(dir.path().to_owned())).with_size_limit(200);
        let args: Vec<OsString> = ["status", "--porcelain=v2"].map(Into::into).into();
        for _ in 0..40 {
            log.command(&record(&args, dir.path()));
        }
        let mut names: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "gitcroc.1.log",
                "gitcroc.2.log",
                "gitcroc.3.log",
                "gitcroc.4.log",
                "gitcroc.log"
            ]
        );
        for name in names {
            let text = std::fs::read_to_string(dir.path().join(&name)).unwrap();
            assert!(text.len() <= 200, "{name}: {} bytes", text.len());
            assert!(text.lines().all(|line| line.contains(r#"git "status""#)));
        }
    }

    #[test]
    fn section_10_the_log_holds_no_output() {
        // A record has no field for stdin, stdout or stderr, so the log
        // cannot hold them; a warning is one line.
        let dir = tempfile::tempdir().unwrap();
        let log = AppLog::open(&Dir(dir.path().to_owned()));
        let args: Vec<OsString> = ["--version"].map(Into::into).into();
        log.command(&record(&args, dir.path()));
        log.warn("cannot watch /repo:\nno space left");
        let text = std::fs::read_to_string(dir.path().join(LOG_FILE)).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2, "{text}");
        assert!(lines[0].contains(r#" git "--version" in "#), "{text}");
        assert!(lines[1].ends_with("warning: cannot watch /repo: no space left"));
    }
}
