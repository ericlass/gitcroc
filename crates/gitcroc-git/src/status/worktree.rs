//! The working copy read with `gix`.

use std::collections::BTreeSet;

use gix::bstr::BString;
use gix::object::tree::EntryMode;
use gix::status::Item;
use gix::status::index_worktree::Item as WorktreeItem;
use gix::status::plumbing::index_as_worktree::{self, EntryStatus};
use gix::status::tree_index::TrackRenames;

use super::{Conflict, Stage, StatusError, UnmergedFile, WorkingCopy, unstaged, untracked};
use crate::FileStatus;
use crate::diff::{ChangedFile, Origin, Side, same_type};

/// `HEAD` against the index and the index against the working tree, with
/// untracked files as `status.showUntrackedFiles` says and `renames`
/// between `HEAD` and the index.
pub(super) fn status(
    repo: &gix::Repository,
    renames: Option<gix::diff::Rewrites>,
) -> Result<WorkingCopy, StatusError> {
    let work_dir = repo.workdir().ok_or(StatusError::NoWorkTree)?.to_owned();
    let hash = repo.object_hash();
    let mut working_copy = WorkingCopy::default();
    let items = repo
        .status(gix::progress::Discard)?
        .tree_index_track_renames(renames.map_or(TrackRenames::Disabled, TrackRenames::Given))
        .into_iter(Vec::<BString>::new())?;
    for item in items {
        match item? {
            Item::TreeIndex(change) => working_copy.staged.extend(staged(change)),
            Item::IndexWorktree(WorktreeItem::Modification {
                entry,
                rela_path,
                status,
                ..
            }) => {
                let index = |mode: gix::index::entry::Mode| {
                    Some(Side {
                        id: entry.id,
                        mode: tree_mode(mode)?,
                        path: rela_path.clone(),
                    })
                };
                match status {
                    EntryStatus::Conflict { summary, entries } => {
                        let stages = entries.map(|entry| {
                            let entry = entry?;
                            Some(Stage {
                                id: entry.id,
                                mode: tree_mode(entry.mode)?,
                            })
                        });
                        working_copy.unmerged.push(UnmergedFile::new(
                            rela_path,
                            conflict(summary),
                            stages,
                        ));
                    }
                    EntryStatus::Change(change) => {
                        let index_mode = tree_mode(entry.mode);
                        let (status, mode) = match change {
                            index_as_worktree::Change::Removed => (FileStatus::Deleted, None),
                            index_as_worktree::Change::Type { worktree_mode } => {
                                (FileStatus::TypeChanged, tree_mode(worktree_mode))
                            }
                            index_as_worktree::Change::Modification {
                                executable_bit_changed,
                                ..
                            } => {
                                let mode = if executable_bit_changed {
                                    index_mode.map(flip_executable)
                                } else {
                                    index_mode
                                };
                                (FileStatus::Modified, mode)
                            }
                            index_as_worktree::Change::SubmoduleModification(submodule) => {
                                if submodule.is_dirty() != Some(true) {
                                    continue;
                                }
                                (FileStatus::Modified, index_mode)
                            }
                        };
                        let status = match (index_mode, mode) {
                            (Some(before), Some(after)) if !same_type(before, after) => {
                                FileStatus::TypeChanged
                            }
                            _ => status,
                        };
                        working_copy.unstaged.extend(unstaged(
                            status,
                            index(entry.mode),
                            mode,
                            &rela_path,
                            hash,
                        ));
                    }
                    EntryStatus::IntentToAdd => {
                        working_copy.unstaged.extend(unstaged(
                            FileStatus::Added,
                            None,
                            tree_mode(entry.mode),
                            &rela_path,
                            hash,
                        ));
                    }
                    EntryStatus::NeedsUpdate(_) => {}
                }
            }
            Item::IndexWorktree(WorktreeItem::DirectoryContents { entry, .. }) => {
                if entry.status == gix::dir::entry::Status::Untracked {
                    let mut path = entry.rela_path;
                    if matches!(
                        entry.disk_kind,
                        Some(gix::dir::entry::Kind::Directory | gix::dir::entry::Kind::Repository)
                    ) {
                        path.push(b'/');
                    }
                    working_copy
                        .unstaged
                        .extend(untracked(&work_dir, &path, hash));
                }
            }
            // Renames between the index and the working tree are not
            // tracked, as in git, so the status never yields these.
            Item::IndexWorktree(WorktreeItem::Rewrite { .. }) => {}
        }
    }
    let unmerged: BTreeSet<&[u8]> = working_copy
        .unmerged
        .iter()
        .map(UnmergedFile::git_path)
        .collect();
    let staged = std::mem::take(&mut working_copy.staged);
    working_copy.staged = staged
        .into_iter()
        .filter(|file| !unmerged.contains(file.git_path()))
        .collect();
    Ok(working_copy)
}

fn staged(change: gix::diff::index::Change) -> Option<ChangedFile> {
    use gix::diff::index::ChangeRef;

    let side = |id: &gix::hash::oid, mode, path: &gix::bstr::BStr| {
        Some(Side {
            id: id.to_owned(),
            mode: tree_mode(mode)?,
            path: path.to_owned(),
        })
    };
    let (status, old, new) = match change {
        ChangeRef::Addition {
            location,
            entry_mode,
            id,
            ..
        } => (FileStatus::Added, None, side(&id, entry_mode, &location)),
        ChangeRef::Deletion {
            location,
            entry_mode,
            id,
            ..
        } => (FileStatus::Deleted, side(&id, entry_mode, &location), None),
        ChangeRef::Modification {
            location,
            previous_entry_mode,
            previous_id,
            entry_mode,
            id,
            ..
        } => {
            let old = side(&previous_id, previous_entry_mode, &location);
            let new = side(&id, entry_mode, &location);
            let same = match (&old, &new) {
                (Some(old), Some(new)) => same_type(old.mode, new.mode),
                _ => true,
            };
            let status = if same {
                FileStatus::Modified
            } else {
                FileStatus::TypeChanged
            };
            (status, old, new)
        }
        ChangeRef::Rewrite {
            source_location,
            source_entry_mode,
            source_id,
            location,
            entry_mode,
            id,
            copy,
            ..
        } => {
            let new = side(&id, entry_mode, &location);
            if copy {
                (FileStatus::Added, None, new)
            } else {
                let old = side(&source_id, source_entry_mode, &source_location);
                (FileStatus::Renamed, old, new)
            }
        }
    };
    ChangedFile::new(status, old, new, Origin::Staged)
}

fn tree_mode(mode: gix::index::entry::Mode) -> Option<EntryMode> {
    mode.to_tree_entry_mode()
}

fn flip_executable(mode: EntryMode) -> EntryMode {
    use gix::object::tree::EntryKind;

    match mode.kind() {
        EntryKind::Blob => EntryKind::BlobExecutable.into(),
        EntryKind::BlobExecutable => EntryKind::Blob.into(),
        _ => mode,
    }
}

fn conflict(summary: index_as_worktree::Conflict) -> Conflict {
    use index_as_worktree::Conflict as C;

    match summary {
        C::BothDeleted => Conflict::BothDeleted,
        C::AddedByUs => Conflict::AddedByUs,
        C::DeletedByThem => Conflict::DeletedByThem,
        C::AddedByThem => Conflict::AddedByThem,
        C::DeletedByUs => Conflict::DeletedByUs,
        C::BothAdded => Conflict::BothAdded,
        C::BothModified => Conflict::BothModified,
    }
}
