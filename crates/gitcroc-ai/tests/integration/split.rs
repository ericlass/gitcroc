//! Splitting a file over the context budget (AI-10).

use gitcroc_ai::{
    Confidence, FileOutcome, FileRequest, HunkAnswer, HunkRequest, HunkStatus, MockProvider,
    Operation, ProviderError, estimate_tokens, prompt, resolve_file,
};

fn hunk(id: u32, size: usize) -> HunkRequest {
    HunkRequest {
        id,
        before: "fn before() {}\n".to_owned(),
        ours: format!("{}\n", "o".repeat(size)),
        base: "base\n".to_owned(),
        theirs: format!("{}\n", "t".repeat(size)),
        after: "fn after() {}\n".to_owned(),
    }
}

fn file(hunks: Vec<HunkRequest>) -> FileRequest {
    FileRequest {
        path: "src/net.rs".to_owned(),
        operation: Operation::Merge,
        ours_label: "main".to_owned(),
        theirs_label: "feature/retry".to_owned(),
        hunks,
        ours_commits: vec!["Tidy the client".to_owned()],
        theirs_commits: vec!["Retry on timeouts".to_owned()],
    }
}

/// The estimated tokens of a file's whole prompt with only `hunks`: system
/// prompt and user message counted together, as the plan counts them.
fn tokens(hunks: Vec<HunkRequest>) -> u64 {
    let request = prompt(&file(hunks));
    estimate_tokens(&format!("{}{}", request.system, request.user))
}

/// A valid reply resolving `ids`.
fn resolved(ids: &[u32]) -> String {
    let hunks: Vec<String> = ids
        .iter()
        .map(|id| {
            format!(
                r#"{{"id": {id}, "status": "resolved", "resolution": "merged {id}\n", "explanation": "Both.", "confidence": "high"}}"#
            )
        })
        .collect();
    format!(r#"{{"hunks": [{}]}}"#, hunks.join(", "))
}

/// The ids of the hunks a request was sent for, read from its fences.
fn sent_ids(user: &str) -> Vec<u32> {
    user.lines()
        .filter_map(|line| line.strip_prefix("Hunk "))
        .filter_map(|rest| rest.strip_suffix(':'))
        .filter_map(|id| id.parse().ok())
        .collect()
}

#[test]
fn ai_10_tokens_estimated_as_chars_over_3() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("abc"), 1);
    assert_eq!(estimate_tokens("abcd"), 2, "rounded up");
    assert_eq!(estimate_tokens("abcdef"), 2);
    // Characters, not bytes: three two-byte and three four-byte characters.
    assert_eq!(estimate_tokens("ééé"), 1);
    assert_eq!(estimate_tokens("🐊🐊🐊🐊"), 2);
}

#[tokio::test]
async fn ai_10_whole_prompt_at_the_budget_is_one_request() {
    let hunks = vec![hunk(1, 2000), hunk(2, 2000)];
    let budget = u32::try_from(tokens(hunks.clone())).unwrap();
    let mock = MockProvider::texts([resolved(&[1, 2])]);
    let result = resolve_file(&mock, &file(hunks.clone()), budget).await;
    assert!(
        matches!(result.outcome, FileOutcome::Answered(_)),
        "{:?}",
        result.outcome
    );
    assert_eq!(mock.requests().len(), 1);

    // One token less and the same file needs two requests.
    let mock = MockProvider::texts([resolved(&[1]), resolved(&[2])]);
    let result = resolve_file(&mock, &file(hunks), budget - 1).await;
    assert!(
        matches!(result.outcome, FileOutcome::Answered(_)),
        "{:?}",
        result.outcome
    );
    assert_eq!(mock.requests().len(), 2);
}

#[tokio::test]
async fn ai_10_hunks_split_across_requests_in_order() {
    // Two hunks of this size fit a request, three do not.
    let budget = u32::try_from(tokens(vec![hunk(1, 3000), hunk(2, 3000)])).unwrap();
    let hunks: Vec<HunkRequest> = (1..=5).map(|id| hunk(id, 3000)).collect();
    let mock = MockProvider::texts([resolved(&[1, 2]), resolved(&[3, 4]), resolved(&[5])]);
    let result = resolve_file(&mock, &file(hunks), budget).await;

    let requests = mock.requests();
    let sent: Vec<Vec<u32>> = requests
        .iter()
        .map(|request| sent_ids(&request.user))
        .collect();
    assert_eq!(sent, [vec![1, 2], vec![3, 4], vec![5]]);
    for request in &requests {
        assert!(
            estimate_tokens(&request.system) + estimate_tokens(&request.user) <= u64::from(budget)
        );
        // Each part repeats the file's path, labels and commit messages.
        for text in [
            "src/net.rs",
            "main",
            "feature/retry",
            "Tidy the client",
            "Retry on timeouts",
        ] {
            assert!(
                request.user.contains(text),
                "{text} missing from {}",
                request.user
            );
        }
        assert!(request.user.contains("Operation: merge"));
    }
    // Each part has its own token.
    let first_lines: Vec<&str> = requests
        .iter()
        .filter_map(|request| {
            request
                .user
                .lines()
                .find(|line| line.starts_with("<<<DATA "))
        })
        .collect();
    assert_eq!(first_lines.len(), 3);
    assert_ne!(first_lines[0], first_lines[1]);

    let FileOutcome::Answered(answer) = result.outcome else {
        panic!("{:?}", result.outcome);
    };
    let ids: Vec<u32> = answer.hunks.iter().map(|hunk| hunk.id).collect();
    assert_eq!(ids, [1, 2, 3, 4, 5]);
    assert_eq!(result.usage.len(), 3);
}

#[tokio::test]
async fn ai_10_a_single_hunk_over_budget_is_unresolved_too_large() {
    let budget = u32::try_from(tokens(vec![hunk(1, 3000)])).unwrap();
    let hunks = vec![hunk(1, 3000), hunk(2, 9000), hunk(3, 3000)];
    let mock = MockProvider::texts([resolved(&[1]), resolved(&[3])]);
    let result = resolve_file(&mock, &file(hunks), budget).await;

    let sent: Vec<Vec<u32>> = mock
        .requests()
        .iter()
        .map(|request| sent_ids(&request.user))
        .collect();
    assert_eq!(sent, [vec![1], vec![3]], "hunk 2 is never sent");
    let FileOutcome::Answered(answer) = result.outcome else {
        panic!("{:?}", result.outcome);
    };
    let ids: Vec<u32> = answer.hunks.iter().map(|hunk| hunk.id).collect();
    assert_eq!(ids, [1, 2, 3]);
    assert_eq!(
        answer.hunks[1],
        HunkAnswer {
            id: 2,
            status: HunkStatus::Unresolved,
            resolution: None,
            explanation: "too large".to_owned(),
            confidence: Confidence::Low,
        }
    );
    assert_eq!(answer.hunks[0].status, HunkStatus::Resolved);
    assert_eq!(answer.hunks[2].status, HunkStatus::Resolved);
}

#[tokio::test]
async fn ai_10_every_hunk_too_large_sends_nothing() {
    let budget = u32::try_from(tokens(vec![hunk(1, 10)])).unwrap();
    let mock = MockProvider::texts(Vec::<String>::new());
    let result = resolve_file(&mock, &file(vec![hunk(1, 3000), hunk(2, 3000)]), budget).await;
    assert!(mock.requests().is_empty());
    assert!(result.usage.is_empty());
    let FileOutcome::Answered(answer) = result.outcome else {
        panic!("{:?}", result.outcome);
    };
    assert!(
        answer
            .hunks
            .iter()
            .all(|hunk| hunk.explanation == "too large")
    );
    assert_eq!(answer.hunks.len(), 2);
}

#[tokio::test]
async fn ai_10_one_failed_part_fails_the_file() {
    let budget = u32::try_from(tokens(vec![hunk(1, 3000)])).unwrap();
    let hunks: Vec<HunkRequest> = (1..=3).map(|id| hunk(id, 3000)).collect();
    let mock = MockProvider::new([
        Ok(gitcroc_ai::ProviderReply {
            text: resolved(&[1]),
            usage: None,
        }),
        Err(ProviderError::Status {
            status: 529,
            message: "overloaded".to_owned(),
        }),
        Ok(gitcroc_ai::ProviderReply {
            text: resolved(&[3]),
            usage: None,
        }),
    ]);
    let result = resolve_file(&mock, &file(hunks), budget).await;

    assert_eq!(
        result.outcome,
        FileOutcome::AiFailed {
            reason: "the provider answered with HTTP 529: overloaded".to_owned()
        },
        "no partial answer is kept"
    );
    // The usage of the requests that were sent is still reported (AI-14),
    // and no request follows the failed part.
    assert_eq!(result.usage.len(), 2);
    assert_eq!(mock.requests().len(), 2);
}
