//! The prompt fixture set (AI-15): conflicts kept as plain text in
//! `crates/gitcroc-ai/fixtures/conflicts/<nn>-<name>/` (QA-2). Each holds
//! `meta.toml`, the whole file of each side in `base`, `ours` and `theirs`,
//! and in `accept/` either acceptable resolutions of the whole file, `1.txt`,
//! `2.txt` and so on, or a file `UNRESOLVED` saying why no resolution is
//! right. The hunks come from [`chunks_from_texts`], as the editor's would.

use std::path::{Path, PathBuf};

use gitcroc_core::{
    AI_MAX_MESSAGE_CHARS, AI_MAX_MESSAGES, AiFileContext, AiOperation, ConflictChunk, ai_hunks,
    chunks_from_texts,
};
use serde::Deserialize;

/// Where the fixture set is, in the source tree.
pub const FIXTURES: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../gitcroc-ai/fixtures/conflicts"
);

/// The operation of `meta.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MetaOperation {
    Merge,
    Rebase,
    CherryPick,
    Revert,
    StashApply,
}

impl From<MetaOperation> for AiOperation {
    fn from(operation: MetaOperation) -> Self {
        match operation {
            MetaOperation::Merge => Self::Merge,
            MetaOperation::Rebase => Self::Rebase,
            MetaOperation::CherryPick => Self::CherryPick,
            MetaOperation::Revert => Self::Revert,
            MetaOperation::StashApply => Self::StashApply,
        }
    }
}

/// `meta.toml`: what AI-5 sends besides the hunks.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Meta {
    path: String,
    operation: MetaOperation,
    ours_label: String,
    theirs_label: String,
    /// Newest first.
    #[serde(default)]
    ours_commits: Vec<String>,
    #[serde(default)]
    theirs_commits: Vec<String>,
}

/// A text as compared: its lines, each without its line ending and
/// trailing whitespace.
pub type Lines = Vec<String>;

/// `text` as compared (AI-15).
pub fn lines(text: &str) -> Lines {
    text.lines()
        .map(|line| line.trim_end().to_owned())
        .collect()
}

/// One acceptable resolution of the whole file.
#[derive(Debug, Clone)]
pub struct Accepted {
    /// The file in `accept/`, such as `1.txt`.
    pub name: String,
    pub lines: Lines,
    /// Each way to read the resolution as the resolved text of the conflict
    /// with one text per hunk in between: the text of each hunk, in order.
    /// The editor never changes the resolved text, so there is at least one.
    pub splits: Vec<Vec<Lines>>,
}

/// What a fixture's hunks should become.
#[derive(Debug, Clone)]
pub enum Expected {
    /// One of these.
    Accepted(Vec<Accepted>),
    /// Nothing: the hunk is left to the user, for the reason `UNRESOLVED`
    /// gives.
    Unresolved,
}

/// One fixture of the set.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// The folder's name, such as `01-rust-imports`.
    pub name: String,
    pub chunks: Vec<ConflictChunk>,
    pub expected: Expected,
    meta: Meta,
}

/// The most splits kept of one accepted resolution.
const MAX_SPLITS: usize = 16;

/// A part of a conflict: resolved text, or a hunk.
enum Part {
    Fixed(Lines),
    Hunk,
}

fn parts(chunks: &[ConflictChunk]) -> Vec<Part> {
    chunks
        .iter()
        .map(|chunk| match chunk {
            ConflictChunk::Resolved { text } => Part::Fixed(lines(text)),
            ConflictChunk::Hunk { .. } => Part::Hunk,
        })
        .collect()
}

/// Every way, up to [`MAX_SPLITS`], to read `accepted` from `at` on as
/// `parts`: the text each hunk got.
fn split_from(
    parts: &[Part],
    accepted: &[String],
    at: usize,
    hunks: &mut Vec<Lines>,
    out: &mut Vec<Vec<Lines>>,
) {
    if out.len() >= MAX_SPLITS {
        return;
    }
    let Some((first, rest)) = parts.split_first() else {
        if at == accepted.len() {
            out.push(hunks.clone());
        }
        return;
    };
    match first {
        Part::Fixed(fixed) => {
            if accepted.get(at..at + fixed.len()) == Some(fixed.as_slice()) {
                split_from(rest, accepted, at + fixed.len(), hunks, out);
            }
        }
        Part::Hunk => {
            for end in at..=accepted.len() {
                hunks.push(accepted.get(at..end).unwrap_or_default().to_vec());
                split_from(rest, accepted, end, hunks, out);
                hunks.pop();
            }
        }
    }
}

/// Every way to read `accepted` as the resolved text of `chunks` with a
/// text per hunk in between.
pub fn splits(chunks: &[ConflictChunk], accepted: &[String]) -> Vec<Vec<Lines>> {
    let mut out = Vec::new();
    split_from(&parts(chunks), accepted, 0, &mut Vec::new(), &mut out);
    out
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))
}

/// The resolutions or the `UNRESOLVED` of `dir`.
fn expected(dir: &Path, chunks: &[ConflictChunk]) -> Result<Expected, String> {
    let unresolved = dir.join("UNRESOLVED");
    if unresolved.exists() {
        if read(&unresolved)?.trim().is_empty() {
            return Err(format!(
                "{} says not why it is unresolved",
                unresolved.display()
            ));
        }
        let files = std::fs::read_dir(dir)
            .map_err(|error| format!("cannot read {}: {error}", dir.display()))?;
        if files.count() != 1 {
            return Err(format!(
                "{} holds UNRESOLVED and resolutions",
                dir.display()
            ));
        }
        return Ok(Expected::Unresolved);
    }
    let mut accepted = Vec::new();
    for number in 1.. {
        let name = format!("{number}.txt");
        let path = dir.join(&name);
        if !path.exists() {
            break;
        }
        let lines = lines(&read(&path)?);
        let splits = splits(chunks, &lines);
        if splits.is_empty() {
            return Err(format!(
                "{} changes the text outside the conflict hunks",
                path.display()
            ));
        }
        accepted.push(Accepted {
            name,
            lines,
            splits,
        });
    }
    if accepted.is_empty() {
        return Err(format!(
            "{} holds no 1.txt and no UNRESOLVED",
            dir.display()
        ));
    }
    Ok(Expected::Accepted(accepted))
}

impl Fixture {
    /// The fixture in `dir`.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let name = dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| format!("{} has no name", dir.display()))?
            .to_owned();
        let meta: Meta = toml::from_str(&read(&dir.join("meta.toml"))?)
            .map_err(|error| format!("{}/meta.toml: {error}", dir.display()))?;
        let [base, ours, theirs] = ["base", "ours", "theirs"].map(|side| read(&dir.join(side)));
        let chunks = chunks_from_texts(&base?, &ours?, &theirs?)
            .ok_or_else(|| format!("{} has a line of marker characters", dir.display()))?;
        if !chunks
            .iter()
            .any(|chunk| matches!(chunk, ConflictChunk::Hunk { .. }))
        {
            return Err(format!("{} has no conflict hunk", dir.display()));
        }
        let expected = expected(&dir.join("accept"), &chunks)?;
        Ok(Self {
            name,
            chunks,
            expected,
            meta,
        })
    }

    /// The number the folder's name starts with, such as `01`.
    pub fn number(&self) -> &str {
        self.name.split('-').next().unwrap_or_default()
    }

    /// The number of hunks.
    pub fn hunk_count(&self) -> usize {
        self.chunks
            .iter()
            .filter(|chunk| matches!(chunk, ConflictChunk::Hunk { .. }))
            .count()
    }

    /// What is sent for the fixture (AI-5): what the core would send for the
    /// same conflict in a repo.
    pub fn context(&self) -> AiFileContext {
        let messages = |messages: &[String]| -> Vec<String> {
            messages
                .iter()
                .take(AI_MAX_MESSAGES)
                .map(|message| message.chars().take(AI_MAX_MESSAGE_CHARS).collect())
                .collect()
        };
        AiFileContext {
            path: self.meta.path.clone(),
            operation: self.meta.operation.into(),
            ours_label: self.meta.ours_label.clone(),
            theirs_label: self.meta.theirs_label.clone(),
            hunks: ai_hunks(&self.chunks),
            ours_commits: messages(&self.meta.ours_commits),
            theirs_commits: messages(&self.meta.theirs_commits),
        }
    }
}

/// Every fixture in `root`, by name. Fails on the first that does not load.
pub fn load_all(root: &Path) -> Result<Vec<Fixture>, String> {
    let entries = std::fs::read_dir(root)
        .map_err(|error| format!("cannot read {}: {error}", root.display()))?;
    let mut dirs: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    dirs.sort();
    dirs.iter().map(|dir| Fixture::load(dir)).collect()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    fn set() -> Vec<Fixture> {
        load_all(Path::new(FIXTURES)).unwrap()
    }

    #[test]
    fn ai_15_at_least_30_fixtures_each_with_an_accepted_resolution() {
        let fixtures = set();
        assert!(fixtures.len() >= 30, "{} fixtures", fixtures.len());
        let mut numbers: Vec<&str> = fixtures.iter().map(Fixture::number).collect();
        numbers.dedup();
        assert_eq!(numbers.len(), fixtures.len(), "a number is used twice");
        let mut unresolved = 0;
        for fixture in &fixtures {
            assert!(
                fixture.number().len() == 2 && fixture.number().bytes().all(|b| b.is_ascii_digit()),
                "{}",
                fixture.name
            );
            assert!(fixture.hunk_count() >= 1, "{}", fixture.name);
            assert_eq!(fixture.context().hunks.len(), fixture.hunk_count());
            match &fixture.expected {
                Expected::Accepted(accepted) => {
                    assert!(!accepted.is_empty(), "{}", fixture.name);
                    for resolution in accepted {
                        assert!(
                            !resolution.splits.is_empty(),
                            "{} {}",
                            fixture.name,
                            resolution.name
                        );
                    }
                }
                Expected::Unresolved => {
                    unresolved += 1;
                    // The whole file is left to the user, so it has one hunk.
                    assert_eq!(fixture.hunk_count(), 1, "{}", fixture.name);
                }
            }
        }
        assert!(unresolved >= 5, "{unresolved} fixtures expect unresolved");
    }

    #[test]
    fn ai_15_a_resolution_is_split_at_the_resolved_text() {
        let chunks =
            chunks_from_texts("a\nb\nc\nd\ne\n", "a\nB\nc\nd\nE\n", "a\nbb\nc\nd\nee\n").unwrap();
        let accepted = lines("a\nB\nbb\nc\nd\n\nE ee  \r\n");
        assert_eq!(
            splits(&chunks, &accepted),
            vec![vec![lines("B\nbb\n"), lines("\nE ee\n")]]
        );
        // Text outside the hunks changed: no split.
        assert!(splits(&chunks, &lines("a\nB\nC\nd\nE\n")).is_empty());
    }
}
