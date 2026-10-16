//! A stand-in for the Claude Code and Codex CLIs (M4-04), built for the
//! tests only. The tests link or copy it into a folder of their own, next to
//! a `script.json` that says how to answer. Every run that is not a version,
//! help or feature query is appended to `records.jsonl` in that folder: its
//! arguments, working directory, what that directory held and its stdin.
//!
//! A script may hold the run on a TCP connection to the test, which releases
//! it by sending a byte; the test sees the process end when the connection
//! closes. No timing.

// Test code (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};

/// The environment variable that turns a copy into the held child.
const CHILD: &str = "GITCROC_FAKE_CLI_CHILD";

fn main() {
    if let Ok(address) = std::env::var(CHILD) {
        // A child left behind by the CLI: it holds a connection until killed.
        let mut stream = TcpStream::connect(address).unwrap();
        let _ = stream.read(&mut [0_u8; 1]);
        return;
    }
    // The path it was started by, not the program file: on Unix the tests
    // link to the program, which stays in the build folder.
    let started_as = PathBuf::from(std::env::args_os().next().unwrap());
    let folder = started_as.parent().unwrap().to_owned();
    let script: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("script.json")).unwrap())
            .unwrap();
    let text = |name: &str| script[name].as_str().unwrap_or_default().to_owned();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let query = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["--version"] => Some("version"),
        ["--help"] => Some("help"),
        ["exec", "--help"] => Some("exec_help"),
        ["features", "list"] => Some("features"),
        _ => None,
    };
    if let Some(query) = query {
        print!("{}", text(query));
        return;
    }

    let mut stdin = String::new();
    std::io::stdin().read_to_string(&mut stdin).unwrap();
    let cwd = std::env::current_dir().unwrap();
    let mut entries: Vec<String> = std::fs::read_dir(&cwd)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    entries.sort();
    let record = serde_json::json!({
        "args": args,
        "cwd": cwd,
        "cwd_entries": entries,
        "stdin": stdin,
    });
    let mut records = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(folder.join("records.jsonl"))
        .unwrap();
    writeln!(records, "{record}").unwrap();
    drop(records);

    if let Some(address) = script["hold"].as_str() {
        if script["hold_child"].as_bool() == Some(true) {
            hold_child(&std::env::current_exe().unwrap(), address);
        }
        let mut stream = TcpStream::connect(address).unwrap();
        // Released by a byte; a closed connection releases too.
        let _ = stream.read(&mut [0_u8; 1]);
    }

    print!("{}", text("stdout"));
    eprint!("{}", text("stderr"));
    std::io::stdout().flush().unwrap();
    let code = script["exit"].as_i64().unwrap_or(0);
    std::process::exit(i32::try_from(code).unwrap());
}

/// Starts a copy of this program that holds its own connection, as a CLI's
/// helper process would, without waiting for it.
#[expect(
    clippy::zombie_processes,
    reason = "the child is left behind on purpose; the provider kills it"
)]
fn hold_child(exe: &Path, address: &str) {
    std::process::Command::new(exe)
        .env(CHILD, address)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
}
