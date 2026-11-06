//! M6-25a: the accessibility tree of a real window. GPUI builds its
//! AccessKit tree only while assistive technology is active, and its test
//! platform never activates it, so this test opens `gitcroc`'s real
//! window on the real core and `repo-gen` fixtures (QA-2), under Xvfb with
//! a D-Bus session and an enabled AT-SPI bus of its own (`session.sh`). It
//! reaches each of M5-10's views by the app's own actions and keys, writes
//! each view's tree, and checks it with [`checker`]. It fails on the first
//! view that breaks a rule, naming the node and the rule.
//!
//!     just a11y
//!
//! Outside that session (`GITCROC_A11Y` unset, or not Linux) it prints
//! that it skipped and passes, so `cargo test` stays headless (section 12).

// Test code (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[cfg(target_os = "linux")]
mod atspi;
#[cfg(target_os = "linux")]
mod checker;
#[cfg(target_os = "linux")]
mod driver;
#[cfg(target_os = "linux")]
#[allow(dead_code)]
#[path = "../integration/support.rs"]
mod support;
#[cfg(target_os = "linux")]
mod views;

use std::process::ExitCode;

fn main() -> ExitCode {
    #[cfg(target_os = "linux")]
    if std::env::var_os("GITCROC_A11Y").is_some() {
        return views::run();
    }
    println!("a11y_tree: skipped; `just a11y` runs it on Linux under Xvfb with an AT-SPI bus");
    ExitCode::SUCCESS
}
