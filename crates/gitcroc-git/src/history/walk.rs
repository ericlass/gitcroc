//! The history walk (PERF-2): newest first by commit date, never a commit
//! above one of its children, like `git log --date-order`, and resumable at
//! any point.
//!
//! Commits whose children have all been listed wait in `ready`, ordered by
//! commit date. Before the newest of them is listed, every commit with a
//! generation number at least as high is explored, which is enough to have
//! seen all its children: a child's generation is higher than its parent's.
//! With a commit-graph file this explores a little ahead of the listed
//! commits. Without one, generation numbers are unknown and treated as
//! infinite, and the first commit listed explores the whole history, as
//! `git log --date-order` does.

use std::cmp::{Ordering, Reverse};
use std::collections::{BinaryHeap, HashMap};

use gix::hash::{ObjectId, oid};

/// The generation of a commit outside the commit-graph file.
pub(crate) const INFINITE: u32 = u32::MAX;

/// What the walk needs to know about a commit.
#[derive(Debug, Clone)]
pub(crate) struct Info {
    pub generation: u32,
    pub time: i64,
    pub parents: Vec<ObjectId>,
}

/// Where the walk reads commits from.
pub(crate) trait Source {
    type Error;
    fn info(&mut self, id: &oid) -> Result<Info, Self::Error>;
}

#[derive(Debug, Clone)]
struct Node {
    info: Info,
    /// Explored children not listed yet.
    waiting_children: u32,
    explored: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Ready {
    time: i64,
    order: Reverse<u64>,
    id: ObjectId,
}

impl Ord for Ready {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.time, self.order).cmp(&(other.time, other.order))
    }
}

impl PartialOrd for Ready {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The walk state between two pages.
#[derive(Debug, Clone, Default)]
pub(crate) struct Walk {
    /// Commits seen and not listed yet. A listed commit is dropped: all its
    /// children were explored before it, so nothing reaches it again.
    nodes: HashMap<ObjectId, Node>,
    explore: BinaryHeap<(u32, ObjectId)>,
    ready: BinaryHeap<Ready>,
    inserted: u64,
    /// Commits read from the source so far (PERF-2 tests count them).
    read: u64,
}

impl Walk {
    /// Starts a walk at `starts`; duplicates are ignored.
    pub(crate) fn new<S: Source>(starts: &[ObjectId], source: &mut S) -> Result<Self, S::Error> {
        let mut walk = Self::default();
        for id in starts {
            if !walk.nodes.contains_key(id) {
                walk.discover(*id, 0, source)?;
                walk.push_ready(*id);
            }
        }
        Ok(walk)
    }

    /// Whether every commit has been listed.
    pub(crate) fn is_done(&self) -> bool {
        self.nodes.is_empty()
    }

    pub(crate) fn commits_read(&self) -> u64 {
        self.read
    }

    /// The next commit and its parents, or `None` at the end of history.
    pub(crate) fn next<S: Source>(
        &mut self,
        source: &mut S,
    ) -> Result<Option<(ObjectId, Vec<ObjectId>)>, S::Error> {
        while let Some(top) = self.ready.peek() {
            let id = top.id;
            let Some(generation) = self.listable(&id).map(|node| node.info.generation) else {
                self.ready.pop();
                continue;
            };
            while let Some(&(next, explore)) = self.explore.peek()
                && next >= generation
            {
                self.explore.pop();
                self.explore_one(explore, source)?;
            }
            self.ready.pop();
            if self.listable(&id).is_none() {
                // A child turned up; the commit returns once it is listed.
                continue;
            }
            let Some(node) = self.nodes.remove(&id) else {
                continue;
            };
            for parent in &node.info.parents {
                if let Some(parent_node) = self.nodes.get_mut(parent) {
                    parent_node.waiting_children = parent_node.waiting_children.saturating_sub(1);
                    if parent_node.waiting_children == 0 {
                        self.push_ready(*parent);
                    }
                }
            }
            return Ok(Some((id, node.info.parents)));
        }
        Ok(None)
    }

    /// The node if it is waiting for no child.
    fn listable(&self, id: &oid) -> Option<&Node> {
        self.nodes.get(id).filter(|node| node.waiting_children == 0)
    }

    fn discover<S: Source>(
        &mut self,
        id: ObjectId,
        waiting_children: u32,
        source: &mut S,
    ) -> Result<(), S::Error> {
        let info = source.info(&id)?;
        self.read += 1;
        self.explore.push((info.generation, id));
        self.nodes.insert(
            id,
            Node {
                info,
                waiting_children,
                explored: false,
            },
        );
        Ok(())
    }

    fn explore_one<S: Source>(&mut self, id: ObjectId, source: &mut S) -> Result<(), S::Error> {
        let parents = match self.nodes.get_mut(&id) {
            Some(node) if !node.explored => {
                node.explored = true;
                node.info.parents.clone()
            }
            _ => return Ok(()),
        };
        for parent in parents {
            match self.nodes.get_mut(&parent) {
                Some(node) => node.waiting_children += 1,
                None => self.discover(parent, 1, source)?,
            }
        }
        Ok(())
    }

    fn push_ready(&mut self, id: ObjectId) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        self.inserted += 1;
        self.ready.push(Ready {
            time: node.info.time,
            order: Reverse(self.inserted),
            id,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;

    /// A graph from (name, time, parents) with generation numbers computed
    /// like a commit-graph file, or all infinite without one.
    struct Table {
        commits: HashMap<ObjectId, Info>,
    }

    fn id(name: char) -> ObjectId {
        let mut bytes = [0u8; 20];
        bytes[0] = name as u8;
        ObjectId::from_bytes_or_panic(&bytes)
    }

    impl Table {
        fn new(rows: &[(char, i64, &str)], with_graph: bool) -> Self {
            let mut commits: HashMap<ObjectId, Info> = HashMap::new();
            // Rows are listed parents first.
            for (name, time, parents) in rows {
                let parents: Vec<ObjectId> = parents.chars().map(id).collect();
                let generation = if with_graph {
                    1 + parents
                        .iter()
                        .map(|p| commits[p].generation)
                        .max()
                        .unwrap_or(0)
                } else {
                    INFINITE
                };
                commits.insert(
                    id(*name),
                    Info {
                        generation,
                        time: *time,
                        parents,
                    },
                );
            }
            Self { commits }
        }
    }

    impl Source for Table {
        type Error = Infallible;
        fn info(&mut self, id: &oid) -> Result<Info, Infallible> {
            Ok(self.commits[id].clone())
        }
    }

    fn order(rows: &[(char, i64, &str)], starts: &str, with_graph: bool) -> String {
        let mut table = Table::new(rows, with_graph);
        let starts: Vec<ObjectId> = starts.chars().map(id).collect();
        let mut walk = Walk::new(&starts, &mut table).unwrap();
        let mut listed = String::new();
        while let Some((commit, _)) = walk.next(&mut table).unwrap() {
            listed.push(commit.as_bytes()[0] as char);
        }
        listed
    }

    #[test]
    fn perf_2_date_order_with_children_first() {
        // a - b - d (main), a - c (side, newest), d merges nothing.
        let rows = [('a', 1, ""), ('b', 2, "a"), ('c', 5, "a"), ('d', 3, "b")];
        for graph in [true, false] {
            assert_eq!(order(&rows, "dc", graph), "cdba");
        }
    }

    #[test]
    fn perf_2_clock_skew_never_lists_a_parent_first() {
        // b's date is older than its parent's; c is a merge with a skewed
        // side branch.
        let rows = [
            ('a', 10, ""),
            ('b', 1, "a"),
            ('x', 20, "a"),
            ('y', 2, "x"),
            ('c', 30, "by"),
        ];
        for graph in [true, false] {
            let listed = order(&rows, "c", graph);
            assert_eq!(listed.len(), 5, "{listed}");
            for (child, parents) in [('b', "a"), ('x', "a"), ('y', "x"), ('c', "by")] {
                for parent in parents.chars() {
                    let at = |c| listed.find(c).unwrap();
                    assert!(at(child) < at(parent), "{child} after {parent}: {listed}");
                }
            }
        }
    }

    #[test]
    fn perf_2_resuming_a_cloned_walk_continues_it() {
        let rows = [('a', 1, ""), ('b', 2, "a"), ('c', 3, "b"), ('d', 4, "c")];
        let mut table = Table::new(&rows, true);
        let mut walk = Walk::new(&[id('d')], &mut table).unwrap();
        walk.next(&mut table).unwrap();
        let mut copy = walk.clone();
        let rest = |walk: &mut Walk, table: &mut Table| {
            std::iter::from_fn(|| walk.next(table).unwrap())
                .map(|(c, _)| c.as_bytes()[0] as char)
                .collect::<String>()
        };
        assert_eq!(rest(&mut copy, &mut table), "cba");
        assert_eq!(rest(&mut walk, &mut table), "cba");
    }
}
