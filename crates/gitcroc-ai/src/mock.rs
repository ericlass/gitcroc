//! A scripted provider for the tests and the `e2e` build: no model, no
//! network, no timing.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use tokio::sync::Semaphore;

use crate::provider::{Provider, ProviderError, ProviderFuture, ProviderReply, ProviderRequest};

/// The two sides of a held mock: requests arriving, and the test letting
/// them answer.
#[derive(Debug)]
struct Gate {
    arrived: Semaphore,
    released: Semaphore,
}

/// Lets a test see a held request arrive and then let it answer, so
/// cancellation tests need no timing.
#[derive(Debug, Clone)]
pub struct MockGate(Arc<Gate>);

impl MockGate {
    /// Waits until the next request has reached the mock.
    pub async fn arrived(&self) {
        if let Ok(permit) = self.0.arrived.acquire().await {
            permit.forget();
        }
    }

    /// Lets one waiting or future request answer.
    pub fn release(&self) {
        self.0.released.add_permits(1);
    }
}

/// Answers from a queue of scripted replies and records every request it
/// got. With no scripted reply left, a request fails with a transport error.
#[derive(Debug, Default)]
pub struct MockProvider {
    replies: Mutex<VecDeque<Result<ProviderReply, ProviderError>>>,
    requests: Mutex<Vec<ProviderRequest>>,
    gate: Option<MockGate>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl MockProvider {
    /// A mock that answers `replies` in order, each as soon as asked.
    pub fn new(replies: impl IntoIterator<Item = Result<ProviderReply, ProviderError>>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            ..Self::default()
        }
    }

    /// A mock that answers the texts `replies` in order, with no usage.
    pub fn texts<S: Into<String>>(replies: impl IntoIterator<Item = S>) -> Self {
        Self::new(replies.into_iter().map(|text| {
            Ok(ProviderReply {
                text: text.into(),
                usage: None,
            })
        }))
    }

    /// The mock, holding every request until the gate releases it.
    pub fn held(mut self) -> (Self, MockGate) {
        let gate = MockGate(Arc::new(Gate {
            arrived: Semaphore::new(0),
            released: Semaphore::new(0),
        }));
        self.gate = Some(gate.clone());
        (self, gate)
    }

    /// Every request the mock got, in order.
    pub fn requests(&self) -> Vec<ProviderRequest> {
        lock(&self.requests).clone()
    }
}

impl Provider for MockProvider {
    fn complete<'a>(&'a self, request: &'a ProviderRequest) -> ProviderFuture<'a> {
        Box::pin(async move {
            lock(&self.requests).push(request.clone());
            if let Some(MockGate(gate)) = &self.gate {
                gate.arrived.add_permits(1);
                match gate.released.acquire().await {
                    Ok(permit) => permit.forget(),
                    Err(_) => return Err(ProviderError::Cancelled),
                }
            }
            lock(&self.replies).pop_front().unwrap_or_else(|| {
                Err(ProviderError::Transport(
                    "the mock has no scripted reply left".to_owned(),
                ))
            })
        })
    }
}
