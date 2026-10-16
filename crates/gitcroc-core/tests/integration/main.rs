//! The integration tests of `gitcroc-core`, one module per area. They are one
//! test binary so that it links once and `cargo test` runs all of them in
//! parallel, which matters most on Windows CI.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod ai_context;
mod ai_settings;
mod branch;
mod changes;
mod clone;
mod commit;
mod conflict;
mod fsmonitor;
mod group_jobs;
mod groups;
mod history;
mod integrate;
mod jobs;
mod remote;
mod reset;
mod settings;
mod share;
mod stage;
mod stash;
mod summaries;
mod sync;
mod tabs;
mod tag;
mod tree;
mod watch;
mod workspace;
mod workspaces;
