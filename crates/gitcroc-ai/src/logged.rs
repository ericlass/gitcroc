//! AI debug logging (AI-7): each prompt and reply of a provider written to
//! a log the caller names, only when the user turned it on. A request holds
//! no key, so none is ever written (SEC-1).

use std::sync::Arc;

use crate::provider::{Provider, ProviderFuture, ProviderRequest, Role};

/// Where the lines go: the app's log, or the CLI's stderr.
pub type DebugLog = Arc<dyn Fn(&str) + Send + Sync>;

/// `provider`, with each request and its reply or error written to `log`.
pub struct Logged {
    provider: Box<dyn Provider>,
    log: DebugLog,
}

impl Logged {
    pub fn new(provider: Box<dyn Provider>, log: DebugLog) -> Self {
        Self { provider, log }
    }
}

/// The request as the log shows it: every message, in order.
fn request_text(request: &ProviderRequest) -> String {
    let mut text = format!(
        "AI request\n[system]\n{}\n[user]\n{}",
        request.system, request.user
    );
    for message in &request.follow_up {
        let role = match message.role {
            Role::User => "user",
            Role::Assistant => "assistant",
        };
        text.push_str(&format!("\n[{role}]\n{}", message.text));
    }
    text
}

impl Provider for Logged {
    fn complete<'a>(&'a self, request: &'a ProviderRequest) -> ProviderFuture<'a> {
        Box::pin(async move {
            (self.log)(&request_text(request));
            let reply = self.provider.complete(request).await;
            match &reply {
                Ok(reply) => (self.log)(&format!("AI reply\n{}", reply.text)),
                Err(error) => (self.log)(&format!("AI request failed: {error}")),
            }
            reply
        })
    }
}
