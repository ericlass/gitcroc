//! The API providers (D4) against the fake server: the requests they send,
//! the replies, usage and model lists they read, and their errors.

use std::sync::Arc;
use std::time::Duration;

use gitcroc_ai::{
    AnthropicProvider, ApiKey, Message, ModelList, OpenAiProvider, Provider, ProviderError,
    ProviderReply, ProviderRequest, REQUEST_TIMEOUT, Role, Usage,
};

use crate::fake_server::{FakeServer, Held, Reply, closed_url};

const KEY: &str = "sk-test-0123456789";
// Model names appear in the fake server's tests only (AI-1).
pub(crate) const MODEL: &str = "fake-model-1";

pub(crate) fn request() -> ProviderRequest {
    ProviderRequest {
        system: "You resolve conflicts.".to_owned(),
        user: "Resolve hunk 1.".to_owned(),
        follow_up: Vec::new(),
        temperature: Some(0.0),
    }
}

/// A request with a retry's earlier reply and validation error (AI-9).
pub(crate) fn retry_request() -> ProviderRequest {
    ProviderRequest {
        follow_up: vec![
            Message {
                role: Role::Assistant,
                text: "not json".to_owned(),
            },
            Message {
                role: Role::User,
                text: "Your reply is not valid.".to_owned(),
            },
        ],
        ..request()
    }
}

pub(crate) fn anthropic(server: &FakeServer) -> AnthropicProvider {
    AnthropicProvider::new(ApiKey::new(KEY), MODEL)
        .unwrap()
        .with_base_url(&server.url)
}

pub(crate) fn openai(server: &FakeServer, key: Option<&str>) -> OpenAiProvider {
    OpenAiProvider::new(format!("{}/v1", server.url), key.map(ApiKey::new), MODEL).unwrap()
}

/// Server-sent events as the Messages API streams them, the text split
/// over `parts` deltas.
pub(crate) fn stream(parts: &[&str]) -> String {
    let mut events = vec![
        r#"{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[],"usage":{"input_tokens":120,"output_tokens":1}}}"#.to_owned(),
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#.to_owned(),
        r#"{"type":"ping"}"#.to_owned(),
    ];
    for part in parts {
        let delta = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "text_delta", "text": part},
        });
        events.push(delta.to_string());
    }
    events.push(r#"{"type":"content_block_stop","index":0}"#.to_owned());
    events.push(
        r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#
            .to_owned(),
    );
    events.push(r#"{"type":"message_stop"}"#.to_owned());
    events
        .iter()
        .map(|data| {
            let kind = serde_json::from_str::<serde_json::Value>(data).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_owned();
            format!("event: {kind}\ndata: {data}\n\n")
        })
        .collect()
}

pub(crate) fn completion(content: &str, usage: Option<(u32, u32)>) -> String {
    let mut body = serde_json::json!({
        "id": "chatcmpl-1",
        "object": "chat.completion",
        "choices": [{"index": 0, "message": {"role": "assistant", "content": content}, "finish_reason": "stop"}],
    });
    if let Some((prompt, completion)) = usage {
        body["usage"] = serde_json::json!({
            "prompt_tokens": prompt,
            "completion_tokens": completion,
            "total_tokens": prompt + completion,
        });
    }
    body.to_string()
}

#[tokio::test]
async fn ai_1_anthropic_and_openai_list_models_from_the_provider() {
    // The Messages API lists models in pages.
    let server = FakeServer::start([
        Reply::json(
            200,
            r#"{"data":[{"type":"model","id":"fake-a","display_name":"A"}],"has_more":true,"first_id":"fake-a","last_id":"fake-a"}"#,
        ),
        Reply::json(
            200,
            r#"{"data":[{"type":"model","id":"fake-b","display_name":"B"}],"has_more":false,"first_id":"fake-b","last_id":"fake-b"}"#,
        ),
    ])
    .await;
    let models = anthropic(&server).list_models().await.unwrap();
    assert_eq!(
        models,
        ModelList::Offered(vec!["fake-a".to_owned(), "fake-b".to_owned()])
    );
    let requests = server.requests();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].method, "GET");
    assert!(
        requests[0].path.starts_with("/v1/models"),
        "{}",
        requests[0].path
    );
    assert!(
        requests[1].path.contains("after_id=fake-a"),
        "{}",
        requests[1].path
    );
    assert_eq!(requests[0].header("x-api-key"), Some(KEY));
    assert!(requests[0].header("anthropic-version").is_some());

    let server = FakeServer::start([Reply::json(
        200,
        r#"{"object":"list","data":[{"id":"fake-c","object":"model"},{"id":"fake-d","object":"model"}]}"#,
    )])
    .await;
    let models = openai(&server, Some(KEY)).list_models().await.unwrap();
    assert_eq!(
        models,
        ModelList::Offered(vec!["fake-c".to_owned(), "fake-d".to_owned()])
    );
    let requests = server.requests();
    assert_eq!(requests[0].method, "GET");
    assert_eq!(requests[0].path, "/v1/models");
    let bearer = format!("Bearer {KEY}");
    assert_eq!(requests[0].header("authorization"), Some(bearer.as_str()));
}

#[tokio::test]
async fn ai_1_models_not_offered_on_a_404() {
    let server = FakeServer::start([Reply::json(404, r#"{"error":"not found"}"#)]).await;
    assert_eq!(
        anthropic(&server).list_models().await.unwrap(),
        ModelList::NotOffered
    );
    let server = FakeServer::start([Reply::json(404, "404 page not found")]).await;
    assert_eq!(
        openai(&server, None).list_models().await.unwrap(),
        ModelList::NotOffered
    );
}

#[tokio::test]
async fn ai_1_other_errors_listing_models_are_errors() {
    let server = FakeServer::start([Reply::json(
        500,
        r#"{"error":{"message":"the list is broken"}}"#,
    )])
    .await;
    let error = openai(&server, None).list_models().await.unwrap_err();
    assert_eq!(
        error,
        ProviderError::Status {
            status: 500,
            message: "the list is broken".to_owned()
        }
    );
}

#[tokio::test]
async fn d4_anthropic_streamed_reply_is_put_together() {
    let server = FakeServer::start([Reply::events(stream(&[
        r#"{"hunks": "#,
        "[]",
        "}",
        " 🐊\n",
    ]))])
    .await;
    let reply = anthropic(&server).complete(&request()).await.unwrap();
    assert_eq!(reply.text, "{\"hunks\": []} 🐊\n");
}

#[tokio::test]
async fn ai_14_usage_is_reported_when_the_provider_gives_it() {
    // Anthropic: input from message_start, output from message_delta.
    let server = FakeServer::start([Reply::events(stream(&["{}"]))]).await;
    let reply = anthropic(&server).complete(&request()).await.unwrap();
    assert_eq!(
        reply.usage,
        Some(Usage {
            input_tokens: 120,
            output_tokens: 42
        })
    );

    // OpenAI-compatible: from `usage` when present.
    let server = FakeServer::start([
        Reply::json(200, completion("{}", Some((300, 25)))),
        Reply::json(200, completion("{}", None)),
    ])
    .await;
    let provider = openai(&server, Some(KEY));
    let reply = provider.complete(&request()).await.unwrap();
    assert_eq!(
        reply,
        ProviderReply {
            text: "{}".to_owned(),
            usage: Some(Usage {
                input_tokens: 300,
                output_tokens: 25
            })
        }
    );
    let reply = provider.complete(&request()).await.unwrap();
    assert_eq!(reply.usage, None, "a local model may report none");
}

#[tokio::test]
async fn d4_anthropic_request_has_temperature_0_and_the_key_header() {
    let server = FakeServer::start([Reply::events(stream(&["{}"]))]).await;
    anthropic(&server).complete(&retry_request()).await.unwrap();
    let requests = server.requests();
    let sent = &requests[0];
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.path, "/v1/messages");
    assert_eq!(sent.header("x-api-key"), Some(KEY));
    assert!(sent.header("anthropic-version").is_some());
    assert_eq!(sent.header("authorization"), None);
    assert!(!sent.body.contains(KEY), "the key goes in a header only");
    let body = sent.json();
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["temperature"], 0.0);
    assert_eq!(body["stream"], true);
    assert!(body["max_tokens"].as_u64().unwrap() > 0);
    assert_eq!(body["system"], "You resolve conflicts.");
    assert_eq!(
        body["messages"],
        serde_json::json!([
            {"role": "user", "content": "Resolve hunk 1."},
            {"role": "assistant", "content": "not json"},
            {"role": "user", "content": "Your reply is not valid."},
        ])
    );
}

#[tokio::test]
async fn d4_anthropic_without_temperature_sends_none() {
    let server = FakeServer::start([Reply::events(stream(&["{}"]))]).await;
    let request = ProviderRequest {
        temperature: None,
        ..request()
    };
    anthropic(&server).complete(&request).await.unwrap();
    assert!(server.requests()[0].json().get("temperature").is_none());
}

#[tokio::test]
async fn d4_anthropic_error_event_fails_the_request() {
    let body = concat!(
        "event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":5,\"output_tokens\":1}}}\n\n",
        "event: error\ndata: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\",\"message\":\"Overloaded\"}}\n\n",
    );
    let server = FakeServer::start([Reply::events(body)]).await;
    let error = anthropic(&server).complete(&request()).await.unwrap_err();
    assert_eq!(error, ProviderError::Failed("Overloaded".to_owned()));
}

#[tokio::test]
async fn d4_anthropic_stream_cut_short_is_an_error() {
    let mut body = stream(&["{}"]);
    let cut = body.find("event: message_stop").unwrap();
    body.truncate(cut);
    let server = FakeServer::start([Reply::events(body)]).await;
    let error = anthropic(&server).complete(&request()).await.unwrap_err();
    assert!(matches!(error, ProviderError::Malformed(_)), "{error:?}");
}

#[tokio::test]
async fn d4_openai_request_and_reply() {
    let server = FakeServer::start([Reply::json(200, completion("{\"hunks\": []}", None))]).await;
    let reply = openai(&server, Some(KEY))
        .complete(&retry_request())
        .await
        .unwrap();
    assert_eq!(reply.text, "{\"hunks\": []}");
    let requests = server.requests();
    let sent = &requests[0];
    assert_eq!(sent.method, "POST");
    assert_eq!(sent.path, "/v1/chat/completions");
    let bearer = format!("Bearer {KEY}");
    assert_eq!(sent.header("authorization"), Some(bearer.as_str()));
    assert!(!sent.body.contains(KEY), "the key goes in a header only");
    let body = sent.json();
    assert_eq!(body["model"], MODEL);
    assert_eq!(body["temperature"], 0.0);
    assert!(body.get("stream").is_none_or(|stream| stream == false));
    assert_eq!(
        body["messages"],
        serde_json::json!([
            {"role": "system", "content": "You resolve conflicts."},
            {"role": "user", "content": "Resolve hunk 1."},
            {"role": "assistant", "content": "not json"},
            {"role": "user", "content": "Your reply is not valid."},
        ])
    );
}

#[tokio::test]
async fn d4_openai_trailing_slash_in_the_base_url() {
    let server = FakeServer::start([Reply::json(200, completion("{}", None))]).await;
    let provider = OpenAiProvider::new(format!("{}/v1/", server.url), None, MODEL).unwrap();
    provider.complete(&request()).await.unwrap();
    assert_eq!(server.requests()[0].path, "/v1/chat/completions");
}

#[tokio::test]
async fn d4_openai_no_authorization_header_without_a_key() {
    let server = FakeServer::start([
        Reply::json(200, completion("{}", None)),
        Reply::json(200, r#"{"data":[]}"#),
    ])
    .await;
    let provider = openai(&server, None);
    provider.complete(&request()).await.unwrap();
    provider.list_models().await.unwrap();
    for sent in server.requests() {
        assert_eq!(sent.header("authorization"), None, "{}", sent.path);
    }
}

#[tokio::test]
async fn ai_15_openai_temperature_refused_is_retried_without_and_remembered() {
    let server = FakeServer::start([
        Reply::json(
            400,
            r#"{"error":{"message":"Unsupported parameter: 'temperature' is not supported with this model.","type":"invalid_request_error","param":"temperature"}}"#,
        ),
        Reply::json(200, completion("first", None)),
        Reply::json(200, completion("second", None)),
    ])
    .await;
    let provider = openai(&server, Some(KEY));
    assert_eq!(provider.complete(&request()).await.unwrap().text, "first");
    assert_eq!(provider.complete(&request()).await.unwrap().text, "second");
    let temperatures: Vec<Option<serde_json::Value>> = server
        .requests()
        .iter()
        .map(|sent| sent.json().get("temperature").cloned())
        .collect();
    assert_eq!(
        temperatures,
        [Some(serde_json::json!(0.0)), None, None],
        "retried once without it, then never sent again"
    );
}

#[tokio::test]
async fn ai_15_openai_other_400s_are_not_retried() {
    let server = FakeServer::start([
        Reply::json(400, r#"{"error":{"message":"The model does not exist."}}"#),
        Reply::json(200, completion("unused", None)),
    ])
    .await;
    let error = openai(&server, Some(KEY))
        .complete(&request())
        .await
        .unwrap_err();
    assert_eq!(
        error,
        ProviderError::Status {
            status: 400,
            message: "The model does not exist.".to_owned()
        }
    );
    assert_eq!(server.requests().len(), 1);
}

#[tokio::test]
async fn d4_http_status_carries_the_providers_message() {
    let server = FakeServer::start([Reply::json(
        529,
        r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
    )])
    .await;
    let error = anthropic(&server).complete(&request()).await.unwrap_err();
    assert_eq!(
        error,
        ProviderError::Status {
            status: 529,
            message: "Overloaded".to_owned()
        }
    );

    // A body that is not the usual JSON is shown as it is.
    let server = FakeServer::start([Reply::json(502, "Bad gateway\n")]).await;
    let error = openai(&server, None)
        .complete(&request())
        .await
        .unwrap_err();
    assert_eq!(
        error,
        ProviderError::Status {
            status: 502,
            message: "Bad gateway".to_owned()
        }
    );
}

#[tokio::test]
async fn d4_connection_failure_is_a_transport_error() {
    let url = closed_url().await;
    let provider = OpenAiProvider::new(url.clone(), None, MODEL).unwrap();
    let error = provider.complete(&request()).await.unwrap_err();
    assert!(matches!(error, ProviderError::Transport(_)), "{error:?}");
    let provider = AnthropicProvider::new(ApiKey::new(KEY), MODEL)
        .unwrap()
        .with_base_url(&url);
    let error = provider.list_models().await.unwrap_err();
    assert!(matches!(error, ProviderError::Transport(_)), "{error:?}");
}

#[test]
fn d4_request_timeout_is_5_minutes() {
    assert_eq!(REQUEST_TIMEOUT, Duration::from_secs(5 * 60));
}

#[tokio::test]
async fn sec_1_key_never_in_errors_or_debug() {
    assert_eq!(format!("{:?}", ApiKey::new(KEY)), "***");

    let echo = format!(
        r#"{{"type":"error","error":{{"type":"authentication_error","message":"invalid x-api-key {KEY}"}}}}"#
    );
    let server = FakeServer::start([Reply::json(401, echo.clone())]).await;
    let provider = anthropic(&server);
    assert!(!format!("{provider:?}").contains(KEY));
    let error = provider.complete(&request()).await.unwrap_err();
    assert_eq!(
        error,
        ProviderError::Status {
            status: 401,
            message: "invalid x-api-key ***".to_owned()
        }
    );
    assert!(!format!("{error:?}").contains(KEY));

    let echo = format!(r#"{{"error":{{"message":"Incorrect API key provided: {KEY}"}}}}"#);
    let server = FakeServer::start([Reply::json(401, echo.clone()), Reply::json(401, echo)]).await;
    let provider = openai(&server, Some(KEY));
    assert!(!format!("{provider:?}").contains(KEY));
    for error in [
        provider.complete(&request()).await.unwrap_err(),
        provider.list_models().await.unwrap_err(),
    ] {
        assert!(!error.to_string().contains(KEY), "{error}");
        assert!(!format!("{error:?}").contains(KEY), "{error:?}");
        assert!(error.to_string().contains("***"), "{error}");
    }

    // A body that is not JSON is masked too.
    let server = FakeServer::start([Reply::json(401, format!("bad key {KEY}"))]).await;
    let error = anthropic(&server).list_models().await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "the provider answered with HTTP 401: bad key ***"
    );
}

#[tokio::test]
async fn d4_cancel_ends_a_held_stream_without_a_reply() {
    let full = stream(&["{}"]);
    let split = full.find("event: content_block_start").unwrap();
    let held = Held::default();
    let server = FakeServer::start([Reply::Held {
        content_type: "text/event-stream",
        first: full[..split].to_owned(),
        rest: full[split..].to_owned(),
        held: held.clone(),
    }])
    .await;
    let provider = Arc::new(anthropic(&server));
    let asking = tokio::spawn({
        let provider = Arc::clone(&provider);
        async move { provider.complete(&request()).await }
    });
    held.sent().await;
    asking.abort();
    assert!(asking.await.unwrap_err().is_cancelled(), "no reply");
    // Dropping the request closed the connection.
    held.closed().await;
}

#[tokio::test]
async fn d4_held_stream_answers_once_released() {
    let full = stream(&["{}"]);
    let split = full.find("event: content_block_start").unwrap();
    let held = Held::default();
    let server = FakeServer::start([Reply::Held {
        content_type: "text/event-stream",
        first: full[..split].to_owned(),
        rest: full[split..].to_owned(),
        held: held.clone(),
    }])
    .await;
    let provider = Arc::new(anthropic(&server));
    let asking = tokio::spawn({
        let provider = Arc::clone(&provider);
        async move { provider.complete(&request()).await }
    });
    held.sent().await;
    held.release();
    assert_eq!(asking.await.unwrap().unwrap().text, "{}");
}
