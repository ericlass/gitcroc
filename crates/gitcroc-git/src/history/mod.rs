//! Repo history in pages of at most 500 commits (ARCH-8, PERF-2), with graph
//! lanes per row (UI-9) and a core-side filter (UI-12).

mod lanes;
mod merged;
mod walk;

use std::collections::HashMap;
use std::sync::Arc;

use gix::bstr::ByteSlice;
use gix::hash::{ObjectId, oid};
use serde::Serialize;

use lanes::Lanes;
pub use lanes::{Edge, GraphRow};
pub use merged::{MergedCursor, MergedPage, MergedRow, merged_history};
use walk::{INFINITE, Info, Source, Walk};

use crate::GitRepo;

/// The most commits on one page (ARCH-8).
pub const PAGE_SIZE: usize = 500;

/// Where a history walk starts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryStart {
    /// `HEAD`, every local branch and every remote branch. Tags are labels
    /// only, not starting points.
    All,
    /// One ref or revision, such as `main`, `origin/main` or `v1.0`.
    Ref(String),
    /// One stash entry, `stash@{index}`. Its page holds just that commit.
    Stash(usize),
}

/// A filter on history rows (UI-12): a case-insensitive substring of the
/// subject or the author's name or email, or a prefix of the hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    text: String,
}

impl Filter {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_lowercase(),
        }
    }

    fn matches(&self, row: &CommitRow) -> bool {
        let contains = |field: &str| field.to_lowercase().contains(&self.text);
        contains(&row.subject)
            || contains(&row.author_name)
            || contains(&row.author_email)
            || row.hash.starts_with(&self.text)
    }
}

/// A point in time as git records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
pub struct Time {
    /// Seconds since the Unix epoch. A JavaScript number holds it exactly;
    /// specta refuses `i64` because it would be a `bigint`.
    #[specta(type = i32)]
    pub seconds: i64,
    /// The recorded time zone, in seconds east of UTC.
    pub offset: i32,
}

/// The kind of a ref label on a history row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum RefKind {
    /// `HEAD`, shown only when it is detached.
    Head,
    /// The local branch `HEAD` points to.
    CurrentBranch,
    LocalBranch,
    RemoteBranch,
    Tag,
}

/// A ref pointing at a commit, by its short name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct RefLabel {
    pub kind: RefKind,
    pub name: String,
}

/// One commit of a history page. Text from the repo is decoded lossily
/// (SEC-3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct CommitRow {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub author_name: String,
    pub author_email: String,
    pub author_time: Time,
    pub commit_time: Time,
    pub parents: Vec<String>,
    pub refs: Vec<RefLabel>,
    /// The graph cell; `None` on a filtered page, whose rows are not
    /// contiguous.
    pub graph: Option<GraphRow>,
}

/// One page of history.
#[derive(Debug, Clone, Serialize)]
pub struct Page {
    pub rows: Vec<CommitRow>,
    /// The walk read commits from a commit-graph file. Without one, the
    /// first page explores the whole history first (see `walk`), so a slow
    /// first page without it is worth an offer to write the file (PERF-2).
    pub commit_graph: bool,
    /// Continues the walk on the next page; `None` at the end of history.
    #[serde(skip)]
    pub next: Option<Cursor>,
}

/// Where the next page continues: the walk state and the lane state.
/// Opaque to callers; it is only valid for the start and filter it came
/// from.
#[derive(Debug, Clone)]
pub struct Cursor {
    start: HistoryStart,
    filter: Option<Filter>,
    walk: Walk,
    lanes: Lanes,
    labels: Arc<HashMap<ObjectId, Vec<RefLabel>>>,
}

impl Cursor {
    /// How many commits the walk has read so far, listed or not. Pages never
    /// read the whole history ahead (PERF-2).
    pub fn commits_read(&self) -> u64 {
        self.walk.commits_read()
    }
}

/// Why a history page could not be read.
#[derive(Debug, thiserror::Error)]
pub enum HistoryError {
    #[error("no commit named {name:?}: {source}")]
    UnknownRef {
        name: String,
        #[source]
        source: gix::Error,
    },
    #[error("there is no stash entry stash@{{{index}}}")]
    NoStash { index: usize },
    #[error("the cursor belongs to a different start or filter")]
    CursorMismatch,
    #[error("cannot read the repo: {0}")]
    Read(#[source] Box<dyn std::error::Error + Send + Sync + 'static>),
}

impl From<gix::Error> for HistoryError {
    fn from(error: gix::Error) -> Self {
        Self::Read(Box::new(error))
    }
}

/// Reads from a repo (ARCH-2). Implemented with `gix`; a git CLI fallback can
/// sit behind it without callers noticing.
pub trait RepoRead {
    /// A page of history from `start`, continuing at `cursor` when given,
    /// keeping only rows that match `filter` when given.
    fn history(
        &self,
        start: HistoryStart,
        cursor: Option<Cursor>,
        filter: Option<Filter>,
    ) -> Result<Page, HistoryError>;
}

impl RepoRead for GitRepo {
    fn history(
        &self,
        start: HistoryStart,
        cursor: Option<Cursor>,
        filter: Option<Filter>,
    ) -> Result<Page, HistoryError> {
        let repo = self.thread_local();
        let mut source = GixSource {
            repo: &repo,
            graph: repo.commit_graph_if_enabled().ok().flatten(),
        };
        let cursor = match cursor {
            Some(cursor) if cursor.start == start && cursor.filter == filter => cursor,
            Some(_) => return Err(HistoryError::CursorMismatch),
            None => {
                let starts = match &start {
                    HistoryStart::All => all_starts(&repo)?,
                    HistoryStart::Ref(name) => vec![resolve(&repo, name)?],
                    HistoryStart::Stash(index) => {
                        return stash_page(&repo, &mut source, *index, filter.as_ref());
                    }
                };
                Cursor {
                    start,
                    filter,
                    walk: Walk::new(&starts, &mut source)?,
                    lanes: Lanes::default(),
                    labels: Arc::new(labels(&repo)?),
                }
            }
        };
        page(&repo, &mut source, cursor)
    }
}

fn page(
    repo: &gix::Repository,
    source: &mut GixSource<'_>,
    mut cursor: Cursor,
) -> Result<Page, HistoryError> {
    let mut rows = Vec::new();
    let commit_graph = source.graph.is_some();
    while rows.len() < PAGE_SIZE {
        let Some((id, parents)) = cursor.walk.next(source)? else {
            break;
        };
        let refs = cursor.labels.get(&id).cloned().unwrap_or_default();
        let mut row = commit_row(repo, id, &parents, refs)?;
        match &cursor.filter {
            Some(filter) => {
                if filter.matches(&row) {
                    rows.push(row);
                }
            }
            None => {
                row.graph = Some(cursor.lanes.row(&id, &parents));
                rows.push(row);
            }
        }
    }
    let next = (!cursor.walk.is_done()).then_some(cursor);
    Ok(Page {
        rows,
        commit_graph,
        next,
    })
}

fn commit_row(
    repo: &gix::Repository,
    id: ObjectId,
    parents: &[ObjectId],
    refs: Vec<RefLabel>,
) -> Result<CommitRow, HistoryError> {
    let commit = repo.find_commit(id)?;
    let decoded = commit.decode()?;
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
    let time = |signature: Option<gix::actor::SignatureRef<'_>>| Time {
        seconds: signature.map_or(0, |s| s.seconds()),
        offset: signature
            .and_then(|s| s.time().ok())
            .map_or(0, |t| t.offset),
    };
    let author = decoded.author().ok();
    let short_hash = commit
        .id()
        .shorten()
        .map_or_else(|_| id.to_hex_with_len(7).to_string(), |p| p.to_string());
    Ok(CommitRow {
        hash: id.to_string(),
        short_hash,
        subject: text(decoded.message().summary().as_ref()),
        author_name: author.map(|a| text(a.name.trim())).unwrap_or_default(),
        author_email: author.map(|a| text(a.email.trim())).unwrap_or_default(),
        author_time: time(author),
        commit_time: time(decoded.committer().ok()),
        parents: parents.iter().map(ToString::to_string).collect(),
        refs,
        graph: None,
    })
}

/// `HEAD` and every local and remote branch.
fn all_starts(repo: &gix::Repository) -> Result<Vec<ObjectId>, HistoryError> {
    let mut starts: Vec<ObjectId> = repo
        .head()?
        .try_peel_to_id()?
        .map(|id| id.detach())
        .into_iter()
        .collect();
    let platform = repo.references()?;
    for list in [platform.local_branches()?, platform.remote_branches()?] {
        for reference in list {
            let mut reference = reference?;
            if reference.target().try_id().is_none() {
                continue; // symbolic, like refs/remotes/origin/HEAD
            }
            if let Ok(id) = reference.peel_to_id() {
                starts.push(id.detach());
            }
        }
    }
    starts.retain(|id| is_commit(repo, id));
    Ok(starts)
}

fn is_commit(repo: &gix::Repository, id: &oid) -> bool {
    repo.find_header(id)
        .is_ok_and(|header| header.kind() == gix::object::Kind::Commit)
}

fn resolve(repo: &gix::Repository, name: &str) -> Result<ObjectId, HistoryError> {
    let spec = format!("{name}^{{commit}}");
    repo.rev_parse_single(spec.as_str())
        .map(|id| id.detach())
        .map_err(|source| HistoryError::UnknownRef {
            name: name.to_owned(),
            source,
        })
}

/// The page of a stash: the stash commit alone, as a dot without lines.
fn stash_page(
    repo: &gix::Repository,
    source: &mut GixSource<'_>,
    index: usize,
    filter: Option<&Filter>,
) -> Result<Page, HistoryError> {
    let id = stash(repo, index)?;
    let parents = source.info(&id)?.parents;
    let mut row = commit_row(repo, id, &parents, Vec::new())?;
    let rows = match filter {
        Some(filter) => Some(row).filter(|row| filter.matches(row)),
        None => {
            row.graph = Some(GraphRow {
                lane: 0,
                color: 0,
                up: Vec::new(),
                down: Vec::new(),
            });
            Some(row)
        }
    };
    Ok(Page {
        rows: rows.into_iter().collect(),
        commit_graph: source.graph.is_some(),
        next: None,
    })
}

/// The commit of `stash@{index}`, newest first.
fn stash(repo: &gix::Repository, index: usize) -> Result<ObjectId, HistoryError> {
    let missing = HistoryError::NoStash { index };
    let Ok(Some(reference)) = repo.try_find_reference("refs/stash") else {
        return Err(missing);
    };
    let mut log = reference.log_iter();
    let Ok(Some(mut entries)) = log.rev() else {
        return Err(missing);
    };
    match entries.nth(index) {
        Some(Ok(line)) => Ok(line.new_oid),
        _ => Err(missing),
    }
}

/// Every ref label by the commit it points to.
fn labels(repo: &gix::Repository) -> Result<HashMap<ObjectId, Vec<RefLabel>>, HistoryError> {
    use gix::refs::Category;

    let mut labels: HashMap<ObjectId, Vec<RefLabel>> = HashMap::new();
    let head = repo.head()?;
    let current = head.referent_name().map(|name| name.as_bstr().to_owned());
    if head.is_detached()
        && let Some(id) = head.id()
    {
        labels.entry(id.detach()).or_default().push(RefLabel {
            kind: RefKind::Head,
            name: "HEAD".into(),
        });
    }
    for reference in repo.references()?.all()? {
        let mut reference = reference?;
        if reference.target().try_id().is_none() {
            continue;
        }
        let name = reference.name();
        let kind = match name.category() {
            Some(Category::LocalBranch)
                if current.as_ref().is_some_and(|c| c == name.as_bstr()) =>
            {
                RefKind::CurrentBranch
            }
            Some(Category::LocalBranch) => RefKind::LocalBranch,
            Some(Category::RemoteBranch) => RefKind::RemoteBranch,
            Some(Category::Tag) => RefKind::Tag,
            _ => continue,
        };
        let short = String::from_utf8_lossy(name.shorten()).into_owned();
        if let Ok(id) = reference.peel_to_id() {
            labels
                .entry(id.detach())
                .or_default()
                .push(RefLabel { kind, name: short });
        }
    }
    for list in labels.values_mut() {
        list.sort_by(|a, b| (a.kind as u8, &a.name).cmp(&(b.kind as u8, &b.name)));
    }
    Ok(labels)
}

/// Commits from the commit-graph file when it has them, from the object
/// database otherwise.
struct GixSource<'repo> {
    repo: &'repo gix::Repository,
    graph: Option<gix::commitgraph::Graph>,
}

impl Source for GixSource<'_> {
    type Error = HistoryError;

    fn info(&mut self, id: &oid) -> Result<Info, HistoryError> {
        if let Some(graph) = &self.graph
            && let Some(commit) = graph.commit_by_id(id)
        {
            let parents = commit
                .iter_parents()
                .map(|parent| parent.map(|position| graph.id_at(position).to_owned()))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| HistoryError::Read(Box::new(error)))?;
            let generation = match commit.generation() {
                0 => INFINITE,
                generation => generation,
            };
            return Ok(Info {
                generation,
                time: i64::try_from(commit.committer_timestamp()).unwrap_or(i64::MAX),
                parents,
            });
        }
        let commit = self.repo.find_commit(id.to_owned())?;
        let decoded = commit.decode()?;
        Ok(Info {
            generation: INFINITE,
            time: decoded.committer().map_or(0, |c| c.seconds()),
            parents: decoded.parents().collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(subject: &str, name: &str, email: &str, hash: &str) -> CommitRow {
        let time = Time {
            seconds: 0,
            offset: 0,
        };
        CommitRow {
            hash: hash.into(),
            short_hash: hash.chars().take(7).collect(),
            subject: subject.into(),
            author_name: name.into(),
            author_email: email.into(),
            author_time: time,
            commit_time: time,
            parents: vec![],
            refs: vec![],
            graph: None,
        }
    }

    #[test]
    fn ui_12_filter_matches_subject_author_or_hash_prefix() {
        let commit = row(
            "Fix the Parser",
            "Ada Lovelace",
            "ada@example.com",
            "c0ffee12",
        );
        for text in ["parser", "FIX THE", "lovelace", "ADA@", "c0ff", "C0FFEE12"] {
            assert!(Filter::new(text).matches(&commit), "{text}");
        }
        for text in ["offee", "Babbage", "parsers"] {
            assert!(!Filter::new(text).matches(&commit), "{text}");
        }
    }
}
