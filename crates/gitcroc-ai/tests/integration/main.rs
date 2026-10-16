//! The integration tests of `gitcroc-ai`, one module per area, driven by
//! the mock provider and the recorded responses in `tests/responses`. No
//! test calls a real provider (section 12).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

mod ask;
mod mock;
mod responses;
mod split;
