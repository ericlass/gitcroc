//! The askpass helper (D10): git's prompts reach the user through the
//! running app and the answer goes to git and nowhere else. These run the
//! real `gitcroc` executable as git's helper, against a fake dialog. Nothing
//! here waits for a time.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use gitcroc_app::askpass::{
    ADDRESS_VAR, Answer, AskpassPrompt, AskpassServer, AskpassUi, PromptKind, TOKEN_VAR,
};
use gitcroc_app::platform::{LocalSockets, PlatformSockets};
use gitcroc_core::{
    AppLog, AskpassIssuer, ConfigDir, GitRunner, JobEvents, JobState, JobView, Jobs, LogDir,
    NothingToDo, OnCancel, Outcome, SearchEnv, Summaries, SummaryCache, SummaryView, WorkspaceId,
    Workspaces, WriteJob, add_path,
};
use gitcroc_git::{Write, detect};

const PASSWORD: &str = "s3cret-for-askpass";

/// Answers each prompt with the first answer whose key the prompt
/// contains; `None` cancels. Keeps every prompt it was shown.
struct FakeUi {
    answers: Vec<(&'static str, Option<&'static str>)>,
    asked: Mutex<Vec<AskpassPrompt>>,
}

impl FakeUi {
    fn new(answers: Vec<(&'static str, Option<&'static str>)>) -> Arc<Self> {
        Arc::new(Self {
            answers,
            asked: Mutex::default(),
        })
    }

    fn asked(&self) -> Vec<AskpassPrompt> {
        self.asked.lock().unwrap().clone()
    }
}

impl AskpassUi for FakeUi {
    fn ask(&self, prompt: AskpassPrompt) -> Answer<'_> {
        let answer = self
            .answers
            .iter()
            .find(|(key, _)| prompt.text.contains(key))
            .and_then(|(_, answer)| answer.map(str::to_owned));
        self.asked.lock().unwrap().push(prompt);
        Box::pin(async move { answer })
    }
}

/// The `gitcroc` executable, which git starts as its helper.
fn helper() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_gitcroc"))
}

async fn git() -> GitRunner {
    let git = detect(None, &SearchEnv::from_process())
        .await
        .expect("git 2.40 or newer on PATH");
    GitRunner::new(git.path)
}

fn server(ui: Arc<FakeUi>) -> Arc<AskpassServer> {
    AskpassServer::start(PlatformSockets::listen().unwrap(), helper(), ui)
}

fn job_view(label: &str) -> JobView {
    JobView {
        job_id: gitcroc_core::JobId(1),
        workspace_id: WorkspaceId("ws".to_owned()),
        workspace_name: "Platform".to_owned(),
        label: label.to_owned(),
        repos: Vec::new(),
        repo_names: vec!["api".to_owned()],
        state: JobState::Running,
        done: 0,
        total: 0,
        step: None,
        started_at: None,
        duration_ms: None,
        outcome: None,
        output: String::new(),
        message: None,
        sub_jobs: Vec::new(),
        report: None,
        ai_files: Vec::new(),
    }
}

fn args(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// `git credential fill` for an HTTPS host, with no credential helper: git
/// asks its askpass helper for the username and the password.
fn credential_fill() -> Vec<OsString> {
    args(&["-c", "credential.helper=", "credential", "fill"])
}

const HTTPS_HOST: &[u8] = b"protocol=https\nhost=git.example.com\n\n";

struct Dirs(PathBuf);

impl ConfigDir for Dirs {
    fn config_dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

impl LogDir for Dirs {
    fn log_dir(&self) -> Option<PathBuf> {
        Some(self.0.clone())
    }
}

/// The command log of this test process, in a directory of its own.
fn log_dir() -> &'static Path {
    static DIR: OnceLock<PathBuf> = OnceLock::new();
    DIR.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap().keep();
        let log = Arc::new(AppLog::open(&Dirs(dir.clone())));
        gitcroc_core::log_commands_to(move |record| log.command(record));
        dir
    })
}

/// Every file below `dir`, as lossy text.
fn files_in(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(files_in(&path));
        } else {
            let text = String::from_utf8_lossy(&std::fs::read(&path).unwrap()).into_owned();
            found.push((path, text));
        }
    }
    found
}

#[tokio::test]
async fn d10_askpass_answers_git_credential_prompt() {
    let logs = log_dir();
    let config = tempfile::tempdir().unwrap();
    let ui = FakeUi::new(vec![
        ("Username", Some("alice")),
        ("Password", Some(PASSWORD)),
    ]);
    let server = server(ui.clone());
    let git = git().await;
    let grant = server.issue(&job_view("Fetch")).unwrap();
    let write = Write {
        askpass: Some(grant.askpass()),
        stdin: Some(HTTPS_HOST),
        ..Write::default()
    };

    let out = git
        .run_write(&credential_fill(), config.path(), write)
        .await
        .unwrap();
    let filled = String::from_utf8(out.stdout).unwrap();
    assert!(filled.contains("username=alice\n"), "{filled}");
    assert!(
        filled.contains(&format!("password={PASSWORD}\n")),
        "{filled}"
    );

    // The dialog named the job and the repo, and masked the password.
    let asked = ui.asked();
    let kinds: Vec<PromptKind> = asked.iter().map(|prompt| prompt.kind).collect();
    assert_eq!(kinds, [PromptKind::Text, PromptKind::Secret]);
    assert!(
        asked[0]
            .text
            .starts_with("Username for 'https://git.example.com'")
    );
    assert_eq!(
        (asked[1].job_label.as_str(), asked[1].repo_names.as_slice()),
        ("Fetch", ["api".to_owned()].as_slice())
    );

    // The answer went to git and nowhere else: not to the log, which has
    // the command, nor to the config dir.
    let logged = files_in(logs);
    assert!(
        logged
            .iter()
            .any(|(_, text)| text.contains(r#""credential" "fill""#)),
        "{logged:?}"
    );
    for (path, text) in logged.iter().chain(&files_in(config.path())) {
        assert!(
            !text.contains(PASSWORD),
            "{} holds the password",
            path.display()
        );
    }

    // Once the job ended, its token gets no answer.
    let askpass = grant.askpass().clone();
    drop(grant);
    let write = Write {
        askpass: Some(&askpass),
        stdin: Some(HTTPS_HOST),
        ..Write::default()
    };
    let error = git
        .run_write(&credential_fill(), config.path(), write)
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("could not read Username"),
        "{error}"
    );
    assert_eq!(ui.asked().len(), 2);
}

#[test]
fn d10_askpass_refuses_a_wrong_token() {
    let runtime = tokio::runtime::Runtime::new().unwrap();
    let ui = FakeUi::new(vec![("Password", Some(PASSWORD))]);
    let server = runtime.block_on(async { server(ui.clone()) });
    let _grant = server.issue(&job_view("Push")).unwrap();

    // The helper as git would start it, with a token the app did not issue,
    // with none, and with no app to reach.
    let run = |vars: &[(&str, &str)]| {
        let mut command = std::process::Command::new(helper());
        command.arg("Password for 'https://git.example.com': ");
        command.env_remove(ADDRESS_VAR).env_remove(TOKEN_VAR);
        command.envs(vars.iter().copied());
        command.output().unwrap()
    };
    let address = server.address().to_owned();
    for vars in [
        vec![(ADDRESS_VAR, address.as_str()), (TOKEN_VAR, "not-a-token")],
        vec![(ADDRESS_VAR, address.as_str())],
        vec![(ADDRESS_VAR, "/nowhere/gitcroc-askpass"), (TOKEN_VAR, "x")],
    ] {
        let output = run(&vars);
        assert_eq!(output.status.code(), Some(1), "{vars:?}");
        assert!(output.stdout.is_empty(), "{vars:?}");
    }
    assert!(ui.asked().is_empty());
}

/// Records nothing; the test reads the job's end from its handle.
struct NoEvents;

impl JobEvents for NoEvents {
    fn progress(&self, _: &JobView) {}
    fn ended(&self, _: &JobView) {}
    fn summary(&self, _: SummaryView, _: bool) {}
}

#[tokio::test]
async fn d10_cancelled_prompt_fails_the_job() {
    let _ = log_dir();
    let fixture = repo_gen::Fixture::new().unwrap();
    fixture
        .repo("api")
        .unwrap()
        .commit_file("README.md", "hello\n", "First")
        .unwrap();
    let config = tempfile::tempdir().unwrap();
    let workspaces = Arc::new(Workspaces::load(&Dirs(config.path().to_owned())));
    let id = workspaces.views()[0].id.clone();
    let git = git().await;
    add_path(
        workspaces.clone(),
        id.clone(),
        fixture.path().to_owned(),
        git.clone(),
        gitcroc_core::JobId(0),
        |_| {},
    )
    .await
    .unwrap();
    let repo = workspaces.view(&id).unwrap().repos[0].id.clone();
    let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
    let jobs = Jobs::new(workspaces, summaries, Arc::new(NoEvents));
    let ui = FakeUi::new(vec![("Username", None)]);
    jobs.use_askpass(server(ui.clone()));

    let job = WriteJob {
        workspace_id: id,
        repos: vec![repo],
        label: "Fetch".to_owned(),
        on_cancel: OnCancel::Kill,
        may_conflict: false,
        nothing_to_do: NothingToDo::Never,
        network: true,
    };
    let handle = jobs
        .start(git, job, |run| async move {
            // The test's stand-in for a fetch that needs a password (M2-07).
            let write = Write {
                stdin: Some(HTTPS_HOST),
                ..run.write()
            };
            run.git
                .run_write(&credential_fill(), &run.paths[0], write)
                .await
        })
        .unwrap();
    let view = handle.ended.await.unwrap();

    assert_eq!(view.outcome, Some(Outcome::Failed));
    assert!(view.output.contains("could not read Username"), "{view:?}");
    let asked = ui.asked();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].repo_names, ["api"]);
}
