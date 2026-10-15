//! `gitcroc-cli group run <group> --test-op`: runs a group operation on a
//! group of the active workspace through the core's engine (section 5
//! "Lifecycle of a group operation") and prints the preflight and the job
//! with its report as JSON. Until the real operations arrive (M3-10 to
//! M3-13), only the test operation runs: every repo GRP-8 lets pass will
//! run, and its run starts no git, so it ends *nothing to do*.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gitcroc_core::{
    Class, GitRepo, GitRunner, GitSetup, GroupOperation, GroupOps, JobEvents, JobRun, JobView,
    Jobs, NothingToDo, OnCancel, Output, PlatformConfigDir, PreflightReady, RunFuture,
    SettingsStore, SubJobSpec, Summaries, SummaryCache, SummaryView, TargetSet, Workspaces,
};
use tokio::sync::{mpsc, oneshot};

use crate::workspace::GivenConfigDir;

/// The test operation.
struct TestOp;

impl GroupOperation for TestOp {
    fn name(&self) -> String {
        "Test".to_owned()
    }

    fn destructive(&self) -> Vec<String> {
        Vec::new()
    }

    fn spec(&self) -> SubJobSpec {
        SubJobSpec {
            on_cancel: OnCancel::Finish,
            may_conflict: false,
            nothing_to_do: NothingToDo::WhenRefsUnchanged,
            network: false,
            conflict_allowed: false,
        }
    }

    fn classify(&self, _repo: &GitRepo, _git: &GitRunner) -> Class {
        Class::WillRun
    }

    fn run(&self, _job: JobRun, _path: PathBuf) -> RunFuture {
        Box::pin(async {
            Ok(Output {
                stdout: Vec::new(),
                stderr: Vec::new(),
            })
        })
    }
}

/// Sends each ended job to the run waiting for it.
struct Ends(Mutex<Option<oneshot::Sender<JobView>>>);

impl JobEvents for Ends {
    fn progress(&self, _job: &JobView) {}

    fn ended(&self, job: &JobView) {
        let mut sender = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(sender) = sender.take() {
            let _ = sender.send(job.clone());
        }
    }

    fn summary(&self, _view: SummaryView, _read: bool) {}
}

/// Runs the test operation on group `name` of the active workspace and
/// returns `{ "preflight": ..., "job": ... }` as JSON. A preflight that
/// needs confirmation runs only with `confirm`; without it, only the
/// preflight is printed and the result is an error.
pub fn run(config_dir: Option<&PathBuf>, name: &str, confirm: bool) -> Result<String, String> {
    let settings = Arc::new(match config_dir {
        Some(dir) => SettingsStore::load(&GivenConfigDir(dir.clone())),
        None => SettingsStore::load(&PlatformConfigDir),
    });
    let workspaces = Arc::new(match config_dir {
        Some(dir) => Workspaces::load_with(&GivenConfigDir(dir.clone()), settings.clone()),
        None => Workspaces::load_with(&PlatformConfigDir, settings.clone()),
    });
    let id = workspaces
        .active()
        .ok_or_else(|| "no workspace is open".to_owned())?;
    let group = crate::group::group(&workspaces, &id, name)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))?;
    let setup = GitSetup::new();
    let status = runtime.block_on(setup.detect());
    let git = runtime
        .block_on(setup.git())
        .ok_or_else(|| format!("no usable git: {status:?}"))?;
    let git = GitRunner::new(git.path);
    let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
    let (end, ended) = oneshot::channel();
    let jobs = Jobs::new(
        workspaces.clone(),
        summaries.clone(),
        Arc::new(Ends(Mutex::new(Some(end)))),
    );
    let (ready, mut readies) = mpsc::unbounded_channel();
    let groups = GroupOps::new(workspaces, summaries, settings, jobs, move |result| {
        let _ = ready.send(result.clone());
    });
    let (preflight, job) = runtime.block_on(async {
        let target = TargetSet::Group { group_id: group };
        groups
            .preflight(git.clone(), &id, &target, Arc::new(TestOp))
            .map_err(|error| error.to_string())?;
        let preflight: PreflightReady = readies
            .recv()
            .await
            .ok_or_else(|| "the preflight never finished".to_owned())?;
        if preflight.needs_confirmation && !confirm {
            return Ok((preflight, None));
        }
        groups
            .start(git, preflight.preflight_id, true)
            .map_err(|error| error.to_string())?;
        let job = ended.await.map_err(|_| "the job never ended".to_owned())?;
        Ok::<_, String>((preflight, Some(job)))
    })?;
    let confirmed = job.is_some();
    let json = serde_json::to_string_pretty(&serde_json::json!({
        "preflight": preflight,
        "job": job,
    }))
    .map_err(|error| format!("cannot write the result as JSON: {error}"))?;
    if confirmed {
        Ok(json)
    } else {
        println!("{json}");
        Err("the operation needs confirmation; run it again with --confirm".to_owned())
    }
}
