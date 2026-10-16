//! The integration tests of `gitcroc-app`, one module per area. They are one
//! test binary so that it links once and `cargo test` runs all of them in
//! parallel, which matters most on Windows CI.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod ai;
mod ai_providers;
mod askpass;
mod build;
mod fonts;
mod ipc;
mod platform;
mod security;
mod start;
