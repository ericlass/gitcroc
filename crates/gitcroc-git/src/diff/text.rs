//! The line diff of one text file (UI-17): hunks with 3 or 10 lines of
//! context or the whole file, optionally ignoring whitespace like `git diff
//! -w`. Text that is not UTF-8 is decoded lossily (SEC-3).

use std::borrow::Cow;

use gix::diff::blob::{Algorithm, Diff, InternedInput, Interner};
use serde::{Deserialize, Serialize};

/// How much unchanged text surrounds each change (UI-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Context {
    #[default]
    Three,
    Ten,
    /// One hunk spanning the file, and both sides' full text.
    WholeFile,
}

impl Context {
    fn lines(self) -> usize {
        match self {
            Self::Three => 3,
            Self::Ten => 10,
            Self::WholeFile => usize::MAX,
        }
    }
}

/// What a diff line is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum LineKind {
    Context,
    Added,
    Removed,
}

/// One line of a hunk, without its line terminator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct Line {
    pub kind: LineKind,
    /// 1-based line number on the old side; `None` for an added line.
    pub old_line: Option<u32>,
    /// 1-based line number on the new side; `None` for a removed line.
    pub new_line: Option<u32>,
    pub text: String,
}

/// A run of changes with its context. Starts and counts are as in a
/// unified diff header: `@@ -old_start,old_lines +new_start,new_lines @@`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct Hunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub lines: Vec<Line>,
}

/// The diff of a text file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct TextDiff {
    pub added: u32,
    pub removed: u32,
    pub hunks: Vec<Hunk>,
    /// Both sides in full, only with [`Context::WholeFile`] (side-by-side
    /// mode needs them); empty for a side that does not exist.
    pub old_text: Option<String>,
    pub new_text: Option<String>,
    /// The side's last line has no line terminator.
    pub old_missing_newline: bool,
    pub new_missing_newline: bool,
}

/// The lines of `data`, each with its terminator.
pub(crate) fn split_lines(data: &[u8]) -> Vec<&[u8]> {
    data.split_inclusive(|&byte| byte == b'\n').collect()
}

/// The compared lines of both sides, interned.
pub(crate) struct Lines<'a> {
    pub old: Vec<&'a [u8]>,
    pub new: Vec<&'a [u8]>,
    input: InternedInput<Cow<'a, [u8]>>,
    ignore_whitespace: bool,
}

impl<'a> Lines<'a> {
    /// Splits both sides into lines. Lines compare by their bytes, or by
    /// their bytes without any whitespace when `ignore_whitespace` is set.
    pub(crate) fn new(old: &'a [u8], new: &'a [u8], ignore_whitespace: bool) -> Self {
        let old = split_lines(old);
        let new = split_lines(new);
        let key = |line: &&'a [u8]| -> Cow<'a, [u8]> {
            if ignore_whitespace {
                Cow::Owned(
                    line.iter()
                        .copied()
                        .filter(|byte| !byte.is_ascii_whitespace())
                        .collect(),
                )
            } else {
                Cow::Borrowed(*line)
            }
        };
        let mut interner = Interner::new(old.len() + new.len());
        let before = old.iter().map(|line| interner.intern(key(line))).collect();
        let after = new.iter().map(|line| interner.intern(key(line))).collect();
        Self {
            old,
            new,
            input: InternedInput {
                before,
                after,
                interner,
            },
            ignore_whitespace,
        }
    }

    /// The line diff with `algorithm`, with git's slider heuristic unless
    /// whitespace is ignored (the heuristic reads indentation).
    pub(crate) fn diff(&self, algorithm: Algorithm) -> Diff {
        let mut diff = Diff::compute(algorithm, &self.input);
        if self.ignore_whitespace {
            diff.postprocess_no_heuristic(&self.input);
        } else {
            diff.postprocess_lines(&self.input);
        }
        diff
    }
}

/// Builds the hunks of `diff` with `context`. `old_data` and `new_data` are
/// the full sides, kept only for [`Context::WholeFile`].
pub(crate) fn text_diff(
    lines: &Lines<'_>,
    diff: &Diff,
    context: Context,
    old_data: &[u8],
    new_data: &[u8],
) -> TextDiff {
    let text = |line: &[u8]| {
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        String::from_utf8_lossy(line).into_owned()
    };
    let changes: Vec<_> = diff.hunks().collect();
    let around = context.lines();
    let mut hunks = Vec::new();
    let mut at = 0;
    while at < changes.len() {
        // Changes whose context touches or overlaps join one hunk.
        let mut end = at + 1;
        while end < changes.len()
            && (changes[end].before.start as usize)
                .saturating_sub(changes[end - 1].before.end as usize)
                <= around.saturating_mul(2)
        {
            end += 1;
        }
        let first = &changes[at];
        let last = &changes[end - 1];
        let lead = (first.before.start as usize).min(around);
        let old_from = first.before.start as usize - lead;
        let new_from = first.after.start as usize - lead;
        let trail = (lines.old.len() - last.before.end as usize).min(around);
        let old_to = last.before.end as usize + trail;
        let new_to = last.after.end as usize + trail;

        let mut hunk_lines = Vec::new();
        let (mut old_at, mut new_at) = (old_from, new_from);
        let mut push = |kind, old: Option<usize>, new: Option<usize>, line: &[u8]| {
            hunk_lines.push(Line {
                kind,
                old_line: old.map(line_number),
                new_line: new.map(line_number),
                text: text(line),
            });
        };
        for change in &changes[at..end] {
            while old_at < change.before.start as usize {
                push(
                    LineKind::Context,
                    Some(old_at),
                    Some(new_at),
                    lines.new[new_at],
                );
                old_at += 1;
                new_at += 1;
            }
            for index in change.before.start..change.before.end {
                push(
                    LineKind::Removed,
                    Some(index as usize),
                    None,
                    lines.old[index as usize],
                );
            }
            for index in change.after.start..change.after.end {
                push(
                    LineKind::Added,
                    None,
                    Some(index as usize),
                    lines.new[index as usize],
                );
            }
            old_at = change.before.end as usize;
            new_at = change.after.end as usize;
        }
        while old_at < old_to {
            push(
                LineKind::Context,
                Some(old_at),
                Some(new_at),
                lines.new[new_at],
            );
            old_at += 1;
            new_at += 1;
        }
        hunks.push(Hunk {
            old_start: header_start(old_from, old_to),
            old_lines: count(old_to - old_from),
            new_start: header_start(new_from, new_to),
            new_lines: count(new_to - new_from),
            lines: hunk_lines,
        });
        at = end;
    }
    let whole = |data: &[u8]| {
        (context == Context::WholeFile).then(|| String::from_utf8_lossy(data).into_owned())
    };
    let missing_newline = |data: &[u8]| !data.is_empty() && !data.ends_with(b"\n");
    TextDiff {
        added: diff.count_additions(),
        removed: diff.count_removals(),
        hunks,
        old_text: whole(old_data),
        new_text: whole(new_data),
        old_missing_newline: missing_newline(old_data),
        new_missing_newline: missing_newline(new_data),
    }
}

fn count(lines: usize) -> u32 {
    u32::try_from(lines).unwrap_or(u32::MAX)
}

fn line_number(index: usize) -> u32 {
    count(index + 1)
}

/// git's hunk start: the first line, or the line before an empty range.
fn header_start(from: usize, to: usize) -> u32 {
    if to > from {
        line_number(from)
    } else {
        count(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(old: &str, new: &str, context: Context, ignore_whitespace: bool) -> TextDiff {
        let lines = Lines::new(old.as_bytes(), new.as_bytes(), ignore_whitespace);
        let diff = lines.diff(Algorithm::Histogram);
        text_diff(&lines, &diff, context, old.as_bytes(), new.as_bytes())
    }

    /// A hunk as unified diff text, header first.
    fn unified(hunk: &Hunk) -> String {
        let mut out = format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
        );
        for line in &hunk.lines {
            let sign = match line.kind {
                LineKind::Context => ' ',
                LineKind::Added => '+',
                LineKind::Removed => '-',
            };
            out.push(sign);
            out.push_str(&line.text);
            out.push('\n');
        }
        out
    }

    fn numbered(range: std::ops::RangeInclusive<u32>) -> String {
        range.map(|n| format!("{n}\n")).collect()
    }

    /// `numbered(1..=last)` with the given lines replaced.
    fn changed(last: u32, changes: &[(u32, &str)]) -> String {
        (1..=last)
            .map(|n| match changes.iter().find(|(at, _)| *at == n) {
                Some((_, text)) => format!("{text}\n"),
                None => format!("{n}\n"),
            })
            .collect()
    }

    #[test]
    fn ui_17_three_lines_of_context_and_separate_hunks() {
        let old = numbered(1..=20);
        let new = changed(20, &[(2, "two"), (18, "eighteen")]);
        let result = diff(&old, &new, Context::Three, false);
        let hunks: Vec<String> = result.hunks.iter().map(unified).collect();
        assert_eq!(
            hunks,
            [
                "@@ -1,5 +1,5 @@\n 1\n-2\n+two\n 3\n 4\n 5\n",
                "@@ -15,6 +15,6 @@\n 15\n 16\n 17\n-18\n+eighteen\n 19\n 20\n",
            ]
        );
        assert_eq!((result.added, result.removed), (2, 2));
        assert_eq!((result.old_text, result.new_text), (None, None));
    }

    #[test]
    fn ui_17_ten_lines_of_context_join_nearby_changes() {
        let old = numbered(1..=40);
        let new = changed(40, &[(5, "five"), (20, "twenty")]);
        assert_eq!(diff(&old, &new, Context::Three, false).hunks.len(), 2);
        let result = diff(&old, &new, Context::Ten, false);
        assert_eq!(result.hunks.len(), 1);
        let hunk = &result.hunks[0];
        assert_eq!((hunk.old_start, hunk.old_lines), (1, 30));
        assert_eq!((hunk.new_start, hunk.new_lines), (1, 30));
    }

    #[test]
    fn ui_17_whole_file_is_one_hunk_with_both_texts() {
        let old = numbered(1..=40);
        let new = changed(40, &[(5, "five"), (35, "thirty-five")]);
        let result = diff(&old, &new, Context::WholeFile, false);
        assert_eq!(result.hunks.len(), 1);
        assert_eq!(result.hunks[0].lines.len(), 42);
        assert_eq!(result.old_text.as_deref(), Some(old.as_str()));
        assert_eq!(result.new_text.as_deref(), Some(new.as_str()));
    }

    #[test]
    fn ui_17_ignore_whitespace() {
        let old = "fn main() {\n    run();\n}\n";
        let new = "fn main()  {\n\trun( );\n}\n";
        assert_eq!(diff(old, new, Context::Three, false).added, 2);
        let result = diff(old, new, Context::Three, true);
        assert_eq!((result.added, result.removed), (0, 0));
        assert!(result.hunks.is_empty());

        // Only the real change shows; context lines show the new text.
        let new = "fn main()  {\n    walk();\n}\n";
        let result = diff(old, new, Context::Three, true);
        assert_eq!(
            unified(&result.hunks[0]),
            "@@ -1,3 +1,3 @@\n fn main()  {\n-    run();\n+    walk();\n }\n"
        );
    }

    #[test]
    fn added_and_removed_files_and_missing_newlines() {
        let result = diff("", "a\nb", Context::Three, false);
        assert_eq!(unified(&result.hunks[0]), "@@ -0,0 +1,2 @@\n+a\n+b\n");
        assert!(result.new_missing_newline && !result.old_missing_newline);
        let result = diff("a\n", "", Context::Three, false);
        assert_eq!(unified(&result.hunks[0]), "@@ -1,1 +0,0 @@\n-a\n");
        // Adding the final newline is a change of that line.
        let result = diff("a\nb", "a\nb\n", Context::Three, false);
        assert_eq!(unified(&result.hunks[0]), "@@ -1,2 +1,2 @@\n a\n-b\n+b\n");
    }

    #[test]
    fn sec_3_lines_that_are_not_utf8_are_decoded_lossily() {
        let lines = Lines::new(b"caf\xe9\r\n", b"caf\xc3\xa9\r\n", false);
        let diff = lines.diff(Algorithm::Histogram);
        let result = text_diff(&lines, &diff, Context::Three, b"", b"");
        let texts: Vec<&str> = result.hunks[0]
            .lines
            .iter()
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(texts, ["caf\u{fffd}", "café"]);
    }
}
