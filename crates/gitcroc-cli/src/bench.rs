//! The benchmark harness (PERF-8): named scenarios run against the reference
//! data sets of spec section 9, which `just bench-data` builds.
//!
//! Each scenario runs once to warm up and then a fixed number of times. The
//! result is printed as a JSON array with one object per scenario and data
//! set: median and 95th percentile in milliseconds, the budget, and whether
//! the median is within it.

use std::ffi::OsString;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use gitcroc_core::GitRepo;
use serde_json::{Value, json};

/// The set every budget is measured on (section 9).
const GATING_SET: &str = "target-project";

/// Stretch sets, measured when they exist locally and `--stretch` is given.
const STRETCH_SETS: [&str; 3] = ["big-history", "big-tree", "many-repos"];

/// The manifest `repo-gen` writes last into each set.
const MANIFEST: &str = "repo-gen.json";

const DEFAULT_RUNS: usize = 10;

pub const USAGE: &str = "bench [--data <dir>] [--runs <n>] [--stretch]";

/// One timed operation, called repeatedly.
type Measured = Box<dyn FnMut() -> Result<(), String>>;

/// A named scenario. `prepare` does the untimed setup and returns the
/// operation to time.
struct Scenario {
    name: &'static str,
    /// The section 9 budget, if the scenario has one.
    budget_ms: Option<f64>,
    sets: &'static [&'static str],
    prepare: fn(&DataSet) -> Result<Measured, String>,
}

const ALL_SETS: &[&str] = &["target-project", "big-history", "big-tree", "many-repos"];

/// Later tasks add their scenarios here (M1-02 to M1-06).
const SCENARIOS: &[Scenario] = &[Scenario {
    name: "open-repo",
    budget_ms: None,
    sets: ALL_SETS,
    prepare: open_repo,
}];

/// Opens the set's largest repo with `gix`; proves the plumbing.
fn open_repo(set: &DataSet) -> Result<Measured, String> {
    let path = set.largest()?.path.clone();
    Ok(Box::new(move || {
        let repo = GitRepo::open(&path).map_err(|error| error.to_string())?;
        black_box(repo);
        Ok(())
    }))
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

/// Runs a scenario on a set and returns its JSON result.
fn measure(scenario: &Scenario, set: &DataSet, runs: usize) -> Result<Value, String> {
    let mut operation = (scenario.prepare)(set)?;
    operation()?; // warm-up
    let mut samples = Vec::with_capacity(runs);
    for _ in 0..runs {
        let started = Instant::now();
        operation()?;
        samples.push(started.elapsed().as_secs_f64() * 1000.0);
    }
    let (median, p95) = summarize(&samples).ok_or("no samples")?;
    let round = |ms: f64| (ms * 1000.0).round() / 1000.0;
    Ok(json!({
        "scenario": scenario.name,
        "data_set": set.name,
        "runs": runs,
        "median_ms": round(median),
        "p95_ms": round(p95),
        "budget_ms": scenario.budget_ms,
        "pass": scenario.budget_ms.is_none_or(|budget| median <= budget),
    }))
}

/// Exits 0 when every scenario is within its budget, 1 when one is not, and 2
/// when the gating set is missing or a scenario fails to run.
pub fn run(options: &Options) -> ExitCode {
    match run_all(options) {
        Ok(results) => {
            let pass = results.iter().all(|result| result["pass"] == true);
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
        for scenario in SCENARIOS
            .iter()
            .filter(|s| s.sets.contains(&set.name.as_str()))
        {
            eprintln!("{} on {}", scenario.name, set.name);
            let result = measure(scenario, set, options.runs)
                .map_err(|error| format!("{} on {}: {error}", scenario.name, set.name))?;
            results.push(result);
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
