//! Reads history pages and commit diffs through gitcroc-git, on background
//! threads, into rows ready to draw.

use gitcroc_git::{
    ChangedFile, Context as DiffContext, Cursor, DiffOptions, DiffRead, DiffSource, FileDiff,
    GitRepo, GraphRow, HistoryStart, LineKind, RefKind, RepoRead, Time,
};
use gpui_kit::SharedString;

/// One history row, its text prepared once.
pub struct Row {
    pub hash: String,
    pub short_hash: SharedString,
    pub subject: SharedString,
    pub author: SharedString,
    pub date: SharedString,
    pub refs: Vec<(RefKind, SharedString)>,
    pub graph: Option<GraphRow>,
    /// The widest lane this row draws, plus one.
    pub lanes: u32,
}

pub struct HistoryPage {
    pub rows: Vec<Row>,
    pub next: Option<Cursor>,
}

pub fn history_page(repo: &GitRepo, cursor: Option<Cursor>) -> Result<HistoryPage, String> {
    let page = repo
        .history(HistoryStart::All, cursor, None)
        .map_err(|error| error.to_string())?;
    let rows = page
        .rows
        .into_iter()
        .map(|commit| {
            let lanes = commit.graph.as_ref().map_or(0, |graph| {
                graph
                    .up
                    .iter()
                    .chain(&graph.down)
                    .flat_map(|edge| [edge.from, edge.to])
                    .chain([graph.lane])
                    .max()
                    .unwrap_or(0)
                    + 1
            });
            Row {
                short_hash: commit.short_hash.into(),
                subject: commit.subject.into(),
                author: commit.author_name.into(),
                date: format_date(commit.author_time).into(),
                refs: commit
                    .refs
                    .into_iter()
                    .map(|label| (label.kind, label.name.into()))
                    .collect(),
                graph: commit.graph,
                lanes,
                hash: commit.hash,
            }
        })
        .collect();
    Ok(HistoryPage {
        rows,
        next: page.next,
    })
}

/// One line of the diff pane.
pub enum DiffLine {
    File {
        path: SharedString,
        stats: SharedString,
    },
    Hunk(SharedString),
    Text {
        kind: LineKind,
        old: SharedString,
        new: SharedString,
        text: SharedString,
    },
    Note(SharedString),
}

pub fn changed_files(repo: &GitRepo, hash: &str) -> Result<Vec<ChangedFile>, String> {
    repo.changed_files(hash, 0)
        .map_err(|error| error.to_string())
}

/// The diff lines of `files`, which come from `changed_files(hash)`.
pub fn diff_lines(repo: &GitRepo, hash: &str, files: &[ChangedFile]) -> Vec<DiffLine> {
    let source = DiffSource::Commit {
        hash: hash.to_owned(),
        parent: 0,
    };
    let options = DiffOptions {
        context: DiffContext::Three,
        ..DiffOptions::default()
    };
    let mut lines = Vec::new();
    for file in files {
        let diff = repo.file_diff(&source, file, options);
        let stats = match &diff {
            Ok(FileDiff::Text(text)) => format!("+{}  −{}", text.added, text.removed),
            Ok(FileDiff::TooLarge { added, removed }) => format!("+{added}  −{removed}"),
            _ => String::new(),
        };
        lines.push(DiffLine::File {
            path: file.path.clone().into(),
            stats: stats.into(),
        });
        match diff {
            Ok(FileDiff::Text(text)) => {
                for hunk in text.hunks {
                    lines.push(DiffLine::Hunk(
                        format!(
                            "@@ -{},{} +{},{} @@",
                            hunk.old_start, hunk.old_lines, hunk.new_start, hunk.new_lines
                        )
                        .into(),
                    ));
                    for line in hunk.lines {
                        let number = |n: Option<u32>| n.map_or(SharedString::default(), |n| n.to_string().into());
                        lines.push(DiffLine::Text {
                            kind: line.kind,
                            old: number(line.old_line),
                            new: number(line.new_line),
                            text: line.text.replace('\t', "    ").into(),
                        });
                    }
                }
            }
            Ok(FileDiff::TooLarge { .. }) => lines.push(DiffLine::Note("Diff too large to show".into())),
            Ok(FileDiff::Binary { .. }) => lines.push(DiffLine::Note("Binary file".into())),
            Ok(FileDiff::Image { .. }) => lines.push(DiffLine::Note("Image".into())),
            Ok(_) => lines.push(DiffLine::Note("No text diff".into())),
            Err(error) => lines.push(DiffLine::Note(error.to_string().into())),
        }
    }
    lines
}

/// `YYYY-MM-DD HH:MM` in the commit's own time zone.
fn format_date(time: Time) -> String {
    let local = time.seconds + i64::from(time.offset);
    let days = local.div_euclid(86_400);
    let minutes = local.rem_euclid(86_400) / 60;
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        minutes / 60,
        minutes % 60
    )
}
