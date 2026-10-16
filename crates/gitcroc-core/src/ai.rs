//! Whether a conflicted file may go to AI (AI-4) and, for one that may,
//! exactly what is sent for it (AI-5). Both are read from the conflict the
//! editor reads (OPS-8), never from the markers on disk. The core defines
//! these types because it does not depend on `gitcroc-ai`; the app maps
//! [`AiFileContext`] field by field onto the prompt's request. Nothing here
//! writes (ARCH-1).

use std::sync::Arc;

use gitcroc_git::{
    ChoiceKind, ConflictChunk, ConflictContent, ConflictError, GitRepo, GitRunner, Operation,
    SidesRead, SidesReader, glob_match,
};
use serde::Serialize;

use crate::conflict::{read, unmerged_of};
use crate::{Changes, ChangesError, WorkspaceId};

/// The most bytes the larger side of a file may have to go to AI (AI-4).
pub const AI_MAX_FILE: u64 = 1024 * 1024;

/// The lines of context sent before and after each hunk (AI-5).
pub const AI_CONTEXT_LINES: usize = 40;

/// The most commit messages sent per side (AI-5).
pub const AI_MAX_MESSAGES: usize = 10;

/// The most characters of a commit message sent (AI-5).
pub const AI_MAX_MESSAGE_CHARS: usize = 2_000;

/// Why a file may not go to AI (AI-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Ineligible {
    /// A conflict the editor shows as a choice (OPS-6).
    NotText { choice: ChoiceKind },
    /// A side has over [`AI_MAX_FILE`] bytes.
    OverOneMb,
    /// The path matches `pattern` of the exclude list.
    Excluded { pattern: String },
}

impl Ineligible {
    /// The reason as the editor shows it.
    pub fn message(&self) -> String {
        match self {
            Self::NotText { choice } => format!("not a text conflict ({})", choice_text(*choice)),
            Self::OverOneMb => "over 1 MB".to_owned(),
            Self::Excluded { pattern } => format!("matches the exclude list ({pattern})"),
        }
    }
}

fn choice_text(choice: ChoiceKind) -> &'static str {
    match choice {
        ChoiceKind::Binary => "binary",
        ChoiceKind::DeletedByUs => "deleted by us",
        ChoiceKind::DeletedByThem => "deleted by them",
        ChoiceKind::BothDeleted => "deleted on both sides",
        ChoiceKind::Mode => "different modes",
        ChoiceKind::Submodule => "submodule",
        ChoiceKind::TypeChange => "a file on one side and a link on the other",
        ChoiceKind::Symlink => "symbolic link",
        ChoiceKind::NotUtf8 => "not UTF-8",
        ChoiceKind::TooLarge => "too large",
        ChoiceKind::MarkerLike => "lines like conflict markers",
    }
}

/// Whether a file may go to AI (AI-4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiEligibility {
    Eligible,
    Ineligible {
        reason: Ineligible,
        /// [`Ineligible::message`].
        message: String,
    },
}

/// What `ai_eligibility_get` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiEligibilityRead {
    Read {
        eligibility: AiEligibility,
    },
    /// The file is no longer in conflict; read the lists again.
    AlreadyResolved,
}

/// The operation a conflict comes from, as sent (AI-5). `git am` is sent as
/// a rebase: it replays a commit onto `HEAD` the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AiOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    StashApply,
}

impl From<Option<Operation>> for AiOperation {
    fn from(operation: Option<Operation>) -> Self {
        match operation {
            Some(Operation::Merge) => Self::Merge,
            Some(Operation::Rebase | Operation::ApplyMailbox) => Self::Rebase,
            Some(Operation::CherryPick) => Self::CherryPick,
            Some(Operation::Revert) => Self::Revert,
            // Unmerged paths with no state file are a stash apply (OPS-1).
            Some(Operation::StashApply) | None => Self::StashApply,
        }
    }
}

/// A hunk as sent (AI-5): its three sides and the text around it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiHunk {
    /// Counted from 1 in file order, over all hunks of the file.
    pub id: u32,
    /// Up to [`AI_CONTEXT_LINES`] lines before and after the hunk, of the
    /// merged text: resolved text and the ours side of the other hunks.
    pub before: String,
    pub ours: String,
    pub base: String,
    pub theirs: String,
    pub after: String,
}

/// Exactly what is sent for one file (AI-5), and nothing else.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AiFileContext {
    /// The path from the repo top, for display.
    pub path: String,
    pub operation: AiOperation,
    /// The labels the editor shows (OPS-4).
    pub ours_label: String,
    pub theirs_label: String,
    /// The hunks asked for, in file order.
    pub hunks: Vec<AiHunk>,
    /// Up to [`AI_MAX_MESSAGES`] messages per side of the commits that
    /// touched the file since the merge base, newest first, each cut at
    /// [`AI_MAX_MESSAGE_CHARS`] characters.
    pub ours_commits: Vec<String>,
    pub theirs_commits: Vec<String>,
}

/// What [`ai_file_context`] read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AiContextRead {
    Context {
        // Boxed: a context is large, and the other variant small.
        context: Box<AiFileContext>,
    },
    Ineligible {
        reason: Ineligible,
        message: String,
    },
}

/// Why the context could not be read.
#[derive(Debug, thiserror::Error)]
pub enum AiContextError {
    #[error(transparent)]
    Conflict(#[from] ConflictError),
    #[error("the file has no hunk {0}")]
    UnknownHunk(u32),
}

/// Whether paths are compared ignoring case: where the file system does.
const IGNORE_CASE: bool = cfg!(any(windows, target_os = "macos"));

/// The first pattern of `exclude` that matches the file name or the path
/// from the repo top of `git_path`.
fn excluded_by<'a>(git_path: &[u8], exclude: &'a [String]) -> Option<&'a str> {
    let name = git_path
        .rsplit(|&byte| byte == b'/')
        .next()
        .unwrap_or(git_path);
    exclude
        .iter()
        .map(|pattern| pattern.trim())
        .find(|pattern| {
            !pattern.is_empty()
                && (glob_match(pattern, name, IGNORE_CASE)
                    || glob_match(pattern, git_path, IGNORE_CASE))
        })
}

/// The bytes of the larger side of a text conflict.
fn larger_side(chunks: &[ConflictChunk]) -> u64 {
    let (mut ours, mut theirs) = (0usize, 0usize);
    for chunk in chunks {
        let (left, right) = match chunk {
            ConflictChunk::Resolved { text } => (text.len(), text.len()),
            ConflictChunk::Hunk { ours, theirs, .. } => (ours.len(), theirs.len()),
        };
        ours = ours.saturating_add(left);
        theirs = theirs.saturating_add(right);
    }
    u64::try_from(ours.max(theirs)).unwrap_or(u64::MAX)
}

/// Why the file at `git_path` (as git stores it) with `content` may not go
/// to AI, or `None` (AI-4).
fn ineligible(
    git_path: &[u8],
    content: &ConflictContent,
    exclude: &[String],
) -> Option<Ineligible> {
    match content {
        ConflictContent::Choice {
            reason: ChoiceKind::TooLarge,
            ..
        } => return Some(Ineligible::OverOneMb),
        ConflictContent::Choice { reason, .. } => {
            return Some(Ineligible::NotText { choice: *reason });
        }
        // The editor's limit could change; this one stays.
        ConflictContent::Text { chunks } if larger_side(chunks) > AI_MAX_FILE => {
            return Some(Ineligible::OverOneMb);
        }
        ConflictContent::Text { .. } => {}
    }
    excluded_by(git_path, exclude).map(|pattern| Ineligible::Excluded {
        pattern: pattern.to_owned(),
    })
}

/// Whether the file at `git_path` (as git stores it) with `content` may go
/// to AI (AI-4): a text conflict the editor shows, at most [`AI_MAX_FILE`]
/// bytes on each side, matching no pattern of `exclude`. Patterns are
/// globs matched against the file name and the path from the repo top,
/// ignoring case on Windows and macOS.
pub fn ai_eligibility(
    git_path: &[u8],
    content: &ConflictContent,
    exclude: &[String],
) -> AiEligibility {
    match ineligible(git_path, content, exclude) {
        None => AiEligibility::Eligible,
        Some(reason) => AiEligibility::Ineligible {
            message: reason.message(),
            reason,
        },
    }
}

/// The last `count` lines of `text`.
fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let skip = lines.len().saturating_sub(count);
    lines.into_iter().skip(skip).collect()
}

/// The first `count` lines of `text`.
fn first_lines(text: &str, count: usize) -> String {
    text.split_inclusive('\n').take(count).collect()
}

/// The hunks of `chunks` whose ids `wanted` lists, or all of them, each
/// with the merged text around it.
fn hunks(chunks: &[ConflictChunk], wanted: Option<&[u32]>) -> Result<Vec<AiHunk>, AiContextError> {
    // The merged text, and where each hunk's ours side is in it.
    let mut merged = String::new();
    let mut places = Vec::new();
    for chunk in chunks {
        match chunk {
            ConflictChunk::Resolved { text } => merged.push_str(text),
            ConflictChunk::Hunk { ours, base, theirs } => {
                let start = merged.len();
                merged.push_str(ours);
                places.push((start, merged.len(), ours, base, theirs));
            }
        }
    }
    let count = u32::try_from(places.len()).unwrap_or(u32::MAX);
    if let Some(unknown) = wanted
        .into_iter()
        .flatten()
        .find(|&&id| id == 0 || id > count)
    {
        return Err(AiContextError::UnknownHunk(*unknown));
    }
    let mut hunks = Vec::new();
    for (id, (start, end, ours, base, theirs)) in (1..).zip(places) {
        if wanted.is_some_and(|wanted| !wanted.contains(&id)) {
            continue;
        }
        hunks.push(AiHunk {
            id,
            before: last_lines(merged.get(..start).unwrap_or_default(), AI_CONTEXT_LINES),
            ours: ours.clone(),
            base: base.clone(),
            theirs: theirs.clone(),
            after: first_lines(merged.get(end..).unwrap_or_default(), AI_CONTEXT_LINES),
        });
    }
    Ok(hunks)
}

/// `message` cut at [`AI_MAX_MESSAGE_CHARS`] characters.
fn cut(message: String) -> String {
    if message.chars().count() <= AI_MAX_MESSAGE_CHARS {
        return message;
    }
    message.chars().take(AI_MAX_MESSAGE_CHARS).collect()
}

/// What is sent for the conflicted file at `git_path` (as git stores it)
/// of `repo` (AI-5), or why it may not be sent (AI-4). Only the hunks
/// `wanted` lists are included, so a hunk resolved in the editor is not
/// sent; `None` includes all. `git` reads the commits where `gix` cannot
/// (ARCH-2). `Ok(None)` when the file is not in conflict. Blocks: call it
/// on a blocking thread.
pub fn ai_file_context(
    repo: &GitRepo,
    git: &GitRunner,
    git_path: &[u8],
    exclude: &[String],
    wanted: Option<&[u32]>,
) -> Result<Option<AiContextRead>, AiContextError> {
    let Some(conflict) = repo.conflict(git_path)? else {
        return Ok(None);
    };
    let chunks = match (
        &conflict.content,
        ineligible(git_path, &conflict.content, exclude),
    ) {
        (ConflictContent::Text { chunks }, None) => chunks,
        (_, Some(reason)) => {
            return Ok(Some(AiContextRead::Ineligible {
                message: reason.message(),
                reason,
            }));
        }
        // A choice is always ineligible.
        (ConflictContent::Choice { reason, .. }, None) => {
            let reason = Ineligible::NotText { choice: *reason };
            return Ok(Some(AiContextRead::Ineligible {
                message: reason.message(),
                reason,
            }));
        }
    };
    let hunks = hunks(chunks, wanted)?;
    let sides =
        SidesReader::new(repo.clone(), git.clone()).side_messages(git_path, AI_MAX_MESSAGES)?;
    let context = AiFileContext {
        path: conflict.path,
        operation: sides.operation.into(),
        ours_label: conflict.labels.ours,
        theirs_label: conflict.labels.theirs,
        hunks,
        ours_commits: sides.ours.into_iter().map(cut).collect(),
        theirs_commits: sides.theirs.into_iter().map(cut).collect(),
    };
    Ok(Some(AiContextRead::Context {
        context: Box::new(context),
    }))
}

/// Whether unmerged file `file` of view `handle` may go to AI (AI-4), with
/// the exclude list `exclude`, so the editor can show why not. `None` when
/// a newer view of the workspace replaced the view.
pub async fn ai_eligibility_get(
    changes: Arc<Changes>,
    id: WorkspaceId,
    handle: u32,
    file: u32,
    exclude: Vec<String>,
) -> Result<Option<AiEligibilityRead>, ChangesError> {
    let Some((view, path)) = unmerged_of(&changes, &id, handle, file)? else {
        return Ok(None);
    };
    Ok(Some(match read(&view, path.clone()).await? {
        Some(conflict) => AiEligibilityRead::Read {
            eligibility: ai_eligibility(&path, &conflict.content, &exclude),
        },
        None => AiEligibilityRead::AlreadyResolved,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(chunks: Vec<ConflictChunk>) -> ConflictContent {
        ConflictContent::Text { chunks }
    }

    #[test]
    fn ai_4_over_1_mb_holds_beside_the_editor_limit() {
        // A text conflict the editor shows with a side over 1 MB, as when
        // the editor's limit is raised.
        let big = "x".repeat(usize::try_from(AI_MAX_FILE).unwrap());
        let content = text(vec![
            ConflictChunk::Resolved { text: big },
            ConflictChunk::Hunk {
                ours: "o\n".into(),
                base: String::new(),
                theirs: String::new(),
            },
        ]);
        assert_eq!(
            ai_eligibility(b"a.txt", &content, &[]),
            AiEligibility::Ineligible {
                reason: Ineligible::OverOneMb,
                message: "over 1 MB".into()
            }
        );
        let small = text(vec![ConflictChunk::Resolved {
            text: "x".repeat(usize::try_from(AI_MAX_FILE).unwrap()),
        }]);
        assert_eq!(
            ai_eligibility(b"a.txt", &small, &[]),
            AiEligibility::Eligible
        );
    }

    #[test]
    fn ai_5_messages_are_cut_on_a_character_boundary() {
        let message = format!("{}é tail", "a".repeat(AI_MAX_MESSAGE_CHARS - 1));
        let cut = cut(message);
        assert_eq!(cut.chars().count(), AI_MAX_MESSAGE_CHARS);
        assert!(cut.ends_with('é'));
    }
}
