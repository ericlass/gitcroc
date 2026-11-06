//! Graph lanes (UI-9), computed in the core (ARCH-6) one row at a time, so a
//! page continues the graph of the page before it (PERF-2).
//!
//! A lane is a vertical line waiting for one commit further down. Each row
//! places its commit on the leftmost lane waiting for it (or on a new lane),
//! ends every other lane waiting for it there, continues the commit's lane to
//! its first parent and opens or joins one lane per further parent. Lanes do
//! not shift sideways; a freed lane is reused by the next line that needs one.

use gix::hash::{ObjectId, oid};
use serde::Serialize;

/// The graph cell of one history row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GraphRow {
    /// The lane the commit sits on.
    pub lane: u32,
    /// The color index of the commit's line.
    pub color: u32,
    /// Lines in the upper half of the row, from the top edge (`from`) to the
    /// middle (`to`): lines ending at the commit, and lines passing it.
    pub up: Vec<Edge>,
    /// Lines in the lower half, from the middle (`from`) to the bottom edge
    /// (`to`): lines leaving the commit for its parents, and lines passing it.
    pub down: Vec<Edge>,
}

/// One line segment within half a row. A line keeps its color index from
/// the row where it starts to the row where it ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Edge {
    pub from: u32,
    pub to: u32,
    pub color: u32,
}

#[derive(Debug, Clone)]
struct Line {
    waiting_for: ObjectId,
    color: u32,
}

/// The open lanes between two rows.
#[derive(Debug, Clone, Default)]
pub(crate) struct Lanes {
    lines: Vec<Option<Line>>,
    next_color: u32,
}

impl Lanes {
    /// Lays out the next row: `id` with its `parents` in order.
    pub(crate) fn row(&mut self, id: &oid, parents: &[ObjectId]) -> GraphRow {
        let existing = self
            .lines
            .iter()
            .position(|line| line.as_ref().is_some_and(|l| l.waiting_for == id));
        let mut row = GraphRow {
            lane: 0,
            color: 0,
            up: Vec::new(),
            down: Vec::new(),
        };
        let (lane, color) = match existing {
            Some(lane) => (lane, self.lines[lane].as_ref().map_or(0, |l| l.color)),
            None => (self.free_lane(), self.new_color()),
        };

        // Upper half: lines into the commit end there; the others pass.
        let mut passing = Vec::new();
        for (index, slot) in self.lines.iter_mut().enumerate() {
            let Some(line) = slot else { continue };
            let index = lane_number(index);
            if line.waiting_for == id {
                row.up.push(edge(index, lane_number(lane), line.color));
                *slot = None;
            } else {
                row.up.push(edge(index, index, line.color));
                passing.push(index);
            }
        }

        // Lower half: the commit's lane continues to its first parent, and
        // every further parent gets a lane of its own or joins the lane
        // already waiting for it.
        if let Some((first, others)) = parents.split_first() {
            self.set(lane, *first, color);
            row.down
                .push(edge(lane_number(lane), lane_number(lane), color));
            for parent in others {
                let target = match self.waiting_for(parent) {
                    Some(target) => target,
                    None => {
                        let target = self.free_lane();
                        let color = self.new_color();
                        self.set(target, *parent, color);
                        target
                    }
                };
                let color = self.lines[target].as_ref().map_or(color, |l| l.color);
                row.down
                    .push(edge(lane_number(lane), lane_number(target), color));
            }
        }
        for index in passing {
            if let Some(line) = &self.lines[index as usize] {
                row.down.push(edge(index, index, line.color));
            }
        }
        row.down.sort_by_key(|edge| (edge.from, edge.to));

        while self.lines.last().is_some_and(Option::is_none) {
            self.lines.pop();
        }
        row.lane = lane_number(lane);
        row.color = color;
        row
    }

    fn waiting_for(&self, id: &oid) -> Option<usize> {
        self.lines
            .iter()
            .position(|line| line.as_ref().is_some_and(|l| l.waiting_for == id))
    }

    fn free_lane(&mut self) -> usize {
        match self.lines.iter().position(Option::is_none) {
            Some(index) => index,
            None => {
                self.lines.push(None);
                self.lines.len() - 1
            }
        }
    }

    fn set(&mut self, lane: usize, waiting_for: ObjectId, color: u32) {
        if lane >= self.lines.len() {
            self.lines.resize(lane + 1, None);
        }
        self.lines[lane] = Some(Line { waiting_for, color });
    }

    fn new_color(&mut self) -> u32 {
        let color = self.next_color;
        self.next_color = self.next_color.wrapping_add(1);
        color
    }
}

fn edge(from: u32, to: u32, color: u32) -> Edge {
    Edge { from, to, color }
}

/// Lane indices fit a `u32`: there are never more lanes than commits on a
/// page plus the lanes open before it.
fn lane_number(index: usize) -> u32 {
    u32::try_from(index).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A commit id from a one-letter name.
    fn id(name: char) -> ObjectId {
        let mut bytes = [0u8; 20];
        bytes[0] = name as u8;
        ObjectId::from_bytes_or_panic(&bytes)
    }

    /// Lays out `rows` of (commit, parents) and returns, per row, the lane,
    /// the color, and the edges as `from-to:color` strings.
    fn layout(rows: &[(char, &str)]) -> Vec<(u32, u32, String, String)> {
        let mut lanes = Lanes::default();
        let text = |edges: &[Edge]| {
            edges
                .iter()
                .map(|e| format!("{}-{}:{}", e.from, e.to, e.color))
                .collect::<Vec<_>>()
                .join(" ")
        };
        rows.iter()
            .map(|(commit, parents)| {
                let parents: Vec<ObjectId> = parents.chars().map(id).collect();
                let row = lanes.row(&id(*commit), &parents);
                (row.lane, row.color, text(&row.up), text(&row.down))
            })
            .collect()
    }

    fn expect(rows: &[(u32, u32, &str, &str)]) -> Vec<(u32, u32, String, String)> {
        rows.iter()
            .map(|(lane, color, up, down)| (*lane, *color, (*up).into(), (*down).into()))
            .collect()
    }

    #[test]
    fn ui_9_lanes_linear_history() {
        assert_eq!(
            layout(&[('c', "b"), ('b', "a"), ('a', "")]),
            expect(&[
                (0, 0, "", "0-0:0"),
                (0, 0, "0-0:0", "0-0:0"),
                (0, 0, "0-0:0", ""),
            ])
        );
    }

    #[test]
    fn ui_9_lanes_one_fork_and_merge() {
        // m merges f into main; f and b both come from a.
        assert_eq!(
            layout(&[('m', "bf"), ('f', "a"), ('b', "a"), ('a', "")]),
            expect(&[
                (0, 0, "", "0-0:0 0-1:1"),
                (1, 1, "0-0:0 1-1:1", "0-0:0 1-1:1"),
                (0, 0, "0-0:0 1-1:1", "0-0:0 1-1:1"),
                (0, 0, "0-0:0 1-0:1", ""),
            ])
        );
    }

    #[test]
    fn ui_9_lanes_octopus_merge() {
        assert_eq!(
            layout(&[('o', "abc"), ('c', "r"), ('b', "r"), ('a', "r"), ('r', "")]),
            expect(&[
                (0, 0, "", "0-0:0 0-1:1 0-2:2"),
                (2, 2, "0-0:0 1-1:1 2-2:2", "0-0:0 1-1:1 2-2:2"),
                (1, 1, "0-0:0 1-1:1 2-2:2", "0-0:0 1-1:1 2-2:2"),
                (0, 0, "0-0:0 1-1:1 2-2:2", "0-0:0 1-1:1 2-2:2"),
                (0, 0, "0-0:0 1-0:1 2-0:2", ""),
            ])
        );
    }

    #[test]
    fn ui_9_lanes_two_unrelated_roots() {
        // Two histories with no common commit, merged at the top.
        assert_eq!(
            layout(&[('m', "ax"), ('x', ""), ('a', "")]),
            expect(&[
                (0, 0, "", "0-0:0 0-1:1"),
                (1, 1, "0-0:0 1-1:1", "0-0:0"),
                (0, 0, "0-0:0", ""),
            ])
        );
    }

    #[test]
    fn ui_9_lanes_a_branch_that_ends_frees_its_lane() {
        // Branch tip t (no child here) ends at fork point a; the next new
        // line, from tip u, reuses the freed lane 1 with a new color.
        assert_eq!(
            layout(&[('b', "a"), ('t', "a"), ('a', "z"), ('u', "z"), ('z', "")]),
            expect(&[
                (0, 0, "", "0-0:0"),
                (1, 1, "0-0:0", "0-0:0 1-1:1"),
                (0, 0, "0-0:0 1-0:1", "0-0:0"),
                (1, 2, "0-0:0", "0-0:0 1-1:2"),
                (0, 0, "0-0:0 1-0:2", ""),
            ])
        );
    }

    #[test]
    fn ui_9_merge_parent_already_on_a_lane_joins_it() {
        // m merges a, which lane 0 (from b) already waits for.
        assert_eq!(
            layout(&[('b', "a"), ('m', "xa"), ('x', "a"), ('a', "")]),
            expect(&[
                (0, 0, "", "0-0:0"),
                (1, 1, "0-0:0", "0-0:0 1-0:0 1-1:1"),
                (1, 1, "0-0:0 1-1:1", "0-0:0 1-1:1"),
                (0, 0, "0-0:0 1-0:1", ""),
            ])
        );
    }
}
