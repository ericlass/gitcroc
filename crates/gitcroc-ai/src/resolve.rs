//! Resolving one file: its hunks split over the context budget, each part
//! asked with its one retry, the answers merged in hunk order (AI-10).

use crate::answer::{Confidence, FileAnswer, HunkAnswer, HunkStatus};
use crate::ask::{FileOutcome, FileResult, ask};
use crate::prompt::{FileRequest, PromptVersion, plan, request};
use crate::provider::Provider;

/// The answer for a hunk that alone exceeds the budget and is not sent.
fn too_large(id: u32) -> HunkAnswer {
    HunkAnswer {
        id,
        status: HunkStatus::Unresolved,
        resolution: None,
        explanation: "too large".to_owned(),
        confidence: Confidence::Low,
    }
}

/// Asks `provider` to resolve the hunks of `file` in as few requests as fit
/// `budget` tokens, one after another. A hunk that alone exceeds the budget
/// comes back unresolved, "too large". When one request ends "AI failed",
/// the file does, with that reason, no partial answer and no further request.
pub async fn resolve_file(provider: &dyn Provider, file: &FileRequest, budget: u32) -> FileResult {
    resolve_file_with(provider, PromptVersion::current(), file, budget).await
}

/// [`resolve_file`] with prompt `version`, so the fixture set can compare
/// versions (AI-15).
pub async fn resolve_file_with(
    provider: &dyn Provider,
    version: &PromptVersion,
    file: &FileRequest,
    budget: u32,
) -> FileResult {
    let plan = plan(version, file, budget);
    let mut answered = Vec::with_capacity(file.hunks.len());
    let mut usage = Vec::with_capacity(plan.parts.len());
    for part in &plan.parts {
        let ids: Vec<u32> = part.iter().map(|hunk| hunk.id).collect();
        let result = ask(provider, request(version, file, part), &ids).await;
        usage.extend(result.usage);
        match result.outcome {
            FileOutcome::Answered(answer) => answered.extend(answer.hunks),
            FileOutcome::AiFailed { reason } => {
                return FileResult {
                    outcome: FileOutcome::AiFailed { reason },
                    usage,
                };
            }
        }
    }
    // The parts answer their hunks in file order, so the sent hunks' answers
    // come in the order of `file.hunks` with the too-large ones left out.
    let mut answered = answered.into_iter();
    let hunks = file
        .hunks
        .iter()
        .filter_map(|hunk| {
            if plan.too_large.contains(&hunk.id) {
                Some(too_large(hunk.id))
            } else {
                answered.next()
            }
        })
        .collect();
    FileResult {
        outcome: FileOutcome::Answered(FileAnswer { hunks }),
        usage,
    }
}
