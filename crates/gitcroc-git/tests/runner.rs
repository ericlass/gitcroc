//! The git CLI runner (ARCH-3). Needs git on PATH; no network, no repo.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};

use gitcroc_git::{GitError, GitRunner, Kill, SearchEnv, Write, detect};

async fn runner() -> GitRunner {
    let git = detect(None, &SearchEnv::from_process())
        .await
        .expect("git 2.40 or newer on PATH");
    GitRunner::new(git.path)
}

fn args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

#[tokio::test]
async fn arch_3_runner_passes_arguments_without_a_shell() {
    let git = runner().await;
    let out = git.run(&args(&["--version"]), None).await.unwrap();
    assert!(out.stdout.starts_with(b"git version "), "{out:?}");

    // A shell would split this, expand the variables, or run `echo`.
    let value = r#"two  words 'single' "double" ; echo pwned & $HOME %PATH% | `id`"#;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config");
    let mut set = vec![OsString::from("config"), OsString::from("--file")];
    set.push(file.clone().into_os_string());
    let mut get = set.clone();
    set.extend(args(&["test.value", value]));
    get.extend(args(&["--get", "test.value"]));

    git.run(&set, None).await.unwrap();
    let out = git.run(&get, None).await.unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), format!("{value}\n"));
}

#[tokio::test]
async fn arch_3_runner_sets_c_locale_and_disables_prompts() {
    let git = runner().await;
    let command = git.command(&args(&["status"]), None);
    let env: HashMap<&OsStr, Option<&OsStr>> = command.as_std().get_envs().collect();
    assert_eq!(env.get(OsStr::new("LC_ALL")), Some(&Some(OsStr::new("C"))));
    assert_eq!(
        env.get(OsStr::new("GIT_TERMINAL_PROMPT")),
        Some(&Some(OsStr::new("0")))
    );
    assert_eq!(command.as_std().get_program(), git.git().as_os_str());
}

#[tokio::test]
async fn runner_reports_exit_code_and_stderr() {
    let git = runner().await;
    let dir = tempfile::tempdir().unwrap();
    let err = git
        .run(&args(&["rev-parse", "--git-dir"]), Some(dir.path()))
        .await
        .unwrap_err();
    match err {
        GitError::Failed { code, stderr, .. } => {
            assert_eq!(code, Some(128));
            // LC_ALL=C keeps the message in English.
            let stderr = String::from_utf8(stderr).unwrap();
            assert!(stderr.contains("not a git repository"), "{stderr}");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[tokio::test]
async fn writes_never_wait_on_an_editor() {
    let git = runner().await;
    let dir = tempfile::tempdir().unwrap();
    let editor = |command: &tokio::process::Command| {
        command
            .as_std()
            .get_envs()
            .find(|(name, _)| *name == OsStr::new("GIT_EDITOR"))
            .and_then(|(_, value)| value.map(OsStr::to_owned))
    };
    let write = git.write_command(&args(&["commit"]), dir.path());
    assert_eq!(editor(&write), Some(OsString::from("true")));
    let env: HashMap<&OsStr, Option<&OsStr>> = write.as_std().get_envs().collect();
    assert_eq!(env.get(OsStr::new("LC_ALL")), Some(&Some(OsStr::new("C"))));
    // Reads are unchanged.
    assert_eq!(editor(&git.command(&args(&["status"]), None)), None);
}

#[tokio::test]
async fn a_write_sends_stderr_lines_while_git_runs_and_keeps_all_output() {
    let git = runner().await;
    let dir = tempfile::tempdir().unwrap();
    let lines = std::sync::Mutex::new(Vec::new());
    let on_line = |line: &str| lines.lock().unwrap().push(line.to_owned());
    let write = Write {
        kill: None,
        on_line: Some(&on_line),
    };
    let error = git
        .run_write(&args(&["rev-parse", "--git-dir"]), dir.path(), write)
        .await
        .unwrap_err();
    let GitError::Failed { code, stderr, .. } = error else {
        panic!("expected Failed, got {error:?}");
    };
    assert_eq!(code, Some(128));
    let stderr = String::from_utf8(stderr).unwrap();
    let lines = lines.into_inner().unwrap();
    assert_eq!(lines.concat(), stderr.replace('\n', ""));
    assert!(lines[0].contains("not a git repository"), "{lines:?}");
}

#[tokio::test]
async fn grp_10_a_write_killed_before_it_starts_never_runs() {
    let git = runner().await;
    let dir = tempfile::tempdir().unwrap();
    let kill = Kill::new();
    kill.kill();
    let write = Write {
        kill: Some(&kill),
        on_line: None,
    };
    let result = git
        .run_write(&args(&["init", "--quiet"]), dir.path(), write)
        .await;
    assert!(matches!(result, Err(GitError::Killed)), "{result:?}");
    assert!(!dir.path().join(".git").exists());
}
