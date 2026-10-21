//! The integration tests of `gitcroc-cli`, one module per area. They are one
//! test binary so that it links once and `cargo test` runs all of them in
//! parallel, which matters most on Windows CI.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod ai;
mod bench;
mod cli;
mod conflict;
mod diff;
mod group_branches;
mod group_commit;
mod group_integrate;
mod group_run;
mod group_sync;
mod group_world;
mod history;
mod quality_gates;
mod status;
mod user_docs;
mod workspace;
mod workspace_policy;
