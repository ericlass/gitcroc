//! The git CLI runner (ARCH-3). Needs git on PATH; no network, no repo.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};

use gitcroc_git::{ASKPASS_VARS, Askpass, GitError, GitRunner, Kill, SearchEnv, Write, detect};

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
    let write = git.write_command(&args(&["commit"]), dir.path(), &Write::default());
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
        on_line: Some(&on_line),
        ..Write::default()
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
        ..Write::default()
    };
    let result = git
        .run_write(&args(&["init", "--quiet"]), dir.path(), write)
        .await;
    assert!(matches!(result, Err(GitError::Killed)), "{result:?}");
    assert!(!dir.path().join(".git").exists());
}

/// The askpass variables a command sets (`Some`) or removes (`None`); a
/// variable it leaves alone, which git would inherit, is missing.
fn askpass_vars<'a>(
    envs: impl Iterator<Item = (&'a OsStr, Option<&'a OsStr>)>,
) -> HashMap<String, Option<String>> {
    envs.filter(|(key, _)| ASKPASS_VARS.iter().any(|var| OsStr::new(var) == *key))
        .map(|(key, value)| {
            let value = value.map(|value| value.to_string_lossy().into_owned());
            (key.to_string_lossy().into_owned(), value)
        })
        .collect()
}

#[test]
fn d10_reads_get_no_askpass() {
    let git = GitRunner::new("git".into());
    let removed: HashMap<String, Option<String>> = ASKPASS_VARS
        .iter()
        .map(|var| ((*var).to_owned(), None))
        .collect();
    let read = args(&["status"]);
    let cwd = std::path::Path::new(".");

    // Reads, blocking or not, and writes that are not network writes: a
    // value inherited from the app's environment is removed.
    let async_read = git.command(&read, None);
    assert_eq!(askpass_vars(async_read.as_std().get_envs()), removed);
    let blocking_read = git.blocking_command(&read, None);
    assert_eq!(askpass_vars(blocking_read.get_envs()), removed);
    let local_write = git.write_command(&read, cwd, &Write::default());
    assert_eq!(askpass_vars(local_write.as_std().get_envs()), removed);

    // A network write gets the helper, and the variables it needs.
    let askpass = Askpass {
        program: "/opt/gitcroc/gitcroc".into(),
        env: vec![("GITCROC_ASKPASS_TOKEN".to_owned(), "token".to_owned())],
    };
    let network = Write {
        askpass: Some(&askpass),
        ..Write::default()
    };
    let network_write = git.write_command(&read, cwd, &network);
    let program = Some("/opt/gitcroc/gitcroc".to_owned());
    assert_eq!(
        askpass_vars(network_write.as_std().get_envs()),
        HashMap::from([
            ("GIT_ASKPASS".to_owned(), program.clone()),
            ("SSH_ASKPASS".to_owned(), program),
            ("SSH_ASKPASS_REQUIRE".to_owned(), Some("prefer".to_owned())),
        ])
    );
    assert!(
        network_write.as_std().get_envs().any(
            |(key, value)| key == "GITCROC_ASKPASS_TOKEN" && value == Some(OsStr::new("token"))
        )
    );
}

#[tokio::test]
async fn a_write_reads_its_stdin() {
    let git = runner().await;
    let dir = tempfile::tempdir().unwrap();
    let input = b"blob content\n";
    let write = Write {
        stdin: Some(input),
        ..Write::default()
    };
    let out = git
        .run_write(&args(&["hash-object", "--stdin"]), dir.path(), write)
        .await
        .unwrap();
    // `git hash-object` of the same bytes.
    assert_eq!(
        out.stdout,
        b"de2fd67c30b7f83c93682ed83ebff8a57a447cb5\n".to_vec()
    );
}

#[tokio::test]
async fn d9_a_read_only_runner_refuses_writes_before_starting_git() {
    let dir = tempfile::tempdir().unwrap();
    // A git that cannot start: a refusal must come before any start.
    let missing = GitRunner::new(dir.path().join("no-git")).read_only();
    assert!(missing.is_read_only());
    for write in [&["commit", "-m", "x"][..], &["-c", "a=b", "log"], &["log"]] {
        let error = missing
            .run_write(&args(write), dir.path(), Write::default())
            .await
            .unwrap_err();
        assert!(
            matches!(error, GitError::ReadOnly { .. }),
            "{write:?}: {error}"
        );
    }
    for write in [
        &["commit", "-m", "x"][..],
        &["-C", "log", "status"],
        &["--literal-pathspecs", "update-index", "--add"],
        &["-c", "core.editor=log", "rebase", "--continue"],
        &[],
    ] {
        let error = missing.run_blocking(&args(write), None).unwrap_err();
        assert!(
            matches!(error, GitError::ReadOnly { .. }),
            "{write:?}: {error}"
        );
        let error = missing.run(&args(write), None).await.unwrap_err();
        assert!(
            matches!(error, GitError::ReadOnly { .. }),
            "{write:?}: {error}"
        );
    }
    // A read gets as far as starting git.
    let error = missing
        .run_blocking(&args(&["--literal-pathspecs", "log", "-z"]), None)
        .unwrap_err();
    assert!(matches!(error, GitError::Spawn { .. }), "{error}");

    let git = runner().await.read_only();
    let output = git.run(&args(&["--version"]), None).await.unwrap();
    assert!(output.stdout.starts_with(b"git version"));
}
