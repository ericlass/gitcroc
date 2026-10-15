//! The system git the app uses (PLAT-3, D13): found at startup without
//! blocking anything (ARCH-7), held here, and checked again on request.

use std::path::Path;

use gitcroc_git::{DetectError, GitInstall, GitVersion, SearchEnv};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

/// Whether a usable git was found, as shown to the user. Paths and versions
/// are converted to text here, for display only (PLAT-2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GitStatus {
    /// Detection has not finished yet.
    Checking,
    /// git at `path` is new enough.
    Usable { path: String, version: String },
    /// No git at any of the `searched` paths.
    NotFound { searched: Vec<String> },
    /// git at `path` is version `found`; `required` or newer is needed.
    TooOld {
        path: String,
        found: String,
        required: String,
    },
    /// git at `path` did not report a version; `output` is what it printed or
    /// why it did not run.
    Unparseable { path: String, output: String },
}

impl GitStatus {
    /// Whether the app can run with this git.
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Usable { .. })
    }

    fn from_detection(detection: &Result<GitInstall, DetectError>) -> Self {
        let text = |path: &Path| path.display().to_string();
        match detection {
            Ok(git) => Self::Usable {
                path: text(&git.path),
                version: version_text(git.version),
            },
            Err(DetectError::NotFound { searched }) => Self::NotFound {
                searched: searched.iter().map(|p| text(p)).collect(),
            },
            Err(DetectError::TooOld {
                path,
                found,
                required,
            }) => Self::TooOld {
                path: text(path),
                found: version_text(*found),
                required: version_text(*required),
            },
            Err(DetectError::Unparseable { path, output }) => Self::Unparseable {
                path: text(path),
                output: output.clone(),
            },
        }
    }
}

fn version_text((major, minor, patch): GitVersion) -> String {
    format!("{major}.{minor}.{patch}")
}

/// Holds the latest detection result for the whole app.
#[derive(Debug, Default)]
pub struct GitSetup {
    /// `None` while the first detection runs.
    detection: RwLock<Option<Result<GitInstall, DetectError>>>,
    /// Runs one detection at a time, so an older one cannot overwrite a newer.
    detecting: Mutex<()>,
}

impl GitSetup {
    pub fn new() -> Self {
        Self::default()
    }

    /// The latest result, or [`GitStatus::Checking`] before the first one.
    pub async fn git_status(&self) -> GitStatus {
        match &*self.detection.read().await {
            None => GitStatus::Checking,
            Some(detection) => GitStatus::from_detection(detection),
        }
    }

    /// The usable git, if one was found.
    pub async fn git(&self) -> Option<GitInstall> {
        self.detection.read().await.clone()?.ok()
    }

    /// Detects git in this process's environment and keeps the result.
    /// Until it finishes, [`git_status`](Self::git_status) reports
    /// [`GitStatus::Checking`].
    pub async fn detect(&self) -> GitStatus {
        self.detect_with(&SearchEnv::from_process()).await
    }

    async fn detect_with(&self, env: &SearchEnv) -> GitStatus {
        let _one_at_a_time = self.detecting.lock().await;
        *self.detection.write().await = None;
        let detection = gitcroc_git::detect(None, env).await;
        let status = GitStatus::from_detection(&detection);
        *self.detection.write().await = Some(detection);
        status
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    #[tokio::test]
    async fn plat_3_core_holds_the_detection_result() {
        let setup = GitSetup::new();
        assert_eq!(setup.git_status().await, GitStatus::Checking);
        assert_eq!(setup.git().await, None);

        let usable = setup.detect().await;
        assert!(usable.is_usable(), "{usable:?}");
        assert_eq!(setup.git_status().await, usable);
        assert!(setup.git().await.is_some());

        let empty = SearchEnv {
            path: Some(OsString::new()),
            ..SearchEnv::default()
        };
        let missing = setup.detect_with(&empty).await;
        assert_eq!(missing, GitStatus::NotFound { searched: vec![] });
        assert_eq!(setup.git_status().await, missing);
        assert_eq!(setup.git().await, None);
    }

    #[test]
    fn plat_3_status_shows_versions_as_text() {
        let status = GitStatus::from_detection(&Err(DetectError::TooOld {
            path: "/usr/bin/git".into(),
            found: (2, 30, 0),
            required: (2, 40, 0),
        }));
        assert_eq!(
            status,
            GitStatus::TooOld {
                path: Path::new("/usr/bin/git").display().to_string(),
                found: "2.30.0".to_owned(),
                required: "2.40.0".to_owned(),
            }
        );
    }
}
