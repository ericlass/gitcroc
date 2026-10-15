//! The integration tests of `gitcroc-git`, one module per area. They are one
//! test binary so that it links once and `cargo test` runs all of them in
//! parallel, which matters most on Windows CI.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod command_log;
mod conflict;
mod detect;
mod diff;
mod fixtures;
mod history;
mod ops_matrix;
mod refs;
mod runner;
mod status;
mod writes;
