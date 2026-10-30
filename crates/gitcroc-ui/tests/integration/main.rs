//! The integration tests of `gitcroc-ui`, one module per area: headless,
//! on GPUI's test platform, with no display and no GPU (section 12). They
//! are one test binary so that it links once.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

// GPUI's test macro names the crate `gpui`.
extern crate gpui_kit as gpui;

mod askpass;
mod background_tabs;
mod branches;
mod changes;
mod colors;
mod commit;
mod conflict;
mod conflict_view;
mod diff;
mod git_setup;
mod history;
mod history_merged;
mod history_search;
mod jobs;
mod operations;
mod panes;
mod perf;
mod stage;
mod strings;
mod support;
mod tabs;
mod theme;
mod title_bar;
mod tree;
mod update;
mod window;
