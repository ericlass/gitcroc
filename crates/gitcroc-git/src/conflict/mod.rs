//! One conflicted file as the conflict editor shows it (OPS-4, OPS-6,
//! OPS-8): built from the index stages, never from the markers in the file
//! on disk, with both sides labelled by the branch or commit they come
//! from. A file the editor cannot show as text hunks is a choice between
//! the two versions. Nothing here writes; resolving is in
//! [`crate::ops::conflict`].

mod sides;
mod text;

use std::path::Path;

use gix::bstr::{BStr, ByteSlice};
use gix::index::entry::{Mode, Stage};
use gix::merge::blob::builtin_driver::text::ConflictStyle;
use serde::Serialize;

pub use sides::{SideMessages, SidesRead, SidesReader};
pub use text::{ConflictChunk, MARKER_SIZE, first_marker_line};

use crate::status::conflict::conflict_labels;
use crate::{ConflictLabels, GitRepo, StatusError};

/// The most bytes a side of a text conflict may have; a larger file is a
/// choice conflict, so no more crosses IPC (ARCH-8).
pub const MAX_CONFLICT_TEXT: u64 = 1024 * 1024;

/// git's rule for binary content: a NUL in the first 8,000 bytes.
const BINARY_PROBE: usize = 8_000;

/// A conflicted file (OPS-3, OPS-8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct Conflict {
    /// The path for display, decoded lossily (PLAT-2).
    pub path: String,
    pub labels: ConflictLabels,
    pub content: ConflictContent,
    /// The file on disk is not what git wrote for the conflict: it was
    /// edited outside the app (OPS-8). Only for a text conflict.
    pub edited_outside: bool,
    /// The file's text when it was edited outside, is UTF-8 and has at
    /// most [`MAX_CONFLICT_TEXT`] bytes: what the editor opens to keep the
    /// file as it is (OPS-8).
    pub on_disk: Option<String>,
    /// The stages as the index has them, for choosing a side and to tell
    /// this conflict from a later one of the same path.
    #[serde(skip)]
    stages: ConflictStages,
}

/// The base, ours and theirs stages of a conflicted path; `None` where a
/// side has no file.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct ConflictStages {
    pub base: Option<StageEntry>,
    pub ours: Option<StageEntry>,
    pub theirs: Option<StageEntry>,
}

/// A side's conflict stage: what choosing it puts in the index (OPS-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageEntry {
    pub id: gix::ObjectId,
    /// The mode as git writes it, `100644` for example.
    pub mode: u32,
}

impl StageEntry {
    /// The mode in octal, as `git update-index --cacheinfo` takes it.
    pub fn mode_text(&self) -> String {
        format!("{:o}", self.mode)
    }

    pub fn is_submodule(&self) -> bool {
        self.mode == Mode::COMMIT.bits()
    }
}

impl Conflict {
    /// What `side` holds in the index; `None` when it deleted the file.
    pub fn stage(&self, side: crate::ops::conflict::Side) -> Option<StageEntry> {
        match side {
            crate::ops::conflict::Side::Ours => self.stages.ours,
            crate::ops::conflict::Side::Theirs => self.stages.theirs,
        }
    }

    /// All three stages: the same for the same conflict, different once
    /// any side's content or mode changed.
    pub fn stages(&self) -> ConflictStages {
        self.stages
    }
}

/// What the editor shows of a conflicted file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConflictContent {
    /// The file merged in diff3 style, in order (OPS-8).
    Text { chunks: Vec<ConflictChunk> },
    /// A choice between the two versions (OPS-6).
    Choice {
        reason: ChoiceKind,
        ours: SideHolds,
        theirs: SideHolds,
    },
}

/// Why a conflict is a choice and not text hunks (OPS-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ChoiceKind {
    /// Binary content: a NUL in the first 8,000 bytes of a side, or the
    /// `binary`, `-diff` or `-merge` attribute.
    Binary,
    DeletedByUs,
    DeletedByThem,
    /// Neither side kept the file, as after a rename on each side.
    BothDeleted,
    /// Both sides are files whose modes differ, and neither kept the
    /// base's: both added with different modes.
    Mode,
    /// A side is a submodule.
    Submodule,
    /// A file on one side and a symbolic link on the other.
    TypeChange,
    /// A symbolic link on both sides.
    Symlink,
    /// A side is not UTF-8. Shown as text it would be rewritten in another
    /// encoding.
    NotUtf8,
    /// A side has over [`MAX_CONFLICT_TEXT`] bytes (ARCH-8).
    TooLarge,
    /// A line starts with a run of marker characters too long to tell
    /// conflict markers from text.
    MarkerLike,
}

/// What a side of a choice conflict holds (OPS-6).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SideHolds {
    Deleted,
    /// Sizes in bytes, typed for specta like [`crate::Image::size`].
    File {
        #[specta(type = u32)]
        size: u64,
        executable: bool,
    },
    Symlink {
        #[specta(type = u32)]
        size: u64,
    },
    /// The submodule's commit, a full hash.
    Submodule {
        commit: String,
    },
}

/// Why a conflict could not be read.
#[derive(Debug, thiserror::Error)]
pub enum ConflictError {
    #[error("the repo has no working tree")]
    NoWorkTree,
    #[error(transparent)]
    State(#[from] StatusError),
    #[error("cannot read the conflict: {0}")]
    Read(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl ConflictError {
    fn read(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        Self::Read(Box::new(error))
    }
}

/// `ours` and `theirs` merged against `base` in diff3 style, as resolved
/// text and hunks: the merge [`GitRepo::conflict`] uses on the index stages
/// (OPS-8), for texts from elsewhere, such as the prompt fixtures (AI-15).
/// `None` when a line starts with a run of marker characters too long to
/// tell conflict markers from text.
pub fn chunks_from_texts(base: &str, ours: &str, theirs: &str) -> Option<Vec<ConflictChunk>> {
    text::chunks(ours.as_bytes(), base.as_bytes(), theirs.as_bytes())
}

/// A conflict stage of the path: its blob and mode.
#[derive(Debug, Clone, Copy)]
struct Entry {
    id: gix::ObjectId,
    mode: Mode,
}

fn is_file(mode: Mode) -> bool {
    mode == Mode::FILE || mode == Mode::FILE_EXECUTABLE
}

/// The attributes that make git merge a path as binary, and its marker
/// size.
struct Attributes {
    binary: bool,
    marker_size: u8,
}

fn attributes(repo: &gix::Repository, path: &BStr) -> Result<Attributes, ConflictError> {
    use gix::worktree::stack::state::attributes::Source;

    let index = repo
        .index_or_load_from_head_or_empty()
        .map_err(ConflictError::read)?;
    let mut stack = repo
        .attributes_only(&index, Source::WorktreeThenIdMapping)
        .map_err(ConflictError::read)?;
    let mut outcome = stack.selected_attribute_matches(["diff", "merge", "conflict-marker-size"]);
    stack
        .at_entry(path, None)
        .map_err(ConflictError::read)?
        .matching_attributes(&mut outcome);
    let mut found = Attributes {
        binary: false,
        marker_size: MARKER_SIZE,
    };
    for matched in outcome.iter_selected() {
        let state = matched.assignment.state;
        match matched.assignment.name.as_str() {
            "diff" if state.is_unset() => found.binary = true,
            "merge"
                if state.is_unset() || state.as_bstr().is_some_and(|value| value == "binary") =>
            {
                found.binary = true;
            }
            "conflict-marker-size" => {
                if let Some(size) = state
                    .as_bstr()
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<u8>().ok())
                    .filter(|&size| size > 0)
                {
                    found.marker_size = size;
                }
            }
            _ => {}
        }
    }
    Ok(found)
}

/// How git writes conflicts in this repo: `merge.conflictStyle`.
fn conflict_style(repo: &gix::Repository) -> ConflictStyle {
    let config = repo.config_snapshot();
    match config
        .string("merge.conflictStyle")
        .as_deref()
        .map(|value| value.as_bytes())
    {
        Some(b"diff3") => ConflictStyle::Diff3,
        Some(b"zdiff3") => ConflictStyle::ZealousDiff3,
        _ => ConflictStyle::Merge,
    }
}

impl GitRepo {
    /// The conflict of the file at `git_path` (as git stores it), or `None`
    /// when the index has no conflict stages for it: it was resolved
    /// meanwhile.
    pub fn conflict(&self, git_path: &[u8]) -> Result<Option<Conflict>, ConflictError> {
        let repo = self.thread_local();
        let path = git_path.as_bstr();
        let index = repo.index_or_empty().map_err(ConflictError::read)?;
        let stage = |stage| {
            index
                .entry_by_path_and_stage(path, stage)
                .map(|entry| Entry {
                    id: entry.id,
                    mode: entry.mode,
                })
        };
        let (base, ours, theirs) = (stage(Stage::Base), stage(Stage::Ours), stage(Stage::Theirs));
        if base.is_none() && ours.is_none() && theirs.is_none() {
            return Ok(None);
        }
        let labels = conflict_labels(&repo)?;
        let reader = Reader { repo: &repo, path };
        let (content, edited) = match reader.choice(base, ours, theirs)? {
            Some(reason) => (
                ConflictContent::Choice {
                    reason,
                    ours: reader.holds(ours)?,
                    theirs: reader.holds(theirs)?,
                },
                None,
            ),
            None => reader.text(base, ours, theirs)?,
        };
        let entry = |entry: Option<Entry>| {
            entry.map(|entry| StageEntry {
                id: entry.id,
                mode: entry.mode.bits(),
            })
        };
        Ok(Some(Conflict {
            path: path.to_str_lossy().into_owned(),
            labels,
            content,
            edited_outside: edited.is_some(),
            on_disk: edited.flatten(),
            stages: ConflictStages {
                base: entry(base),
                ours: entry(ours),
                theirs: entry(theirs),
            },
        }))
    }
}

/// Reads the stages of one path.
struct Reader<'a> {
    repo: &'a gix::Repository,
    path: &'a BStr,
}

impl Reader<'_> {
    fn blob(&self, entry: Option<Entry>) -> Result<Vec<u8>, ConflictError> {
        match entry {
            Some(entry) if is_file(entry.mode) || entry.mode == Mode::SYMLINK => Ok(self
                .repo
                .find_blob(entry.id)
                .map_err(ConflictError::read)?
                .detach()
                .data),
            _ => Ok(Vec::new()),
        }
    }

    fn size(&self, entry: Entry) -> Result<u64, ConflictError> {
        Ok(self
            .repo
            .find_header(entry.id)
            .map_err(ConflictError::read)?
            .size())
    }

    fn holds(&self, entry: Option<Entry>) -> Result<SideHolds, ConflictError> {
        let Some(entry) = entry else {
            return Ok(SideHolds::Deleted);
        };
        Ok(if entry.mode == Mode::COMMIT {
            SideHolds::Submodule {
                commit: entry.id.to_string(),
            }
        } else if entry.mode == Mode::SYMLINK {
            SideHolds::Symlink {
                size: self.size(entry)?,
            }
        } else {
            SideHolds::File {
                size: self.size(entry)?,
                executable: entry.mode == Mode::FILE_EXECUTABLE,
            }
        })
    }

    /// Why the stages are a choice conflict, or `None` for text (OPS-6).
    fn choice(
        &self,
        base: Option<Entry>,
        ours: Option<Entry>,
        theirs: Option<Entry>,
    ) -> Result<Option<ChoiceKind>, ConflictError> {
        let (ours, theirs) = match (ours, theirs) {
            (None, None) => return Ok(Some(ChoiceKind::BothDeleted)),
            (None, Some(_)) => return Ok(Some(ChoiceKind::DeletedByUs)),
            (Some(_), None) => return Ok(Some(ChoiceKind::DeletedByThem)),
            (Some(ours), Some(theirs)) => (ours, theirs),
        };
        if ours.mode == Mode::COMMIT || theirs.mode == Mode::COMMIT {
            return Ok(Some(ChoiceKind::Submodule));
        }
        match (ours.mode == Mode::SYMLINK, theirs.mode == Mode::SYMLINK) {
            (true, true) => return Ok(Some(ChoiceKind::Symlink)),
            (true, false) | (false, true) => return Ok(Some(ChoiceKind::TypeChange)),
            (false, false) => {}
        }
        // A mode only one side changed merges as git merges it; two
        // different changes do not.
        let base_mode = base.map(|base| base.mode);
        if ours.mode != theirs.mode
            && base_mode != Some(ours.mode)
            && base_mode != Some(theirs.mode)
        {
            return Ok(Some(ChoiceKind::Mode));
        }
        let text_base = base.filter(|base| is_file(base.mode));
        for entry in [Some(ours), Some(theirs), text_base].into_iter().flatten() {
            if self.size(entry)? > MAX_CONFLICT_TEXT {
                return Ok(Some(ChoiceKind::TooLarge));
            }
        }
        if attributes(self.repo, self.path)?.binary {
            return Ok(Some(ChoiceKind::Binary));
        }
        let blobs = [
            self.blob(Some(ours))?,
            self.blob(Some(theirs))?,
            self.blob(text_base)?,
        ];
        if blobs
            .iter()
            .any(|blob| blob.iter().take(BINARY_PROBE).any(|&byte| byte == 0))
        {
            return Ok(Some(ChoiceKind::Binary));
        }
        if blobs.iter().any(|blob| std::str::from_utf8(blob).is_err()) {
            return Ok(Some(ChoiceKind::NotUtf8));
        }
        Ok(None)
    }

    /// The text conflict and, when the file on disk differs from what git
    /// wrote for it, its text if it can be shown (OPS-8).
    fn text(
        &self,
        base: Option<Entry>,
        ours: Option<Entry>,
        theirs: Option<Entry>,
    ) -> Result<(ConflictContent, Option<Option<String>>), ConflictError> {
        // A missing or non-file base (both added) merges against nothing.
        let base = self.blob(base.filter(|base| is_file(base.mode)))?;
        let ours = self.blob(ours)?;
        let theirs = self.blob(theirs)?;
        let Some(chunks) = text::chunks(&ours, &base, &theirs) else {
            let content = ConflictContent::Choice {
                reason: ChoiceKind::MarkerLike,
                ours: SideHolds::File {
                    size: ours.len() as u64,
                    executable: false,
                },
                theirs: SideHolds::File {
                    size: theirs.len() as u64,
                    executable: false,
                },
            };
            return Ok((content, None));
        };
        let work_dir = self.repo.workdir().ok_or(ConflictError::NoWorkTree)?;
        let edited = self.edited_outside(work_dir, &ours, &base, &theirs)?;
        Ok((ConflictContent::Text { chunks }, edited))
    }

    /// Whether the file on disk differs from the stages rendered as git
    /// renders them, ignoring the labels after the markers and line
    /// endings: `None` when it does not, else its text when it is UTF-8
    /// and at most [`MAX_CONFLICT_TEXT`] bytes. A missing or unreadable
    /// file counts as edited, without text.
    fn edited_outside(
        &self,
        work_dir: &Path,
        ours: &[u8],
        base: &[u8],
        theirs: &[u8],
    ) -> Result<Option<Option<String>>, ConflictError> {
        let Ok(relative) = gix::path::from_bstr(self.path) else {
            return Ok(Some(None));
        };
        let Ok(on_disk) = std::fs::read(work_dir.join(relative)) else {
            return Ok(Some(None));
        };
        let size = attributes(self.repo, self.path)?.marker_size;
        let style = conflict_style(self.repo);
        let rendered = text::render(ours, base, theirs, style, size);
        let size = usize::from(size);
        if rendered.is_some_and(|rendered| {
            text::without_labels(&on_disk, size) == text::without_labels(&rendered, size)
        }) {
            return Ok(None);
        }
        let shown = u64::try_from(on_disk.len()).is_ok_and(|len| len <= MAX_CONFLICT_TEXT);
        Ok(Some(
            shown.then(|| String::from_utf8(on_disk).ok()).flatten(),
        ))
    }
}
