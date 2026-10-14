//! The patch of one hunk of a working copy diff (UI-15), for `git apply`.
//! It is built from the sides' bytes as git stores them, never from the
//! text shown, and only while the sides are still the ones the shown diff
//! came from.

use gix::bstr::{BString, ByteSlice};
use gix::diff::blob::platform::prepare_diff::Operation;
use gix::hash::ObjectId;

use super::text::{self, LineKind};
use super::{ChangedFile, DiffError, DiffOptions, FileStatus, Origin, Side, resource_cache};
use super::{set_sides, submodule};

/// The content a text diff was read from: the hash of each side's bytes as
/// git stores them, `None` for a side that does not exist. A hunk is
/// applied only while the sides still hash the same.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ContentKey {
    old: Option<ObjectId>,
    new: Option<ObjectId>,
}

/// What applying one hunk takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HunkPatch {
    /// A patch for `git apply` (`--recount`), the paths in it as git
    /// stores them.
    Patch(Vec<u8>),
    /// The hunk is the whole file: it is added, deleted, untracked or
    /// changed type, so it is staged, unstaged or discarded as a file.
    WholeFile,
}

/// The key of two sides' data; `None` for a missing side.
pub(super) fn content_key(
    hash: gix::hash::Kind,
    old: Option<&[u8]>,
    new: Option<&[u8]>,
) -> Result<ContentKey, DiffError> {
    let key = |data: Option<&[u8]>| {
        data.map(|data| gix::objs::compute_hash(hash, gix::objs::Kind::Blob, data))
            .transpose()
            .map_err(|e| DiffError::Read(e.into()))
    };
    Ok(ContentKey {
        old: key(old)?,
        new: key(new)?,
    })
}

/// `file` with its sides as the index, `HEAD` and the working tree hold
/// them now. A side that appeared or went away since the list was read
/// makes it [`DiffError::FileChanged`].
fn current_sides(repo: &gix::Repository, file: &ChangedFile) -> Result<ChangedFile, DiffError> {
    let index = repo.index_or_empty()?;
    let from_index = |side: &Side| -> Option<Side> {
        let entry = index.entry_by_path(side.path.as_bstr())?;
        Some(Side {
            id: entry.id,
            mode: entry.mode.to_tree_entry_mode()?,
            path: side.path.clone(),
        })
    };
    let (old, new) = match file.origin {
        Origin::Unstaged => {
            let old = match &file.old {
                Some(side) => Some(from_index(side).ok_or(DiffError::FileChanged)?),
                None => None,
            };
            // The working tree side keeps its null id; its bytes are read
            // from the file.
            (old, file.new.clone())
        }
        Origin::Staged => {
            let old = match &file.old {
                Some(side) => {
                    let mut tree = repo.head_tree()?;
                    let entry = tree
                        .peel_to_entry_by_path(gix::path::from_bstr(side.path.as_bstr())?)?
                        .ok_or(DiffError::FileChanged)?;
                    Some(Side {
                        id: entry.object_id(),
                        mode: entry.mode(),
                        path: side.path.clone(),
                    })
                }
                None => None,
            };
            let new = match &file.new {
                Some(side) => Some(from_index(side).ok_or(DiffError::FileChanged)?),
                None => None,
            };
            (old, new)
        }
        Origin::Commit(..) => return Err(DiffError::FileMismatch),
    };
    Ok(ChangedFile {
        old,
        new,
        ..file.clone()
    })
}

/// The patch of hunk `hunk` of `file`'s diff with `options`, if its sides
/// still hash to `shown`, the key of the diff the user saw.
pub(super) fn hunk_patch(
    repo: &gix::Repository,
    file: &ChangedFile,
    options: DiffOptions,
    hunk: usize,
    shown: &ContentKey,
) -> Result<HunkPatch, DiffError> {
    if options.ignore_whitespace {
        return Err(DiffError::IgnoredWhitespace);
    }
    let work_dir = match file.origin {
        Origin::Unstaged => Some(repo.workdir().ok_or(DiffError::NoWorkTree)?.to_owned()),
        Origin::Staged => None,
        Origin::Commit(..) => return Err(DiffError::FileMismatch),
    };
    if submodule(file, None).is_some() || file.new.as_ref().is_some_and(|s| s.mode.is_tree()) {
        return Err(DiffError::NotText);
    }
    let current = current_sides(repo, file)?;
    let mut cache = resource_cache(repo, work_dir.as_deref())?;
    set_sides(repo, &mut cache, &current)?;
    let prepared = cache
        .prepare_diff()
        .map_err(|e| DiffError::Read(e.into()))?;
    let algorithm = match prepared.operation {
        Operation::InternalDiff { algorithm } => algorithm,
        Operation::ExternalCommand { .. } => repo.diff_algorithm()?,
        Operation::SourceOrDestinationIsBinary => return Err(DiffError::NotText),
    };
    let old = prepared.old.data.as_slice();
    let new = prepared.new.data.as_slice();
    if content_key(repo.object_hash(), old, new)? != *shown {
        return Err(DiffError::FileChanged);
    }
    let (old, new) = (old.unwrap_or_default(), new.unwrap_or_default());
    let lines = text::Lines::new(old, new, false);
    let diff = lines.diff(algorithm);
    let changes: Vec<_> = diff.hunks().collect();
    let spans = text::hunk_spans(&lines, &changes, options.context);
    let span = spans.get(hunk).ok_or(DiffError::NoHunk(hunk))?;
    let whole = matches!(
        file.status,
        FileStatus::Added | FileStatus::Deleted | FileStatus::Untracked | FileStatus::TypeChanged
    ) || current.old.is_none()
        || current.new.is_none();
    if whole {
        return Ok(HunkPatch::WholeFile);
    }

    // Both sides exist; a rename's hunk changes the new path only.
    let path = BString::from(file.git_path());
    let mut patch = Vec::new();
    patch.extend_from_slice(b"diff --git ");
    patch.extend_from_slice(&quoted(b"a/", &path));
    patch.push(b' ');
    patch.extend_from_slice(&quoted(b"b/", &path));
    patch.extend_from_slice(b"\n--- ");
    patch.extend_from_slice(&quoted(b"a/", &path));
    patch.extend_from_slice(b"\n+++ ");
    patch.extend_from_slice(&quoted(b"b/", &path));
    patch.push(b'\n');
    let (old_start, old_lines, new_start, new_lines) = span.header();
    patch.extend_from_slice(
        format!("@@ -{old_start},{old_lines} +{new_start},{new_lines} @@\n").as_bytes(),
    );
    for (kind, _, _, line) in text::span_lines(&lines, &changes, span) {
        patch.push(match kind {
            LineKind::Context => b' ',
            LineKind::Added => b'+',
            LineKind::Removed => b'-',
        });
        patch.extend_from_slice(line);
        if !line.ends_with(b"\n") {
            patch.extend_from_slice(b"\n\\ No newline at end of file\n");
        }
    }
    Ok(HunkPatch::Patch(patch))
}

/// `prefix` and `path` as a patch names a file: quoted the way git quotes
/// it when it holds a space, a quote, a backslash, a control character or
/// a byte outside ASCII, which `git apply` reads back.
fn quoted(prefix: &[u8], path: &[u8]) -> Vec<u8> {
    let plain = |byte: u8| byte.is_ascii_graphic() && byte != b'"' && byte != b'\\';
    let mut out = Vec::with_capacity(prefix.len() + path.len() + 2);
    if prefix.iter().chain(path).all(|&byte| plain(byte)) {
        out.extend_from_slice(prefix);
        out.extend_from_slice(path);
        return out;
    }
    out.push(b'"');
    for &byte in prefix.iter().chain(path) {
        match byte {
            b'"' => out.extend_from_slice(b"\\\""),
            b'\\' => out.extend_from_slice(b"\\\\"),
            b'\t' => out.extend_from_slice(b"\\t"),
            b'\n' => out.extend_from_slice(b"\\n"),
            b' ' => out.push(b' '),
            _ if plain(byte) => out.push(byte),
            _ => out.extend_from_slice(format!("\\{byte:03o}").as_bytes()),
        }
    }
    out.push(b'"');
    out
}

#[cfg(test)]
mod tests {
    use super::quoted;

    #[test]
    fn paths_are_quoted_as_git_quotes_them() {
        assert_eq!(quoted(b"a/", b"src/main.rs"), b"a/src/main.rs");
        assert_eq!(quoted(b"b/", b"two words"), b"\"b/two words\"");
        assert_eq!(
            quoted(b"a/", "tab\there \"q\" \\ é".as_bytes()),
            b"\"a/tab\\there \\\"q\\\" \\\\ \\303\\251\"".to_vec()
        );
    }
}
