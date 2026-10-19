//! `gitcroc-cli ai eval`: the prompt fixture set run against a chosen
//! provider and prompt version, each hunk scored, so prompt versions can be
//! compared (AI-15). Run by hand through `just ai-eval`, never in `just
//! check` or CI. The provider is one of `settings.toml`, one given by its
//! kind and fields, or `--mock` replies for the tests. A key is read only
//! from the environment variable `--key-env` names, and never printed
//! (SEC-1). What is sent is the fixture's text, as AI-5 would send it for
//! the same conflict in a repo.

mod fixture;
mod score;

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use gitcroc_ai::{
    Confidence, FileOutcome, Logged, MockProvider, PROMPT_VERSION, PromptVersion, Provider, Usage,
    connect, resolve_file_with,
};
use gitcroc_core::{
    DEFAULT_CONTEXT_BUDGET, PlatformConfigDir, ProviderEntry, ProviderId, ProviderKind,
    SettingsStore,
};
use serde::Serialize;

use crate::ai::is_variable_name;
use crate::ai_resolve::{key_from, mock_replies, spec};
use crate::proposer::file_request;
use crate::workspace::GivenConfigDir;
use fixture::{FIXTURES, Fixture, load_all};
use score::{FileScore, HunkScore, Score, score};

pub const USAGE: &str = "ai eval (--provider <id> [--config-dir <dir>] | --kind <kind> [--model <model>] [--base-url <url>] [--path <program>] | --mock <replies.json>) [--key-env <NAME>] [--prompt <version> | --compare <v1,v2>] [--only <nn>] [--json]";

/// Where the provider comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// One of `settings.toml`.
    Settings {
        id: String,
        /// The platform config dir when `None`.
        config_dir: Option<PathBuf>,
    },
    /// One given by its fields, as `settings.toml` would hold them.
    Given {
        kind: ProviderKind,
        model: Option<String>,
        base_url: Option<String>,
        path: Option<PathBuf>,
    },
    /// A JSON array of the replies the mock gives, in order.
    Mock(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub source: Source,
    /// The environment variable holding the key.
    pub key_env: Option<String>,
    /// The prompt versions to run, in order: one, or the two compared.
    pub prompts: Vec<String>,
    /// The fixture whose number this is, or all.
    pub only: Option<String>,
    pub json: bool,
}

fn kind(name: &str) -> Option<ProviderKind> {
    Some(match name {
        "anthropic" => ProviderKind::Anthropic,
        "openai" => ProviderKind::Openai,
        "claude_code" => ProviderKind::ClaudeCode,
        "codex" => ProviderKind::Codex,
        _ => return None,
    })
}

/// Parses the arguments after `ai eval`.
pub fn parse_args(args: &[OsString]) -> Option<Options> {
    let mut flags: BTreeMap<&str, String> = BTreeMap::new();
    let mut json = false;
    let mut rest = args.iter();
    while let Some(flag) = rest.next() {
        let flag = flag.to_str()?;
        if flag == "--json" {
            (!json).then_some(())?;
            json = true;
            continue;
        }
        let known = [
            "--provider",
            "--config-dir",
            "--kind",
            "--model",
            "--base-url",
            "--path",
            "--mock",
            "--key-env",
            "--prompt",
            "--compare",
            "--only",
        ];
        let flag = known.into_iter().find(|known| *known == flag)?;
        let value = rest.next()?.to_str()?.to_owned();
        flags.insert(flag, value).is_none().then_some(())?;
    }
    let mut take = |flag: &str| flags.remove(flag);
    let key_env = take("--key-env");
    if key_env
        .as_deref()
        .is_some_and(|name| !is_variable_name(name))
    {
        return None;
    }
    let prompts = match (take("--prompt"), take("--compare")) {
        (Some(_), Some(_)) => return None,
        (Some(version), None) => vec![version],
        (None, Some(versions)) => {
            let versions: Vec<String> = versions.split(',').map(str::to_owned).collect();
            (versions.len() == 2).then_some(versions)?
        }
        (None, None) => vec![PROMPT_VERSION.to_owned()],
    };
    let only = take("--only");
    let source = match (take("--provider"), take("--kind"), take("--mock")) {
        (Some(id), None, None) => Source::Settings {
            id,
            config_dir: take("--config-dir").map(PathBuf::from),
        },
        (None, Some(name), None) => Source::Given {
            kind: kind(&name)?,
            model: take("--model"),
            base_url: take("--base-url"),
            path: take("--path").map(PathBuf::from),
        },
        (None, None, Some(file)) => Source::Mock(PathBuf::from(file)),
        _ => return None,
    };
    // A flag left over belongs to another source.
    flags.is_empty().then_some(Options {
        source,
        key_env,
        prompts,
        only,
        json,
    })
}

/// The provider of `options`, its context budget, and how it is named in
/// the report. With `settings.toml`'s AI debug logging on, prompts and
/// replies go to stderr (AI-7).
fn provider(options: &Options) -> Result<(Box<dyn Provider>, u32, String), String> {
    let (entry, debug_log) = match &options.source {
        Source::Mock(file) => {
            let provider = MockProvider::texts(mock_replies(file)?);
            return Ok((
                Box::new(provider),
                DEFAULT_CONTEXT_BUDGET,
                "mock".to_owned(),
            ));
        }
        Source::Settings { id, config_dir } => {
            let settings = match config_dir {
                Some(dir) => SettingsStore::load(&GivenConfigDir(dir.clone())),
                None => SettingsStore::load(&PlatformConfigDir),
            };
            let ai = settings.ai();
            let entry = ai
                .providers
                .into_iter()
                .find(|entry| entry.id.0 == *id)
                .ok_or_else(|| format!("settings.toml has no provider {id}"))?;
            (entry, ai.debug_log)
        }
        Source::Given {
            kind,
            model,
            base_url,
            path,
        } => {
            let entry = ProviderEntry {
                id: ProviderId("eval".to_owned()),
                name: kind.to_string(),
                kind: *kind,
                model: model.clone(),
                base_url: base_url.clone(),
                path: path.clone(),
                context_budget: DEFAULT_CONTEXT_BUDGET,
                consent: None,
            };
            (entry, false)
        }
    };
    let key = key_from(options.key_env.as_deref(), &entry);
    let provider = connect(spec(&entry, key)?).map_err(|error| error.to_string())?;
    let provider: Box<dyn Provider> = if debug_log {
        Box::new(Logged::new(
            provider,
            Arc::new(|text: &str| eprintln!("{text}")),
        ))
    } else {
        provider
    };
    let model = entry.model.as_deref().unwrap_or("its default model");
    let name = format!("{} ({}, {model})", entry.name, entry.kind);
    Ok((provider, entry.context_budget, name))
}

/// One fixture's result.
#[derive(Debug, Clone, Serialize)]
pub struct FixtureReport {
    pub name: String,
    /// `resolution` or `unresolved`.
    pub expected: &'static str,
    /// The file matches an accepted resolution, or is unresolved as
    /// expected.
    pub correct: bool,
    /// The accepted resolution the file matches, such as `2.txt`.
    pub matches: Option<String>,
    pub hunks: Vec<HunkScore>,
    /// Why the file is "AI failed" (AI-9).
    pub failed: Option<String>,
    /// Each request's tokens, `null` where the provider reported none
    /// (AI-14).
    pub usage: Vec<Option<Usage>>,
}

/// Hunks by score, of one confidence.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ScoreCounts {
    pub matches: u32,
    pub correct_unresolved: u32,
    pub unexpected_unresolved: u32,
    pub differs: u32,
}

impl ScoreCounts {
    fn add(&mut self, hunk: &HunkScore) {
        match (&hunk.score, hunk.correct) {
            (Score::Match, _) => self.matches += 1,
            (Score::Unresolved, true) => self.correct_unresolved += 1,
            (Score::Unresolved, false) => self.unexpected_unresolved += 1,
            (Score::Differs, _) => self.differs += 1,
            (Score::Failed, _) => {}
        }
    }
}

/// Every request's tokens added up (AI-14).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct UsageTotal {
    pub requests: u32,
    /// The requests whose provider reported their tokens.
    pub reported: u32,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Confidence against score.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct ByConfidence {
    pub high: ScoreCounts,
    pub medium: ScoreCounts,
    pub low: ScoreCounts,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub fixtures: u32,
    /// The fixtures right as a whole.
    pub correct_fixtures: u32,
    pub hunks: u32,
    pub matches: u32,
    pub correct_unresolved: u32,
    pub unexpected_unresolved: u32,
    pub differs: u32,
    pub failed: u32,
    pub confidence: ByConfidence,
    pub usage: UsageTotal,
}

/// One run of the set with one prompt version.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub prompt: String,
    pub provider: String,
    pub fixtures: Vec<FixtureReport>,
    pub summary: Summary,
}

fn summary(fixtures: &[FixtureReport]) -> Summary {
    let mut summary = Summary::default();
    for fixture in fixtures {
        summary.fixtures += 1;
        summary.correct_fixtures += u32::from(fixture.correct);
        for hunk in &fixture.hunks {
            summary.hunks += 1;
            let mut counts = ScoreCounts::default();
            counts.add(hunk);
            summary.matches += counts.matches;
            summary.correct_unresolved += counts.correct_unresolved;
            summary.unexpected_unresolved += counts.unexpected_unresolved;
            summary.differs += counts.differs;
            summary.failed += u32::from(hunk.score == Score::Failed);
            match hunk.confidence {
                Some(Confidence::High) => summary.confidence.high.add(hunk),
                Some(Confidence::Medium) => summary.confidence.medium.add(hunk),
                Some(Confidence::Low) => summary.confidence.low.add(hunk),
                None => {}
            }
        }
        for usage in &fixture.usage {
            summary.usage.requests += 1;
            if let Some(usage) = usage {
                summary.usage.reported += 1;
                summary.usage.input_tokens += u64::from(usage.input_tokens);
                summary.usage.output_tokens += u64::from(usage.output_tokens);
            }
        }
    }
    summary
}

/// Runs `fixtures` against `provider` with prompt `version`, one after
/// another, and scores each hunk.
pub async fn evaluate(
    provider: &dyn Provider,
    budget: u32,
    version: &PromptVersion,
    provider_name: &str,
    fixtures: &[Fixture],
) -> Report {
    let mut reports = Vec::with_capacity(fixtures.len());
    for fixture in fixtures {
        let request = file_request(fixture.context());
        let result = resolve_file_with(provider, version, &request, budget).await;
        let (hunks, file) = score(fixture, &result.outcome);
        reports.push(FixtureReport {
            name: fixture.name.clone(),
            expected: match fixture.expected {
                fixture::Expected::Accepted(_) => "resolution",
                fixture::Expected::Unresolved => "unresolved",
            },
            correct: file != FileScore::Wrong,
            matches: match file {
                FileScore::Matches(name) => Some(name),
                FileScore::Unresolved | FileScore::Wrong => None,
            },
            hunks,
            failed: match result.outcome {
                FileOutcome::AiFailed { reason } => Some(reason),
                FileOutcome::Answered(_) => None,
            },
            usage: result.usage,
        });
    }
    Report {
        prompt: version.name.to_owned(),
        provider: provider_name.to_owned(),
        summary: summary(&reports),
        fixtures: reports,
    }
}

/// A fixture whose score changed between the compared versions.
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub fixture: String,
    /// Each version's hunk scores, in the order of `--compare`.
    pub hunks: Vec<Vec<HunkScore>>,
}

/// The fixtures whose hunk scores differ between `runs`.
fn changes(runs: &[Report]) -> Vec<Change> {
    let Some((first, others)) = runs.split_first() else {
        return Vec::new();
    };
    first
        .fixtures
        .iter()
        .enumerate()
        .filter_map(|(index, fixture)| {
            let hunks: Vec<Vec<HunkScore>> = runs
                .iter()
                .filter_map(|run| run.fixtures.get(index))
                .map(|fixture| {
                    fixture
                        .hunks
                        .iter()
                        .map(|hunk| HunkScore {
                            confidence: None,
                            ..hunk.clone()
                        })
                        .collect()
                })
                .collect();
            let changed = others.iter().any(|run| {
                run.fixtures.get(index).map(|other| scores(&other.hunks))
                    != Some(scores(&fixture.hunks))
            });
            changed.then(|| Change {
                fixture: fixture.name.clone(),
                hunks,
            })
        })
        .collect()
}

fn scores(hunks: &[HunkScore]) -> Vec<(Score, bool)> {
    hunks
        .iter()
        .map(|hunk| (hunk.score.clone(), hunk.correct))
        .collect()
}

fn score_text(hunk: &HunkScore) -> &'static str {
    match (&hunk.score, hunk.correct) {
        (Score::Match, _) => "match",
        (Score::Differs, _) => "differs",
        (Score::Unresolved, true) => "unresolved (correct)",
        (Score::Unresolved, false) => "unresolved",
        (Score::Failed, _) => "AI failed",
    }
}

fn confidence_text(confidence: Option<Confidence>) -> &'static str {
    match confidence {
        Some(Confidence::High) => "high",
        Some(Confidence::Medium) => "medium",
        Some(Confidence::Low) => "low",
        None => "",
    }
}

/// The report as a table and its summary.
fn table(report: &Report) -> String {
    let mut out = format!("prompt {} · {}\n\n", report.prompt, report.provider);
    out.push_str(&format!(
        "{:<40} {:>4}  {:<22} {}\n",
        "fixture", "hunk", "score", "confidence"
    ));
    for fixture in &report.fixtures {
        for hunk in &fixture.hunks {
            out.push_str(&format!(
                "{:<40} {:>4}  {:<22} {}\n",
                fixture.name,
                hunk.id,
                score_text(hunk),
                confidence_text(hunk.confidence)
            ));
        }
        if let Some(reason) = &fixture.failed {
            out.push_str(&format!("{:<40}       {reason}\n", ""));
        }
    }
    let s = &report.summary;
    out.push_str(&format!(
        "\n{} fixtures, {} right as a whole; {} hunks: {} match, {} correct unresolved, {} unresolved \
         where a resolution was expected, {} differ, {} AI failed\n",
        s.fixtures, s.correct_fixtures, s.hunks, s.matches, s.correct_unresolved, s.unexpected_unresolved, s.differs, s.failed
    ));
    out.push_str("\nconfidence  match  correct unresolved  unexpected unresolved  differs\n");
    for (name, counts) in [
        ("high", s.confidence.high),
        ("medium", s.confidence.medium),
        ("low", s.confidence.low),
    ] {
        out.push_str(&format!(
            "{name:<10}  {:>5}  {:>18}  {:>21}  {:>7}\n",
            counts.matches, counts.correct_unresolved, counts.unexpected_unresolved, counts.differs
        ));
    }
    out.push_str(&format!(
        "\n{} requests, {} with usage: {} input tokens, {} output tokens\n",
        s.usage.requests, s.usage.reported, s.usage.input_tokens, s.usage.output_tokens
    ));
    out
}

fn compare_table(changed: &[Change], versions: &[String]) -> String {
    let mut out = format!(
        "\nfixtures whose score changed, {}:\n",
        versions.join(" → ")
    );
    if changed.is_empty() {
        out.push_str("none\n");
    }
    for change in changed {
        let per_version: Vec<String> = change
            .hunks
            .iter()
            .map(|hunks| hunks.iter().map(score_text).collect::<Vec<_>>().join(", "))
            .collect();
        out.push_str(&format!(
            "{:<40} {}\n",
            change.fixture,
            per_version.join(" → ")
        ));
    }
    out
}

/// The fixtures `only` names, or all of them.
fn chosen(root: &Path, only: Option<&str>) -> Result<Vec<Fixture>, String> {
    let fixtures = load_all(root)?;
    let Some(only) = only else {
        return Ok(fixtures);
    };
    let chosen: Vec<Fixture> = fixtures
        .into_iter()
        .filter(|fixture| fixture.number() == only || fixture.name == only)
        .collect();
    if chosen.is_empty() {
        return Err(format!("no fixture {only}"));
    }
    Ok(chosen)
}

fn eval(options: &Options) -> Result<String, String> {
    let versions = options
        .prompts
        .iter()
        .map(|name| PromptVersion::named(name).ok_or_else(|| format!("no prompt version {name}")))
        .collect::<Result<Vec<_>, _>>()?;
    let fixtures = chosen(Path::new(FIXTURES), options.only.as_deref())?;
    let (provider, budget, name) = provider(options)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("cannot start the async runtime: {error}"))?;
    let runs: Vec<Report> = versions
        .iter()
        .map(|version| {
            runtime.block_on(evaluate(
                provider.as_ref(),
                budget,
                version,
                &name,
                &fixtures,
            ))
        })
        .collect();
    let json = |value: serde_json::Value| {
        serde_json::to_string_pretty(&value)
            .map_err(|error| format!("cannot write the report as JSON: {error}"))
    };
    match runs.as_slice() {
        [report] if options.json => {
            json(serde_json::to_value(report).map_err(|error| error.to_string())?)
        }
        [report] => Ok(table(report)),
        _ if options.json => json(serde_json::json!({ "runs": runs, "changed": changes(&runs) })),
        _ => {
            let mut out: String = runs.iter().map(table).collect::<Vec<_>>().join("\n");
            out.push_str(&compare_table(&changes(&runs), &options.prompts));
            Ok(out)
        }
    }
}

/// Runs the eval and prints its report. Exits 2 when it cannot run.
pub fn run(options: &Options) -> ExitCode {
    match eval(options) {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::fixture::{Expected, Lines, lines};
    use super::*;

    fn args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn ai_15_eval_arguments() {
        assert_eq!(
            parse_args(&args(&[
                "--provider",
                "p2",
                "--key-env",
                "KEY",
                "--only",
                "07",
                "--json"
            ])),
            Some(Options {
                source: Source::Settings {
                    id: "p2".to_owned(),
                    config_dir: None,
                },
                key_env: Some("KEY".to_owned()),
                prompts: vec!["v1".to_owned()],
                only: Some("07".to_owned()),
                json: true,
            })
        );
        assert_eq!(
            parse_args(&args(&[
                "--kind",
                "claude_code",
                "--path",
                "/bin/claude",
                "--compare",
                "v1,v2"
            ])),
            Some(Options {
                source: Source::Given {
                    kind: ProviderKind::ClaudeCode,
                    model: None,
                    base_url: None,
                    path: Some(PathBuf::from("/bin/claude")),
                },
                key_env: None,
                prompts: vec!["v1".to_owned(), "v2".to_owned()],
                only: None,
                json: false,
            })
        );
        for bad in [
            &[][..],
            &["--provider", "p1", "--kind", "openai"],
            &["--kind", "gemini"],
            &["--provider", "p1", "--model", "m"],
            &["--kind", "openai", "--config-dir", "c"],
            &["--provider", "p1", "--prompt", "v1", "--compare", "v1,v2"],
            &["--provider", "p1", "--compare", "v1"],
            &["--provider", "p1", "--key-env", "sk-live-abc"],
            &["--provider", "p1", "--json", "--json"],
            &["--provider", "p1", "--only", "1", "--only", "2"],
            &["--provider", "p1", "--key", "sk"],
        ] {
            assert_eq!(parse_args(&args(bad)), None, "{bad:?}");
        }
    }

    /// A reply giving each hunk `texts`' text, with high confidence.
    fn reply(texts: &[Lines]) -> String {
        let hunks: Vec<serde_json::Value> = (1..)
            .zip(texts)
            .map(|(id, text)| {
                let resolution: String = text.iter().map(|line| format!("{line}\n")).collect();
                serde_json::json!({
                    "id": id,
                    "status": "resolved",
                    "resolution": resolution,
                    "explanation": "Kept both.",
                    "confidence": "high",
                })
            })
            .collect();
        serde_json::json!({ "hunks": hunks }).to_string()
    }

    #[tokio::test]
    async fn ai_15_eval_scores_a_matching_and_a_differing_reply() {
        let fixtures = load_all(Path::new(FIXTURES)).unwrap();
        let fixture = fixtures
            .iter()
            .find(|fixture| {
                matches!(&fixture.expected, Expected::Accepted(_)) && fixture.hunk_count() >= 2
            })
            .unwrap();
        let Expected::Accepted(accepted) = &fixture.expected else {
            unreachable!()
        };
        let split = accepted[0].splits[0].clone();
        let mut differing = split.clone();
        differing[1] = lines("something else entirely\n");
        let mock = MockProvider::texts([reply(&split), reply(&differing)]);
        let version = PromptVersion::current();
        let both = [fixture.clone(), fixture.clone()];

        let report = evaluate(&mock, DEFAULT_CONTEXT_BUDGET, version, "mock", &both).await;

        let [matching, differs] = &report.fixtures[..] else {
            panic!("{report:?}")
        };
        assert!(matching.correct);
        assert_eq!(matching.matches.as_deref(), Some("1.txt"));
        assert!(
            matching
                .hunks
                .iter()
                .all(|hunk| hunk.score == Score::Match && hunk.correct)
        );
        assert!(!differs.correct);
        assert_eq!(differs.hunks[0].score, Score::Match);
        assert_eq!(differs.hunks[1].score, Score::Differs);
        assert!(!differs.hunks[1].correct);
        assert_eq!(differs.hunks[1].confidence, Some(Confidence::High));
        let hunks = u32::try_from(fixture.hunk_count()).unwrap();
        assert_eq!(report.summary.fixtures, 2);
        assert_eq!(report.summary.correct_fixtures, 1);
        assert_eq!(report.summary.hunks, 2 * hunks);
        assert_eq!(report.summary.matches, 2 * hunks - 1);
        assert_eq!(report.summary.differs, 1);
        assert_eq!(report.summary.confidence.high.differs, 1);
        assert_eq!(report.summary.usage.requests, 2);
        // What was sent is the fixture as AI-5 sends it.
        let sent = &mock.requests()[0].user;
        assert!(sent.contains(&fixture.context().path));
    }

    #[tokio::test]
    async fn ai_15_eval_scores_unresolved_and_failed() {
        let fixtures = load_all(Path::new(FIXTURES)).unwrap();
        let unresolved = fixtures
            .iter()
            .find(|fixture| matches!(fixture.expected, Expected::Unresolved))
            .unwrap();
        let resolvable = fixtures
            .iter()
            .find(|fixture| {
                matches!(fixture.expected, Expected::Accepted(_)) && fixture.hunk_count() == 1
            })
            .unwrap();
        let left = r#"{"hunks":[{"id":1,"status":"unresolved","explanation":"Contradictory.","confidence":"low"}]}"#;
        // The last fixture gets no reply: the mock fails it.
        let mock = MockProvider::texts([left, left]);
        let set = [unresolved.clone(), resolvable.clone(), resolvable.clone()];

        let report = evaluate(
            &mock,
            DEFAULT_CONTEXT_BUDGET,
            PromptVersion::current(),
            "mock",
            &set,
        )
        .await;

        let [expected, unexpected, failed] = &report.fixtures[..] else {
            panic!("{report:?}")
        };
        assert!(expected.correct);
        assert_eq!(expected.hunks[0].score, Score::Unresolved);
        assert!(expected.hunks[0].correct);
        assert!(!unexpected.correct);
        assert_eq!(unexpected.hunks[0].score, Score::Unresolved);
        assert!(!unexpected.hunks[0].correct);
        assert_eq!(failed.hunks[0].score, Score::Failed);
        assert_eq!(failed.hunks[0].confidence, None);
        assert!(
            failed
                .failed
                .as_deref()
                .unwrap()
                .contains("no scripted reply left")
        );
        assert_eq!(report.summary.correct_unresolved, 1);
        assert_eq!(report.summary.unexpected_unresolved, 1);
        assert_eq!(report.summary.failed, 1);
        assert_eq!(report.summary.confidence.low.correct_unresolved, 1);
    }

    #[tokio::test]
    async fn ai_15_every_accepted_resolution_scores_as_a_match() {
        let fixtures = load_all(Path::new(FIXTURES)).unwrap();
        let mut set = Vec::new();
        let mut replies = Vec::new();
        for fixture in &fixtures {
            match &fixture.expected {
                Expected::Accepted(accepted) => {
                    for resolution in accepted {
                        set.push(fixture.clone());
                        replies.push(reply(&resolution.splits[0]));
                    }
                }
                Expected::Unresolved => {
                    set.push(fixture.clone());
                    replies.push(
                        r#"{"hunks":[{"id":1,"status":"unresolved","explanation":"Contradictory.","confidence":"high"}]}"#
                            .to_owned(),
                    );
                }
            }
        }
        let mock = MockProvider::texts(replies);

        let report = evaluate(
            &mock,
            DEFAULT_CONTEXT_BUDGET,
            PromptVersion::current(),
            "mock",
            &set,
        )
        .await;

        for fixture in &report.fixtures {
            assert!(fixture.correct, "{fixture:?}");
        }
        assert_eq!(
            report.summary.differs + report.summary.failed + report.summary.unexpected_unresolved,
            0
        );
    }

    #[test]
    fn ai_15_compare_lists_the_fixtures_whose_score_changed() {
        let hunk = |score: Score, correct: bool| HunkScore {
            id: 1,
            score,
            correct,
            confidence: Some(Confidence::High),
        };
        let run = |prompt: &str, scores: [Score; 2]| Report {
            prompt: prompt.to_owned(),
            provider: "mock".to_owned(),
            fixtures: ["01-a", "02-b"]
                .into_iter()
                .zip(scores)
                .map(|(name, score)| FixtureReport {
                    name: name.to_owned(),
                    expected: "resolution",
                    correct: score == Score::Match,
                    matches: None,
                    hunks: vec![hunk(score.clone(), score == Score::Match)],
                    failed: None,
                    usage: vec![None],
                })
                .collect(),
            summary: Summary::default(),
        };
        let runs = [
            run("v1", [Score::Match, Score::Differs]),
            run("v2", [Score::Match, Score::Match]),
        ];

        let changed = changes(&runs);

        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].fixture, "02-b");
        assert_eq!(changed[0].hunks[0][0].score, Score::Differs);
        assert_eq!(changed[0].hunks[1][0].score, Score::Match);
    }
}
