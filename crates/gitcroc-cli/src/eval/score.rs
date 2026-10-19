//! How a fixture's hunks did (AI-15). A hunk *matches* when its proposal
//! is its part of an accepted resolution, after line endings are
//! normalized and trailing whitespace trimmed; it is scored against the
//! accepted resolution, and the way of reading it, that matches the most
//! hunks. The file matches when the whole resolved file, with the
//! proposals applied, equals an accepted resolution. An unresolved hunk is
//! correct when the fixture expects it.

use gitcroc_ai::{Confidence, FileAnswer, FileOutcome, HunkAnswer, HunkStatus};
use gitcroc_core::ConflictChunk;
use serde::Serialize;

use super::fixture::{Expected, Fixture, Lines, lines};

/// How one hunk did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Score {
    Match,
    Differs,
    Unresolved,
    /// The file is "AI failed" (AI-9).
    Failed,
}

/// One hunk's score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HunkScore {
    pub id: u32,
    pub score: Score,
    /// A match, or an unresolved hunk the fixture expects.
    pub correct: bool,
    /// `None` when the file failed.
    pub confidence: Option<Confidence>,
}

/// Whether `hunk`'s proposal is `part`.
fn is(hunk: &HunkAnswer, part: &Lines) -> bool {
    hunk.status == HunkStatus::Resolved
        && hunk.resolution.as_deref().map(lines).as_ref() == Some(part)
}

/// The text of each hunk in the accepted resolution, and its way of
/// reading it, that matches the most of `hunks`.
fn best<'a>(
    accepted: &'a [super::fixture::Accepted],
    hunks: &[HunkAnswer],
) -> Option<&'a Vec<Lines>> {
    accepted
        .iter()
        .flat_map(|resolution| &resolution.splits)
        .max_by_key(|split| {
            hunks
                .iter()
                .zip(split.iter())
                .filter(|(hunk, part)| is(hunk, part))
                .count()
        })
}

/// The whole file with `answer`'s resolutions in place of the hunks, or
/// `None` when a hunk is unresolved.
fn applied(chunks: &[ConflictChunk], answer: &FileAnswer) -> Option<String> {
    let mut hunks = answer.hunks.iter();
    let mut file = String::new();
    for chunk in chunks {
        match chunk {
            ConflictChunk::Resolved { text } => file.push_str(text),
            ConflictChunk::Hunk { .. } => file.push_str(hunks.next()?.resolution.as_deref()?),
        }
    }
    Some(file)
}

/// Whether a file is right as a whole.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileScore {
    /// It equals the accepted resolution of `accept/` this names.
    Matches(String),
    /// It is left unresolved, as expected.
    Unresolved,
    Wrong,
}

/// The scores of `fixture`'s hunks for `outcome`, and of the whole file.
pub fn score(fixture: &Fixture, outcome: &FileOutcome) -> (Vec<HunkScore>, FileScore) {
    let answer = match outcome {
        FileOutcome::Answered(answer) => answer,
        FileOutcome::AiFailed { .. } => {
            let failed = (1..=u32::try_from(fixture.hunk_count()).unwrap_or(u32::MAX))
                .map(|id| HunkScore {
                    id,
                    score: Score::Failed,
                    correct: false,
                    confidence: None,
                })
                .collect();
            return (failed, FileScore::Wrong);
        }
    };
    let scored = |hunk: &HunkAnswer, score: Score, correct: bool| HunkScore {
        id: hunk.id,
        score,
        correct,
        confidence: Some(hunk.confidence),
    };
    match &fixture.expected {
        Expected::Unresolved => {
            let hunks: Vec<HunkScore> = answer
                .hunks
                .iter()
                .map(|hunk| match hunk.status {
                    HunkStatus::Unresolved => scored(hunk, Score::Unresolved, true),
                    HunkStatus::Resolved => scored(hunk, Score::Differs, false),
                })
                .collect();
            let file = if hunks.iter().all(|hunk| hunk.correct) {
                FileScore::Unresolved
            } else {
                FileScore::Wrong
            };
            (hunks, file)
        }
        Expected::Accepted(accepted) => {
            let split = best(accepted, &answer.hunks);
            let hunks = answer
                .hunks
                .iter()
                .enumerate()
                .map(|(index, hunk)| {
                    if hunk.status == HunkStatus::Unresolved {
                        scored(hunk, Score::Unresolved, false)
                    } else if split
                        .and_then(|split| split.get(index))
                        .is_some_and(|part| is(hunk, part))
                    {
                        scored(hunk, Score::Match, true)
                    } else {
                        scored(hunk, Score::Differs, false)
                    }
                })
                .collect();
            let file = applied(&fixture.chunks, answer).map(|file| lines(&file));
            let matched =
                file.and_then(|file| accepted.iter().find(|resolution| resolution.lines == file));
            let file = matched.map_or(FileScore::Wrong, |resolution| {
                FileScore::Matches(resolution.name.clone())
            });
            (hunks, file)
        }
    }
}
