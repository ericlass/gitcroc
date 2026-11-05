//! The workflows' handling of updater keys (SEC-7): the project's key in
//! `release.yml` (M6-28b), and the update test's throwaway key in
//! `packages.yml` (M6-28c). Each is read as text: its steps are the list
//! items under each job's `steps:`, indented six spaces.

// Integration test crate: helpers outside #[test] fns are test code too (QA-4).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::Path;

fn workflow(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.github/workflows")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn release_yml() -> String {
    workflow("release.yml")
}

/// A step: its job, its name and its lines, without comments.
#[derive(Debug)]
struct Step {
    job: String,
    name: String,
    lines: Vec<String>,
}

impl Step {
    fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// The value of the step's own `if:`, empty without one.
    fn condition(&self) -> &str {
        self.lines
            .iter()
            .find_map(|line| line.strip_prefix("        if: "))
            .unwrap_or("")
    }

    /// The names its `env:` sets.
    fn env(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut in_env = false;
        for line in &self.lines {
            if line == "        env:" {
                in_env = true;
            } else if in_env {
                match line.strip_prefix("          ") {
                    Some(entry) if !entry.starts_with(' ') => {
                        names.push(entry.split(':').next().unwrap_or("").to_owned());
                    }
                    Some(_) => {}
                    None => in_env = false,
                }
            }
        }
        names
    }
}

/// Every line that is not a comment, with the step it belongs to, if any.
fn lines_and_steps(yml: &str) -> (Vec<(Option<usize>, String)>, Vec<Step>) {
    let mut steps: Vec<Step> = Vec::new();
    let mut lines = Vec::new();
    let mut job = String::new();
    let mut current = None;
    for line in yml.lines() {
        if line.trim_start().starts_with('#') || line.trim().is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        if indent == 2 && line.ends_with(':') {
            job = line.trim().trim_end_matches(':').to_owned();
        }
        if indent <= 4 {
            current = None;
        }
        if let Some(item) = line.strip_prefix("      - ") {
            steps.push(Step {
                job: job.clone(),
                name: item.strip_prefix("name: ").unwrap_or(item).to_owned(),
                lines: Vec::new(),
            });
            current = Some(steps.len() - 1);
        }
        if let Some(index) = current {
            let step = &mut steps[index];
            if let Some(name) = line.strip_prefix("        name: ") {
                step.name = name.to_owned();
            }
            step.lines.push(line.to_owned());
        }
        lines.push((current, line.to_owned()));
    }
    (lines, steps)
}

#[test]
fn sec_7_tag_builds_verify_with_the_public_key() {
    let (_, steps) = lines_and_steps(&release_yml());
    let verifying: Vec<&Step> = steps
        .iter()
        .filter(|step| step.text().contains("release-manifest verify"))
        .collect();
    let tag: Vec<&&Step> = verifying
        .iter()
        .filter(|step| step.condition() == "github.event_name == 'push'")
        .collect();
    assert_eq!(
        tag.len(),
        1,
        "one verify step runs on tag builds: {verifying:#?}"
    );
    assert!(
        !tag[0].text().contains("--public-key"),
        "tag builds verify with the project's key: {:#?}",
        tag[0]
    );
    for step in &verifying {
        assert!(
            step.condition() == "github.event_name == 'push'"
                || step.condition() == "github.event_name != 'push'",
            "a verify step runs on tag builds or on dry runs: {step:#?}"
        );
    }

    // In the signing job, after signing and before the signatures leave it.
    let sign_job: Vec<&Step> = steps.iter().filter(|step| step.job == "sign").collect();
    let position = |what: &str| {
        sign_job
            .iter()
            .position(|step| step.text().contains(what))
            .unwrap_or_else(|| panic!("the sign job has a step with {what}"))
    };
    let signs = position("release-manifest sign");
    let verifies = sign_job
        .iter()
        .position(|step| step.name == tag[0].name)
        .unwrap();
    let uploads = position("actions/upload-artifact");
    assert!(signs < verifies && verifies < uploads, "{sign_job:#?}");
}

#[test]
fn sec_7_only_the_signing_step_sees_the_key() {
    let yml = release_yml();
    let (lines, steps) = lines_and_steps(&yml);
    let mut seen_in = Vec::new();
    for (step, line) in &lines {
        if line.contains("TAURI_SIGNING_PRIVATE_KEY") || line.contains("secrets.") {
            let step = step.unwrap_or_else(|| panic!("outside any step: {line}"));
            seen_in.push(step);
        }
    }
    seen_in.dedup();
    assert_eq!(seen_in.len(), 1, "the key appears in one step");
    let signing = &steps[seen_in[0]];
    assert_eq!(signing.job, "sign");
    assert_eq!(signing.name, "Sign the update packages");
    assert!(signing.text().contains("release-manifest sign"));
    let env = signing.env();
    for name in [
        "TAURI_SIGNING_PRIVATE_KEY",
        "TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
    ] {
        assert!(env.iter().any(|set| set == name), "{name} in env: {env:?}");
    }
    // Each line naming a secret sets it in `env`, for tag builds only.
    for line in signing
        .lines
        .iter()
        .filter(|line| line.contains("secrets."))
    {
        assert!(
            line.starts_with("          TAURI_SIGNING_PRIVATE_KEY"),
            "{line}"
        );
        assert!(
            line.contains("github.event_name == 'push' && secrets."),
            "{line}"
        );
    }
    // No secrets reach packages.yml.
    assert!(!yml.contains("secrets: inherit"));
    assert!(!yml.lines().any(|line| line.trim() == "secrets:"));
}

/// SEC-7: RELEASING.md checks a draft's signature by hand with the key the
/// workflow and the app check with.
#[test]
fn sec_7_releasing_checks_with_the_project_key() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../RELEASING.md");
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains(release_manifest::sign::PUBLIC_KEY));
    assert!(text.contains("minisign -V"));
}

/// M6-28c: in packages.yml, the update test's key exists only in update
/// test runs. Each build job makes its own; only its signing step has the
/// secret half in its environment; it is not uploaded, and the job's last
/// step deletes it. The feature is only turned on with the key.
#[test]
fn sec_7_update_test_key_stays_in_its_build_job() {
    let yml = workflow("packages.yml");
    let (_, steps) = lines_and_steps(&yml);
    let builds = ["windows", "macos", "linux"];
    for job in builds {
        let job_steps: Vec<&Step> = steps.iter().filter(|step| step.job == job).collect();
        let named = |name: &str| {
            job_steps
                .iter()
                .position(|step| step.name == name)
                .unwrap_or_else(|| panic!("{job} has the step {name}"))
        };
        let made = named("Make the update test's key");
        let signs = named("Sign B's update package with the update test's key");
        let deleted = named("Delete the update test's key");
        assert!(made < signs && signs < deleted, "{job}");
        assert_eq!(deleted, job_steps.len() - 1, "{job}: deleted last");
        assert_eq!(
            job_steps[deleted].condition(),
            "always() && inputs.update-test"
        );
        assert!(
            job_steps[deleted]
                .text()
                .contains("rm -f \"$RUNNER_TEMP/update-test.key\"")
        );
        for step in [made, signs] {
            assert_eq!(job_steps[step].condition(), "inputs.update-test", "{job}");
        }
    }
    for step in &steps {
        let text = step.text();
        if text.contains("TAURI_SIGNING_PRIVATE_KEY")
            || text.contains("cat \"$RUNNER_TEMP/update-test.key\"")
        {
            assert_eq!(
                step.name, "Sign B's update package with the update test's key",
                "the secret key is read in {step:#?}"
            );
        }
        if text.contains("update-test.key") {
            assert!(builds.contains(&step.job.as_str()), "{step:#?}");
            assert!(!text.contains("upload"), "{step:#?}");
        }
        if text.contains("gitcroc-app/update-test") {
            assert_eq!(step.name, "Make the update test's key", "{step:#?}");
        }
    }
    assert!(!yml.contains("secrets."));
}
