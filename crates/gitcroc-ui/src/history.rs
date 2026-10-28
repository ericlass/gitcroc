//! One workspace's history as the core pages it (ARCH-8, PERF-2), and the
//! source the tree selection shows (section 8 "History pane"). A port of
//! rc.1's `stores/history.ts` and `history/rows.ts`, without a UI toolkit.
//! The core walks, merges, lays out lanes and filters (ARCH-6); this keeps
//! its pages, prepares their text off the UI thread (ARCH-7) and remembers
//! where the user was in each source (ARCH-9, UI-29).

use std::collections::HashMap;

use gitcroc_app::app::{App, AppError};
use gitcroc_core::{
    GraphRow, HistoryTarget, HistoryView, RefKind, RepoId, Time, TreeItem, WorkspaceId,
};
use gpui_kit::SharedString;

use crate::strings;
use crate::tree::repo_of;

/// The id of the "Uncommitted changes" row; commit rows use their hash.
pub const UNCOMMITTED_ID: &str = "uncommitted";

/// What a tree selection shows in the history pane (section 8).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistorySource {
    /// Positions are kept under it.
    pub key: String,
    /// `None` for the merged history of a group or several repos.
    pub repo: Option<RepoId>,
    pub target: HistoryTarget,
    /// The "Uncommitted changes" row may lead the list (UI-10, UI-34).
    pub uncommitted: bool,
}

/// The history a tree node shows, or `None` for none (Ungrouped, a group's
/// nodes, nothing). A repo and the nodes that only group its refs show the
/// whole repo. Each history has one key wherever the repo appears (UI-2).
pub fn source_of(item: &TreeItem) -> Option<HistorySource> {
    let repo = repo_of(item)?;
    Some(match item {
        TreeItem::Branch { name, current, .. } => HistorySource {
            key: format!("branch:{}:{name}", repo.0),
            repo: Some(repo.clone()),
            target: HistoryTarget::Branch { name: name.clone() },
            uncommitted: *current,
        },
        TreeItem::RemoteBranch { name, .. } => HistorySource {
            key: format!("remote_branch:{}:{name}", repo.0),
            repo: Some(repo.clone()),
            target: HistoryTarget::RemoteBranch { name: name.clone() },
            uncommitted: false,
        },
        TreeItem::Tag { name, .. } => HistorySource {
            key: format!("tag:{}:{name}", repo.0),
            repo: Some(repo.clone()),
            target: HistoryTarget::Tag { name: name.clone() },
            uncommitted: false,
        },
        TreeItem::Stash { index, .. } => HistorySource {
            key: format!("stash:{}:{index}", repo.0),
            repo: Some(repo.clone()),
            target: HistoryTarget::Stash { index: *index },
            uncommitted: false,
        },
        _ => HistorySource {
            key: format!("repo:{}", repo.0),
            repo: Some(repo.clone()),
            target: HistoryTarget::Repo,
            uncommitted: true,
        },
    })
}

/// One history row, its text prepared once.
#[derive(Debug, Clone)]
pub struct Row {
    pub hash: String,
    pub short_hash: SharedString,
    pub subject: SharedString,
    pub author: SharedString,
    pub date: SharedString,
    /// The full author and committer dates, for a tooltip.
    pub dates: SharedString,
    pub refs: Vec<(RefKind, SharedString)>,
    pub graph: Option<GraphRow>,
    pub repo_id: RepoId,
}

/// A page of history ready for the list; the text was prepared off the UI
/// thread (ARCH-7).
#[derive(Debug)]
pub struct Page {
    pub rows: Vec<Row>,
    pub handle: u32,
    pub done: bool,
    pub repos: Vec<RepoId>,
    pub offer_commit_graph: bool,
}

/// Where the user was in one source's history.
#[derive(Debug, Clone, Default)]
struct Position {
    rows: usize,
    scroll_top: f32,
    selected: Option<String>,
}

/// What a load restores once its first pages are read.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Restore {
    pub rows: usize,
    pub scroll_top: f32,
}

/// What showing a source did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Show {
    /// The same source, so nothing is read again.
    Kept,
    /// No source: the pane shows its empty state.
    Empty,
    /// A new source: read it, then restore this position.
    Load(Restore),
}

/// What the changes pane shows for the selected row (M6-12).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Uncommitted { repo: RepoId },
    Commit { repo: RepoId, hash: String },
}

/// The loaded history of one workspace.
#[derive(Debug)]
pub struct History {
    source: Option<HistorySource>,
    rows: Vec<Row>,
    repos: Vec<RepoId>,
    handle: Option<u32>,
    done: bool,
    loading: bool,
    error: Option<String>,
    selected: Option<String>,
    scroll_top: f32,
    positions: HashMap<String, Position>,
}

impl Default for History {
    fn default() -> Self {
        Self {
            source: None,
            rows: Vec::new(),
            repos: Vec::new(),
            handle: None,
            done: true,
            loading: false,
            error: None,
            selected: None,
            scroll_top: 0.,
            positions: HashMap::new(),
        }
    }
}

impl History {
    pub fn source(&self) -> Option<&HistorySource> {
        self.source.as_ref()
    }

    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub fn repos(&self) -> &[RepoId] {
        &self.repos
    }

    pub fn handle(&self) -> Option<u32> {
        self.handle
    }

    pub fn done(&self) -> bool {
        self.done
    }

    pub fn loading(&self) -> bool {
        self.loading
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    pub fn scroll_top(&self) -> f32 {
        self.scroll_top
    }

    /// The id of `row`: its hash, and in a merged history its repo's id
    /// before it, as the same commit may be in two repos.
    pub fn row_id(&self, row: &Row) -> String {
        row_id_of(self.source.as_ref(), row)
    }

    pub fn row_id_at(&self, index: usize) -> Option<String> {
        self.rows.get(index).map(|row| self.row_id(row))
    }

    /// The repo and hash of commit row `row_id`.
    pub fn commit_of(&self, row_id: &str) -> Option<(RepoId, String)> {
        commit_of(self.source.as_ref()?, row_id)
    }

    /// What the selected row shows in the changes pane (M6-12).
    pub fn selection(&self) -> Option<Selection> {
        let selected = self.selected.as_deref()?;
        if selected == UNCOMMITTED_ID {
            return self.source.as_ref().and_then(|source| {
                source
                    .repo
                    .clone()
                    .map(|repo| Selection::Uncommitted { repo })
            });
        }
        self.commit_of(selected)
            .map(|(repo, hash)| Selection::Commit { repo, hash })
    }

    /// Shows `source`, keeping where the user was in the previous one.
    pub fn show(&mut self, source: Option<HistorySource>) -> Show {
        let same = match (&self.source, &source) {
            (Some(current), Some(next)) => current.key == next.key,
            (None, None) => true,
            _ => false,
        };
        if same {
            if let (Some(current), Some(next)) = (&mut self.source, source) {
                current.uncommitted = next.uncommitted;
            }
            return Show::Kept;
        }
        self.save_position();
        let saved = source
            .as_ref()
            .and_then(|source| self.positions.get(&source.key).cloned())
            .unwrap_or_default();
        self.source = source;
        self.rows.clear();
        self.repos.clear();
        self.handle = None;
        self.done = true;
        self.loading = false;
        self.error = None;
        self.selected = saved.selected;
        self.scroll_top = saved.scroll_top;
        match &self.source {
            None => Show::Empty,
            Some(_) => Show::Load(Restore {
                rows: saved.rows,
                scroll_top: saved.scroll_top,
            }),
        }
    }

    /// The position the user left `source` at.
    fn save_position(&mut self) {
        let Some(source) = &self.source else {
            return;
        };
        self.positions.insert(
            source.key.clone(),
            Position {
                rows: self.rows.len(),
                scroll_top: self.scroll_top,
                selected: self.selected.clone(),
            },
        );
    }

    pub fn start_load(&mut self) {
        self.loading = true;
        self.error = None;
        self.done = false;
    }

    /// Replaces the rows with a first read.
    pub fn set_page(&mut self, page: Page) {
        self.rows = page.rows;
        self.repos = page.repos;
        self.handle = Some(page.handle);
        self.done = page.done;
        self.loading = false;
        self.error = None;
        if self.selected.as_ref().is_some_and(|selected| {
            !self
                .rows
                .iter()
                .any(|row| row_id_of(self.source.as_ref(), row) == *selected)
        }) {
            self.selected = None;
        }
    }

    /// Adds a page to the loaded rows.
    pub fn append(&mut self, page: Page) {
        self.rows.extend(page.rows);
        self.handle = Some(page.handle);
        self.done = page.done;
        self.loading = false;
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.loading = loading;
    }

    pub fn set_error(&mut self, error: String) {
        self.error = Some(error);
        self.loading = false;
        self.done = true;
    }

    pub fn select(&mut self, row_id: Option<String>) {
        self.selected = row_id;
    }

    pub fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_deref()?;
        self.rows
            .iter()
            .position(|row| row_id_of(self.source.as_ref(), row) == selected)
    }

    pub fn set_scroll_top(&mut self, top: f32) {
        self.scroll_top = top;
    }
}

/// The id of `row` of `source` (UI-9).
pub fn row_id_of(source: Option<&HistorySource>, row: &Row) -> String {
    match source.and_then(|source| source.repo.as_ref()) {
        Some(_) => row.hash.clone(),
        None => format!("{}:{}", row.repo_id.0, row.hash),
    }
}

/// The repo and hash of commit row `row_id` of `source`.
pub fn commit_of(source: &HistorySource, row_id: &str) -> Option<(RepoId, String)> {
    if let Some(repo) = &source.repo {
        return Some((repo.clone(), row_id.to_owned()));
    }
    // A hash has no colon; a repo id may.
    let at = row_id.rfind(':')?;
    Some((RepoId(row_id[..at].to_owned()), row_id[at + 1..].to_owned()))
}

/// Reads the first page of `source`, and more pages until `rows_needed` are
/// loaded, on the app's runtime. The text is prepared there too (ARCH-7).
pub async fn read(
    app: App,
    workspace_id: WorkspaceId,
    source: HistorySource,
    text: String,
    rows_needed: usize,
) -> Result<Option<Page>, AppError> {
    let Some(first) = app
        .history_get(
            workspace_id.clone(),
            source.repo.clone(),
            source.target.clone(),
            text,
        )
        .await?
    else {
        return Ok(None);
    };
    let mut rows = prepare_rows(first.rows);
    let mut handle = first.handle;
    let mut done = first.done;
    let repos = first.repos;
    let offer_commit_graph = first.offer_commit_graph;
    while rows.len() < rows_needed && !done {
        let Some(next) = app.history_more(workspace_id.clone(), handle).await? else {
            return Ok(None);
        };
        rows.extend(prepare_rows(next.rows));
        handle = next.handle;
        done = next.done;
    }
    Ok(Some(Page {
        rows,
        handle,
        done,
        repos,
        offer_commit_graph,
    }))
}

/// Reads the next page of the walk `handle`.
pub async fn read_more(
    app: App,
    workspace_id: WorkspaceId,
    handle: u32,
) -> Result<Option<Page>, AppError> {
    let Some(view) = app.history_more(workspace_id, handle).await? else {
        return Ok(None);
    };
    Ok(Some(prepare(view)))
}

fn prepare(view: HistoryView) -> Page {
    Page {
        rows: prepare_rows(view.rows),
        handle: view.handle,
        done: view.done,
        repos: view.repos,
        offer_commit_graph: view.offer_commit_graph,
    }
}

fn prepare_rows(rows: Vec<gitcroc_core::HistoryRow>) -> Vec<Row> {
    rows.into_iter()
        .map(|row| {
            let commit = row.commit;
            Row {
                short_hash: commit.short_hash.into(),
                subject: commit.subject.into(),
                author: commit.author_name.into(),
                date: format_date(commit.author_time).into(),
                dates: strings::history::dates(
                    &format_full_date(commit.author_time),
                    &format_full_date(commit.commit_time),
                )
                .into(),
                refs: commit
                    .refs
                    .into_iter()
                    .map(|label| (label.kind, label.name.into()))
                    .collect(),
                graph: commit.graph,
                hash: commit.hash,
                repo_id: row.repo_id,
            }
        })
        .collect()
}

const MINUTE: i64 = 60;
const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;

/// `YYYY-MM-DD HH:MM` in the commit's recorded time zone.
fn format_date(time: Time) -> String {
    let local = time.seconds + i64::from(time.offset);
    let days = local.div_euclid(DAY);
    let minutes = local.rem_euclid(DAY) / MINUTE;
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        minutes / 60,
        minutes % 60
    )
}

/// `YYYY-MM-DD HH:MM:SS +HHMM` in the commit's recorded time zone.
fn format_full_date(time: Time) -> String {
    let local = time.seconds + i64::from(time.offset);
    let days = local.div_euclid(DAY);
    let of_day = local.rem_euclid(DAY);
    let (year, month, day) = civil(days);
    let offset = i64::from(time.offset);
    let sign = if offset < 0 { '-' } else { '+' };
    let offset = offset.abs();
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} {sign}{:02}{:02}",
        of_day / HOUR,
        of_day / MINUTE % 60,
        of_day % 60,
        offset / HOUR,
        offset / MINUTE % 60
    )
}

/// The date `days` after 1970-01-01, by Howard Hinnant's `civil_from_days`.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
