//! Finding git and checking its version (PLAT-3). Fake gits are scripts
//! written in code into a temp dir (QA-2).

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gitcroc_git::{DetectError, GitVersion, MIN_GIT, SearchEnv, candidates, detect, parse_version};

/// Writes a script that prints `output` like `git --version` would.
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
/// writing holds that handle until it execs, and running the script fails
/// with "Text file busy" meanwhile. Once one run succeeds, no such handle
/// is left and none can appear, since ours is closed.
#[cfg(not(windows))]
fn wait_until_runnable(path: &Path) {
    for _ in 0..1000 {
        match std::process::Command::new(path).output() {
            Err(error) if error.raw_os_error() == Some(26) => {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            result => {
                result.unwrap();
                return;
            }
        }
    }
    panic!("{} stays busy", path.display());
}

fn empty_env() -> SearchEnv {
    SearchEnv {
        path: Some(OsString::new()),
        ..SearchEnv::default()
    }
}

#[test]
fn plat_3_version_parsing() {
    let cases: &[(&str, Option<GitVersion>)] = &[
        ("git version 2.43.0", Some((2, 43, 0))),
        ("git version 2.43.0\n", Some((2, 43, 0))),
        ("git version 2.47.1.windows.2", Some((2, 47, 1))),
        ("git version 2.39.5 (Apple Git-154)", Some((2, 39, 5))),
        ("git version 2.48.0.rc1", Some((2, 48, 0))),
        ("git version 10.0.12", Some((10, 0, 12))),
        ("", None),
        ("git version", None),
        ("git version 2.43", None),
        ("git version two.43.0", None),
        ("git version 2.43.x", None),
        ("git version +2.43.0", None),
        ("version 2.43.0", None),
        ("hub version 2.43.0", None),
        ("git: command not found", None),
        ("git version 99999999999.0.0", None),
    ];
    for (output, expected) in cases {
        assert_eq!(parse_version(output), *expected, "{output:?}");
    }
    assert!(parse_version("git version 2.39.5").unwrap() < MIN_GIT);
    assert!(parse_version("git version 2.40.0").unwrap() >= MIN_GIT);
}

#[tokio::test]
async fn plat_3_detects_git_on_path() {
    let env = SearchEnv::from_process();
    let git = detect(None, &env).await.unwrap();
    assert!(git.version >= MIN_GIT, "{git:?}");
    let dirs: Vec<PathBuf> = std::env::split_paths(env.path.as_ref().unwrap()).collect();
    assert!(
        dirs.iter()
            .any(|dir| git.path.parent() == Some(dir.as_path())),
        "{} is not in a PATH directory",
        git.path.display()
    );
}

#[tokio::test]
async fn plat_3_reports_not_found() {
    assert_eq!(
        detect(None, &empty_env()).await,
        Err(DetectError::NotFound { searched: vec![] })
    );

    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("no-git-here");
    let env = SearchEnv {
        path: Some(dir.path().as_os_str().to_owned()),
        ..SearchEnv::default()
    };
    let Err(DetectError::NotFound { searched }) = detect(Some(&missing), &env).await else {
        panic!("expected NotFound");
    };
    assert_eq!(searched[0], missing);
    assert_eq!(searched.len(), 2);
}

#[tokio::test]
async fn plat_3_reports_too_old() {
    let dir = tempfile::tempdir().unwrap();
    let git = fake_git(dir.path(), "git version 2.30.0");
    assert_eq!(
        detect(Some(&git), &empty_env()).await,
        Err(DetectError::TooOld {
            path: git,
            found: (2, 30, 0),
            required: MIN_GIT,
        })
    );
}

#[tokio::test]
async fn plat_3_explicit_path_comes_before_path() {
    // The real git on PATH is new enough; the explicit one wins anyway.
    let dir = tempfile::tempdir().unwrap();
    let git = fake_git(dir.path(), "git version 2.39.5 (Apple Git-154)");
    let result = detect(Some(&git), &SearchEnv::from_process()).await;
    assert!(
        matches!(&result, Err(DetectError::TooOld { path, .. }) if *path == git),
        "{result:?}"
    );
}

#[tokio::test]
async fn plat_3_reports_unparseable_output() {
    let dir = tempfile::tempdir().unwrap();
    let git = fake_git(dir.path(), "not git at all");
    assert_eq!(
        detect(Some(&git), &empty_env()).await,
        Err(DetectError::Unparseable {
            path: git,
            output: "not git at all".to_owned(),
        })
    );
}

#[test]
fn plat_3_search_order() {
    let env = SearchEnv {
        path: Some(std::env::join_paths(["/a", "", "/b"]).unwrap()),
        program_files: Some("C:\\PF".into()),
        program_files_x86: Some("C:\\PF86".into()),
        local_app_data: Some(OsString::new()),
        homebrew_prefixes: vec!["/brew".into()],
    };
    let exe = if cfg!(windows) { "git.exe" } else { "git" };
    let mut expected = vec![
        PathBuf::from("/settings/git"),
        Path::new("/a").join(exe),
        Path::new("/b").join(exe),
    ];
    if cfg!(windows) {
        // An empty %LocalAppData% is skipped.
        expected.push(PathBuf::from("C:\\PF\\Git\\cmd\\git.exe"));
        expected.push(PathBuf::from("C:\\PF86\\Git\\cmd\\git.exe"));
    }
    expected.push(Path::new("/brew").join("bin").join("git"));
    assert_eq!(candidates(Some(Path::new("/settings/git")), &env), expected);
}

#[test]
fn plat_3_searches_homebrew_on_macos_only() {
    let prefixes = SearchEnv::from_process().homebrew_prefixes;
    if cfg!(target_os = "macos") {
        assert_eq!(
            prefixes,
            [PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")]
        );
    } else {
        assert!(prefixes.is_empty(), "{prefixes:?}");
    }
}
