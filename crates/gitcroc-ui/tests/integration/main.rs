//! The integration tests of `gitcroc-ui`, one module per area: headless,
//! on GPUI's test platform, with no display and no GPU (section 12). They
//! are one test binary so that it links once.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

// GPUI's test macro names the crate `gpui`.
extern crate gpui_kit as gpui;

mod ai_resolve;
mod ai_resolve_all;
mod ai_settings;
mod askpass;
mod background_tabs;
mod branches;
mod changes;
mod clone;
mod colors;
mod commit;
mod conflict;
mod conflict_banner;
mod conflict_cards;
mod conflict_flow;
mod conflict_lifecycle;
mod conflict_view;
mod diff;
mod git_setup;
mod group_commit;
mod group_names;
mod group_ops;
mod groups;
mod history;
mod history_merged;
mod history_search;
mod jobs;
mod keyboard;
mod labels;
mod operations;
mod palette;
mod palette_ops;
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
