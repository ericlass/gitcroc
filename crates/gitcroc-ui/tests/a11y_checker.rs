//! The accessibility tree checker's own tests (M6-25a). They need no
//! display; the harness that uses the checker, `tests/a11y_tree`, has no
//! test harness of its own to run them.

// Test code (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

#[allow(dead_code)]
#[path = "a11y_tree/checker.rs"]
mod checker;
