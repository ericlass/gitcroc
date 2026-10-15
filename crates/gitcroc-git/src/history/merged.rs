//! The merged history of several repos (PERF-5): a lazy k-way merge of one
//! walk per repo, newest first by commit date. Each walk starts at its
//! repo's `HEAD` and holds at most one row ahead, the one the merge compares
//! next, so a repo is walked no further than the page needs. Ties go to the
//! repo given first; within a repo the walk's own order holds.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;

use gix::hash::ObjectId;

use super::walk::Walk;
use super::{CommitRow, Filter, GixSource, HistoryError, PAGE_SIZE, RefLabel, commit_row, labels};
use crate::GitRepo;

/// A row of a merged page and the index of its repo in the paths given.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergedRow {
    pub repo: usize,
    pub row: CommitRow,
}

/// One page of a merged history.
#[derive(Debug, Clone)]
pub struct MergedPage {
    /// Rows have no graph cell: a merged list has no lanes (UI-9).
    pub rows: Vec<MergedRow>,
    /// Continues the merge on the next page; `None` once every repo ended.
    pub next: Option<MergedCursor>,
}

/// Where the next merged page continues: each repo's walk and its next row.
/// Opaque to callers; it is only valid for the paths and filter it came
/// from.
#[derive(Debug, Clone)]
pub struct MergedCursor {
    paths: Vec<PathBuf>,
    filter: Option<Filter>,
    streams: Vec<Stream>,
}

impl MergedCursor {
    /// How many commits the walk of repo `index` has read so far.
    pub fn commits_read(&self, index: usize) -> u64 {
        match self.streams.get(index) {
            Some(Stream::Walking(walking)) => walking.walk.commits_read(),
            Some(Stream::Ended { read, .. }) => *read,
            _ => 0,
        }
    }

    /// How many rows the walk of repo `index` has produced so far, listed or
    /// waiting as its next row.
    pub fn rows_read(&self, index: usize) -> u64 {
        match self.streams.get(index) {
            Some(Stream::Walking(walking)) => walking.rows,
            Some(Stream::Ended { rows, .. }) => *rows,
            _ => 0,
        }
    }
}

/// One repo's part of the merge.
#[derive(Debug, Clone)]
enum Stream {
    /// Not opened yet.
    Pending,
    Walking(Box<Walking>),
    /// No more rows: the walk ended, or the repo is missing or unborn.
    Ended {
        read: u64,
        rows: u64,
    },
}

/// A repo's walk under way.
#[derive(Debug, Clone)]
struct Walking {
    walk: Walk,
    labels: Arc<HashMap<ObjectId, Vec<RefLabel>>>,
    /// The next row, not listed yet; `None` until it is read.
    head: Option<CommitRow>,
    rows: u64,
}

/// A page of the merged history of the repos at `paths`, continuing at
/// `cursor` when given, keeping only rows that match `filter` when given
/// (UI-12). A path that does not exist and a repo whose `HEAD` is unborn
/// give no rows.
pub fn merged_history(
    paths: &[PathBuf],
    cursor: Option<MergedCursor>,
    filter: Option<Filter>,
) -> Result<MergedPage, HistoryError> {
    let mut cursor = match cursor {
        Some(cursor) if cursor.paths == paths && cursor.filter == filter => cursor,
        Some(_) => return Err(HistoryError::CursorMismatch),
        None => MergedCursor {
            paths: paths.to_vec(),
            filter,
            streams: vec![Stream::Pending; paths.len()],
        },
    };
    // Repos are opened on this page only, and only those whose next row is
    // needed, as a single repo's walk does (M1-14).
    let mut open: Vec<Option<gix::Repository>> = vec![None; paths.len()];
    let mut heap = BinaryHeap::new();
    for index in 0..paths.len() {
        if let Some(time) = cursor.fill(index, &mut open)? {
            heap.push((time, Reverse(index)));
        }
    }
    let mut rows = Vec::new();
    while rows.len() < PAGE_SIZE {
        let Some((_, Reverse(index))) = heap.pop() else {
            break;
        };
        if let Some(Stream::Walking(walking)) = cursor.streams.get_mut(index)
            && let Some(row) = walking.head.take()
        {
            rows.push(MergedRow { repo: index, row });
        }
        if let Some(time) = cursor.fill(index, &mut open)? {
            heap.push((time, Reverse(index)));
        }
    }
    let more = cursor.streams.iter().any(|stream| match stream {
        Stream::Walking(walking) => walking.head.is_some() || !walking.walk.is_done(),
        Stream::Pending => true,
        Stream::Ended { .. } => false,
    });
    Ok(MergedPage {
        rows,
        next: more.then_some(cursor),
    })
}

impl MergedCursor {
    /// Makes sure repo `index` has its next row read, and returns that row's
    /// commit time; `None` when the repo has no more rows.
    fn fill(
        &mut self,
        index: usize,
        open: &mut [Option<gix::Repository>],
    ) -> Result<Option<i64>, HistoryError> {
        let (Some(stream), Some(path), Some(slot)) = (
            self.streams.get_mut(index),
            self.paths.get(index),
            open.get_mut(index),
        ) else {
            return Ok(None);
        };
        match stream {
            Stream::Ended { .. } => return Ok(None),
            Stream::Walking(walking) => {
                if let Some(row) = &walking.head {
                    return Ok(Some(row.commit_time.seconds));
                }
            }
            Stream::Pending => {}
        }
        let repo = match slot {
            Some(repo) => repo,
            None => {
                if !path.exists() {
                    *stream = Stream::Ended { read: 0, rows: 0 };
                    return Ok(None);
                }
                let opened = GitRepo::open(path)
                    .map_err(|error| HistoryError::Read(Box::new(error)))?
                    .thread_local();
                slot.insert(opened)
            }
        };
        let mut source = GixSource {
            repo,
            graph: repo.commit_graph_if_enabled().ok().flatten(),
        };
        if matches!(stream, Stream::Pending) {
            let Some(start) = repo.head()?.try_peel_to_id()?.map(|id| id.detach()) else {
                *stream = Stream::Ended { read: 0, rows: 0 };
                return Ok(None);
            };
            *stream = Stream::Walking(Box::new(Walking {
                walk: Walk::new(&[start], &mut source)?,
                labels: Arc::new(labels(repo)?),
                head: None,
                rows: 0,
            }));
        }
        let Stream::Walking(walking) = stream else {
            return Ok(None);
        };
        let Walking {
            walk,
            labels,
            head,
            rows,
        } = walking.as_mut();
        while let Some((id, parents)) = walk.next(&mut source)? {
            let refs = labels.get(&id).cloned().unwrap_or_default();
            let row = commit_row(repo, id, &parents, refs)?;
            if self
                .filter
                .as_ref()
                .is_none_or(|filter| filter.matches(&row))
            {
                *rows += 1;
                let time = row.commit_time.seconds;
                *head = Some(row);
                return Ok(Some(time));
            }
        }
        *stream = Stream::Ended {
            read: walk.commits_read(),
            rows: *rows,
        };
        Ok(None)
    }
}
