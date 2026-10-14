//! The text of a conflicted file, merged in memory from its index stages
//! (OPS-8): split into resolved text and conflict hunks for the editor, and
//! rendered with conflict markers as git writes them, to tell whether the
//! file on disk was edited since. The file on disk is never parsed.

use gix::bstr::ByteSlice;
use gix::diff::blob::{Algorithm, InternedInput};
use gix::merge::blob::builtin_driver::text::{Conflict as Strategy, ConflictStyle, Labels, Merge};
use serde::Serialize;

/// git's conflict marker length, unless `conflict-marker-size` says
/// otherwise.
pub const MARKER_SIZE: u8 = 7;

/// The characters git starts its conflict marker lines with.
const MARKERS: [u8; 4] = *b"<|=>";

/// The diff git merges text with: `merge-ort` uses histogram.
const ALGORITHM: Algorithm = Algorithm::Histogram;

/// A part of a conflicted text file (OPS-3, OPS-8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConflictChunk {
    /// Text both sides agree on, or that only one side changed.
    Resolved { text: String },
    /// Lines the two sides changed differently, with the base they started
    /// from, in diff3 style. Each part ends with its line ending.
    Hunk {
        ours: String,
        base: String,
        theirs: String,
    },
}

/// The longest run of one marker character that starts a line of `data`.
fn longest_marker_run(data: &[u8]) -> usize {
    data.lines()
        .filter_map(|line| {
            let first = *line.first()?;
            MARKERS
                .contains(&first)
                .then(|| line.iter().take_while(|&&byte| byte == first).count())
        })
        .max()
        .unwrap_or(0)
}

/// Line `line` without its line ending.
fn content(line: &[u8]) -> &[u8] {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    line.strip_suffix(b"\r").unwrap_or(line)
}

/// Which marker `line` is, at length `size` and without a label.
fn marker(line: &[u8], size: usize) -> Option<u8> {
    let line = content(line);
    let first = *line.first()?;
    (MARKERS.contains(&first) && line.len() == size && line.iter().all(|&byte| byte == first))
        .then_some(first)
}

/// `ours` and `theirs` merged against `base` in diff3 style, as resolved
/// text and conflict hunks. `None` when a line of the inputs starts with a
/// run of marker characters too long to tell markers from text (over 254).
pub(crate) fn chunks(ours: &[u8], base: &[u8], theirs: &[u8]) -> Option<Vec<ConflictChunk>> {
    // Markers longer than any run in the inputs cannot be mistaken for
    // their text.
    let longest = [ours, base, theirs]
        .map(longest_marker_run)
        .into_iter()
        .max()
        .unwrap_or(0);
    let size = u8::try_from(longest.saturating_add(1).max(usize::from(MARKER_SIZE))).ok()?;
    let merged = render(ours, base, theirs, ConflictStyle::Diff3, size)?;
    let size = usize::from(size);

    let mut all = Vec::new();
    let mut resolved = Vec::new();
    // The hunk being read: ours, base, theirs, and which part comes next.
    let mut hunk: Option<([Vec<u8>; 3], usize)> = None;
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    for line in merged.lines_with_terminator() {
        match (marker(line, size), &mut hunk) {
            (Some(b'<'), None) => {
                if !resolved.is_empty() {
                    all.push(ConflictChunk::Resolved {
                        text: text(&std::mem::take(&mut resolved)),
                    });
                }
                hunk = Some((Default::default(), 0));
            }
            (Some(b'|'), Some((_, part))) => *part = 1,
            (Some(b'='), Some((_, part))) => *part = 2,
            (Some(b'>'), Some(([ours, base, theirs], _))) => {
                all.push(ConflictChunk::Hunk {
                    ours: text(ours),
                    base: text(base),
                    theirs: text(theirs),
                });
                hunk = None;
            }
            (_, Some((parts, part))) => parts.get_mut(*part)?.extend_from_slice(line),
            (_, None) => resolved.extend_from_slice(line),
        }
    }
    if hunk.is_some() {
        return None;
    }
    if !resolved.is_empty() {
        all.push(ConflictChunk::Resolved {
            text: text(&resolved),
        });
    }
    Some(all)
}

/// `ours` and `theirs` merged against `base` with conflict markers of
/// `size` in `style`, without labels. `None` when `size` is 0.
pub(crate) fn render(
    ours: &[u8],
    base: &[u8],
    theirs: &[u8],
    style: ConflictStyle,
    size: u8,
) -> Option<Vec<u8>> {
    let mut input = InternedInput::new(&[][..], &[][..]);
    let merge = Merge::new(&mut input, ours, base, theirs, ALGORITHM);
    let mut out = Vec::new();
    merge.run(
        &mut out,
        Labels::default(),
        Strategy::Keep {
            style,
            marker_size: std::num::NonZeroU8::new(size)?,
        },
    );
    Some(out)
}

/// `text` with CRLF read as LF and the label after each conflict marker
/// of `size` removed, so that two renderings of a conflict compare equal
/// whatever labels and line endings they were written with.
pub(crate) fn without_labels(text: &[u8], size: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    for line in text.lines_with_terminator() {
        let ends = line.ends_with(b"\n");
        let line = content(line);
        let labelled = line.first().is_some_and(|first| {
            MARKERS.contains(first)
                && line.len() > size
                && line.iter().take(size).all(|byte| byte == first)
                && line.get(size) == Some(&b' ')
        });
        out.extend_from_slice(if labelled {
            line.get(..size).unwrap_or(line)
        } else {
            line
        });
        if ends {
            out.push(b'\n');
        }
    }
    out
}

/// The first line of `text`, counted from 1, that is a conflict marker
/// line (OPS-5): exactly seven `<`, `=`, `|` or `>` at its start, then a
/// space or the line's end.
pub fn first_marker_line(text: &str) -> Option<u32> {
    let size = usize::from(MARKER_SIZE);
    text.as_bytes()
        .lines()
        .position(|line| {
            let line = content(line);
            line.first().is_some_and(|first| {
                MARKERS.contains(first)
                    && line.len() >= size
                    && line.iter().take(size).all(|byte| byte == first)
                    && matches!(line.get(size), None | Some(b' '))
            })
        })
        .and_then(|index| u32::try_from(index.saturating_add(1)).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved(text: &str) -> ConflictChunk {
        ConflictChunk::Resolved { text: text.into() }
    }

    fn hunk(ours: &str, base: &str, theirs: &str) -> ConflictChunk {
        ConflictChunk::Hunk {
            ours: ours.into(),
            base: base.into(),
            theirs: theirs.into(),
        }
    }

    #[test]
    fn ops_8_chunks_split_resolved_text_and_hunks() {
        let base = "a\nb\nc\nd\n";
        let ours = "a\nours\nc\nd\nmore\n";
        let theirs = "a\ntheirs\nc\nd\n";
        assert_eq!(
            chunks(ours.as_bytes(), base.as_bytes(), theirs.as_bytes()),
            Some(vec![
                resolved("a\n"),
                hunk("ours\n", "b\n", "theirs\n"),
                resolved("c\nd\nmore\n"),
            ])
        );
    }

    #[test]
    fn ops_8_marker_like_text_in_a_side_stays_text() {
        let base = "a\n";
        let ours = "<<<<<<< not a marker\n=======\n";
        let theirs = ">>>>>>>>\n";
        assert_eq!(
            chunks(ours.as_bytes(), base.as_bytes(), theirs.as_bytes()),
            Some(vec![hunk(ours, base, theirs)])
        );
    }

    #[test]
    fn ops_8_crlf_files_keep_their_line_endings() {
        let base = "a\r\nb\r\n";
        let ours = "a\r\nours\r\n";
        let theirs = "a\r\ntheirs\r\n";
        assert_eq!(
            chunks(ours.as_bytes(), base.as_bytes(), theirs.as_bytes()),
            Some(vec![
                resolved("a\r\n"),
                hunk("ours\r\n", "b\r\n", "theirs\r\n")
            ])
        );
    }

    #[test]
    fn ops_8_labels_and_line_endings_do_not_count_as_edits() {
        let git = b"<<<<<<< HEAD\r\nours\r\n||||||| merged common ancestors\r\nb\r\n=======\r\ntheirs\r\n>>>>>>> feature\r\n";
        let ours = b"<<<<<<<\nours\n|||||||\nb\n=======\ntheirs\n>>>>>>>\n";
        assert_eq!(without_labels(git, 7), without_labels(ours, 7));
        assert_ne!(without_labels(b"<<<<<<<< x\n", 7), b"<<<<<<<\n");
    }

    #[test]
    fn ops_5_marker_lines_are_exactly_seven_characters() {
        for text in [
            "<<<<<<<\n",
            "a\n======= x\n",
            "a\n|||||||",
            ">>>>>>> theirs\r\n",
        ] {
            assert!(first_marker_line(text).is_some(), "{text:?}");
        }
        for text in [
            "<<<<<<<<\n",
            " =======\n",
            "<<<<<<\n",
            ">>>>>>>x\n",
            "=-=-=-=\n",
        ] {
            assert_eq!(first_marker_line(text), None, "{text:?}");
        }
        assert_eq!(first_marker_line("a\nb\n=======\n"), Some(3));
    }
}
