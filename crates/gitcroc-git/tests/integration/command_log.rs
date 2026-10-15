//! Every git process a runner starts is reported to the command log, with
//! its arguments, working directory, duration and exit code (section 10).

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gitcroc_git::{Exit, GitRunner, SearchEnv, detect, log_commands_to};

/// What the log got: arguments, working directory and exit.
type Logged = Vec<(Vec<OsString>, Option<PathBuf>, Exit)>;

#[tokio::test]
async fn section_10_every_git_process_is_logged() {
    let logged: Arc<Mutex<Logged>> = Arc::default();
    let sink = logged.clone();
    assert!(log_commands_to(move |record| {
        sink.lock().unwrap().push((
            record.args.to_vec(),
            record.cwd.map(ToOwned::to_owned),
            record.exit,
        ));
    }));

    // Detection runs `git --version` too.
    let git = detect(None, &SearchEnv::from_process()).await.unwrap();
    let git = GitRunner::new(git.path);
    let dir = tempfile::tempdir().unwrap();
    let rev_parse: Vec<OsString> = ["rev-parse", "--git-dir"].map(Into::into).into();
    assert!(git.run(&rev_parse, Some(dir.path())).await.is_err());
    let status: Vec<OsString> = ["status"].map(Into::into).into();
    assert!(git.run_blocking(&status, Some(dir.path())).is_err());

    let logged = logged.lock().unwrap();
    let version: Vec<OsString> = vec!["--version".into()];
    assert!(
        logged.contains(&(version, None, Exit::Code(0))),
        "{logged:?}"
    );
    assert!(logged.contains(&(rev_parse, Some(dir.path().to_owned()), Exit::Code(128))));
    assert!(logged.contains(&(status, Some(dir.path().to_owned()), Exit::Code(128))));
    // Set once per process.
    assert!(!log_commands_to(|_| {}));
}
