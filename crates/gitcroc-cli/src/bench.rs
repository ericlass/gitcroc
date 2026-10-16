//! The benchmark harness (PERF-8): named scenarios run against the reference
//! data sets of spec section 9, which `just bench-data` builds.
//!
//! Each scenario runs once to warm up and then a fixed number of times. The
//! result is printed as a JSON array with one object per scenario and data
//! set: median and 95th percentile in milliseconds, the budget, and whether
//! the median is within it.

use std::cell::RefCell;
use std::ffi::{OsStr, OsString};
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gitcroc_core::{
    ChangedFile, DiffOptions, DiffRead, DiffSource, GitRepo, GitRunner, GitSetup, GroupOp,
    GroupOps, HistoryStart, JobEvents, JobView, Jobs, Outcome, Prune, RepoId, RepoRead,
    SettingsStore, StatusRead, StatusReader, Summaries, SummaryCache, SummaryView, TargetSet,
    WorkspaceId, Workspaces, merged_history, refresh_summaries,
};
use serde_json::{Value, json};
use tokio::sync::mpsc;

use crate::summaries::GivenCacheDir;
use crate::workspace::GivenConfigDir;

/// The set every budget is measured on (section 9).
pub const GATING_SET: &str = "target-project";

/// Stretch sets, measured when they exist locally and `--stretch` is given.
const STRETCH_SETS: [&str; 3] = ["big-history", "big-tree", "many-repos"];

/// The manifest `repo-gen` writes last into each set.
const MANIFEST: &str = "repo-gen.json";

const DEFAULT_RUNS: usize = 10;

pub const USAGE: &str = "bench [--data <dir>] [--runs <n>] [--stretch]";

/// One step of a scenario, called repeatedly.
type Step = Box<dyn FnMut() -> Result<(), String>>;

/// What a scenario times: `timed`, each time after the untimed `before`,
/// when there is one.
struct Measured {
    before: Option<Step>,
    timed: Step,
}

/// A scenario that times all it does.
fn timed(step: impl FnMut() -> Result<(), String> + 'static) -> Measured {
    Measured {
        before: None,
        timed: Box::new(step),
    }
}

/// A named scenario. `prepare` does the untimed setup and returns the
/// operation to time, or `None` when the set is too small for it.
struct Scenario {
    name: &'static str,
    /// The sets it runs on, with the section 9 budget on each, if any.
    sets: &'static [(&'static str, Option<f64>)],
    prepare: fn(&DataSet) -> Result<Option<Measured>, String>,
}

/// Later tasks add their scenarios here.
const SCENARIOS: &[Scenario] = &[
    // Cold start, the core part: from process start to every repo's cached
    // summary (PERF-3). M1-12 measures the tree becoming visible.
    Scenario {
        name: "cold-start-core",
        sets: &[
            ("target-project", Some(1000.0)),
            ("many-repos", Some(1500.0)),
        ],
        prepare: cold_start,
    },
    // Full status refresh of every repo after start, the core part: a core
    // started on a warm cache reads every repo again (PERF-3, PERF-4);
    // `bench-ui` measures until the tree shows them.
    Scenario {
        name: "refresh-all",
        sets: &[
            ("target-project", Some(2000.0)),
            ("many-repos", Some(5000.0)),
        ],
        prepare: refresh_all,
    },
    Scenario {
        name: "open-repo",
        sets: &[
            ("target-project", None),
            ("big-history", None),
            ("big-tree", None),
            ("many-repos", None),
        ],
        prepare: open_repo,
    },
    // Select the largest repo: first page of history (the core part; M1-12
    // measures the UI part).
    Scenario {
        name: "history-first-page",
        sets: &[
            ("target-project", Some(200.0)),
            ("big-history", Some(300.0)),
            ("many-repos", None),
        ],
        prepare: history_first_page,
    },
    // Select a group of all the set's repos: the first page of their merged
    // history (PERF-5; the core part, `bench-ui` measures the UI part).
    Scenario {
        name: "group-history-first-page",
        sets: &[("target-project", Some(300.0)), ("many-repos", None)],
        prepare: group_history_first_page,
    },
    // A group fetch of every repo from local bare remotes, at the default
    // parallelism, reported only (section 9 stretch).
    Scenario {
        name: "group-fetch",
        sets: &[("many-repos", None)],
        prepare: group_fetch,
    },
    // Later pages cost no more than the first: page 100 needs 50,000 commits,
    // so target-project shows its last page, the 10th.
    Scenario {
        name: "history-page-10",
        sets: &[("target-project", None)],
        prepare: |set| history_later_page(set, 10),
    },
    Scenario {
        name: "history-page-100",
        sets: &[("big-history", None)],
        prepare: |set| history_later_page(set, 100),
    },
    // Select a commit with under 5,000 changed lines: header, file list and
    // the first file's diff (the core part; M1-12 measures the UI part).
    Scenario {
        name: "select-commit",
        sets: &[("target-project", Some(150.0)), ("big-history", None)],
        prepare: select_commit,
    },
    // The largest commit, reported only: in a generated set the root commit
    // of the largest repo, which adds 30% of its files.
    Scenario {
        name: "select-commit-large",
        sets: &[("target-project", None)],
        prepare: select_root_commit,
    },
    // Working copy status of the 60,000-file repo, git defaults, the repo
    // already open.
    Scenario {
        name: "status-60k",
        sets: &[("target-project", Some(500.0))],
        prepare: |set| status(set, false),
    },
    // Stretch: big-tree with the untracked cache and, where git has a
    // built-in one (macOS, Windows), the file system monitor on.
    Scenario {
        name: "status-big-tree",
        sets: &[("big-tree", Some(1000.0))],
        prepare: |set| status(set, true),
    },
];

/// Reads the working copy of the largest repo. With `caches`, the repo's
/// config first turns on the untracked cache and, where git has one, the
/// built-in file system monitor, and a plain `git status` fills them; this
/// changes only the bench data.
fn status(set: &DataSet, caches: bool) -> Result<Option<Measured>, String> {
    let path = set.largest()?.path.clone();
    let git = git_runner()?;
    if caches {
        let monitor = if cfg!(any(target_os = "macos", target_os = "windows")) {
            "true"
        } else {
            "false"
        };
        for args in [
            &["config", "core.untrackedCache", "true"][..],
            &["config", "core.fsmonitor", monitor],
            &["status", "--porcelain"],
        ] {
            let args: Vec<OsString> = args.iter().map(OsString::from).collect();
            git.run_blocking(&args, Some(&path))
                .map_err(|error| error.to_string())?;
        }
    }
    let repo = GitRepo::open(&path).map_err(|error| error.to_string())?;
    let reader = StatusReader::new(repo, git);
    if let Some(fallback) = reader.fallback().map_err(|error| error.to_string())? {
        eprintln!("status on {} runs git: {fallback:?}", set.name);
    }
    Ok(Some(timed(move || {
        let working_copy = reader.status().map_err(|error| error.to_string())?;
        black_box(working_copy);
        Ok(())
    })))
}

/// Starts `gitcroc-cli summaries --cached-only` as a new process on a
/// workspace of every repo of the set, its cache filled by one untimed
/// refresh, and checks that every repo has its cached summary.
fn cold_start(set: &DataSet) -> Result<Option<Measured>, String> {
    let exe =
        std::env::current_exe().map_err(|error| format!("cannot find this program: {error}"))?;
    let dirs = tempfile::tempdir().map_err(|error| format!("cannot make a temp dir: {error}"))?;
    let config = dirs.path().join("config");
    let cache = dirs.path().join("cache");
    let mut summaries = cached_workspace(&exe, &config, &cache, set)?;
    summaries.push("--cached-only".into());
    let expected = set.repos.len();
    Ok(Some(timed(move || {
        // The dirs live as long as the operation.
        let _dirs = &dirs;
        let stdout = run_self(&exe, &summaries)?;
        let views: Value = serde_json::from_slice(&stdout)
            .map_err(|error| format!("cannot parse the summaries: {error}"))?;
        let cached = views.as_array().map_or(0, |views| {
            views
                .iter()
                .filter(|view| !view["summary"].is_null())
                .count()
        });
        if cached == expected {
            Ok(())
        } else {
            Err(format!("{cached} of {expected} summaries were cached"))
        }
    })))
}

/// Makes a workspace of every repo of the set in config dir `config` and
/// fills summary cache `cache` with one refresh. Returns the arguments of
/// that `summaries` run.
fn cached_workspace(
    exe: &Path,
    config: &Path,
    cache: &Path,
    set: &DataSet,
) -> Result<Vec<OsString>, String> {
    for repo in &set.repos {
        let add = [
            "workspace".into(),
            "--config-dir".into(),
            config.as_os_str().to_owned(),
            "add".into(),
            repo.path.clone().into_os_string(),
        ];
        run_self(exe, &add)?;
    }
    let summaries: Vec<OsString> = vec![
        "summaries".into(),
        "--config-dir".into(),
        config.as_os_str().to_owned(),
        "--cache-dir".into(),
        cache.as_os_str().to_owned(),
    ];
    run_self(exe, &summaries)?;
    Ok(summaries)
}

/// A core as the app has it after start: the workspaces and the summaries
/// from the cache.
struct Core {
    workspaces: Arc<Workspaces>,
    summaries: Arc<Summaries>,
    workspace: WorkspaceId,
    repos: Vec<RepoId>,
}

impl Core {
    fn load(config: &Path, cache: &Path) -> Result<Self, String> {
        let workspaces = Arc::new(Workspaces::load(&GivenConfigDir(config.to_owned())));
        let cache = SummaryCache::open(&GivenCacheDir(cache.to_owned()));
        let summaries = Arc::new(Summaries::load(cache, &workspaces));
        let view = workspaces
            .active()
            .and_then(|id| workspaces.view(&id).ok())
            .ok_or("no workspace is open")?;
        Ok(Self {
            workspaces,
            summaries,
            workspace: view.id,
            repos: view.repos.into_iter().map(|repo| repo.id).collect(),
        })
    }
}

/// A runtime with worker threads, as the app has.
fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))
}

/// A workspace of every repo of the set, its summary cache filled by one
/// refresh. Each run starts a new core on that cache, untimed, and times a
/// refresh of every repo, as after the app's start, until each repo's
/// summary is read; one that is not read fails the run.
fn refresh_all(set: &DataSet) -> Result<Option<Measured>, String> {
    let exe =
        std::env::current_exe().map_err(|error| format!("cannot find this program: {error}"))?;
    let dirs = tempfile::tempdir().map_err(|error| format!("cannot make a temp dir: {error}"))?;
    let config = dirs.path().join("config");
    let cache = dirs.path().join("cache");
    cached_workspace(&exe, &config, &cache, set)?;
    let git = git_runner()?;
    let runtime = runtime()?;
    let core: Rc<RefCell<Option<Core>>> = Rc::default();
    let loaded = core.clone();
    let before = move || {
        loaded.replace(Some(Core::load(&config, &cache)?));
        Ok(())
    };
    let timed = move || {
        // The dirs live as long as the operation.
        let _dirs = &dirs;
        let core = core.take().ok_or("no core was started")?;
        runtime.block_on(refresh_summaries(
            core.workspaces.clone(),
            core.summaries.clone(),
            core.workspace.clone(),
            core.repos.clone(),
            git.clone(),
            |_| {},
        ));
        let views = core
            .summaries
            .views(&core.workspaces, &core.workspace)
            .map_err(|error| error.to_string())?;
        let read = views
            .iter()
            .filter(|view| view.summary.is_some() && !view.stale && view.error.is_none())
            .count();
        if read == core.repos.len() {
            Ok(())
        } else {
            Err(format!("{read} of {} repos were read", core.repos.len()))
        }
    };
    Ok(Some(Measured {
        before: Some(Box::new(before)),
        timed: Box::new(timed),
    }))
}

/// Sends each ended job on.
struct Ended(mpsc::UnboundedSender<JobView>);

impl JobEvents for Ended {
    fn progress(&self, _job: &JobView) {}

    fn ended(&self, job: &JobView) {
        let _ = self.0.send(job.clone());
    }

    fn summary(&self, _view: SummaryView, _read: bool) {}
}

/// The group of every repo in `group-fetch`.
const ALL_GROUP: &str = "All repos";

/// A group fetch of every repo of the set, timed from the preflight to the
/// job's end, which comes after every repo was read again, with the default
/// parallelism. Each repo gets a bare remote and a working copy in a
/// throwaway dir, both borrowing the set's objects (`--shared`), so the set
/// is never written. Before each run, untimed, every remote gets a new
/// commit, so that each fetch brings one; a repo that does not end
/// *succeeded* fails the run.
fn group_fetch(set: &DataSet) -> Result<Option<Measured>, String> {
    let exe =
        std::env::current_exe().map_err(|error| format!("cannot find this program: {error}"))?;
    let dirs = tempfile::tempdir().map_err(|error| format!("cannot make a temp dir: {error}"))?;
    let git = git_runner()?;
    let fetch_git = git.clone();
    let run = move |args: &[&OsStr], cwd: Option<&Path>| -> Result<String, String> {
        let args: Vec<OsString> = args.iter().map(|&arg| arg.to_owned()).collect();
        let output = git
            .run_blocking(&args, cwd)
            .map_err(|error| error.to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    };
    let work = dirs.path().join("work");
    let mut remotes = Vec::with_capacity(set.repos.len());
    eprintln!(
        "group-fetch on {}: a bare remote and a working copy of {} repos",
        set.name,
        set.repos.len()
    );
    for (index, repo) in set.repos.iter().enumerate() {
        let remote = dirs.path().join("remotes").join(format!("{index}.git"));
        let copy = work.join(index.to_string());
        let quiet = OsStr::new("--quiet");
        let shared = OsStr::new("--shared");
        run(
            &[
                OsStr::new("clone"),
                quiet,
                OsStr::new("--bare"),
                shared,
                repo.path.as_os_str(),
                remote.as_os_str(),
            ],
            None,
        )?;
        run(
            &[
                OsStr::new("clone"),
                quiet,
                shared,
                remote.as_os_str(),
                copy.as_os_str(),
            ],
            None,
        )?;
        remotes.push(remote);
    }
    let config = dirs.path().join("config");
    let add = [
        "workspace".into(),
        "--config-dir".into(),
        config.clone().into_os_string(),
        "add".into(),
        work.into_os_string(),
    ];
    run_self(&exe, &add)?;

    let settings = Arc::new(SettingsStore::load(&GivenConfigDir(config.clone())));
    let workspaces = Arc::new(Workspaces::load_with(
        &GivenConfigDir(config),
        settings.clone(),
    ));
    let view = workspaces
        .active()
        .and_then(|id| workspaces.view(&id).ok())
        .ok_or("no workspace is open")?;
    if view.repos.len() != set.repos.len() {
        return Err(format!(
            "the workspace has {} of {} repos",
            view.repos.len(),
            set.repos.len()
        ));
    }
    let repos: Vec<RepoId> = view.repos.iter().map(|repo| repo.id.clone()).collect();
    let group = workspaces
        .create_group(&view.id, ALL_GROUP, &repos)
        .map_err(|error| error.to_string())?;
    let runtime = runtime()?;
    let summaries = Arc::new(Summaries::load(SummaryCache::none(), &workspaces));
    let (end, mut ended) = mpsc::unbounded_channel();
    let jobs = Jobs::new(workspaces.clone(), summaries.clone(), Arc::new(Ended(end)));
    let (ready, mut readies) = mpsc::unbounded_channel();
    let groups = GroupOps::new(workspaces, summaries, settings, jobs, move |result| {
        let _ = ready.send(result.clone());
    });

    let mut wave = 0;
    let before = move || {
        wave += 1;
        let message = format!("Wave {wave}");
        for remote in &remotes {
            let commit = run(
                &[
                    OsStr::new("-c"),
                    OsStr::new("user.name=Bench"),
                    OsStr::new("-c"),
                    OsStr::new("user.email=bench@example.com"),
                    OsStr::new("commit-tree"),
                    OsStr::new("HEAD^{tree}"),
                    OsStr::new("-p"),
                    OsStr::new("HEAD"),
                    OsStr::new("-m"),
                    OsStr::new(&message),
                ],
                Some(remote),
            )?;
            run(
                &[
                    OsStr::new("update-ref"),
                    OsStr::new("HEAD"),
                    OsStr::new(&commit),
                ],
                Some(remote),
            )?;
        }
        Ok(())
    };
    let workspace = view.id;
    let timed = move || {
        // The dirs live as long as the operation.
        let _dirs = &dirs;
        let job = runtime.block_on(async {
            let target = TargetSet::Group {
                group_id: group.clone(),
            };
            let op = GroupOp::Fetch {
                prune: Prune::Config,
            }
            .operation()
            .map_err(|error| error.to_string())?;
            groups
                .preflight(fetch_git.clone(), &workspace, &target, op)
                .map_err(|error| error.to_string())?;
            let preflight = readies.recv().await.ok_or("the preflight never finished")?;
            if preflight.needs_confirmation {
                return Err("the group fetch needs confirmation".to_owned());
            }
            groups
                .start(fetch_git.clone(), preflight.preflight_id, false)
                .map_err(|error| error.to_string())?;
            ended.recv().await.ok_or("the job never ended".to_owned())
        })?;
        let report = job.report.ok_or("the group job has no report")?;
        let failed: Vec<_> = report
            .iter()
            .filter(|repo| repo.outcome != Outcome::Succeeded)
            .collect();
        if failed.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "{} repos did not fetch, such as {:?}",
                failed.len(),
                failed[0]
            ))
        }
    };
    Ok(Some(Measured {
        before: Some(Box::new(before)),
        timed: Box::new(timed),
    }))
}

/// Runs this program with `args` and returns its output.
fn run_self(exe: &Path, args: &[OsString]) -> Result<Vec<u8>, String> {
    let output = Command::new(exe)
        .args(args)
        .output()
        .map_err(|error| format!("cannot run {}: {error}", exe.display()))?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// The git the app would use, found as at startup (PLAT-3).
fn git_runner() -> Result<GitRunner, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))?;
    let setup = GitSetup::new();
    let status = runtime.block_on(setup.detect());
    let git = runtime
        .block_on(setup.git())
        .ok_or_else(|| format!("no usable git: {status:?}"))?;
    Ok(GitRunner::new(git.path))
}

/// Opens the set's largest repo with `gix`; proves the plumbing.
fn open_repo(set: &DataSet) -> Result<Option<Measured>, String> {
    let path = set.largest()?.path.clone();
    Ok(Some(timed(move || {
        let repo = GitRepo::open(&path).map_err(|error| error.to_string())?;
        black_box(repo);
        Ok(())
    })))
}

/// Opens the largest repo and reads its first page of history.
fn history_first_page(set: &DataSet) -> Result<Option<Measured>, String> {
    let path = set.largest()?.path.clone();
    Ok(Some(timed(move || {
        let repo = GitRepo::open(&path).map_err(|error| error.to_string())?;
        let page = repo
            .history(HistoryStart::All, None, None)
            .map_err(|error| error.to_string())?;
        black_box(page);
        Ok(())
    })))
}

/// Reads the first page of the merged history of every repo of the set,
/// opening each repo as a page does.
fn group_history_first_page(set: &DataSet) -> Result<Option<Measured>, String> {
    let paths: Vec<PathBuf> = set.repos.iter().map(|repo| repo.path.clone()).collect();
    Ok(Some(timed(move || {
        let page = merged_history(&paths, None, None).map_err(|error| error.to_string())?;
        black_box(page);
        Ok(())
    })))
}

/// Reads page `number` of the largest repo, continuing from page
/// `number - 1`, which is read untimed. `None` when the history is shorter.
fn history_later_page(set: &DataSet, number: usize) -> Result<Option<Measured>, String> {
    let repo = GitRepo::open(&set.largest()?.path).map_err(|error| error.to_string())?;
    let mut cursor = None;
    for page in 1..number {
        cursor = repo
            .history(HistoryStart::All, cursor, None)
            .map_err(|error| error.to_string())?
            .next;
        if cursor.is_none() {
            eprintln!("{} has only {page} pages of history", set.name);
            return Ok(None);
        }
    }
    Ok(Some(timed(move || {
        let page = repo
            .history(HistoryStart::All, cursor.clone(), None)
            .map_err(|error| error.to_string())?;
        black_box(page);
        Ok(())
    })))
}

/// Changed lines a commit may have for `select-commit` (section 9).
const SELECT_COMMIT_LINES: u32 = 5_000;

/// The commit with the most changed lines under [`SELECT_COMMIT_LINES`]
/// among the first page of the largest repo's history.
fn select_commit(set: &DataSet) -> Result<Option<Measured>, String> {
    let repo = GitRepo::open(&set.largest()?.path).map_err(|error| error.to_string())?;
    let page = repo
        .history(HistoryStart::All, None, None)
        .map_err(|error| error.to_string())?;
    let mut best: Option<(u32, String)> = None;
    for row in &page.rows {
        let files = repo
            .changed_files(&row.hash, 0)
            .map_err(|error| error.to_string())?;
        let Some(lines) = changed_lines(&files) else {
            continue;
        };
        if lines < SELECT_COMMIT_LINES && best.as_ref().is_none_or(|(most, _)| lines > *most) {
            best = Some((lines, row.hash.clone()));
        }
    }
    let Some((lines, hash)) = best else {
        eprintln!("{} has no commit with counted lines", set.name);
        return Ok(None);
    };
    eprintln!(
        "select-commit on {}: {hash}, {lines} changed lines",
        set.name
    );
    Ok(Some(select(repo, hash)))
}

/// All changed lines of a list; `None` when a count is missing.
fn changed_lines(files: &[ChangedFile]) -> Option<u32> {
    files.iter().try_fold(0u32, |total, file| {
        Some(
            total
                .saturating_add(file.added?)
                .saturating_add(file.removed?),
        )
    })
}

/// The root commit of the largest repo.
fn select_root_commit(set: &DataSet) -> Result<Option<Measured>, String> {
    let repo = GitRepo::open(&set.largest()?.path).map_err(|error| error.to_string())?;
    let mut cursor = None;
    loop {
        let page = repo
            .history(HistoryStart::All, cursor, None)
            .map_err(|error| error.to_string())?;
        if let Some(root) = page.rows.iter().find(|row| row.parents.is_empty()) {
            return Ok(Some(select(repo, root.hash.clone())));
        }
        cursor = page.next;
        if cursor.is_none() {
            return Err(format!("{} has no root commit", set.name));
        }
    }
}

/// Reads a commit's header, its changed files and the first file's diff.
fn select(repo: GitRepo, hash: String) -> Measured {
    timed(move || {
        let text = |error: gitcroc_core::DiffError| error.to_string();
        let commit = repo.commit(&hash).map_err(text)?;
        let files = repo.changed_files(&hash, 0).map_err(text)?;
        let file = files.first().ok_or("the commit changed no file")?;
        let source = DiffSource::Commit {
            hash: hash.clone(),
            parent: 0,
        };
        let diff = repo
            .file_diff(&source, file, DiffOptions::default())
            .map_err(text)?;
        black_box((commit, files, diff));
        Ok(())
    })
}

/// A data set as its manifest describes it.
pub struct DataSet {
    name: String,
    repos: Vec<SetRepo>,
}

struct SetRepo {
    path: PathBuf,
    files: Option<u64>,
}

impl DataSet {
    /// Reads `<root>/<name>/repo-gen.json`. `None` when the set has not been
    /// built (or not completely).
    fn load(root: &Path, name: &str) -> Result<Option<Self>, String> {
        let dir = root.join(name);
        let manifest = dir.join(MANIFEST);
        let text = match std::fs::read_to_string(&manifest) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("cannot read {}: {error}", manifest.display())),
        };
        let json: Value = serde_json::from_str(&text)
            .map_err(|error| format!("cannot parse {}: {error}", manifest.display()))?;
        let repos = json["repos"]
            .as_array()
            .ok_or_else(|| format!("{} lists no repos", manifest.display()))?
            .iter()
            .map(|repo| {
                let path = repo["path"]
                    .as_str()
                    .ok_or_else(|| format!("{}: a repo has no path", manifest.display()))?;
                Ok(SetRepo {
                    path: dir.join(path),
                    files: repo["files"].as_u64(),
                })
            })
            .collect::<Result<_, String>>()?;
        Ok(Some(Self {
            name: name.to_owned(),
            repos,
        }))
    }

    /// The repo with the most files; the only one in a one-repo set.
    fn largest(&self) -> Result<&SetRepo, String> {
        self.repos
            .iter()
            .max_by_key(|repo| repo.files.unwrap_or(0))
            .ok_or_else(|| format!("{} has no repos", self.name))
    }
}

pub struct Options {
    data: PathBuf,
    runs: usize,
    stretch: bool,
}

/// Parses the arguments after `bench`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let mut options = Options {
        data: std::env::var_os("GITCROC_BENCH_DATA")
            .filter(|dir| !dir.is_empty())
            .map_or_else(|| PathBuf::from("target/bench-data"), PathBuf::from),
        runs: DEFAULT_RUNS,
        stretch: false,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.to_str()? {
            "--data" => options.data = PathBuf::from(args.next()?),
            "--runs" => {
                options.runs = args.next()?.to_str()?.parse().ok().filter(|&n| n > 0)?;
            }
            "--stretch" => options.stretch = true,
            _ => return None,
        }
    }
    Some(options)
}

/// Median and 95th percentile (nearest rank) of the samples, in the same
/// unit. `None` without samples.
pub fn summarize(samples: &[f64]) -> Option<(f64, f64)> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_by(f64::total_cmp);
    let n = sorted.len();
    let median = if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    };
    let rank = (n * 95).div_ceil(100).max(1);
    Some((median, sorted[rank - 1]))
}

/// Runs a scenario on a set and returns its JSON result, or `None` when the
/// set is too small for it.
fn measure(
    scenario: &Scenario,
    budget_ms: Option<f64>,
    set: &DataSet,
    runs: usize,
) -> Result<Option<Value>, String> {
    let Some(mut operation) = (scenario.prepare)(set)? else {
        return Ok(None);
    };
    let mut once = || {
        if let Some(before) = operation.before.as_mut() {
            before()?;
        }
        let started = Instant::now();
        (operation.timed)()?;
        Ok::<_, String>(started.elapsed().as_secs_f64() * 1000.0)
    };
    once()?; // warm-up
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        samples.push(once()?);
    }
    let (median, p95) = summarize(&samples).ok_or("no samples")?;
    let round = |ms: f64| (ms * 1000.0).round() / 1000.0;
    Ok(Some(json!({
        "scenario": scenario.name,
        "data_set": set.name,
        "runs": runs,
        "median_ms": round(median),
        "p95_ms": round(p95),
        "budget_ms": budget_ms,
        "pass": budget_ms.is_none_or(|budget| median <= budget),
    })))
}

/// Exits 0 when every scenario on the gating set is within its budget, 1
/// when one is not, and 2 when the gating set is missing or a scenario fails
/// to run. Stretch budgets are reported but do not fail the run (section 9).
pub fn run(options: &Options) -> ExitCode {
    match run_all(options) {
        Ok(results) => {
            let pass = results
                .iter()
                .filter(|result| result["data_set"] == GATING_SET)
                .all(|result| result["pass"] == true);
            match serde_json::to_string_pretty(&results) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("cannot write the results as JSON: {error}");
                    return ExitCode::from(2);
                }
            }
            if pass {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

fn run_all(options: &Options) -> Result<Vec<Value>, String> {
    let mut sets = vec![DataSet::load(&options.data, GATING_SET)?.ok_or_else(|| {
        format!(
            "no {GATING_SET} data in {}; run `just bench-data {GATING_SET}` first",
            options.data.display()
        )
    })?];
    if options.stretch {
        for name in STRETCH_SETS {
            match DataSet::load(&options.data, name)? {
                Some(set) => sets.push(set),
                None => eprintln!("skipping {name}: not built"),
            }
        }
    }
    let mut results = Vec::new();
    for set in &sets {
        for scenario in SCENARIOS {
            let Some(&(_, budget_ms)) = scenario.sets.iter().find(|(name, _)| *name == set.name)
            else {
                continue;
            };
            eprintln!("{} on {}", scenario.name, set.name);
            let result = measure(scenario, budget_ms, set, options.runs)
                .map_err(|error| format!("{} on {}: {error}", scenario.name, set.name))?;
            results.extend(result);
        }
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perf_8_summary_is_median_and_nearest_rank_p95() {
        assert_eq!(summarize(&[]), None);
        assert_eq!(summarize(&[4.0]), Some((4.0, 4.0)));
        assert_eq!(summarize(&[3.0, 1.0, 2.0]), Some((2.0, 3.0)));
        assert_eq!(summarize(&[4.0, 1.0, 3.0, 2.0]), Some((2.5, 4.0)));
        let hundred: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(summarize(&hundred), Some((50.5, 95.0)));
    }

    #[test]
    fn perf_8_bench_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        let options = parse_args(&args(&["--data", "d", "--runs", "3", "--stretch"])).unwrap();
        assert_eq!(
            (options.data, options.runs, options.stretch),
            (PathBuf::from("d"), 3, true)
        );
        assert!(parse_args(&args(&["--runs", "0"])).is_none());
        assert!(parse_args(&args(&["--runs"])).is_none());
        assert!(parse_args(&args(&["extra"])).is_none());
    }
}
