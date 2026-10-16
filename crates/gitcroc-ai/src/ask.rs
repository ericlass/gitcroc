//! Asking a provider for a file's hunks until it gives a valid answer or
//! has had its one retry (AI-9).

use crate::answer::{FileAnswer, Usage};
use crate::provider::{Message, Provider, ProviderRequest, Role};
use crate::validate::validate;

/// How asking for a file ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileOutcome {
    /// A valid answer for every requested hunk.
    Answered(FileAnswer),
    /// "AI failed", with the reason shown to the user (AI-9).
    AiFailed { reason: String },
}

/// The outcome for a file, and the usage of each request sent for it in
/// order, `None` where the provider reported none (AI-14).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileResult {
    pub outcome: FileOutcome,
    pub usage: Vec<Option<Usage>>,
}

/// The user message of the retry after an invalid reply.
fn correction(error: &str) -> String {
    format!(
        "Your reply is not valid: {error}.\n\
         Reply again with the corrected JSON object only, and nothing else."
    )
}

/// Asks `provider` to resolve the hunks `hunk_ids` of a file. An invalid
/// reply is retried once with its validation error; a second invalid reply,
/// or any provider error, marks the file "AI failed" (AI-9). Provider errors
/// are not retried.
pub async fn ask(
    provider: &dyn Provider,
    mut request: ProviderRequest,
    hunk_ids: &[u32],
) -> FileResult {
    let mut usage = Vec::with_capacity(2);
    let mut retried = false;
    loop {
        let reply = match provider.complete(&request).await {
            Ok(reply) => reply,
            Err(error) => {
                usage.push(None);
                let reason = error.to_string();
                return FileResult {
                    outcome: FileOutcome::AiFailed { reason },
                    usage,
                };
            }
        };
        usage.push(reply.usage);
        match validate(&reply.text, hunk_ids) {
            Ok(answer) => {
                return FileResult {
                    outcome: FileOutcome::Answered(answer),
                    usage,
                };
            }
            Err(invalid) if retried => {
                return FileResult {
                    outcome: FileOutcome::AiFailed {
                        reason: format!("invalid reply after one retry: {invalid}"),
                    },
                    usage,
                };
            }
            Err(invalid) => {
                retried = true;
                request.follow_up.push(Message {
                    role: Role::Assistant,
                    text: reply.text,
                });
                request.follow_up.push(Message {
                    role: Role::User,
                    text: correction(&invalid.to_string()),
                });
            }
        }
    }
}
