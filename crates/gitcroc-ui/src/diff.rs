//! The rows of a text diff (UI-17), a port of rc.1's `changes/diffDoc.ts`
//! and of how its MergeView laid out both files. Inline, the core's hunks
//! with both line numbers; side by side, both whole files paired line by
//! line, with each run of removed lines beside the added lines that
//! replaced it, and unchanged runs folded to the context. Which lines
//! changed is the core's answer, whitespace and context applied; nothing
//! is diffed here. Rows are built off the UI thread, as `SharedString`s.

use std::collections::HashSet;
use std::ops::Range;

use gitcroc_core::{Context, Hunk, LineKind, TextDiff};
use gpui_kit::SharedString;

/// Columns a tab advances to, as rc.1's editor showed them.
const TAB_WIDTH: usize = 4;
/// The fewest unchanged lines a fold hides, as rc.1's MergeView folded.
pub const MIN_FOLD: usize = 4;

/// One side's line in side-by-side mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub kind: LineKind,
    /// 1-based, on its own side.
    pub number: u32,
    pub text: SharedString,
}

/// A row of the diff, before folding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffRow {
    /// A hunk's `@@ -a,b +c,d @@` header; `index` counts the hunks.
    Hunk { index: usize, header: SharedString },
    /// An inline line with its number on each side it is on.
    Line {
        kind: LineKind,
        old: Option<u32>,
        new: Option<u32>,
        text: SharedString,
    },
    /// A side-by-side row; `None` is an empty cell where one side has
    /// fewer lines.
    Pair {
        old: Option<Cell>,
        new: Option<Cell>,
    },
}

/// A row as the list shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown<'a> {
    Row(&'a DiffRow),
    /// Folded unchanged lines: the fold's index and how many it hides.
    Fold {
        fold: usize,
        lines: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    Rows(Range<usize>),
    Fold(usize),
}

/// A text diff laid out as rows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffDoc {
    side_by_side: bool,
    rows: Vec<DiffRow>,
    /// Runs of rows that fold, in order.
    folds: Vec<Range<usize>>,
    /// The row of each hunk's header.
    hunks: Vec<usize>,
    /// The longest line, in columns.
    widest: usize,
    /// The largest line number.
    last_number: u32,
    unfolded: HashSet<usize>,
    segments: Vec<Segment>,
    /// Where each segment starts in the shown rows, and past the last.
    starts: Vec<usize>,
}

/// `text` with its tabs advanced to the next tab stop.
pub fn expand_tabs(text: &str) -> String {
    if !text.contains('\t') {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len() + TAB_WIDTH);
    let mut column = 0;
    for ch in text.chars() {
        if ch == '\t' {
            let spaces = TAB_WIDTH - column % TAB_WIDTH;
            out.extend(std::iter::repeat_n(' ', spaces));
            column += spaces;
        } else {
            out.push(ch);
            column += 1;
        }
    }
    out
}

/// A hunk's header, as in a unified diff.
pub fn hunk_header(hunk: &Hunk) -> String {
    format!(
        "@@ -{},{} +{},{} @@",
        hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
    )
}

/// The lines of a whole file, without their terminators, as the core
/// numbers them.
fn file_lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n')
        .map(|line| {
            let line = line.strip_suffix('\n').unwrap_or(line);
            line.strip_suffix('\r').unwrap_or(line)
        })
        .collect()
}

/// A run of removed lines and the added lines that replaced them, by
/// 1-based line numbers. `hunk` is set on a hunk's first run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    old: u32,
    removed: u32,
    new: u32,
    added: u32,
    hunk: Option<usize>,
}

/// The runs of changed lines of `diff`, as `changeRanges` finds them.
fn runs(diff: &TextDiff) -> Vec<Run> {
    let mut runs = Vec::new();
    for (index, hunk) in diff.hunks.iter().enumerate() {
        // The last line seen on each side; a run starts after it.
        let mut last_old = if hunk.old_lines > 0 {
            hunk.old_start.saturating_sub(1)
        } else {
            hunk.old_start
        };
        let mut last_new = if hunk.new_lines > 0 {
            hunk.new_start.saturating_sub(1)
        } else {
            hunk.new_start
        };
        let mut run: Option<Run> = None;
        let mut first = true;
        for line in &hunk.lines {
            if line.kind == LineKind::Context {
                runs.extend(run.take());
            } else {
                let run = run.get_or_insert_with(|| Run {
                    old: last_old + 1,
                    removed: 0,
                    new: last_new + 1,
                    added: 0,
                    hunk: std::mem::take(&mut first).then_some(index),
                });
                if line.kind == LineKind::Removed {
                    run.removed += 1;
                } else {
                    run.added += 1;
                }
            }
            last_old = line.old_line.unwrap_or(last_old);
            last_new = line.new_line.unwrap_or(last_new);
        }
        runs.extend(run);
    }
    runs
}

impl DiffDoc {
    /// The hunks one after the other, each line with both its numbers.
    pub fn inline(diff: &TextDiff) -> Self {
        let mut doc = Self::default();
        for (index, hunk) in diff.hunks.iter().enumerate() {
            doc.hunks.push(doc.rows.len());
            doc.rows.push(DiffRow::Hunk {
                index,
                header: hunk_header(hunk).into(),
            });
            for line in &hunk.lines {
                let text = expand_tabs(&line.text);
                doc.widest = doc.widest.max(text.chars().count());
                doc.last_number = doc
                    .last_number
                    .max(line.old_line.unwrap_or(0))
                    .max(line.new_line.unwrap_or(0));
                doc.rows.push(DiffRow::Line {
                    kind: line.kind,
                    old: line.old_line,
                    new: line.new_line,
                    text: text.into(),
                });
            }
        }
        doc.layout();
        doc
    }

    /// Both whole files side by side, unchanged runs folded to `context`;
    /// `None` while the diff came without them.
    pub fn side_by_side(diff: &TextDiff, context: Context) -> Option<Self> {
        let old = file_lines(diff.old_text.as_deref()?);
        let new = file_lines(diff.new_text.as_deref()?);
        let margin = match context {
            Context::Three => Some(3),
            Context::Ten => Some(10),
            Context::WholeFile => None,
        };
        let mut doc = Self {
            side_by_side: true,
            ..Self::default()
        };
        let cell = |doc: &mut Self, kind, lines: &[&str], number: u32| {
            let text = expand_tabs(lines.get(number as usize - 1).copied().unwrap_or(""));
            doc.widest = doc.widest.max(text.chars().count());
            doc.last_number = doc.last_number.max(number);
            Cell {
                kind,
                number,
                text: text.into(),
            }
        };
        // The next line of each side not yet shown, from 1.
        let (mut next_old, mut next_new) = (1u32, 1u32);
        let mut first = true;
        for run in runs(diff) {
            doc.unchanged(
                &old,
                &new,
                (next_old, run.old),
                (next_new, run.new),
                first,
                false,
                margin,
                &cell,
            );
            first = false;
            if let Some(index) = run.hunk {
                doc.hunks.push(doc.rows.len());
                doc.rows.push(DiffRow::Hunk {
                    index,
                    header: diff
                        .hunks
                        .get(index)
                        .map(hunk_header)
                        .unwrap_or_default()
                        .into(),
                });
            }
            for at in 0..run.removed.max(run.added) {
                let old = (at < run.removed)
                    .then(|| cell(&mut doc, LineKind::Removed, &old, run.old + at));
                let new =
                    (at < run.added).then(|| cell(&mut doc, LineKind::Added, &new, run.new + at));
                doc.rows.push(DiffRow::Pair { old, new });
            }
            next_old = run.old + run.removed;
            next_new = run.new + run.added;
        }
        let ends = (old.len() as u32 + 1, new.len() as u32 + 1);
        doc.unchanged(
            &old,
            &new,
            (next_old, ends.0),
            (next_new, ends.1),
            first,
            true,
            margin,
            &cell,
        );
        doc.layout();
        Some(doc)
    }

    /// Pairs the unchanged lines `old.0..old.1` and `new.0..new.1`, and
    /// folds what lies over `margin` lines from a change. At the start or
    /// the end of the file only the side next to a change keeps a margin.
    #[allow(clippy::too_many_arguments)]
    fn unchanged(
        &mut self,
        old_lines: &[&str],
        new_lines: &[&str],
        old: (u32, u32),
        new: (u32, u32),
        start: bool,
        end: bool,
        margin: Option<usize>,
        cell: &impl Fn(&mut Self, LineKind, &[&str], u32) -> Cell,
    ) {
        let first = self.rows.len();
        let old_count = old.1.saturating_sub(old.0);
        let new_count = new.1.saturating_sub(new.0);
        // Equal unless the core's lines and the whole files disagree.
        for at in 0..old_count.max(new_count) {
            let old =
                (at < old_count).then(|| cell(self, LineKind::Context, old_lines, old.0 + at));
            let new =
                (at < new_count).then(|| cell(self, LineKind::Context, new_lines, new.0 + at));
            self.rows.push(DiffRow::Pair { old, new });
        }
        let Some(margin) = margin else {
            return;
        };
        let count = self.rows.len() - first;
        let before = if start { 0 } else { margin };
        let after = if end { 0 } else { margin };
        let hidden = count.saturating_sub(before + after);
        if hidden >= MIN_FOLD {
            self.folds.push(first + before..first + before + hidden);
        }
    }

    /// Rebuilds the shown rows from the folds that are still folded.
    fn layout(&mut self) {
        self.segments.clear();
        let mut at = 0;
        for (index, fold) in self.folds.iter().enumerate() {
            if self.unfolded.contains(&index) {
                continue;
            }
            if at < fold.start {
                self.segments.push(Segment::Rows(at..fold.start));
            }
            self.segments.push(Segment::Fold(index));
            at = fold.end;
        }
        if at < self.rows.len() {
            self.segments.push(Segment::Rows(at..self.rows.len()));
        }
        self.starts.clear();
        let mut start = 0;
        for segment in &self.segments {
            self.starts.push(start);
            start += match segment {
                Segment::Rows(rows) => rows.len(),
                Segment::Fold(_) => 1,
            };
        }
        self.starts.push(start);
    }

    pub fn side_by_side_mode(&self) -> bool {
        self.side_by_side
    }

    /// The number of rows shown.
    pub fn len(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every row before folding, in order.
    pub fn rows(&self) -> &[DiffRow] {
        &self.rows
    }

    /// The segment shown row `index` is in, and the index within it.
    fn locate(&self, index: usize) -> Option<(&Segment, usize)> {
        let segment = self
            .starts
            .partition_point(|start| *start <= index)
            .checked_sub(1)?;
        Some((
            self.segments.get(segment)?,
            index - self.starts.get(segment)?,
        ))
    }

    /// The row before folding that shown row `index` is; `None` for a fold.
    pub fn row_index(&self, index: usize) -> Option<usize> {
        match self.locate(index)? {
            (Segment::Rows(rows), at) => Some(rows.start + at).filter(|row| *row < rows.end),
            (Segment::Fold(_), _) => None,
        }
    }

    /// Shown row `index`.
    pub fn get(&self, index: usize) -> Option<Shown<'_>> {
        match self.locate(index)? {
            (Segment::Rows(_), _) => self.rows.get(self.row_index(index)?).map(Shown::Row),
            (Segment::Fold(fold), _) => Some(Shown::Fold {
                fold: *fold,
                lines: self.folds.get(*fold).map_or(0, Range::len),
            }),
        }
    }

    /// Shows the lines of fold `fold`.
    pub fn unfold(&mut self, fold: usize) {
        if fold < self.folds.len() && self.unfolded.insert(fold) {
            self.layout();
        }
    }

    /// The folds opened so far, to open again in the same file's diff read
    /// anew.
    pub fn unfolded(&self) -> &HashSet<usize> {
        &self.unfolded
    }

    pub fn set_unfolded(&mut self, unfolded: &HashSet<usize>) {
        self.unfolded = unfolded
            .iter()
            .copied()
            .filter(|fold| *fold < self.folds.len())
            .collect();
        self.layout();
    }

    pub fn hunk_count(&self) -> usize {
        self.hunks.len()
    }

    /// The shown row of hunk `hunk`'s header; headers are never folded.
    pub fn hunk_row(&self, hunk: usize) -> Option<usize> {
        let row = *self.hunks.get(hunk)?;
        self.segments
            .iter()
            .zip(&self.starts)
            .find_map(|(segment, start)| match segment {
                Segment::Rows(rows) if rows.contains(&row) => Some(start + row - rows.start),
                _ => None,
            })
    }

    /// The longest line, in columns, for scrolling sideways.
    pub fn widest(&self) -> usize {
        self.widest
    }

    /// The digits of the largest line number, for the gutters' width.
    pub fn number_digits(&self) -> usize {
        self.last_number.max(1).to_string().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gitcroc_core::Line;

    fn line(kind: LineKind, old: Option<u32>, new: Option<u32>, text: &str) -> Line {
        Line {
            kind,
            old_line: old,
            new_line: new,
            text: text.to_owned(),
        }
    }

    fn context(old: u32, new: u32, text: &str) -> Line {
        line(LineKind::Context, Some(old), Some(new), text)
    }

    fn removed(old: u32, text: &str) -> Line {
        line(LineKind::Removed, Some(old), None, text)
    }

    fn added(new: u32, text: &str) -> Line {
        line(LineKind::Added, None, Some(new), text)
    }

    fn diff(hunks: Vec<Hunk>, old: Option<&str>, new: Option<&str>) -> TextDiff {
        TextDiff {
            added: 0,
            removed: 0,
            hunks,
            old_text: old.map(str::to_owned),
            new_text: new.map(str::to_owned),
            old_missing_newline: false,
            new_missing_newline: false,
            content: Default::default(),
        }
    }

    fn hunk(old: (u32, u32), new: (u32, u32), lines: Vec<Line>) -> Hunk {
        Hunk {
            old_start: old.0,
            old_lines: old.1,
            new_start: new.0,
            new_lines: new.1,
            lines,
        }
    }

    /// The shown rows in a compact form: `@0` for a hunk header, `~4` for
    /// a fold of four lines, `-2 a | +2 b` for a side-by-side pair, and
    /// `2 3 text` for an inline line.
    fn shown(doc: &DiffDoc) -> Vec<String> {
        let cell = |cell: &Option<Cell>| match cell {
            None => "_".to_owned(),
            Some(cell) => {
                let sign = match cell.kind {
                    LineKind::Context => " ",
                    LineKind::Added => "+",
                    LineKind::Removed => "-",
                };
                format!("{sign}{} {}", cell.number, cell.text)
            }
        };
        (0..doc.len())
            .map(|index| match doc.get(index).unwrap() {
                Shown::Fold { lines, .. } => format!("~{lines}"),
                Shown::Row(DiffRow::Hunk { index, .. }) => format!("@{index}"),
                Shown::Row(DiffRow::Pair { old, new }) => format!("{} | {}", cell(old), cell(new)),
                Shown::Row(DiffRow::Line { old, new, text, .. }) => format!(
                    "{} {} {text}",
                    old.map_or("_".into(), |n| n.to_string()),
                    new.map_or("_".into(), |n| n.to_string())
                ),
            })
            .collect()
    }

    /// Lines `1..=count` named `prefix1`, `prefix2`, ….
    fn numbered(prefix: &str, count: u32) -> String {
        (1..=count).map(|n| format!("{prefix}{n}\n")).collect()
    }

    #[test]
    fn ui_17_inline_rows_are_the_hunks_with_both_line_numbers() {
        let doc = DiffDoc::inline(&diff(
            vec![hunk(
                (1, 2),
                (1, 2),
                vec![context(1, 1, "a"), removed(2, "b"), added(2, "B")],
            )],
            None,
            None,
        ));
        assert_eq!(shown(&doc), ["@0", "1 1 a", "2 _ b", "_ 2 B"]);
        assert_eq!(doc.hunk_row(0), Some(0));
        assert!(!doc.side_by_side_mode());
        let DiffRow::Hunk { header, .. } = &doc.rows[0] else {
            panic!("no header");
        };
        assert_eq!(header.as_ref(), "@@ -1,2 +1,2 @@");
    }

    #[test]
    fn ui_17_side_by_side_pairs_uneven_runs() {
        let old = "a\nb\nc\nd\n";
        let new = "a\nB1\nB2\nB3\nd\n";
        let doc = DiffDoc::side_by_side(
            &diff(
                vec![hunk(
                    (1, 4),
                    (1, 5),
                    vec![
                        context(1, 1, "a"),
                        removed(2, "b"),
                        removed(3, "c"),
                        added(2, "B1"),
                        added(3, "B2"),
                        added(4, "B3"),
                        context(4, 5, "d"),
                    ],
                )],
                Some(old),
                Some(new),
            ),
            Context::Three,
        )
        .unwrap();
        assert_eq!(
            shown(&doc),
            [
                " 1 a |  1 a",
                "@0",
                "-2 b | +2 B1",
                "-3 c | +3 B2",
                "_ | +4 B3",
                " 4 d |  5 d",
            ]
        );
        assert_eq!(doc.hunk_row(0), Some(1));
    }

    #[test]
    fn ui_17_side_by_side_folds_unchanged_runs_to_the_context() {
        // 20 lines; line 10 changes. Three lines of context on each side
        // leave 6 hidden before and 7 after.
        let old = numbered("l", 20);
        let new = old.replace("l10\n", "L10\n");
        let doc = DiffDoc::side_by_side(
            &diff(
                vec![hunk(
                    (7, 7),
                    (7, 7),
                    vec![
                        context(7, 7, "l7"),
                        context(8, 8, "l8"),
                        context(9, 9, "l9"),
                        removed(10, "l10"),
                        added(10, "L10"),
                        context(11, 11, "l11"),
                        context(12, 12, "l12"),
                        context(13, 13, "l13"),
                    ],
                )],
                Some(&old),
                Some(&new),
            ),
            Context::Three,
        )
        .unwrap();
        assert_eq!(
            shown(&doc),
            [
                "~6",
                " 7 l7 |  7 l7",
                " 8 l8 |  8 l8",
                " 9 l9 |  9 l9",
                "@0",
                "-10 l10 | +10 L10",
                " 11 l11 |  11 l11",
                " 12 l12 |  12 l12",
                " 13 l13 |  13 l13",
                "~7",
            ]
        );
        assert_eq!(doc.hunk_row(0), Some(4));

        let mut doc = doc;
        doc.unfold(1);
        assert_eq!(doc.len(), 9 + 7);
        assert_eq!(shown(&doc).last().unwrap(), " 20 l20 |  20 l20");
        assert_eq!(doc.unfolded(), &HashSet::from([1]));
        doc.unfold(0);
        assert_eq!(doc.len(), 21);
        assert_eq!(doc.hunk_row(0), Some(9));
    }

    #[test]
    fn ui_17_a_fold_hides_at_least_four_lines() {
        // Two changes 9 lines apart: with 3 lines of context on each side,
        // 3 lines are left between them, too few to fold. 10 lines of
        // context fold nothing at all.
        let old = numbered("l", 12);
        let new = old.replace("l1\n", "L1\n").replace("l11\n", "L11\n");
        let lines = |doc: &DiffDoc| shown(doc).iter().filter(|row| row.starts_with('~')).count();
        let mut hunk_lines = vec![removed(1, "l1"), added(1, "L1")];
        hunk_lines.extend((2..=10).map(|n| context(n, n, &format!("l{n}"))));
        hunk_lines.extend([removed(11, "l11"), added(11, "L11"), context(12, 12, "l12")]);
        let text = diff(
            vec![hunk((1, 12), (1, 12), hunk_lines)],
            Some(&old),
            Some(&new),
        );
        assert_eq!(
            lines(&DiffDoc::side_by_side(&text, Context::Three).unwrap()),
            0
        );
        assert_eq!(
            lines(&DiffDoc::side_by_side(&text, Context::Ten).unwrap()),
            0
        );
        assert_eq!(
            DiffDoc::side_by_side(&text, Context::Three).unwrap().len(),
            13
        );
    }

    #[test]
    fn ui_17_whole_file_context_folds_nothing() {
        let old = numbered("l", 30);
        let new = format!("{old}l31\n");
        let doc = DiffDoc::side_by_side(
            &diff(
                vec![hunk((30, 0), (31, 1), vec![added(31, "l31")])],
                Some(&old),
                Some(&new),
            ),
            Context::WholeFile,
        )
        .unwrap();
        assert_eq!(doc.len(), 32);
        assert_eq!(doc.hunk_row(0), Some(30));
        assert_eq!(shown(&doc)[31], "_ | +31 l31");
    }

    #[test]
    fn ui_17_side_by_side_of_an_added_file_and_files_without_a_final_newline() {
        let doc = DiffDoc::side_by_side(
            &diff(
                vec![hunk((0, 0), (1, 2), vec![added(1, "a"), added(2, "b")])],
                Some(""),
                Some("a\r\nb"),
            ),
            Context::Three,
        )
        .unwrap();
        assert_eq!(shown(&doc), ["@0", "_ | +1 a", "_ | +2 b"]);

        let doc = DiffDoc::side_by_side(
            &diff(
                vec![hunk((1, 1), (1, 1), vec![removed(1, "x"), added(1, "y")])],
                Some("x"),
                Some("y"),
            ),
            Context::Three,
        )
        .unwrap();
        assert_eq!(shown(&doc), ["@0", "-1 x | +1 y"]);
    }

    #[test]
    fn ui_17_side_by_side_waits_for_the_whole_files() {
        assert!(DiffDoc::side_by_side(&diff(Vec::new(), None, Some("")), Context::Three).is_none());
    }

    #[test]
    fn tabs_advance_to_the_next_stop() {
        assert_eq!(expand_tabs("\tx"), "    x");
        assert_eq!(expand_tabs("ab\tc"), "ab  c");
        assert_eq!(expand_tabs("abcd\te"), "abcd    e");
        let doc = DiffDoc::inline(&diff(
            vec![hunk((1, 1), (1, 1), vec![context(1, 1, "\tlong line")])],
            None,
            None,
        ));
        assert_eq!(doc.widest(), 13);
        assert_eq!(doc.number_digits(), 1);
    }
}
