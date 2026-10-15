//! Fixture repos for tests (QA-2) and the generator of the reference data of
//! spec section 9. A developer tool, not a product crate: product crates use
//! it as a dev-dependency only.
//!
//! Every repo write runs the `git` CLI with an argument array (ARCH-1,
//! ARCH-3); the generator feeds `git fast-import`.

mod fixture;
mod generate;
mod git;
mod rng;
mod sets;

pub use fixture::{FIXTURE_EMAIL, FIXTURE_NAME, Fixture, Repo};
pub use generate::{RepoSpec, generate_repo};
pub use sets::{
    GroupSpec, MANIFEST, Outcome, SET_NAMES, SetSpec, WORKSPACE, big_tree, clone_big_history,
    generate_set, many_repos, target_project,
};

/// Errors from the fixtures and the generator.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{args} failed with exit code {code:?}: {stderr}")]
    Git {
        args: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("cannot write JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cannot write TOML: {0}")]
    Toml(#[from] toml::ser::Error),
    #[error("{0}")]
    Invalid(String),
}
