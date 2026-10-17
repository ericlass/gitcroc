//! The core's [`Proposer`] over `gitcroc-ai`'s ask loop (M4-08): the file's
//! context mapped field by field onto the prompt's request, and the answer
//! back onto the core's proposal. `gitcroc-app` has its own copy; the
//! mapping is all the two share.

use gitcroc_ai::{
    Confidence, FileOutcome, FileRequest, FileResult, HunkRequest, HunkStatus, Operation, Provider,
    resolve_file,
};
use gitcroc_core::{
    AiFileContext, AiOperation, AiUsage, FileProposal, Kill, ProposalConfidence, ProposalStatus,
    ProposeFuture, ProposedHunk, Proposer,
};

/// A provider asked through the ask loop, within its context budget
/// (AI-10).
pub struct AiProposer {
    provider: Box<dyn Provider>,
    budget: u32,
}

impl AiProposer {
    pub fn new(provider: Box<dyn Provider>, budget: u32) -> Self {
        Self { provider, budget }
    }
}

impl Proposer for AiProposer {
    fn propose(&self, context: AiFileContext, cancel: Kill) -> ProposeFuture<'_> {
        Box::pin(async move {
            let file = file_request(context);
            // Dropping the ask drops its request, or kills its CLI.
            tokio::select! {
                biased;
                () = cancel.requested() => Err("cancelled".to_owned()),
                result = resolve_file(self.provider.as_ref(), &file, self.budget) => proposal(result),
            }
        })
    }
}

fn operation(operation: AiOperation) -> Operation {
    match operation {
        AiOperation::Merge => Operation::Merge,
        AiOperation::Rebase => Operation::Rebase,
        AiOperation::CherryPick => Operation::CherryPick,
        AiOperation::Revert => Operation::Revert,
        AiOperation::StashApply => Operation::StashApply,
    }
}

fn file_request(context: AiFileContext) -> FileRequest {
    FileRequest {
        path: context.path,
        operation: operation(context.operation),
        ours_label: context.ours_label,
        theirs_label: context.theirs_label,
        hunks: context
            .hunks
            .into_iter()
            .map(|hunk| HunkRequest {
                id: hunk.id,
                before: hunk.before,
                ours: hunk.ours,
                base: hunk.base,
                theirs: hunk.theirs,
                after: hunk.after,
            })
            .collect(),
        ours_commits: context.ours_commits,
        theirs_commits: context.theirs_commits,
    }
}

fn proposal(result: FileResult) -> Result<FileProposal, String> {
    let answer = match result.outcome {
        FileOutcome::Answered(answer) => answer,
        FileOutcome::AiFailed { reason } => return Err(reason),
    };
    Ok(FileProposal {
        hunks: answer
            .hunks
            .into_iter()
            .map(|hunk| ProposedHunk {
                id: hunk.id,
                status: match hunk.status {
                    HunkStatus::Resolved => ProposalStatus::Resolved,
                    HunkStatus::Unresolved => ProposalStatus::Unresolved,
                },
                resolution: hunk.resolution,
                explanation: hunk.explanation,
                confidence: match hunk.confidence {
                    Confidence::High => ProposalConfidence::High,
                    Confidence::Medium => ProposalConfidence::Medium,
                    Confidence::Low => ProposalConfidence::Low,
                },
            })
            .collect(),
        usage: result
            .usage
            .into_iter()
            .map(|usage| {
                usage.map(|usage| AiUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                })
            })
            .collect(),
    })
}
