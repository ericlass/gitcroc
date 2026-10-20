//! The system git the app uses (PLAT-3, D13): found at startup without
//! blocking anything (ARCH-7), held here, and checked again on request.

use std::path::{Path, PathBuf};

use gitcroc_git::{DetectError, GitInstall, GitVersion, SearchEnv};
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock};

/// Whether a usable git was found, as shown to the user. Paths and versions
/// are converted to text here, for display only (PLAT-2). Every result says
/// which git was chosen in Settings, if one was (`chosen`), and whether that
/// path had no executable, so the search went on without it
/// (`chosen_missing`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GitStatus {
    /// Detection has not finished yet.
    Checking,
    /// git at `path` is new enough.
    Usable {
        path: String,
        version: String,
        chosen: Option<String>,
        chosen_missing: bool,
    },
    /// No git at any of the `searched` paths.
    NotFound {
        searched: Vec<String>,
        chosen: Option<String>,
        chosen_missing: bool,
    },
    /// git at `path` is version `found`; `required` or newer is needed.
    TooOld {
        path: String,
        found: String,
        required: String,
        chosen: Option<String>,
        chosen_missing: bool,
    },
    /// git at `path` did not report a version; `output` is what it printed or
    /// why it did not run.
    Unparseable {
        path: String,
        output: String,
        chosen: Option<String>,
        chosen_missing: bool,
    },
}

impl GitStatus {
    /// Whether the app can run with this git.
    pub fn is_usable(&self) -> bool {
        matches!(self, Self::Usable { .. })
    }

    fn from_detection(detection: &Detection) -> Self {
        let text = |path: &Path| path.display().to_string();
        let chosen = detection.chosen.as_deref();
        // The chosen git is tried first, so a git found elsewhere means the
        // chosen path had none.
        let missing = |found: &Path| chosen.is_some_and(|chosen| chosen != found);
        let chosen_text = chosen.map(text);
        match &detection.result {
            Ok(git) => Self::Usable {
                path: text(&git.path),
                version: version_text(git.version),
                chosen: chosen_text,
                chosen_missing: missing(&git.path),
            },
            Err(DetectError::NotFound { searched }) => Self::NotFound {
                searched: searched.iter().map(|p| text(p)).collect(),
                chosen: chosen_text,
                chosen_missing: chosen.is_some(),
            },
            Err(DetectError::TooOld {
                path,
                found,
                required,
            }) => Self::TooOld {
                path: text(path),
                found: version_text(*found),
                required: version_text(*required),
                chosen: chosen_text,
                chosen_missing: missing(path),
            },
            Err(DetectError::Unparseable { path, output }) => Self::Unparseable {
                path: text(path),
                output: output.clone(),
                chosen: chosen_text,
                chosen_missing: missing(path),
            },
        }
    }
}

fn version_text((major, minor, patch): GitVersion) -> String {
    format!("{major}.{minor}.{patch}")
}

/// One detection: the git path chosen in Settings, if any, and what was found.
#[derive(Debug, Clone)]
struct Detection {
    chosen: Option<PathBuf>,
    result: Result<GitInstall, DetectError>,
}

/// Holds the latest detection result for the whole app.
#[derive(Debug, Default)]
pub struct GitSetup {
    /// `None` while the first detection runs.
    detection: RwLock<Option<Detection>>,
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
        self.detection.read().await.clone()?.result.ok()
    }

    /// Detects git in this process's environment and keeps the result.
    /// Until it finishes, [`git_status`](Self::git_status) reports
    /// [`GitStatus::Checking`].
    pub async fn detect(&self) -> GitStatus {
        self.detect_chosen(None).await
    }

    /// As [`detect`](Self::detect), trying `chosen`, the git path from
    /// Settings, first. A chosen path with no executable is passed over and
    /// the search goes on, as M0-06 made it; the result says so.
    pub async fn detect_chosen(&self, chosen: Option<&Path>) -> GitStatus {
        self.detect_with(chosen, &SearchEnv::from_process()).await
    }

    async fn detect_with(&self, chosen: Option<&Path>, env: &SearchEnv) -> GitStatus {
        let _one_at_a_time = self.detecting.lock().await;
        *self.detection.write().await = None;
        let detection = Detection {
            chosen: chosen.map(Path::to_path_buf),
            result: gitcroc_git::detect(chosen, env).await,
        };
        let status = GitStatus::from_detection(&detection);
        *self.detection.write().await = Some(detection);
        status
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsString;

    use super::*;

    fn empty_env() -> SearchEnv {
        SearchEnv {
            path: Some(OsString::new()),
            ..SearchEnv::default()
        }
    }

    /// The git a search without a chosen path finds: the real one on `PATH`.
    /// A fake cannot stand in for it, since on Windows the search looks for
    /// `git.exe`.
    async fn git_on_path() -> GitInstall {
        gitcroc_git::detect(None, &SearchEnv::from_process())
            .await
            .unwrap()
    }

    fn text(path: &Path) -> String {
        path.display().to_string()
    }

    /// Writes a script into `dir` that prints `output` like `git --version`
    /// would (QA-2), as gitcroc-git's detection tests do.
    fn fake_git(dir: &Path, output: &str) -> PathBuf {
        #[cfg(windows)]
        {
            let path = dir.join("git.cmd");
            std::fs::write(&path, format!("@echo {output}\r\n")).unwrap();
            path
        }
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            let path = dir.join("git");
            std::fs::write(&path, format!("#!/bin/sh\necho '{output}'\n")).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            wait_until_runnable(&path);
            path
        }
    }

    /// A child forked by another test thread while the script was open for
    /// writing holds that handle until it execs, and running the script
    /// fails with "Text file busy" meanwhile; one successful run means no
    /// such handle is left.
    #[cfg(not(windows))]
    fn wait_until_runnable(path: &Path) {
        for _ in 0..1000 {
            match std::process::Command::new(path).output() {
                Err(error) if error.raw_os_error() == Some(26) => {
                    std::thread::yield_now();
                }
                result => {
                    result.unwrap();
                    return;
                }
            }
        }
        panic!("{} stays busy", path.display());
    }

    #[tokio::test]
    async fn plat_3_core_holds_the_detection_result() {
        let setup = GitSetup::new();
        assert_eq!(setup.git_status().await, GitStatus::Checking);
        assert_eq!(setup.git().await, None);

        let usable = setup.detect().await;
        assert!(usable.is_usable(), "{usable:?}");
        assert_eq!(setup.git_status().await, usable);
        assert!(setup.git().await.is_some());

        let missing = setup.detect_with(None, &empty_env()).await;
        assert_eq!(
            missing,
            GitStatus::NotFound {
                searched: vec![],
                chosen: None,
                chosen_missing: false,
            }
        );
        assert_eq!(setup.git_status().await, missing);
        assert_eq!(setup.git().await, None);
    }

    #[tokio::test]
    async fn plat_3_chosen_git_is_used_first() {
        let chosen_dir = tempfile::tempdir().unwrap();
        let chosen = fake_git(chosen_dir.path(), "git version 2.47.1");

        let setup = GitSetup::new();
        let status = setup
            .detect_with(Some(&chosen), &SearchEnv::from_process())
            .await;
        assert_eq!(
            status,
            GitStatus::Usable {
                path: text(&chosen),
                version: "2.47.1".to_owned(),
                chosen: Some(text(&chosen)),
                chosen_missing: false,
            }
        );
        assert_eq!(setup.git().await.map(|git| git.path), Some(chosen));
    }

    #[tokio::test]
    async fn plat_3_chosen_git_too_old_is_reported() {
        // The new enough git on PATH does not stand in for a chosen one.
        let chosen_dir = tempfile::tempdir().unwrap();
        let chosen = fake_git(chosen_dir.path(), "git version 2.39.5");

        let setup = GitSetup::new();
        let status = setup
            .detect_with(Some(&chosen), &SearchEnv::from_process())
            .await;
        assert_eq!(
            status,
            GitStatus::TooOld {
                path: text(&chosen),
                found: "2.39.5".to_owned(),
                required: "2.40.0".to_owned(),
                chosen: Some(text(&chosen)),
                chosen_missing: false,
            }
        );
        assert_eq!(setup.git().await, None);

        let unreadable = tempfile::tempdir().unwrap();
        let chosen = fake_git(unreadable.path(), "not git at all");
        let status = setup
            .detect_with(Some(&chosen), &SearchEnv::from_process())
            .await;
        assert_eq!(
            status,
            GitStatus::Unparseable {
                path: text(&chosen),
                output: "not git at all".to_owned(),
                chosen: Some(text(&chosen)),
                chosen_missing: false,
            }
        );
    }

    #[tokio::test]
    async fn plat_3_missing_chosen_git_falls_through_and_says_so() {
        let found = git_on_path().await;
        let gone = tempfile::tempdir().unwrap();
        let chosen = gone.path().join("bin").join("git");

        let setup = GitSetup::new();
        let status = setup
            .detect_with(Some(&chosen), &SearchEnv::from_process())
            .await;
        assert_eq!(
            status,
            GitStatus::Usable {
                path: text(&found.path),
                version: version_text(found.version),
                chosen: Some(text(&chosen)),
                chosen_missing: true,
            }
        );
        assert_eq!(setup.git().await, Some(found));

        // A folder is not an executable either.
        let status = setup
            .detect_with(Some(gone.path()), &SearchEnv::from_process())
            .await;
        assert!(
            matches!(
                status,
                GitStatus::Usable {
                    chosen_missing: true,
                    ..
                }
            ),
            "{status:?}"
        );

        // With nothing else to find, the chosen path heads the list.
        let status = setup.detect_with(Some(&chosen), &empty_env()).await;
        assert_eq!(
            status,
            GitStatus::NotFound {
                searched: vec![text(&chosen)],
                chosen: Some(text(&chosen)),
                chosen_missing: true,
            }
        );
    }

    #[tokio::test]
    async fn plat_3_clearing_the_path_detects_again() {
        let found = git_on_path().await;
        let chosen_dir = tempfile::tempdir().unwrap();
        let chosen = fake_git(chosen_dir.path(), "git version 2.30.0");
        let env = SearchEnv::from_process();

        let setup = GitSetup::new();
        let status = setup.detect_with(Some(&chosen), &env).await;
        assert!(!status.is_usable(), "{status:?}");
        let status = setup.detect_with(None, &env).await;
        assert_eq!(
            status,
            GitStatus::Usable {
                path: text(&found.path),
                version: version_text(found.version),
                chosen: None,
                chosen_missing: false,
            }
        );
        assert_eq!(setup.git_status().await, status);
        assert_eq!(setup.git().await, Some(found));
    }

    #[test]
    fn plat_3_status_shows_versions_as_text() {
        let status = GitStatus::from_detection(&Detection {
            chosen: None,
            result: Err(DetectError::TooOld {
                path: "/usr/bin/git".into(),
                found: (2, 30, 0),
                required: (2, 40, 0),
            }),
        });
        assert_eq!(
            status,
            GitStatus::TooOld {
                path: Path::new("/usr/bin/git").display().to_string(),
                found: "2.30.0".to_owned(),
                required: "2.40.0".to_owned(),
                chosen: None,
                chosen_missing: false,
            }
        );
    }
}
