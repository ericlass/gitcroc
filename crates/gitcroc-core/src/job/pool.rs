//! The one pool of workers that jobs and the background refresh share
//! (PERF-4): as many places as the setting `group_parallelism`,
//! [`POOL_SIZE`] by default. A job waiting for a place gets the next one free
//! before any refresh read waiting; refresh reads keep their own order and
//! their lowest thread priority.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::oneshot;

use crate::lock;

/// Workers in the pool until the settings say otherwise.
pub const POOL_SIZE: usize = 8;

/// Who asks for a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    /// A job the user started: served first.
    Job,
    /// A background refresh read.
    Refresh,
}

#[derive(Debug, Default)]
struct State {
    size: usize,
    /// Places in use, which is more than `size` for a while after the pool
    /// shrank.
    held: usize,
    jobs: VecDeque<oneshot::Sender<()>>,
    refreshes: VecDeque<oneshot::Sender<()>>,
}

/// The pool. Cheap to share as `Arc<Pool>`.
#[derive(Debug)]
pub struct Pool {
    state: Mutex<State>,
}

impl Pool {
    pub fn new(size: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                size: size.max(1),
                ..State::default()
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // Each change is a counter step or a queue push or pop.
        lock(&self.state)
    }

    /// Waits for a worker: at once when one is free, else when one is
    /// given back and no request of a higher priority, or an earlier one of
    /// the same, waits. The place is held until the [`Worker`] is dropped.
    pub async fn worker(self: &Arc<Self>, priority: Priority) -> Worker {
        let receiver = {
            let mut state = self.lock();
            if state.held < state.size {
                state.held += 1;
                return Worker { pool: self.clone() };
            }
            let (sender, receiver) = oneshot::channel();
            match priority {
                Priority::Job => state.jobs.push_back(sender),
                Priority::Refresh => state.refreshes.push_back(sender),
            }
            receiver
        };
        let mut waiting = Waiting {
            pool: self.clone(),
            receiver: Some(receiver),
        };
        if let Some(receiver) = waiting.receiver.as_mut() {
            // The sender is only dropped after a send or with the pool,
            // which this holds.
            let _ = receiver.await;
        }
        waiting.receiver = None;
        Worker { pool: self.clone() }
    }

    /// How many requests of each priority wait: (jobs, refreshes).
    pub fn waiting(&self) -> (usize, usize) {
        let state = self.lock();
        (state.jobs.len(), state.refreshes.len())
    }

    /// How many places it has.
    pub fn size(&self) -> usize {
        self.lock().size
    }

    /// Gives the pool `size` places, at least one. Growing, it hands the new
    /// places to requests waiting; shrinking, it takes places back as they
    /// are given back, and nothing running stops.
    pub fn resize(&self, size: usize) {
        let mut state = self.lock();
        state.size = size.max(1);
        hand_out(&mut state);
    }

    /// Gives a place back and hands free places to the requests waiting.
    fn give_back(&self) {
        let mut state = self.lock();
        state.held = state.held.saturating_sub(1);
        hand_out(&mut state);
    }
}

/// Hands each free place to the first request waiting, jobs first. A
/// request given up meanwhile is passed over.
fn hand_out(state: &mut State) {
    while state.held < state.size {
        let Some(next) = state
            .jobs
            .pop_front()
            .or_else(|| state.refreshes.pop_front())
        else {
            return;
        };
        if next.send(()).is_ok() {
            state.held += 1;
        }
    }
}

/// A place in the pool, given back when dropped.
#[derive(Debug)]
pub struct Worker {
    pool: Arc<Pool>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.pool.give_back();
    }
}

/// A request waiting for a place. Dropped before it got one, as when its
/// job is cancelled, it gives back a place handed to it in between.
struct Waiting {
    pool: Arc<Pool>,
    receiver: Option<oneshot::Receiver<()>>,
}

impl Drop for Waiting {
    fn drop(&mut self) {
        if let Some(mut receiver) = self.receiver.take() {
            receiver.close();
            if receiver.try_recv().is_ok() {
                self.pool.give_back();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    use super::*;

    fn poll<F: Future>(future: std::pin::Pin<&mut F>) -> Poll<F::Output> {
        future.poll(&mut Context::from_waker(Waker::noop()))
    }

    #[test]
    fn a_request_given_up_keeps_no_place() {
        let pool = Pool::new(1);
        let held = poll(pin!(pool.worker(Priority::Job)));
        let Poll::Ready(held) = held else {
            panic!("the first worker waits")
        };
        {
            let mut waiting = pin!(pool.worker(Priority::Job));
            assert!(poll(waiting.as_mut()).is_pending());
            // Handed the place, then dropped before it took it.
            drop(held);
        }
        assert_eq!(pool.waiting(), (0, 0));
        assert!(poll(pin!(pool.worker(Priority::Refresh))).is_ready());
    }

    #[test]
    fn a_resized_pool_hands_out_new_places_and_takes_back_old_ones() {
        let pool = Pool::new(1);
        let Poll::Ready(first) = poll(pin!(pool.worker(Priority::Job))) else {
            panic!("the first worker waits")
        };
        let mut second = pin!(pool.worker(Priority::Job));
        assert!(poll(second.as_mut()).is_pending());
        pool.resize(2);
        let Poll::Ready(second) = poll(second) else {
            panic!("the new place is not handed out")
        };
        pool.resize(1);
        drop(first);
        let mut third = pin!(pool.worker(Priority::Job));
        assert!(
            poll(third.as_mut()).is_pending(),
            "two places are still held"
        );
        drop(second);
        assert!(poll(third).is_ready());
    }
}
