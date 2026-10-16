//! The fake CLI of `tests/support/fake_cli.rs`, in a folder of its own with
//! the script it answers from, and the runs it recorded.

use std::path::{Path, PathBuf};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// A run of the fake: what a CLI provider gave it.
#[derive(Debug, Clone)]
pub struct Run {
    pub args: Vec<String>,
    pub cwd: PathBuf,
    /// What the working directory held when the run started.
    pub cwd_entries: Vec<String>,
    pub stdin: String,
}

/// A copy of the fake, named like the CLI it stands in for.
pub struct FakeCli {
    folder: tempfile::TempDir,
    path: PathBuf,
}

impl FakeCli {
    /// A fake `name` (`claude` or `codex`) answering from `script`: the
    /// output of `version`, `help`, `exec_help` and `features` queries, and
    /// a run's `stdout`, `stderr`, `exit` and `hold`.
    pub fn new(name: &str, script: &serde_json::Value) -> Self {
        let folder = tempfile::tempdir().unwrap();
        let path = folder
            .path()
            .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
        let program = env!("CARGO_BIN_EXE_fake-cli");
        // A link on Unix: a copy is written through a handle that a process
        // forked meanwhile by another test holds, and Linux then refuses to
        // run it ("text file busy"). The fake finds its script by the path
        // it was started by.
        #[cfg(unix)]
        std::os::unix::fs::symlink(program, &path).unwrap();
        #[cfg(not(unix))]
        std::fs::copy(program, &path).unwrap();
        std::fs::write(folder.path().join("script.json"), script.to_string()).unwrap();
        Self { folder, path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Every run, in order.
    pub fn runs(&self) -> Vec<Run> {
        let Ok(text) = std::fs::read_to_string(self.folder.path().join("records.jsonl")) else {
            return Vec::new();
        };
        text.lines()
            .map(|line| {
                let record: serde_json::Value = serde_json::from_str(line).unwrap();
                let strings = |name: &str| -> Vec<String> {
                    record[name]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|value| value.as_str().unwrap().to_owned())
                        .collect()
                };
                Run {
                    args: strings("args"),
                    cwd: PathBuf::from(record["cwd"].as_str().unwrap()),
                    cwd_entries: strings("cwd_entries"),
                    stdin: record["stdin"].as_str().unwrap().to_owned(),
                }
            })
            .collect()
    }

    /// The only run.
    pub fn run(&self) -> Run {
        let runs = self.runs();
        assert_eq!(runs.len(), 1, "{runs:?}");
        runs.into_iter().next().unwrap()
    }
}

/// Where a held fake connects: the test sees each process start, and end
/// when its connection closes.
pub struct Hold {
    listener: TcpListener,
    pub address: String,
}

impl Hold {
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        Self { listener, address }
    }

    /// Waits for the next held process.
    pub async fn started(&self) -> Process {
        Process(self.listener.accept().await.unwrap().0)
    }
}

/// A held process.
pub struct Process(TcpStream);

impl Process {
    /// Lets the process go on.
    pub async fn release(&mut self) {
        self.0.write_all(b"1").await.unwrap();
    }

    /// Waits until the process is gone: its connection closes.
    pub async fn ended(mut self) {
        let mut byte = [0_u8; 1];
        // Killed, the connection ends or is reset; never a byte.
        if let Ok(read) = self.0.read(&mut byte).await {
            assert_eq!(read, 0);
        }
    }
}
