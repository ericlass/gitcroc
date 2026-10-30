//! Decorations disappear when emptied. Keep bounded, rope-sharing snapshots
//! alongside the editor's own undo history; comparison runs off the UI thread.
use gpui_kit::base::input::Rope;
use std::ops::Range;
#[derive(Clone)]
pub struct Snapshot {
    pub pinned: bool,
    pub text: Rope,
    pub untouched: Vec<bool>,
    pub unsaved: bool,
    pub hunks: Vec<(Range<usize>, bool)>,
}
pub struct Recovery {
    initial: Snapshot,
    states: Vec<Snapshot>,
    at: usize,
}
impl Recovery {
    pub fn new(mut initial: Snapshot) -> Self {
        initial.pinned = true;
        Self {
            initial: initial.clone(),
            states: vec![initial],
            at: 0,
        }
    }
    pub fn record(&mut self, mut current: Snapshot, takes: &[usize]) -> Snapshot {
        if takes.is_empty() {
            // Undo can skip coalesced typing snapshots. Redo searches forward.
            let found = (0..self.at)
                .rev()
                .chain(self.at + 1..self.states.len())
                .find(|i| self.states[*i].text == current.text);
            if let Some(at) = found {
                self.at = at;
                return self.states[at].clone();
            }
            let mut source = self.at;
            let mut delta = Delta::between(&self.states[source].text, &current.text);
            // Change events may be delivered together: Undo and immediate typing
            // can arrive without the intermediate restored text. Recover from the
            // closest retained boundary, then map its byte positions through typing.
            if self.states[source]
                .hunks
                .iter()
                .any(|(r, taken)| *taken || r.is_empty())
            {
                for (at, state) in self.states.iter().enumerate().filter(|(_, s)| s.pinned) {
                    let candidate = Delta::between(&state.text, &current.text);
                    if candidate.cost() < delta.cost() {
                        source = at;
                        delta = candidate;
                    }
                }
            }
            for (id, (hunk, old)) in current
                .hunks
                .iter_mut()
                .zip(&self.states[source].hunks)
                .enumerate()
            {
                hunk.1 = old.1;
                if hunk.0.is_empty() || self.states[self.at].hunks[id].1 {
                    hunk.0 = delta.map(&old.0);
                }
            }
            self.at = source;
        } else {
            for (id, (hunk, old)) in current
                .hunks
                .iter_mut()
                .zip(&self.states[self.at].hunks)
                .enumerate()
            {
                hunk.1 = old.1 || takes.contains(&id);
            }
        }
        let erased = current
            .hunks
            .iter()
            .zip(&self.states[self.at].hunks)
            .any(|((range, _), (old, _))| range.is_empty() != old.is_empty());
        if !takes.is_empty() || erased {
            self.states[self.at].pinned = true;
            current.pinned = true;
        }
        current.unsaved = current.text != self.initial.text;
        current.untouched = current
            .hunks
            .iter()
            .zip(&self.initial.hunks)
            .map(|((range, taken), (original, _))| {
                !taken
                    && current.text.slice(range.clone())
                        == self.initial.text.slice(original.clone())
            })
            .collect();
        self.states.truncate(self.at + 1);
        self.states.push(current.clone());
        // Bound recovery memory separately from the editor's undo history.
        if self.states.len() > 1001 {
            // Do not evict a Take or erased-anchor boundary for coalesced typing.
            let disposable = self.states.iter().position(|s| !s.pinned);
            if let Some(at) = disposable {
                self.states.remove(at);
            } else if self.states.len() > 2001 {
                self.states.remove(0);
            }
        }
        self.at = self.states.len() - 1;
        current
    }
}

/// One contiguous edit, used only when the engine lost an empty anchor.
struct Delta {
    start: usize,
    end: usize,
    inserted: usize,
}
impl Delta {
    fn between(old: &Rope, new: &Rope) -> Self {
        let start = old
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>();
        let suffix = old
            .chars_at(old.len())
            .reversed()
            .zip(new.chars_at(new.len()).reversed())
            .take_while(|(a, b)| a == b)
            .map(|(c, _)| c.len_utf8())
            .sum::<usize>()
            .min(old.len().min(new.len()) - start);
        Self {
            start,
            end: old.len() - suffix,
            inserted: new.len() - start - suffix,
        }
    }
    fn cost(&self) -> usize {
        self.end - self.start + self.inserted
    }
    fn map(&self, range: &Range<usize>) -> Range<usize> {
        let shift = |at: usize| at + self.inserted - (self.end - self.start);
        let start = if range.start >= self.end {
            shift(range.start)
        } else {
            range.start.min(self.start)
        };
        let end = if range.end <= self.start {
            range.end
        } else if range.end >= self.end {
            shift(range.end)
        } else {
            self.start + self.inserted
        };
        start..end.max(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn state(text: &str, range: Range<usize>, taken: bool) -> Snapshot {
        Snapshot {
            pinned: false,
            text: Rope::from_str(text),
            hunks: vec![(range, taken)],
            untouched: vec![!taken],
            unsaved: false,
        }
    }
    #[test]
    fn ops_3_recovery_keeps_atomic_snapshots_across_coalesced_typing() {
        let mut history = Recovery::new(state("marker", 0..6, false));
        history.record(state("pre marker", 4..10, false), &[]);
        history.record(state("pre ", 4..4, true), &[0]);
        for n in 1..1100 {
            history.record(state(&format!("pre {}", "x".repeat(n)), 4..4, true), &[]);
        }
        assert!(history.record(state("pre ", 4..4, true), &[]).hunks[0].1);
        let restored = history.record(state("pre marker", 4..4, true), &[]);
        assert!(!restored.hunks[0].1);
        assert_eq!(restored.hunks[0].0, 4..10);
    }
}
