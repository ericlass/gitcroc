//! The Anthropic Messages API, streamed (section 7 "Providers").

use reqwest::header::CONTENT_TYPE;
use serde_json::json;

use crate::http::{
    ApiKey, ModelList, body, client, error_message, failure, model_ids, parse, tokens, transport,
    trim_base, usage,
};
use crate::provider::{
    Provider, ProviderError, ProviderFuture, ProviderReply, ProviderRequest, Role,
};

/// The API version every request names.
const API_VERSION: &str = "2023-06-01";

/// The most tokens a reply may have. A reply repeats each hunk's resolution
/// with an explanation, so it is sized for the larger hunks a request can
/// carry, not for a chat answer.
pub const ANTHROPIC_MAX_TOKENS: u32 = 16_384;

/// Calls the Messages API with a key and a model the user chose.
#[derive(Debug)]
pub struct AnthropicProvider {
    api_key: ApiKey,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl AnthropicProvider {
    pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";

    /// A provider for `model`, which may be empty while only listing models.
    pub fn new(api_key: ApiKey, model: impl Into<String>) -> Result<Self, ProviderError> {
        Ok(Self {
            api_key,
            model: model.into(),
            base_url: Self::DEFAULT_BASE_URL.to_owned(),
            client: client()?,
        })
    }

    /// The same provider at another address; the tests' fake server.
    #[must_use]
    pub fn with_base_url(mut self, base_url: &str) -> Self {
        self.base_url = trim_base(base_url);
        self
    }

    fn key(&self) -> Option<&ApiKey> {
        Some(&self.api_key)
    }

    /// The models the key may use, from `GET /v1/models`, page by page (AI-1).
    pub async fn list_models(&self) -> Result<ModelList, ProviderError> {
        let mut models = Vec::new();
        let mut after: Option<String> = None;
        loop {
            let mut query = vec![("limit", "1000".to_owned())];
            if let Some(after) = after.take() {
                query.push(("after_id", after));
            }
            let url =
                reqwest::Url::parse_with_params(&format!("{}/v1/models", self.base_url), &query)
                    .map_err(|error| ProviderError::Transport(error.to_string()))?;
            let response = self
                .client
                .get(url)
                .header("x-api-key", self.api_key.expose())
                .header("anthropic-version", API_VERSION)
                .send()
                .await
                .map_err(|error| transport(&error, self.key()))?;
            if response.status() == reqwest::StatusCode::NOT_FOUND {
                return Ok(ModelList::NotOffered);
            }
            let page = parse(&body(response, self.key()).await?)?;
            models.extend(model_ids(&page)?);
            let more = page.get("has_more").and_then(serde_json::Value::as_bool) == Some(true);
            match page.get("last_id").and_then(serde_json::Value::as_str) {
                Some(last) if more => after = Some(last.to_owned()),
                _ => return Ok(ModelList::Offered(models)),
            }
        }
    }

    async fn send(&self, request: &ProviderRequest) -> Result<ProviderReply, ProviderError> {
        let mut messages = vec![json!({"role": "user", "content": request.user})];
        messages.extend(request.follow_up.iter().map(|message| {
            let role = match message.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            json!({"role": role, "content": message.text})
        }));
        let mut payload = json!({
            "model": self.model,
            "max_tokens": ANTHROPIC_MAX_TOKENS,
            "system": request.system,
            "messages": messages,
            "stream": true,
        });
        if let Some(temperature) = request.temperature {
            payload["temperature"] = json!(temperature);
        }
        let mut response = self
            .client
            .post(format!("{}/v1/messages", self.base_url))
            .header("x-api-key", self.api_key.expose())
            .header("anthropic-version", API_VERSION)
            .header(CONTENT_TYPE, "application/json")
            .body(payload.to_string())
            .send()
            .await
            .map_err(|error| transport(&error, self.key()))?;
        if !response.status().is_success() {
            return Err(failure(response, self.key()).await);
        }
        let mut events = Events::default();
        let mut reply = Reply::default();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| transport(&error, self.key()))?
        {
            for data in events.push(&chunk)? {
                if reply.apply(&data, self.key())? {
                    return Ok(reply.finish());
                }
            }
        }
        Err(ProviderError::Malformed(
            "the stream ended before the reply was complete".to_owned(),
        ))
    }
}

impl Provider for AnthropicProvider {
    fn complete<'a>(&'a self, request: &'a ProviderRequest) -> ProviderFuture<'a> {
        Box::pin(self.send(request))
    }
}

/// Splits a server-sent event stream into the `data` of each event. Bytes
/// arrive in arbitrary chunks; a line is read only once it is whole.
#[derive(Debug, Default)]
struct Events {
    pending: Vec<u8>,
    data: Vec<String>,
}

impl Events {
    /// Takes a chunk and returns the data of every event it completed.
    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, ProviderError> {
        self.pending.extend_from_slice(chunk);
        let mut done = Vec::new();
        while let Some(end) = self.pending.iter().position(|&byte| byte == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            let line = std::str::from_utf8(&line)
                .map_err(|error| ProviderError::Malformed(error.to_string()))?
                .trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    done.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(data) = line.strip_prefix("data:") {
                self.data
                    .push(data.strip_prefix(' ').unwrap_or(data).to_owned());
            }
            // `event:`, `id:`, `retry:` and comments: the data's `type`
            // says everything needed.
        }
        Ok(done)
    }
}

/// The input tokens of a Messages API `usage`, which Claude Code reports
/// too: those read from or written to a prompt cache count as input.
pub(crate) fn input_tokens(usage: &serde_json::Value) -> Option<u32> {
    usage.get("input_tokens")?;
    [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ]
    .iter()
    .filter_map(|name| tokens(usage.get(*name)))
    .reduce(u32::saturating_add)
}

/// The reply as its events arrive.
#[derive(Debug, Default)]
struct Reply {
    text: String,
    input: Option<u32>,
    output: Option<u32>,
}

impl Reply {
    /// Applies one event; `true` once the message is complete.
    fn apply(&mut self, data: &str, key: Option<&ApiKey>) -> Result<bool, ProviderError> {
        let event = parse(data)?;
        match event.get("type").and_then(serde_json::Value::as_str) {
            Some("message_start") => {
                let usage = event.pointer("/message/usage");
                self.count(usage);
            }
            Some("content_block_delta") => {
                let delta = event.get("delta");
                if delta
                    .and_then(|delta| delta.get("type"))
                    .and_then(serde_json::Value::as_str)
                    == Some("text_delta")
                {
                    let text = delta
                        .and_then(|delta| delta.get("text"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    self.text.push_str(text);
                }
            }
            Some("message_delta") => self.count(event.get("usage")),
            Some("message_stop") => return Ok(true),
            Some("error") => {
                return Err(ProviderError::Failed(error_message(0, data, key)));
            }
            _ => {}
        }
        Ok(false)
    }

    /// Takes the counts an event's `usage` gives.
    fn count(&mut self, usage: Option<&serde_json::Value>) {
        let Some(usage) = usage else {
            return;
        };
        if let Some(input) = input_tokens(usage) {
            self.input = Some(input);
        }
        if let Some(output) = tokens(usage.get("output_tokens")) {
            self.output = Some(output);
        }
    }

    fn finish(self) -> ProviderReply {
        ProviderReply {
            text: self.text,
            usage: usage(self.input, self.output),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d4_events_split_anywhere_are_put_together() {
        let stream = "event: a\r\ndata: {\"x\":\r\ndata: \"🐊\"}\r\n\r\n: comment\n\ndata:{}\n\n";
        let mut events = Events::default();
        let mut data = Vec::new();
        // Byte by byte, so a chunk ends inside the four-byte character.
        for byte in stream.as_bytes() {
            data.extend(events.push(std::slice::from_ref(byte)).unwrap());
        }
        assert_eq!(data, ["{\"x\":\n\"🐊\"}", "{}"]);
    }

    #[test]
    fn ai_14_usage_counts_cached_input() {
        let mut reply = Reply::default();
        reply.count(Some(&json!({
            "input_tokens": 10,
            "cache_read_input_tokens": 90,
            "output_tokens": 1,
        })));
        reply.count(Some(&json!({"output_tokens": 7})));
        assert_eq!(
            reply.finish().usage,
            Some(crate::answer::Usage {
                input_tokens: 100,
                output_tokens: 7
            })
        );
    }
}
