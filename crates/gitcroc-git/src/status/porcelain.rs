//! The working copy read from `git status --porcelain=v2 -z`, the fallback
//! of ARCH-2. Paths are git's bytes; `-z` leaves them unquoted (PLAT-2).

use std::path::Path;

use gix::bstr::{BString, ByteSlice};
use gix::hash::ObjectId;
use gix::object::tree::EntryMode;

use super::{Stage, StatusError, UnmergedFile, UnmergedKind, WorkingCopy, unstaged, untracked};
use crate::FileStatus;
use crate::diff::{ChangedFile, Origin, Side, same_type};

pub(super) fn parse(
    output: &[u8],
    hash: gix::hash::Kind,
    work_dir: &Path,
) -> Result<WorkingCopy, StatusError> {
    let mut working_copy = WorkingCopy::default();
    let mut records = output.split(|byte| *byte == 0);
    while let Some(record) = records.next() {
        let Some((&kind, _)) = record.split_first() else {
            continue;
        };
        match kind {
            b'1' => {
                let [_, xy, _sub, m_head, m_index, m_tree, h_head, h_index, path] = fields(record)?;
                let path = BString::from(path);
                let entry = Entry::new(xy, [m_head, m_index, m_tree], [h_head, h_index])?;
                entry.push(&mut working_copy, None, path, hash)?;
            }
            b'2' => {
                let [
                    _,
                    xy,
                    _sub,
                    m_head,
                    m_index,
                    m_tree,
                    h_head,
                    h_index,
                    _score,
                    path,
                ] = fields(record)?;
                let original = records
                    .next()
                    .ok_or_else(|| bad(record, "no original path"))?;
                let entry = Entry::new(xy, [m_head, m_index, m_tree], [h_head, h_index])?;
                entry.push(
                    &mut working_copy,
                    Some(BString::from(original)),
                    BString::from(path),
                    hash,
                )?;
            }
            b'u' => {
                let [_, xy, _sub, m1, m2, m3, _m_tree, h1, h2, h3, path] = fields(record)?;
                let stage = |mode: &[u8], id: &[u8]| -> Result<Option<Stage>, StatusError> {
                    Ok(parse_mode(mode)?.map(|mode| Stage {
                        id: parse_id(id).unwrap_or_else(|| ObjectId::null(hash)),
                        mode,
                    }))
                };
                let stages = [stage(m1, h1)?, stage(m2, h2)?, stage(m3, h3)?];
                working_copy.unmerged.push(UnmergedFile::new(
                    BString::from(path),
                    conflict(xy).ok_or_else(|| bad(record, "unknown conflict"))?,
                    stages,
                ));
            }
            b'?' => {
                let path = record.get(2..).ok_or_else(|| bad(record, "no path"))?;
                working_copy
                    .unstaged
                    .extend(untracked(work_dir, path, hash));
            }
            // Headers and ignored files, which are not asked for.
            b'#' | b'!' => {}
            _ => return Err(bad(record, "unknown record")),
        }
    }
    Ok(working_copy)
}

/// An ordinary or renamed entry: its two status letters, its modes in
/// `HEAD`, the index and the working tree, and its ids in `HEAD` and the
/// index.
struct Entry {
    staged: u8,
    unstaged: u8,
    modes: [Option<EntryMode>; 3],
    ids: [Option<ObjectId>; 2],
}

impl Entry {
    fn new(xy: &[u8], modes: [&[u8]; 3], ids: [&[u8]; 2]) -> Result<Self, StatusError> {
        let [staged, unstaged] = xy else {
            return Err(bad(xy, "status letters"));
        };
        Ok(Self {
            staged: *staged,
            unstaged: *unstaged,
            modes: [
                parse_mode(modes[0])?,
                parse_mode(modes[1])?,
                parse_mode(modes[2])?,
            ],
            ids: [parse_id(ids[0]), parse_id(ids[1])],
        })
    }

    fn push(
        &self,
        working_copy: &mut WorkingCopy,
        original: Option<BString>,
        path: BString,
        hash: gix::hash::Kind,
    ) -> Result<(), StatusError> {
        let side = |mode: Option<EntryMode>, id: Option<ObjectId>, path: &BString| {
            Some(Side {
                id: id?,
                mode: mode?,
                path: path.clone(),
            })
        };
        let [m_head, m_index, m_tree] = self.modes;
        let [h_head, h_index] = self.ids;
        let index = side(m_index, h_index, &path);
        if self.staged != b'.' {
            let head = side(m_head, h_head, original.as_ref().unwrap_or(&path));
            let status = match self.staged {
                b'R' => FileStatus::Renamed,
                b'C' | b'A' => FileStatus::Added,
                b'D' => FileStatus::Deleted,
                _ => kind_of_change(m_head, m_index),
            };
            // A copy is listed as an added file, as in a commit's list.
            let head = if self.staged == b'C' { None } else { head };
            working_copy.staged.extend(ChangedFile::new(
                status,
                head,
                index.clone(),
                Origin::Staged,
            ));
        }
        if self.unstaged != b'.' {
            let status = match self.unstaged {
                b'D' => FileStatus::Deleted,
                b'A' => FileStatus::Added,
                _ => kind_of_change(m_index, m_tree),
            };
            // An intent-to-add entry has no index side yet.
            let index = if self.unstaged == b'A' { None } else { index };
            working_copy
                .unstaged
                .extend(unstaged(status, index, m_tree, &path, hash));
        }
        Ok(())
    }
}

fn kind_of_change(old: Option<EntryMode>, new: Option<EntryMode>) -> FileStatus {
    match (old, new) {
        (Some(old), Some(new)) if !same_type(old, new) => FileStatus::TypeChanged,
        _ => FileStatus::Modified,
    }
}

fn conflict(xy: &[u8]) -> Option<UnmergedKind> {
    Some(match xy {
        b"DD" => UnmergedKind::BothDeleted,
        b"AU" => UnmergedKind::AddedByUs,
        b"UD" => UnmergedKind::DeletedByThem,
        b"UA" => UnmergedKind::AddedByThem,
        b"DU" => UnmergedKind::DeletedByUs,
        b"AA" => UnmergedKind::BothAdded,
        b"UU" => UnmergedKind::BothModified,
        _ => return None,
    })
}

/// The first `N - 1` space-separated fields and the rest, which is the
/// path and may hold spaces.
fn fields<const N: usize>(record: &[u8]) -> Result<[&[u8]; N], StatusError> {
    let mut parts = record.splitn(N, |byte| *byte == b' ');
    let mut fields = [&[][..]; N];
    for field in &mut fields {
        *field = parts.next().ok_or_else(|| bad(record, "too few fields"))?;
    }
    Ok(fields)
}

/// An octal mode; `None` for `000000`, a side that does not exist.
fn parse_mode(text: &[u8]) -> Result<Option<EntryMode>, StatusError> {
    let bits = text
        .to_str()
        .ok()
        .and_then(|text| u32::from_str_radix(text, 8).ok())
        .ok_or_else(|| bad(text, "mode"))?;
    if bits == 0 {
        return Ok(None);
    }
    EntryMode::try_from(bits)
        .map(Some)
        .map_err(|_| bad(text, "mode"))
}

/// An object id; `None` for the null id.
fn parse_id(text: &[u8]) -> Option<ObjectId> {
    ObjectId::from_hex(text).ok().filter(|id| !id.is_null())
}

fn bad(record: &[u8], what: &str) -> StatusError {
    StatusError::Parse(format!("{what} in {:?}", record.as_bstr()))
}
