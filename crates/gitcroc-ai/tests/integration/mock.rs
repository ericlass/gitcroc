//! The mock provider holds a request until the test releases it, so a test
//! can cancel a request in flight without timing.

use std::sync::Arc;

use gitcroc_ai::{FileOutcome, MockProvider, ask};

use crate::ask::request;
use crate::responses::{HUNK_IDS, VALID};

#[tokio::test]
async fn mock_holds_a_request_until_released() {
    let (mock, gate) = MockProvider::texts([VALID]).held();
    let mock = Arc::new(mock);
    let asking = tokio::spawn({
        let mock = Arc::clone(&mock);
        async move { ask(mock.as_ref(), request(), &HUNK_IDS).await }
    });
    gate.arrived().await;
    assert_eq!(mock.requests().len(), 1);
    assert!(!asking.is_finished());
    gate.release();
    let result = asking.await.unwrap();
    assert!(
        matches!(result.outcome, FileOutcome::Answered(_)),
        "{:?}",
        result.outcome
    );
}

#[tokio::test]
async fn mock_request_in_flight_can_be_dropped() {
    let (mock, gate) = MockProvider::texts([VALID, VALID]).held();
    let mock = Arc::new(mock);
    let asking = tokio::spawn({
        let mock = Arc::clone(&mock);
        async move { ask(mock.as_ref(), request(), &HUNK_IDS).await }
    });
    gate.arrived().await;
    asking.abort();
    assert!(asking.await.unwrap_err().is_cancelled());

    // The next request still waits for its own release.
    gate.release();
    let result = ask(mock.as_ref(), request(), &HUNK_IDS).await;
    assert!(
        matches!(result.outcome, FileOutcome::Answered(_)),
        "{:?}",
        result.outcome
    );
    assert_eq!(mock.requests().len(), 2);
}
