//! Any OpenAI-compatible Chat Completions endpoint: OpenAI, OpenRouter,
//! Ollama, LM Studio and similar (section 7 "Providers").

use std::sync::atomic::{AtomicBool, Ordering};

use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use serde_json::json;

use crate::http::{
    ApiKey, ModelList, body, client, error_message, model_ids, parse, tokens, transport, trim_base,
    usage,
};
use crate::provider::{
    Provider, ProviderError, ProviderFuture, ProviderReply, ProviderRequest, Role,
};

/// Calls `{base_url}/chat/completions`, with a key where the endpoint
/// needs one.
#[derive(Debug)]
pub struct OpenAiProvider {
    base_url: String,
    api_key: Option<ApiKey>,
    model: String,
    client: reqwest::Client,
    /// Set once the endpoint refused `temperature`, for the rest of the
    /// session. Some local models do (AI-15).
    no_temperature: AtomicBool,
}

impl OpenAiProvider {
    /// A provider for `model`, which may be empty while only listing models.
    /// `base_url` includes the API's path, as in `https://api.openai.com/v1`.
    pub fn new(
        base_url: impl AsRef<str>,
        api_key: Option<ApiKey>,
        model: impl Into<String>,
    ) -> Result<Self, ProviderError> {
        Ok(Self {
            base_url: trim_base(base_url.as_ref()),
            api_key,
            model: model.into(),
            client: client()?,
            no_temperature: AtomicBool::new(false),
        })
    }

    fn with_key(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.api_key {
            Some(key) => builder.header(AUTHORIZATION, format!("Bearer {}", key.expose())),
            None => builder,
        }
    }

    /// The models of the endpoint, from `GET {base_url}/models` (AI-1).
    pub async fn list_models(&self) -> Result<ModelList, ProviderError> {
        let key = self.api_key.as_ref();
        let response = self
            .with_key(self.client.get(format!("{}/models", self.base_url)))
            .send()
            .await
            .map_err(|error| transport(&error, key))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(ModelList::NotOffered);
        }
        let list = parse(&body(response, key).await?)?;
        Ok(ModelList::Offered(model_ids(&list)?))
    }

    async fn send(&self, request: &ProviderRequest) -> Result<ProviderReply, ProviderError> {
        let key = self.api_key.as_ref();
        let mut messages = vec![
            json!({"role": "system", "content": request.system}),
            json!({"role": "user", "content": request.user}),
        ];
        messages.extend(request.follow_up.iter().map(|message| {
            let role = match message.role {
                Role::User => "user",
                Role::Assistant => "assistant",
            };
            json!({"role": role, "content": message.text})
        }));
        let mut temperature = request
            .temperature
            .filter(|_| !self.no_temperature.load(Ordering::Relaxed));
        loop {
            let mut payload = json!({"model": self.model, "messages": messages});
            if let Some(temperature) = temperature {
                payload["temperature"] = json!(temperature);
            }
            let response = self
                .with_key(
                    self.client
                        .post(format!("{}/chat/completions", self.base_url)),
                )
                .header(CONTENT_TYPE, "application/json")
                .body(payload.to_string())
                .send()
                .await
                .map_err(|error| transport(&error, key))?;
            let status = response.status();
            if status.is_success() {
                let text = response
                    .text()
                    .await
                    .map_err(|error| transport(&error, key))?;
                return reply(&parse(&text)?);
            }
            let text = response
                .text()
                .await
                .map_err(|error| transport(&error, key))?;
            // A 400 naming `temperature` is retried once without it.
            if status == reqwest::StatusCode::BAD_REQUEST
                && temperature.is_some()
                && text.to_lowercase().contains("temperature")
            {
                self.no_temperature.store(true, Ordering::Relaxed);
                temperature = None;
                continue;
            }
            return Err(ProviderError::Status {
                status: status.as_u16(),
                message: error_message(status.as_u16(), &text, key),
            });
        }
    }
}

/// The text and usage of a completion.
fn reply(completion: &serde_json::Value) -> Result<ProviderReply, ProviderError> {
    let text = completion
        .pointer("/choices/0/message/content")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| ProviderError::Malformed("the reply has no message content".to_owned()))?;
    let counts = completion.get("usage");
    Ok(ProviderReply {
        text: text.to_owned(),
        usage: usage(
            tokens(counts.and_then(|counts| counts.get("prompt_tokens"))),
            tokens(counts.and_then(|counts| counts.get("completion_tokens"))),
        ),
    })
}

impl Provider for OpenAiProvider {
    fn complete<'a>(&'a self, request: &'a ProviderRequest) -> ProviderFuture<'a> {
        Box::pin(self.send(request))
    }
}
