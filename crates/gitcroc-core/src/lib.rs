//! Workspace, group and repo model, job engine, preflight, result reports, config and cache.

/// Errors returned by `gitcroc-core`.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

// Deliberately failing test to show CI going red (M0-03 acceptance); reverted
// in the next commit.
#[cfg(test)]
mod ci_red_green {
    #[test]
    fn deliberately_fails() {
        assert_eq!(1, 2);
    }
}
