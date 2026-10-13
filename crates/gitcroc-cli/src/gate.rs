//! The nightly regression gate (PERF-8): compares a `bench` run with the
//! committed baseline, and makes a baseline from several runs.
//!
//! Only the gating set counts, and only scenarios with a section 9 budget
//! fail: PERF-8 gates budgets. A budgeted scenario fails when its median is
//! over its budget, or more than [`REGRESSION_LIMIT`] slower than the
//! baseline's and also at least [`REGRESSION_FLOOR_MS`] slower, since hosted
//! runners vary by more than 20% on scenarios of a few milliseconds. Other
//! slowdowns are reported. A scenario the baseline does not have yet is
//! reported and passes, so a new scenario can land before its baseline does.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

use crate::bench::{GATING_SET, summarize};

/// How much slower than the baseline a median may be (PERF-8).
pub const REGRESSION_LIMIT: f64 = 0.20;

/// How many milliseconds slower a median must also be to fail.
pub const REGRESSION_FLOOR_MS: f64 = 2.0;

pub const COMPARE_USAGE: &str = "bench-compare <results.json> <baseline.json>";
pub const BASELINE_USAGE: &str = "bench-baseline <results.json>...";

/// One scenario on one set, as `bench` prints it.
#[derive(Debug, Clone, PartialEq)]
pub struct Measured {
    pub scenario: String,
    pub data_set: String,
    pub median_ms: f64,
    pub budget_ms: Option<f64>,
}

impl Measured {
    fn from_json(value: &Value) -> Result<Self, String> {
        let text = |key: &str| {
            value[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("a result has no {key}: {value}"))
        };
        Ok(Self {
            scenario: text("scenario")?,
            data_set: text("data_set")?,
            median_ms: value["median_ms"]
                .as_f64()
                .ok_or_else(|| format!("a result has no median_ms: {value}"))?,
            budget_ms: value["budget_ms"].as_f64(),
        })
    }

    fn same(&self, other: &Self) -> bool {
        self.scenario == other.scenario && self.data_set == other.data_set
    }
}

/// What the gate found for one scenario of the gating set.
#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Pass,
    /// The baseline has no such scenario.
    NoBaseline,
    OverBudget {
        median_ms: f64,
        budget_ms: f64,
    },
    /// Over the limit, but without a budget or under the floor: reported.
    Slower {
        median_ms: f64,
        baseline_ms: f64,
        slower: f64,
    },
    /// `slower` is the fraction over the baseline, 0.21 for 21%.
    Regressed {
        median_ms: f64,
        baseline_ms: f64,
        slower: f64,
    },
}

impl Verdict {
    pub fn fails(&self) -> bool {
        matches!(self, Self::OverBudget { .. } | Self::Regressed { .. })
    }
}

/// Judges one result against its baseline, if there is one. A result over
/// its budget fails whatever the baseline says.
pub fn judge(result: &Measured, baseline: Option<&Measured>) -> Verdict {
    if let Some(budget_ms) = result.budget_ms
        && result.median_ms > budget_ms
    {
        return Verdict::OverBudget {
            median_ms: result.median_ms,
            budget_ms,
        };
    }
    let Some(baseline) = baseline else {
        return Verdict::NoBaseline;
    };
    let (median_ms, baseline_ms) = (result.median_ms, baseline.median_ms);
    let slower = median_ms / baseline_ms - 1.0;
    if slower <= REGRESSION_LIMIT {
        Verdict::Pass
    } else if result.budget_ms.is_some() && median_ms - baseline_ms >= REGRESSION_FLOOR_MS {
        Verdict::Regressed {
            median_ms,
            baseline_ms,
            slower,
        }
    } else {
        Verdict::Slower {
            median_ms,
            baseline_ms,
            slower,
        }
    }
}

/// Every gating-set result with its verdict.
pub fn compare<'a>(results: &'a [Measured], baseline: &[Measured]) -> Vec<(&'a Measured, Verdict)> {
    results
        .iter()
        .filter(|result| result.data_set == GATING_SET)
        .map(|result| {
            let base = baseline.iter().find(|base| base.same(result));
            (result, judge(result, base))
        })
        .collect()
}

/// The baseline from several runs: per scenario and set, the median of the
/// runs' medians. A scenario must be in every run.
pub fn baseline(runs: &[Vec<Measured>]) -> Result<Vec<Measured>, String> {
    let first = runs.first().ok_or("no runs to make a baseline from")?;
    first
        .iter()
        .map(|result| {
            let medians = runs
                .iter()
                .map(|run| {
                    run.iter()
                        .find(|other| other.same(result))
                        .map(|other| other.median_ms)
                        .ok_or_else(|| {
                            format!(
                                "{} on {} is not in every run",
                                result.scenario, result.data_set
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let (median_ms, _) = summarize(&medians).ok_or("no runs")?;
            Ok(Measured {
                median_ms,
                ..result.clone()
            })
        })
        .collect()
}

fn read(path: &Path) -> Result<Vec<Measured>, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let json: Value = serde_json::from_str(&text)
        .map_err(|error| format!("cannot parse {}: {error}", path.display()))?;
    json.as_array()
        .ok_or_else(|| format!("{} is not a JSON array", path.display()))?
        .iter()
        .map(Measured::from_json)
        .collect()
}

/// Parses the file arguments of `bench-compare` (two) or `bench-baseline`
/// (one or more).
pub fn parse_files(args: &[OsString]) -> Option<Vec<PathBuf>> {
    (!args.is_empty()).then(|| args.iter().map(PathBuf::from).collect())
}

/// Prints each gating-set verdict; exits 1 when one fails, 2 when the files
/// cannot be read.
pub fn run_compare(results: &Path, baseline: &Path) -> ExitCode {
    let (results, baseline) = match read(results).and_then(|r| Ok((r, read(baseline)?))) {
        Ok(both) => both,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let verdicts = compare(&results, &baseline);
    for (result, verdict) in &verdicts {
        let name = format!("{} on {}", result.scenario, result.data_set);
        match verdict {
            Verdict::Pass => println!("pass  {name}: {:.3} ms", result.median_ms),
            Verdict::NoBaseline => {
                println!(
                    "new   {name}: {:.3} ms, not in the baseline",
                    result.median_ms
                );
            }
            Verdict::Slower {
                median_ms,
                baseline_ms,
                slower,
            } => println!(
                "slow  {name}: {median_ms:.3} ms is {:.0}% slower than the baseline's {baseline_ms:.3} ms; not gated",
                slower * 100.0
            ),
            Verdict::OverBudget {
                median_ms,
                budget_ms,
            } => println!("FAIL  {name}: {median_ms:.3} ms is over its budget of {budget_ms} ms"),
            Verdict::Regressed {
                median_ms,
                baseline_ms,
                slower,
            } => println!(
                "FAIL  {name}: {median_ms:.3} ms is {:.0}% slower than the baseline's {baseline_ms:.3} ms",
                slower * 100.0
            ),
        }
    }
    if verdicts.iter().any(|(_, verdict)| verdict.fails()) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Prints the baseline of the runs as JSON, in `bench`'s shape.
pub fn run_baseline(files: &[PathBuf]) -> ExitCode {
    let made = files
        .iter()
        .map(|file| read(file))
        .collect::<Result<Vec<_>, _>>()
        .and_then(|runs| baseline(&runs));
    let baseline = match made {
        Ok(baseline) => baseline,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let round = |ms: f64| (ms * 1000.0).round() / 1000.0;
    let json: Vec<Value> = baseline
        .iter()
        .map(|result| {
            json!({
                "scenario": result.scenario,
                "data_set": result.data_set,
                "runs": files.len(),
                "median_ms": round(result.median_ms),
                "budget_ms": result.budget_ms,
            })
        })
        .collect();
    match serde_json::to_string_pretty(&json) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write the baseline as JSON: {error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(scenario: &str, median_ms: f64, budget_ms: Option<f64>) -> Measured {
        Measured {
            scenario: scenario.into(),
            data_set: GATING_SET.into(),
            median_ms,
            budget_ms,
        }
    }

    #[test]
    fn perf_8_regression_over_20_percent_fails() {
        let base = measured("history-first-page", 100.0, Some(200.0));
        let slower = |median_ms| {
            judge(
                &measured("history-first-page", median_ms, Some(200.0)),
                Some(&base),
            )
        };
        assert!(slower(121.0).fails(), "21% slower must fail");
        assert!(matches!(slower(121.0), Verdict::Regressed { .. }));
        assert_eq!(slower(119.0), Verdict::Pass, "19% slower passes");
        assert_eq!(slower(80.0), Verdict::Pass, "faster passes");

        // Over its budget fails even when the baseline was as slow.
        let over = measured("status-60k", 510.0, Some(500.0));
        let as_slow = measured("status-60k", 505.0, Some(500.0));
        assert_eq!(
            judge(&over, Some(&as_slow)),
            Verdict::OverBudget {
                median_ms: 510.0,
                budget_ms: 500.0
            }
        );
        assert!(judge(&over, None).fails());
    }

    #[test]
    fn perf_8_small_or_unbudgeted_slowdowns_are_only_reported() {
        // 50% slower, but 1 ms: runner noise.
        let base = measured("cold-start-core", 2.0, Some(1000.0));
        let small = judge(&measured("cold-start-core", 3.0, Some(1000.0)), Some(&base));
        assert!(matches!(small, Verdict::Slower { .. }));
        assert!(!small.fails());
        // No budget: reported, never failed.
        let base = measured("history-page-10", 20.0, None);
        let unbudgeted = judge(&measured("history-page-10", 40.0, None), Some(&base));
        assert!(matches!(unbudgeted, Verdict::Slower { .. }));
        assert!(!unbudgeted.fails());
    }

    #[test]
    fn perf_8_only_the_gating_set_counts_and_new_scenarios_pass() {
        let results = vec![
            measured("open-repo", 2.0, None),
            Measured {
                data_set: "big-history".into(),
                ..measured("history-first-page", 900.0, Some(300.0))
            },
        ];
        let verdicts = compare(&results, &[]);
        assert_eq!(verdicts.len(), 1, "only target-project is gated");
        assert_eq!(verdicts[0].1, Verdict::NoBaseline);
        assert!(!verdicts[0].1.fails());
    }

    #[test]
    fn perf_8_baseline_is_the_median_of_the_runs() {
        let run = |median_ms| vec![measured("select-commit", median_ms, Some(150.0))];
        let made = baseline(&[run(30.0), run(10.0), run(20.0)]).unwrap();
        assert_eq!(made, vec![measured("select-commit", 20.0, Some(150.0))]);

        let missing = baseline(&[run(30.0), Vec::new()]);
        assert!(
            missing.is_err(),
            "a scenario missing from a run is an error"
        );
    }
}
