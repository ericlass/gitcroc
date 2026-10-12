//! Commit details, changed files and the diff of one file at a time (UI-14,
//! UI-16, UI-17, UI-18, ARCH-8).
//!
//! Paths stay git's bytes inside the core (PLAT-2). A changed file crosses
//! IPC as a display string and its id, its index in the list; callers ask
//! for a diff with the [`ChangedFile`] the id names, never with a path.

mod text;

use gix::bstr::{BString, ByteSlice};
use gix::diff::blob::platform::prepare_diff::Operation;
use gix::diff::blob::platform::resource::Data;
use gix::diff::blob::{Platform, ResourceKind};
use gix::hash::ObjectId;
use gix::object::tree::EntryMode;
use gix::object::tree::diff::ChangeDetached;
use serde::Serialize;

pub use text::{Context, Hunk, Line, LineKind, TextDiff};

use crate::{GitRepo, Time};

/// A diff with more changed lines is only counted, unless the caller asks
/// to load it anyway (UI-18).
pub const MAX_DIFF_LINES: u32 = 20_000;

/// Images up to this size per side are returned whole (UI-18).
pub const MAX_IMAGE_BYTES: u64 = 10 * 1024 * 1024;

/// Line counts are given for lists of at most this many files; longer lists
/// leave them out so the list stays fast.
pub const MAX_COUNTED_FILES: usize = 1_000;

/// A commit author or committer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Person {
    pub name: String,
    pub email: String,
    pub time: Time,
}

/// A commit's header (UI-14). Text is decoded lossily (SEC-3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommitDetails {
    pub hash: String,
    pub author: Person,
    pub committer: Person,
    /// The full message.
    pub message: String,
    pub parents: Vec<String>,
}

/// What a diff compares. M1-04 adds the working copy and the index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    /// A commit against one of its parents, 0 being the first (UI-16). A
    /// root commit is compared with the empty tree.
    Commit { hash: String, parent: usize },
}

/// How a file changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    /// File, symbolic link and submodule turned into one another.
    TypeChanged,
}

/// Both modes, as git writes them (`100644`), when they differ.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModeChange {
    pub old: String,
    pub new: String,
}

/// One entry of a changed-file list. It holds no content (ARCH-8).
#[derive(Debug, Clone, Serialize)]
pub struct ChangedFile {
    /// The index in the list it came from.
    pub id: usize,
    /// The path for display, decoded lossily; never sent back.
    pub path: String,
    /// The path before a rename, for display.
    pub old_path: Option<String>,
    pub status: FileStatus,
    pub mode_change: Option<ModeChange>,
    /// Line counts for text files when the list is short enough
    /// ([`MAX_COUNTED_FILES`]).
    pub added: Option<u32>,
    pub removed: Option<u32>,
    #[serde(skip)]
    old: Option<Side>,
    #[serde(skip)]
    new: Option<Side>,
    /// The commit and parent the list was made for.
    #[serde(skip)]
    origin: (ObjectId, usize),
}

impl ChangedFile {
    /// The path as git stores it: bytes, `/`-separated (PLAT-2).
    pub fn git_path(&self) -> &[u8] {
        self.new
            .as_ref()
            .or(self.old.as_ref())
            .map_or(&[], |side| side.path.as_slice())
    }
}

/// One side of a changed file.
#[derive(Debug, Clone)]
struct Side {
    id: ObjectId,
    mode: EntryMode,
    path: BString,
}

/// Options of a file diff (UI-17, UI-18).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiffOptions {
    pub ignore_whitespace: bool,
    pub context: Context,
    /// Return the hunks even when there are over [`MAX_DIFF_LINES`].
    pub load_anyway: bool,
}

/// An image format shown as a picture (UI-18).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    Png,
    Jpeg,
    Gif,
    Webp,
    Bmp,
}

impl ImageFormat {
    /// The format whose signature `data` starts with.
    fn sniff(data: &[u8]) -> Option<Self> {
        if data.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if data.starts_with(b"\xff\xd8\xff") {
            Some(Self::Jpeg)
        } else if data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if data.len() >= 12 && data.starts_with(b"RIFF") && &data[8..12] == b"WEBP" {
            Some(Self::Webp)
        } else if data.starts_with(b"BM") {
            Some(Self::Bmp)
        } else {
            None
        }
    }
}

/// One side of an image diff.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Image {
    pub format: ImageFormat,
    pub size: u64,
    /// The file's bytes. Not part of the JSON form; the pane gets them
    /// another way (M1-09).
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

/// The diff of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FileDiff {
    Text(TextDiff),
    /// Sizes in bytes; `None` for a side that does not exist.
    Binary {
        old_size: Option<u64>,
        new_size: Option<u64>,
    },
    Image {
        old: Option<Image>,
        new: Option<Image>,
    },
    /// Over [`MAX_DIFF_LINES`] changed lines: the counts only.
    TooLarge {
        added: u32,
        removed: u32,
    },
    /// The submodule's commit on each side.
    Submodule {
        old: Option<String>,
        new: Option<String>,
    },
}

/// Why a commit or diff could not be read.
#[derive(Debug, thiserror::Error)]
pub enum DiffError {
    #[error("no commit named {name:?}: {source}")]
    UnknownCommit {
        name: String,
        #[source]
        source: gix::Error,
    },
    #[error("{commit} has no parent {index}")]
    NoParent { commit: String, index: usize },
    #[error("the file belongs to a different commit or parent")]
    FileMismatch,
    #[error("cannot read the repo: {0}")]
    Read(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl From<gix::Error> for DiffError {
    fn from(error: gix::Error) -> Self {
        Self::Read(Box::new(error))
    }
}

/// Commit and diff reads (ARCH-2). Implemented with `gix`; a git CLI
/// fallback can sit behind it without callers noticing. No call returns
/// more than one file's diff (ARCH-8).
pub trait DiffRead {
    /// The header of the commit `hash` names.
    fn commit(&self, hash: &str) -> Result<CommitDetails, DiffError>;

    /// The files `hash` changed against its parent `parent` (0 is the
    /// first), sorted by path, renames detected as `diff.renames` says.
    fn changed_files(&self, hash: &str, parent: usize) -> Result<Vec<ChangedFile>, DiffError>;

    /// The diff of `file`, which must come from `changed_files` for the
    /// same commit and parent as `source`.
    fn file_diff(
        &self,
        source: &DiffSource,
        file: &ChangedFile,
        options: DiffOptions,
    ) -> Result<FileDiff, DiffError>;
}

impl DiffRead for GitRepo {
    fn commit(&self, hash: &str) -> Result<CommitDetails, DiffError> {
        let repo = self.thread_local();
        let id = resolve(&repo, hash)?;
        let commit = repo.find_commit(id)?;
        let decoded = commit.decode()?;
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
        let person = |signature: Result<gix::actor::SignatureRef<'_>, _>| {
            signature.ok().map_or_else(
                || Person {
                    name: String::new(),
                    email: String::new(),
                    time: Time {
                        seconds: 0,
                        offset: 0,
                    },
                },
                |s| Person {
                    name: text(s.name.trim()),
                    email: text(s.email.trim()),
                    time: Time {
                        seconds: s.seconds(),
                        offset: s.time().map_or(0, |t| t.offset),
                    },
                },
            )
        };
        Ok(CommitDetails {
            hash: id.to_string(),
            author: person(decoded.author()),
            committer: person(decoded.committer()),
            message: text(decoded.message),
            parents: decoded.parents().map(|p| p.to_string()).collect(),
        })
    }

    fn changed_files(&self, hash: &str, parent: usize) -> Result<Vec<ChangedFile>, DiffError> {
        let repo = self.thread_local();
        let id = resolve(&repo, hash)?;
        let commit = repo.find_commit(id)?;
        let parents: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();
        let old_tree = match parents.get(parent) {
            Some(parent) => Some(repo.find_commit(*parent)?.tree()?),
            None if parents.is_empty() && parent == 0 => None,
            None => {
                return Err(DiffError::NoParent {
                    commit: id.to_string(),
                    index: parent,
                });
            }
        };
        let new_tree = commit.tree()?;
        let options = gix::diff::Options::default().with_rewrites(rewrites(&repo)?);
        let changes = repo.diff_tree_to_tree(old_tree.as_ref(), &new_tree, options)?;
        let mut files: Vec<ChangedFile> = changes
            .into_iter()
            .filter_map(|change| changed_file(change, (id, parent)))
            .collect();
        files.sort_by(|a, b| a.git_path().cmp(b.git_path()));
        for (index, file) in files.iter_mut().enumerate() {
            file.id = index;
        }
        if files.len() <= MAX_COUNTED_FILES {
            let mut cache = resource_cache(&repo)?;
            for file in &mut files {
                if let Some((added, removed)) = count_lines(&repo, &mut cache, file)? {
                    file.added = Some(added);
                    file.removed = Some(removed);
                }
            }
        }
        Ok(files)
    }

    fn file_diff(
        &self,
        source: &DiffSource,
        file: &ChangedFile,
        options: DiffOptions,
    ) -> Result<FileDiff, DiffError> {
        let repo = self.thread_local();
        let DiffSource::Commit { hash, parent } = source;
        if (resolve(&repo, hash)?, *parent) != file.origin {
            return Err(DiffError::FileMismatch);
        }
        if let Some(diff) = submodule(file) {
            return Ok(diff);
        }
        let mut cache = resource_cache(&repo)?;
        set_sides(&repo, &mut cache, file)?;
        let prepared = cache
            .prepare_diff()
            .map_err(|e| DiffError::Read(e.into()))?;
        let algorithm = match prepared.operation {
            Operation::InternalDiff { algorithm } => algorithm,
            Operation::ExternalCommand { .. } => repo.diff_algorithm()?,
            Operation::SourceOrDestinationIsBinary => {
                return binary(&repo, file, prepared.old.data, prepared.new.data);
            }
        };
        let old = prepared.old.data.as_slice().unwrap_or_default();
        let new = prepared.new.data.as_slice().unwrap_or_default();
        let lines = text::Lines::new(old, new, options.ignore_whitespace);
        let diff = lines.diff(algorithm);
        let (added, removed) = (diff.count_additions(), diff.count_removals());
        if added.saturating_add(removed) > MAX_DIFF_LINES && !options.load_anyway {
            return Ok(FileDiff::TooLarge { added, removed });
        }
        Ok(FileDiff::Text(text::text_diff(
            &lines,
            &diff,
            options.context,
            old,
            new,
        )))
    }
}

fn resolve(repo: &gix::Repository, name: &str) -> Result<ObjectId, DiffError> {
    let spec = format!("{name}^{{commit}}");
    repo.rev_parse_single(spec.as_str())
        .map(|id| id.detach())
        .map_err(|source| DiffError::UnknownCommit {
            name: name.to_owned(),
            source,
        })
}

/// The blob cache for diffs between trees. Attributes come from the
/// working tree's `.gitattributes` files, then the index, as git reads
/// them; a bare repo has only the index (`HEAD`'s tree).
fn resource_cache(repo: &gix::Repository) -> Result<Platform, DiffError> {
    use gix::worktree::stack::state::attributes::Source;

    let index = repo.index_or_load_from_head_or_empty()?;
    let source = if repo.workdir().is_some() {
        Source::WorktreeThenIdMapping
    } else {
        Source::IdMapping
    };
    let attributes = repo.attributes_only(&index, source)?.detach();
    Ok(gix::diff::resource_cache(
        repo,
        gix::diff::blob::pipeline::Mode::ToGit,
        attributes,
        gix::diff::blob::pipeline::WorktreeRoots::default(),
    )?)
}

/// Rename detection as `diff.renames` configures it, git's default (on, 50%
/// similarity) when unset. Copies are reported as additions.
fn rewrites(repo: &gix::Repository) -> Result<Option<gix::diff::Rewrites>, DiffError> {
    let config = repo.config_snapshot();
    let (rewrites, configured) = gix::diff::new_rewrites(config.plumbing(), false)?;
    let rewrites = if configured {
        rewrites
    } else {
        Some(gix::diff::Rewrites::default())
    };
    Ok(rewrites.map(|rewrites| gix::diff::Rewrites {
        copies: None,
        ..rewrites
    }))
}

/// The list entry of a tree change; `None` for a directory.
fn changed_file(change: ChangeDetached, origin: (ObjectId, usize)) -> Option<ChangedFile> {
    let side = |id, mode, path| Some(Side { id, mode, path });
    let (status, old, new) = match change {
        ChangeDetached::Addition {
            location,
            entry_mode,
            id,
            ..
        } => (FileStatus::Added, None, side(id, entry_mode, location)),
        ChangeDetached::Deletion {
            location,
            entry_mode,
            id,
            ..
        } => (FileStatus::Deleted, side(id, entry_mode, location), None),
        ChangeDetached::Modification {
            location,
            previous_entry_mode,
            previous_id,
            entry_mode,
            id,
        } => {
            let status = if same_type(previous_entry_mode, entry_mode) {
                FileStatus::Modified
            } else {
                FileStatus::TypeChanged
            };
            (
                status,
                side(previous_id, previous_entry_mode, location.clone()),
                side(id, entry_mode, location),
            )
        }
        ChangeDetached::Rewrite {
            source_location,
            source_entry_mode,
            source_id,
            entry_mode,
            id,
            location,
            copy,
            ..
        } => {
            if copy {
                (FileStatus::Added, None, side(id, entry_mode, location))
            } else {
                (
                    FileStatus::Renamed,
                    side(source_id, source_entry_mode, source_location),
                    side(id, entry_mode, location),
                )
            }
        }
    };
    let mode = |side: &Option<Side>| side.as_ref().map(|s| s.mode);
    if mode(&old).or(mode(&new)).is_none_or(|m| m.is_tree()) {
        return None;
    }
    let display = |side: &Side| String::from_utf8_lossy(&side.path).into_owned();
    let path = new.as_ref().or(old.as_ref()).map(display)?;
    let old_path = (status == FileStatus::Renamed)
        .then(|| old.as_ref().map(display))
        .flatten();
    let mode_change = match (mode(&old), mode(&new)) {
        (Some(before), Some(after)) if before != after => Some(ModeChange {
            old: before.kind().as_octal_str().to_string(),
            new: after.kind().as_octal_str().to_string(),
        }),
        _ => None,
    };
    Some(ChangedFile {
        id: 0,
        path,
        old_path,
        status,
        mode_change,
        added: None,
        removed: None,
        old,
        new,
        origin,
    })
}

/// Files, links and submodules are different types; the executable bit is
/// not.
fn same_type(a: EntryMode, b: EntryMode) -> bool {
    let kind = |mode: EntryMode| (mode.is_link(), mode.is_commit(), mode.is_tree());
    kind(a) == kind(b)
}

fn submodule(file: &ChangedFile) -> Option<FileDiff> {
    let commit = |side: &Option<Side>| {
        side.as_ref()
            .filter(|s| s.mode.is_commit())
            .map(|s| s.id.to_string())
    };
    let (old, new) = (commit(&file.old), commit(&file.new));
    (old.is_some() || new.is_some()).then_some(FileDiff::Submodule { old, new })
}

/// Loads both sides into the cache; a missing side is a null id.
fn set_sides(
    repo: &gix::Repository,
    cache: &mut Platform,
    file: &ChangedFile,
) -> Result<(), DiffError> {
    let null = ObjectId::null(repo.object_hash());
    let other = file.new.as_ref().or(file.old.as_ref());
    for (side, kind) in [
        (&file.old, ResourceKind::OldOrSource),
        (&file.new, ResourceKind::NewOrDestination),
    ] {
        let (id, mode, path) = match (side, other) {
            (Some(side), _) => (side.id, side.mode, side.path.as_bstr()),
            (None, Some(other)) => (null, other.mode, other.path.as_bstr()),
            (None, None) => return Ok(()),
        };
        cache
            .set_resource(id, mode.kind(), path, kind, &repo.objects)
            .map_err(|e| DiffError::Read(e.into()))?;
    }
    Ok(())
}

/// Added and removed lines of a text file, `None` for a binary file or a
/// submodule, as `git diff --numstat` counts them.
fn count_lines(
    repo: &gix::Repository,
    cache: &mut Platform,
    file: &ChangedFile,
) -> Result<Option<(u32, u32)>, DiffError> {
    if submodule(file).is_some() {
        return Ok(None);
    }
    set_sides(repo, cache, file)?;
    let prepared = cache
        .prepare_diff()
        .map_err(|e| DiffError::Read(e.into()))?;
    let algorithm = match prepared.operation {
        Operation::InternalDiff { algorithm } => algorithm,
        Operation::ExternalCommand { .. } => repo.diff_algorithm()?,
        Operation::SourceOrDestinationIsBinary => return Ok(None),
    };
    let old = prepared.old.data.as_slice().unwrap_or_default();
    let new = prepared.new.data.as_slice().unwrap_or_default();
    let lines = text::Lines::new(old, new, false);
    let diff = lines.diff(algorithm);
    Ok(Some((diff.count_additions(), diff.count_removals())))
}

/// A binary diff: images up to [`MAX_IMAGE_BYTES`] per side, sizes
/// otherwise.
fn binary(
    repo: &gix::Repository,
    file: &ChangedFile,
    old: Data<'_>,
    new: Data<'_>,
) -> Result<FileDiff, DiffError> {
    let size = |data: Data<'_>| match data {
        Data::Missing => None,
        Data::Buffer { buf, .. } => Some(buf.len() as u64),
        Data::Binary { size } => Some(size),
    };
    let (old_size, new_size) = (size(old), size(new));
    let image = |side: &Option<Side>, size: Option<u64>| -> Result<Option<Image>, DiffError> {
        let (Some(side), Some(size)) = (side, size) else {
            return Ok(None);
        };
        if size > MAX_IMAGE_BYTES {
            return Ok(None);
        }
        let bytes = repo.find_object(side.id)?.detach().data;
        Ok(ImageFormat::sniff(&bytes).map(|format| Image {
            format,
            size,
            bytes,
        }))
    };
    let old_image = image(&file.old, old_size)?;
    let new_image = image(&file.new, new_size)?;
    let complete = |image: &Option<Image>, size: Option<u64>| image.is_some() == size.is_some();
    if (old_image.is_some() || new_image.is_some())
        && complete(&old_image, old_size)
        && complete(&new_image, new_size)
    {
        return Ok(FileDiff::Image {
            old: old_image,
            new: new_image,
        });
    }
    Ok(FileDiff::Binary { old_size, new_size })
}
