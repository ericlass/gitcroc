//! What the API providers share: the key, the client, model lists and
//! turning HTTP failures into `ProviderError`s with the key masked (SEC-1).

use std::fmt;
use std::time::Duration;

use crate::answer::Usage;
use crate::provider::ProviderError;

/// How long a request may take, from connecting to the last byte of the
/// reply. Not a setting.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(5 * 60);

/// The longest provider message an error keeps, in characters.
const MESSAGE_LIMIT: usize = 500;

/// An API key. It is sent only in a header; `Debug` prints `***` and
/// there is no `Display` (SEC-1).
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into())
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("***")
    }
}

/// The models a provider offers (AI-1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelList {
    /// The model names, in the provider's order.
    Offered(Vec<String>),
    /// The endpoint has no list; the user types the name.
    NotOffered,
}

/// The HTTP client of one provider.
pub(crate) fn client() -> Result<reqwest::Client, ProviderError> {
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|error| ProviderError::Transport(chain(&error)))
}

/// `base_url` without a trailing slash, so paths can be appended.
pub(crate) fn trim_base(base_url: &str) -> String {
    base_url.trim_end_matches('/').to_owned()
}

/// `text` with every occurrence of the key replaced by `***`.
pub(crate) fn mask(text: &str, key: Option<&ApiKey>) -> String {
    match key {
        Some(key) if !key.0.is_empty() => text.replace(&key.0, "***"),
        _ => text.to_owned(),
    }
}

/// An error and its sources, joined, since reqwest's own message rarely
/// says why ("error sending request").
fn chain(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let cause_text = cause.to_string();
        if !text.contains(&cause_text) {
            text.push_str(": ");
            text.push_str(&cause_text);
        }
        source = cause.source();
    }
    text
}

/// A failed send or read: a timeout, or the provider could not be reached.
pub(crate) fn transport(error: &reqwest::Error, key: Option<&ApiKey>) -> ProviderError {
    if error.is_timeout() {
        ProviderError::Timeout
    } else {
        ProviderError::Transport(mask(&chain(error), key))
    }
}

/// The message of an error reply: the `error.message` both APIs use, or
/// else the body itself, shortened and with the key masked.
pub(crate) fn error_message(status: u16, body: &str, key: Option<&ApiKey>) -> String {
    let parsed = serde_json::from_str::<serde_json::Value>(body).ok();
    let message = parsed
        .as_ref()
        .and_then(|value| value.pointer("/error/message"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or(body)
        .trim();
    let message = if message.is_empty() {
        reqwest::StatusCode::from_u16(status)
            .ok()
            .and_then(|status| status.canonical_reason())
            .unwrap_or("no message")
    } else {
        message
    };
    let mut message = mask(message, key);
    if let Some((at, _)) = message.char_indices().nth(MESSAGE_LIMIT) {
        message.truncate(at);
        message.push('…');
    }
    message
}

/// The error of a reply whose status is not a success.
pub(crate) async fn failure(response: reqwest::Response, key: Option<&ApiKey>) -> ProviderError {
    let status = response.status().as_u16();
    match response.text().await {
        Ok(text) => ProviderError::Status {
            status,
            message: error_message(status, &text, key),
        },
        Err(error) => transport(&error, key),
    }
}

/// Reads a reply's body, failing on a status other than success.
pub(crate) async fn body(
    response: reqwest::Response,
    key: Option<&ApiKey>,
) -> Result<String, ProviderError> {
    if !response.status().is_success() {
        return Err(failure(response, key).await);
    }
    response
        .text()
        .await
        .map_err(|error| transport(&error, key))
}

/// A JSON body.
pub(crate) fn parse(text: &str) -> Result<serde_json::Value, ProviderError> {
    serde_json::from_str(text).map_err(|error| ProviderError::Malformed(error.to_string()))
}

/// A token count from JSON, saturating at `u32::MAX`.
pub(crate) fn tokens(value: Option<&serde_json::Value>) -> Option<u32> {
    let count = value?.as_u64()?;
    Some(u32::try_from(count).unwrap_or(u32::MAX))
}

/// Usage, when the provider gave both counts (AI-14).
pub(crate) fn usage(input: Option<u32>, output: Option<u32>) -> Option<Usage> {
    Some(Usage {
        input_tokens: input?,
        output_tokens: output?,
    })
}

/// The `id`s of a model list's `data`.
pub(crate) fn model_ids(list: &serde_json::Value) -> Result<Vec<String>, ProviderError> {
    let data = list
        .get("data")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| ProviderError::Malformed("the model list has no data".to_owned()))?;
    Ok(data
        .iter()
        .filter_map(|model| model.get("id")?.as_str())
        .map(str::to_owned)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sec_1_long_messages_are_cut_after_masking() {
        let key = ApiKey::new("secret");
        let body = format!("{}secret", "x".repeat(MESSAGE_LIMIT - 2));
        let message = error_message(401, &body, Some(&key));
        assert!(!message.contains("sec"), "{message}");
        assert!(message.ends_with('…'));
        assert_eq!(error_message(503, "", None), "Service Unavailable");
    }
}
