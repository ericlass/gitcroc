//! `gitcroc-cli bench-ui` (PERF-8, M6-14b): the section 9 budgets the core
//! cannot measure alone, on the GPUI app and `target-project`. It sets up a
//! throwaway config and cache, starts a `perf-trace` build of `gitcroc-next`
//! once to warm up and then `--runs` times, each with a script the app plays
//! in its window (`gitcroc_ui::perf`), and judges what the app measured.
//! Prints a JSON array in the shape `just bench` prints. Run by hand before
//! a release and for a sign-off, never in CI: hosted runners have no fixed
//! speed.
//!
//! The app is a separate process, so the CLI does not depend on the UI
//! crate; the script and trace are JSON both sides write and read.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant, SystemTime};

use serde::Deserialize;
use serde_json::{Value, json};

use crate::bench::{DataSet, GATING_SET, pick_commit, run_self, summarize};

pub const USAGE: &str =
    "bench-ui --app <gitcroc-next> [--data <dir>] [--runs <n>] [--refresh-hz <hz>] [--verbose]";

const DEFAULT_RUNS: usize = 5;

/// Switches between two repos, and between two tabs, each way per run.
const SWITCHES: usize = 4;

/// Lines of the file whose diff `scroll-diff` scrolls: long enough for
/// about 3,000 frames, under the 20,000 changed lines of a large diff.
const DIFF_LINES: usize = 15_000;

/// How long one run of the app may take.
const RUN_TIMEOUT: Duration = Duration::from_secs(600);

/// The highest refresh rate section 9 asks for.
const MAX_HZ: f64 = 144.0;

/// A frame over this many refresh intervals is a dropped frame (section 9).
const JANKY_INTERVALS: f64 = 1.5;

/// The share of dropped frames section 9 allows, in percent.
const JANKY_BUDGET: f64 = 1.0;

/// How far the mean frame interval may be over the refresh interval.
const FRAME_SLACK: f64 = 1.01;

/// The action scenarios and their section 9 budgets, in ms.
const ACTIONS: [(&str, f64); 5] = [
    ("cold-start", 1000.0),
    ("select-largest-repo", 200.0),
    ("select-commit", 150.0),
    ("switch-repo", 100.0),
    ("switch-tab", 100.0),
];

/// Action scenarios reported without a budget: how long a tab's history
/// takes to come back after a switch (UI-31).
const REPORTED: [&str; 1] = ["switch-tab-history"];

/// The scroll runs, each judged by its frame interval and dropped frames.
const SCROLLS: [&str; 2] = ["scroll-history", "scroll-diff"];

/// No row may stay blank longer than this while scrolling (section 9).
const BLANK_BUDGET: f64 = 100.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Options {
    app: PathBuf,
    data: PathBuf,
    runs: usize,
    refresh_hz: Option<f64>,
    verbose: bool,
}

/// Parses the arguments after `bench-ui`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let mut app = None;
    let mut options = Options {
        app: PathBuf::new(),
        data: std::env::var_os("GITCROC_BENCH_DATA")
            .filter(|dir| !dir.is_empty())
            .map_or_else(|| PathBuf::from("target/bench-data"), PathBuf::from),
        runs: DEFAULT_RUNS,
        refresh_hz: None,
        verbose: false,
    };
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.to_str()? {
            "--app" => app = Some(PathBuf::from(args.next()?)),
            "--data" => options.data = PathBuf::from(args.next()?),
            "--runs" => {
                options.runs = args.next()?.to_str()?.parse().ok().filter(|&n| n > 0)?;
            }
            "--refresh-hz" => {
                options.refresh_hz = Some(
                    args.next()?
                        .to_str()?
                        .parse()
                        .ok()
                        .filter(|&hz: &f64| hz > 0.0)?,
                );
            }
            "--verbose" => options.verbose = true,
            _ => return None,
        }
    }
    options.app = app?;
    Some(options)
}

/// What one run of the app measured, as `gitcroc_ui::perf::Trace` writes
/// it.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct Trace {
    pub refresh_hz: Option<f64>,
    pub samples: Vec<Sample>,
    pub scrolls: Vec<Scroll>,
    pub inactive_frames: usize,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Sample {
    pub scenario: String,
    pub ms: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Scroll {
    pub scenario: String,
    pub intervals_ms: Vec<f64>,
    pub longest_blank_ms: f64,
}

/// A scroll run's frames: the mean interval in ms and the share over
/// [`JANKY_INTERVALS`] refresh intervals, in percent. The refresh interval
/// is the display's, capped at [`MAX_HZ`]. `None` without frames.
pub fn frame_stats(intervals_ms: &[f64], refresh_hz: f64) -> Option<(f64, f64)> {
    if intervals_ms.is_empty() || refresh_hz <= 0.0 {
        return None;
    }
    let refresh_ms = 1000.0 / refresh_hz.min(MAX_HZ);
    let count = intervals_ms.len() as f64;
    let mean = intervals_ms.iter().sum::<f64>() / count;
    let janky = intervals_ms
        .iter()
        .filter(|&&ms| ms > refresh_ms * JANKY_INTERVALS)
        .count() as f64;
    Some((mean, janky / count * 100.0))
}

/// Judges the runs' traces: one row per scenario, in the shape `just bench`
/// prints, with the unit. Actions pool their samples from all runs; scroll
/// rows take one value per run.
pub fn judge(traces: &[Trace], refresh_hz: f64) -> Result<Vec<Value>, String> {
    let mut rows = Vec::new();
    let runs = traces.len();
    let round = |value: f64| (value * 1000.0).round() / 1000.0;
    let mut row = |scenario: String, unit: &str, values: &[f64], budget: Option<f64>| {
        let (median, p95) = summarize(values).ok_or(format!("{scenario} has no samples"))?;
        rows.push(json!({
            "scenario": scenario,
            "data_set": GATING_SET,
            "runs": runs,
            "unit": unit,
            "median_ms": round(median),
            "p95_ms": round(p95),
            "budget_ms": budget.map(round),
            "pass": budget.is_none_or(|budget| median <= budget),
        }));
        Ok::<_, String>(())
    };
    let samples = |scenario: &str| -> Vec<f64> {
        traces
            .iter()
            .flat_map(|trace| &trace.samples)
            .filter(|sample| sample.scenario == scenario)
            .map(|sample| sample.ms)
            .collect()
    };
    for (scenario, budget) in ACTIONS {
        row(scenario.to_owned(), "ms", &samples(scenario), Some(budget))?;
    }
    for scenario in REPORTED {
        let values = samples(scenario);
        if !values.is_empty() {
            row(scenario.to_owned(), "ms", &values, None)?;
        }
    }
    let refresh_ms = 1000.0 / refresh_hz.min(MAX_HZ);
    for scenario in SCROLLS {
        let mut frames = Vec::new();
        let mut janky = Vec::new();
        for trace in traces {
            let scroll = trace
                .scrolls
                .iter()
                .find(|scroll| scroll.scenario == scenario)
                .ok_or(format!("a run has no {scenario}"))?;
            let (mean, share) = frame_stats(&scroll.intervals_ms, refresh_hz)
                .ok_or(format!("{scenario} drew no frames"))?;
            frames.push(mean);
            janky.push(share);
        }
        row(
            format!("{scenario}-frame"),
            "ms",
            &frames,
            Some(refresh_ms * FRAME_SLACK),
        )?;
        row(format!("{scenario}-janky"), "%", &janky, Some(JANKY_BUDGET))?;
    }
    let blank: Vec<f64> = traces
        .iter()
        .map(|trace| {
            trace
                .scrolls
                .iter()
                .map(|scroll| scroll.longest_blank_ms)
                .fold(0.0, f64::max)
        })
        .collect();
    row("scroll-blank".to_owned(), "ms", &blank, Some(BLANK_BUDGET))?;
    Ok(rows)
}

/// The ids the script needs, read from the workspaces made for the run.
#[derive(Debug, Clone, PartialEq)]
pub struct Targets {
    pub main: String,
    pub second: String,
    pub diff_tab: String,
    pub largest: String,
    pub other: String,
    pub commit: String,
    pub diff_repo: String,
    pub diff_commit: String,
}

/// The script one run plays (`gitcroc_ui::perf::Script`).
pub fn script(targets: &Targets, started_ms: Option<u64>) -> Value {
    let Targets {
        main,
        second,
        diff_tab,
        largest,
        other,
        commit,
        diff_repo,
        diff_commit,
    } = targets;
    let idle = json!({"step": "idle"});
    let mut steps = vec![
        json!({"step": "cold-start", "scenario": "cold-start"}),
        idle.clone(),
        json!({"step": "select-repo", "scenario": "select-largest-repo", "repo": largest}),
        idle.clone(),
        json!({"step": "select-commit", "scenario": "select-commit", "hash": commit}),
        idle.clone(),
    ];
    for _ in 0..SWITCHES {
        for repo in [other, largest] {
            steps.push(json!({"step": "select-repo", "scenario": "switch-repo", "repo": repo}));
            steps.push(idle.clone());
        }
    }
    steps.push(json!({"step": "scroll-history", "scenario": "scroll-history"}));
    steps.push(idle.clone());
    for _ in 0..SWITCHES {
        for tab in [second, main] {
            steps.push(json!({"step": "switch-tab", "scenario": "switch-tab", "tab": tab}));
            steps.push(json!({"step": "history-shown", "scenario": "switch-tab-history"}));
            steps.push(idle.clone());
        }
    }
    steps.extend([
        json!({"step": "switch-tab", "scenario": null, "tab": diff_tab}),
        idle.clone(),
        json!({"step": "select-repo", "scenario": null, "repo": diff_repo}),
        idle.clone(),
        json!({"step": "select-commit", "scenario": null, "hash": diff_commit}),
        idle,
        json!({"step": "scroll-diff", "scenario": "scroll-diff"}),
    ]);
    json!({"started_ms": started_ms, "steps": steps})
}

/// Exits 0 when every row is within its budget, 1 when one is not, and 2
/// when the data is missing or a run fails.
pub fn run(options: &Options) -> ExitCode {
    match run_all(options) {
        Ok(rows) => {
            let pass = rows.iter().all(|row| row["pass"] == true);
            match serde_json::to_string_pretty(&rows) {
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
    if !options.app.is_file() {
        return Err(format!(
            "no perf-trace app at {}; `just bench-ui` builds it",
            options.app.display()
        ));
    }
    let set = DataSet::load(&options.data, GATING_SET)?.ok_or_else(|| {
        format!(
            "no {GATING_SET} data in {}; run `just bench-data {GATING_SET}` first",
            options.data.display()
        )
    })?;
    let home = tempfile::tempdir().map_err(|error| format!("cannot make a temp dir: {error}"))?;
    eprintln!("setting up the workspaces in {}", home.path().display());
    let targets = set_up(&set, &home.path().join("setup"))?;
    eprintln!("warm-up run");
    run_app(options, home.path(), &targets, "warm-up")?;
    let mut traces = Vec::with_capacity(options.runs);
    for run in 1..=options.runs {
        eprintln!("run {run} of {}", options.runs);
        let trace = run_app(options, home.path(), &targets, &format!("run-{run}"))?;
        if options.verbose {
            report(&trace);
        }
        traces.push(trace);
    }
    let refresh_hz = options
        .refresh_hz
        .or_else(|| traces.iter().find_map(|trace| trace.refresh_hz))
        .ok_or("the platform does not tell the display's refresh rate; pass --refresh-hz")?;
    eprintln!("display refresh rate: {refresh_hz:.1} Hz");
    judge(&traces, refresh_hz)
}

/// Prints one run's samples and scroll runs to stderr.
fn report(trace: &Trace) {
    let mut samples: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for sample in &trace.samples {
        samples
            .entry(&sample.scenario)
            .or_default()
            .push((sample.ms * 10.0).round() / 10.0);
    }
    for (scenario, values) in samples {
        eprintln!("  {scenario}: {values:?} ms");
    }
    for scroll in &trace.scrolls {
        let hz = trace.refresh_hz.unwrap_or(MAX_HZ);
        if let Some((mean, janky)) = frame_stats(&scroll.intervals_ms, hz) {
            eprintln!(
                "  {}: {} frames, mean {mean:.2} ms, {janky:.2}% dropped, longest blank {:.1} ms",
                scroll.scenario,
                scroll.intervals_ms.len(),
                scroll.longest_blank_ms
            );
        }
    }
}

/// The app's config and cache dirs under `home`, as the platform dirs on
/// Linux resolve them from the XDG variables.
fn dirs(home: &Path) -> (PathBuf, PathBuf) {
    (
        home.join("config").join("gitcroc"),
        home.join("cache").join("gitcroc"),
    )
}

/// Makes three workspaces in a throwaway config: every repo of the set,
/// the same repos again in a second tab, and one repo whose only commit
/// adds a long file, for `scroll-diff`. Fills each one's summary cache, and
/// leaves the first active.
fn set_up(set: &DataSet, home: &Path) -> Result<Targets, String> {
    let exe =
        std::env::current_exe().map_err(|error| format!("cannot find this program: {error}"))?;
    let (config, cache) = dirs(home);
    let workspace = |args: &[&str]| -> Result<Value, String> {
        let mut all: Vec<OsString> = vec![
            "workspace".into(),
            "--config-dir".into(),
            config.clone().into(),
        ];
        all.extend(args.iter().map(OsString::from));
        let stdout = run_self(&exe, &all)?;
        serde_json::from_slice(&stdout)
            .map_err(|error| format!("cannot parse the workspace: {error}"))
    };
    let add_all = |paths: &[&Path]| -> Result<(), String> {
        for path in paths {
            let path = path.to_str().ok_or("a repo path is not UTF-8")?;
            workspace(&["add", path])?;
        }
        Ok(())
    };
    let fill_cache = || -> Result<(), String> {
        let summaries: Vec<OsString> = vec![
            "summaries".into(),
            "--config-dir".into(),
            config.clone().into(),
            "--cache-dir".into(),
            cache.clone().into(),
        ];
        run_self(&exe, &summaries).map(drop)
    };
    let active = |tabs: &Value| -> Result<String, String> {
        tabs["active"]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "no workspace is active".to_owned())
    };
    let repo_named = |view: &Value, name: &str| -> Result<String, String> {
        view["repos"]
            .as_array()
            .and_then(|repos| repos.iter().find(|repo| repo["name"] == name))
            .and_then(|repo| repo["id"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| format!("the workspace has no repo {name}"))
    };
    let folder = |path: &Path| -> Result<String, String> {
        path.file_name()
            .and_then(|name| name.to_str())
            .map(str::to_owned)
            .ok_or_else(|| format!("{} has no folder name", path.display()))
    };

    let paths: Vec<&Path> = set.repos.iter().map(|repo| repo.path.as_path()).collect();
    add_all(&paths)?;
    fill_cache()?;
    let view = workspace(&["show"])?;
    let main = view["id"]
        .as_str()
        .ok_or("the workspace has no id")?
        .to_owned();
    let largest_path = &set.largest()?.path;
    let largest = repo_named(&view, &folder(largest_path)?)?;
    let other_path = paths
        .iter()
        .find(|path| *path != largest_path)
        .ok_or("the set has one repo")?;
    let other = repo_named(&view, &folder(other_path)?)?;
    let commit = pick_commit(set)?.ok_or("no commit to select")?;

    let second = active(&workspace(&["create", "Second tab"])?)?;
    add_all(&paths)?;
    fill_cache()?;

    let diff_path = home.join("diff-repo");
    long_file_repo(&diff_path)?;
    let diff_tab = active(&workspace(&["create", "Diff"])?)?;
    add_all(&[&diff_path])?;
    fill_cache()?;
    let diff_repo = repo_named(&workspace(&["show"])?, "diff-repo")?;
    let diff_commit = git(&diff_path, &["rev-parse", "HEAD"])?;

    workspace(&["open", &main])?;
    Ok(Targets {
        main,
        second,
        diff_tab,
        largest,
        other,
        commit,
        diff_repo,
        diff_commit,
    })
}

/// Runs git in `dir` with a fixed identity and returns its trimmed output.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=gitcroc bench",
            "-c",
            "user.email=bench@gitcroc.invalid",
        ])
        .args(args)
        .output()
        .map_err(|error| format!("cannot run git: {error}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

/// A repo whose one commit adds [`DIFF_LINES`] lines of Rust, which the
/// diff highlights.
fn long_file_repo(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir.join("src"))
        .map_err(|error| format!("cannot make {}: {error}", dir.display()))?;
    git(dir, &["init", "--quiet"])?;
    std::fs::write(dir.join("src").join("long.rs"), long_rust(DIFF_LINES))
        .map_err(|error| format!("cannot write the long file: {error}"))?;
    git(dir, &["add", "--all"])?;
    git(dir, &["commit", "--quiet", "-m", "Add a long file"])?;
    Ok(())
}

/// `lines` lines of varied Rust: doc comments, keywords, numbers, strings.
pub fn long_rust(lines: usize) -> String {
    let mut text = String::new();
    let mut item = 0usize;
    while text.lines().count() < lines {
        text.push_str(&format!(
            "/// Item {item}: adds its number to the value.\n\
             pub fn item_{item}(value: u64) -> Result<String, String> {{\n\
             \x20   let total = value.wrapping_mul({item}) + 0x{item:x};\n\
             \x20   if total % 3 == 0 {{\n\
             \x20       return Ok(format!(\"fizz {{total}}\"));\n\
             \x20   }}\n\
             \x20   // Not a multiple of three.\n\
             \x20   Err(\"item {item}\".to_owned())\n\
             }}\n\
             \n"
        ));
        item += 1;
    }
    text.lines()
        .take(lines)
        .fold(String::new(), |mut out, line| {
            out.push_str(line);
            out.push('\n');
            out
        })
}

/// Copies directory `from` to `to`, which must not exist.
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    let text = |error: std::io::Error| format!("cannot copy {}: {error}", from.display());
    std::fs::create_dir_all(to).map_err(text)?;
    for entry in std::fs::read_dir(from).map_err(text)? {
        let entry = entry.map_err(text)?;
        let target = to.join(entry.file_name());
        if entry.file_type().map_err(text)?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target).map_err(text)?;
        }
    }
    Ok(())
}

/// Starts the app with this run's script on a fresh copy of the config and
/// cache [`set_up`] made under `home/setup`, so every run starts from the
/// same tabs and selection, waits for it to end and reads its trace.
fn run_app(options: &Options, home: &Path, targets: &Targets, name: &str) -> Result<Trace, String> {
    let run = home.join("run");
    if run.exists() {
        std::fs::remove_dir_all(&run)
            .map_err(|error| format!("cannot remove {}: {error}", run.display()))?;
    }
    for dir in ["config", "cache"] {
        copy_dir(&home.join("setup").join(dir), &run.join(dir))?;
    }
    let home = run.as_path();
    let script_path = home.join(format!("{name}-script.json"));
    let trace_path = home.join(format!("{name}-trace.json"));
    let started_ms = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_millis();
    let started_ms = u64::try_from(started_ms).map_err(|error| error.to_string())?;
    let text = serde_json::to_string(&script(targets, Some(started_ms)))
        .map_err(|error| error.to_string())?;
    std::fs::write(&script_path, text)
        .map_err(|error| format!("cannot write {}: {error}", script_path.display()))?;
    let mut child = Command::new(&options.app)
        .arg("--bench")
        .arg(&script_path)
        .arg(&trace_path)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_STATE_HOME", home.join("state"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start {}: {error}", options.app.display()))?;
    let deadline = Instant::now() + RUN_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                let _ = child.kill();
                return Err(format!("{name} did not end within {RUN_TIMEOUT:?}"));
            }
            Err(error) => return Err(format!("cannot wait for the app: {error}")),
        }
    };
    if !status.success() {
        return Err(format!("{name}: the app exited with {status}"));
    }
    let text = std::fs::read_to_string(&trace_path)
        .map_err(|error| format!("cannot read {}: {error}", trace_path.display()))?;
    let trace: Trace = serde_json::from_str(&text)
        .map_err(|error| format!("cannot parse {}: {error}", trace_path.display()))?;
    if trace.inactive_frames > 0 {
        return Err(format!(
            "{name}: the window was not active for {} frames while scrolling, which slows \
             them down; keep it in front",
            trace.inactive_frames
        ));
    }
    Ok(trace)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trace(samples: &[(&str, f64)], scrolls: &[(&str, Vec<f64>, f64)]) -> Trace {
        Trace {
            refresh_hz: Some(144.0),
            samples: samples
                .iter()
                .map(|(scenario, ms)| Sample {
                    scenario: (*scenario).to_owned(),
                    ms: *ms,
                })
                .collect(),
            scrolls: scrolls
                .iter()
                .map(|(scenario, intervals, blank)| Scroll {
                    scenario: (*scenario).to_owned(),
                    intervals_ms: intervals.clone(),
                    longest_blank_ms: *blank,
                })
                .collect(),
            inactive_frames: 0,
        }
    }

    #[test]
    fn perf_8_frame_stats() {
        // 144 Hz: 6.94 ms a frame; over 10.4 ms is a dropped frame.
        let mut frames = vec![6.94; 98];
        frames.extend([10.0, 13.9]);
        let (mean, janky) = frame_stats(&frames, 144.0).unwrap();
        assert!((mean - (6.94 * 98.0 + 23.9) / 100.0).abs() < 1e-9);
        assert!((janky - 1.0).abs() < 1e-9);
        // A 240 Hz display is held to 144 Hz.
        assert_eq!(
            frame_stats(&[10.0], 240.0).map(|(_, janky)| janky),
            Some(0.0)
        );
        assert_eq!(
            frame_stats(&[11.0], 240.0).map(|(_, janky)| janky),
            Some(100.0)
        );
        // 60 Hz: 16.7 ms frames are on time.
        assert_eq!(
            frame_stats(&[16.7, 16.7], 60.0).map(|(_, janky)| janky),
            Some(0.0)
        );
        assert_eq!(frame_stats(&[], 144.0), None);
    }

    #[test]
    fn perf_8_bench_ui_judges_each_row() {
        let smooth = vec![6.94; 100];
        let mut stutter = vec![6.94; 97];
        stutter.extend([20.0, 20.0, 20.0]);
        let run = |select: f64, diff: Vec<f64>, blank: f64| {
            trace(
                &[
                    ("cold-start", 600.0),
                    ("select-largest-repo", select),
                    ("select-commit", 90.0),
                    ("switch-repo", 40.0),
                    ("switch-repo", 60.0),
                    ("switch-tab", 50.0),
                    ("switch-tab-history", 300.0),
                ],
                &[
                    ("scroll-history", smooth.clone(), blank),
                    ("scroll-diff", diff, 0.0),
                ],
            )
        };
        let traces = [
            run(150.0, smooth.clone(), 0.0),
            run(250.0, stutter.clone(), 120.0),
            run(260.0, stutter.clone(), 0.0),
        ];
        let rows = judge(&traces, 144.0).unwrap();
        let row = |name: &str| {
            rows.iter()
                .find(|row| row["scenario"] == name)
                .unwrap_or_else(|| panic!("no row {name}"))
                .clone()
        };
        assert_eq!(rows.len(), 11);
        // Reported, never failing.
        assert_eq!(row("switch-tab-history")["budget_ms"], Value::Null);
        assert_eq!(row("switch-tab-history")["pass"], true);
        // Over budget by its median, which pools every run's samples.
        assert_eq!(row("select-largest-repo")["median_ms"], 250.0);
        assert_eq!(row("select-largest-repo")["pass"], false);
        assert_eq!(row("switch-repo")["median_ms"], 50.0);
        assert_eq!(row("switch-repo")["runs"], 3);
        assert_eq!(row("cold-start")["pass"], true);
        // Two of three diff runs dropped 3% of their frames.
        assert_eq!(row("scroll-diff-janky")["unit"], "%");
        assert_eq!(row("scroll-diff-janky")["median_ms"], 3.0);
        assert_eq!(row("scroll-diff-janky")["pass"], false);
        assert_eq!(row("scroll-history-janky")["pass"], true);
        // The frame rows are judged against the refresh interval.
        assert_eq!(row("scroll-history-frame")["budget_ms"], 7.014);
        assert_eq!(row("scroll-history-frame")["pass"], true);
        assert_eq!(row("scroll-diff-frame")["pass"], false);
        // One run of three had a blank row for 120 ms.
        assert_eq!(row("scroll-blank")["median_ms"], 0.0);
        assert_eq!(row("scroll-blank")["p95_ms"], 120.0);
        assert_eq!(row("scroll-blank")["pass"], true);
        // On a 60 Hz display the same frames pass.
        let rows = judge(&traces, 60.0).unwrap();
        assert!(
            rows.iter()
                .find(|row| row["scenario"] == "scroll-diff-frame")
                .is_some_and(|row| row["pass"] == true)
        );
        // A run without a scroll is an error, not a pass.
        let missing = trace(&[("cold-start", 1.0)], &[]);
        assert!(judge(&[missing], 144.0).is_err());
    }

    #[test]
    fn perf_8_bench_ui_script_is_what_the_runner_reads() {
        let targets = Targets {
            main: "w-main".into(),
            second: "w-second".into(),
            diff_tab: "w-diff".into(),
            largest: "r-largest".into(),
            other: "r-other".into(),
            commit: "c-commit".into(),
            diff_repo: "r-diff".into(),
            diff_commit: "c-diff".into(),
        };
        let expected: Value = serde_json::from_str(include_str!(
            "../../gitcroc-ui/tests/integration/bench_script.json"
        ))
        .unwrap();
        assert_eq!(script(&targets, Some(1_700_000_000_000)), expected);
    }

    #[test]
    fn perf_8_long_file_has_its_lines() {
        let text = long_rust(DIFF_LINES);
        assert_eq!(text.lines().count(), DIFF_LINES);
        assert!(text.starts_with("/// Item 0"));
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn perf_8_bench_ui_arguments() {
        let args = |list: &[&str]| -> Vec<OsString> { list.iter().map(OsString::from).collect() };
        let options = parse_args(&args(&[
            "--app",
            "a",
            "--data",
            "d",
            "--runs",
            "3",
            "--refresh-hz",
            "60",
            "--verbose",
        ]))
        .unwrap();
        assert_eq!(
            (
                options.app,
                options.data,
                options.runs,
                options.refresh_hz,
                options.verbose
            ),
            (PathBuf::from("a"), PathBuf::from("d"), 3, Some(60.0), true)
        );
        assert!(
            parse_args(&args(&["--runs", "3"])).is_none(),
            "--app is required"
        );
        assert!(parse_args(&args(&["--app", "a", "--runs", "0"])).is_none());
        assert!(parse_args(&args(&["--app", "a", "--refresh-hz", "0"])).is_none());
        assert!(parse_args(&args(&["--app", "a", "extra"])).is_none());
    }
}
