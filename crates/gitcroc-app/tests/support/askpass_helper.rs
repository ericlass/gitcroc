//! Git's askpass helper for the askpass tests (D10): what the app's binary
//! does first when git starts it as its helper. A copy with a file
//! `<program>.record` beside it writes its exit code to `<program>.exit`,
//! so the quit test sees that it ended, and how.

use std::ffi::OsString;
use std::path::PathBuf;

use gitcroc_app::askpass::helper;
use gitcroc_app::platform::PlatformSockets;

fn main() {
    let code = helper::<PlatformSockets>().unwrap_or(1);
    if let Ok(program) = std::env::current_exe()
        && beside(&program, ".record").exists()
    {
        let _ = std::fs::write(beside(&program, ".exit"), code.to_string());
    }
    std::process::exit(code);
}

/// `program` with `suffix` added to its name.
fn beside(program: &std::path::Path, suffix: &str) -> PathBuf {
    let mut name = OsString::from(program);
    name.push(suffix);
    PathBuf::from(name)
}
