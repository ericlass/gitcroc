//! The ask loop: one retry with the validation error, then "AI failed"
//! (AI-9).

use gitcroc_ai::{
    FileOutcome, Message, MockProvider, ProviderError, ProviderReply, ProviderRequest, Role, Usage,
    ask,
};

use crate::responses::{HUNK_IDS, MARKER_IN_RESOLUTION, MISSING_HUNK, PROSE_BEFORE, VALID};

pub fn request() -> ProviderRequest {
    ProviderRequest {
        system: "Resolve the conflict hunks.".to_owned(),
        user: "file: src/net.rs".to_owned(),
        follow_up: Vec::new(),
        temperature: Some(0.0),
    }
}

fn reply(text: &str, input_tokens: u32) -> Result<ProviderReply, ProviderError> {
    Ok(ProviderReply {
        text: text.to_owned(),
        usage: Some(Usage {
            input_tokens,
            output_tokens: 100,
        }),
    })
}

#[tokio::test]
async fn ai_9_a_valid_reply_is_answered_at_once() {
    let mock = MockProvider::new([reply(VALID, 1000)]);
    let result = ask(&mock, request(), &HUNK_IDS).await;
    let FileOutcome::Answered(answer) = result.outcome else {
        panic!("{:?}", result.outcome);
    };
    assert_eq!(answer.hunks.len(), 2);
    assert_eq!(mock.requests(), [request()]);
    assert_eq!(
        result.usage,
        [Some(Usage {
            input_tokens: 1000,
            output_tokens: 100
        })]
    );
}

#[tokio::test]
async fn ai_9_invalid_reply_is_retried_once_with_the_error() {
    let mock = MockProvider::new([reply(MISSING_HUNK, 1000), reply(VALID, 1200)]);
    let result = ask(&mock, request(), &HUNK_IDS).await;
    assert!(
        matches!(result.outcome, FileOutcome::Answered(_)),
        "{:?}",
        result.outcome
    );

    let requests = mock.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0], request());
    let retry = &requests[1];
    assert_eq!(
        (&retry.system, &retry.user, retry.temperature),
        (&request().system, &request().user, Some(0.0))
    );
    let [answer, correction] = &retry.follow_up[..] else {
        panic!("{:?}", retry.follow_up);
    };
    assert_eq!(
        answer,
        &Message {
            role: Role::Assistant,
            text: MISSING_HUNK.to_owned()
        }
    );
    assert_eq!(correction.role, Role::User);
    assert!(
        correction.text.contains("hunk 2 is missing"),
        "{}",
        correction.text
    );
    assert!(
        correction.text.contains("corrected JSON object only"),
        "{}",
        correction.text
    );

    // Each request's usage is kept (AI-14).
    let input: Vec<_> = result
        .usage
        .iter()
        .map(|usage| usage.map(|usage| usage.input_tokens))
        .collect();
    assert_eq!(input, [Some(1000), Some(1200)]);
}

#[tokio::test]
async fn ai_9_second_invalid_reply_marks_the_file_ai_failed() {
    let mock = MockProvider::texts([PROSE_BEFORE, MARKER_IN_RESOLUTION, VALID]);
    let result = ask(&mock, request(), &HUNK_IDS).await;
    let FileOutcome::AiFailed { reason } = result.outcome else {
        panic!("{:?}", result.outcome);
    };
    assert!(
        reason.contains("the resolution of hunk 2 has a conflict marker line"),
        "{reason}"
    );
    // One retry only: the third reply is never asked for.
    assert_eq!(mock.requests().len(), 2);
    assert_eq!(result.usage, [None, None]);
}

#[tokio::test]
async fn ai_9_provider_errors_are_not_retried() {
    let errors = [
        ProviderError::Transport("connection refused".to_owned()),
        ProviderError::Status {
            status: 429,
            message: "rate limit exceeded".to_owned(),
        },
        ProviderError::Exit {
            program: "claude".to_owned(),
            code: Some(1),
            stderr: "not logged in".to_owned(),
        },
        ProviderError::Cancelled,
    ];
    for error in errors {
        let mock = MockProvider::new([Err(error.clone()), reply(VALID, 1000)]);
        let result = ask(&mock, request(), &HUNK_IDS).await;
        assert_eq!(
            result.outcome,
            FileOutcome::AiFailed {
                reason: error.to_string()
            }
        );
        assert_eq!(mock.requests().len(), 1, "{error}");
        assert_eq!(result.usage, [None]);
    }
    // The error after a retry fails the file as well.
    let mock = MockProvider::new([reply(MISSING_HUNK, 1000), Err(ProviderError::Cancelled)]);
    let result = ask(&mock, request(), &HUNK_IDS).await;
    assert_eq!(
        result.outcome,
        FileOutcome::AiFailed {
            reason: "cancelled".to_owned()
        }
    );
    assert_eq!(mock.requests().len(), 2);
}

#[test]
fn ai_9_provider_errors_say_what_the_provider_said() {
    let status = ProviderError::Status {
        status: 401,
        message: "invalid x-api-key".to_owned(),
    };
    assert_eq!(
        status.to_string(),
        "the provider answered with HTTP 401: invalid x-api-key"
    );
    let killed = ProviderError::Exit {
        program: "codex".to_owned(),
        code: None,
        stderr: String::new(),
    };
    assert_eq!(killed.to_string(), "codex exited with a signal: ");
}
