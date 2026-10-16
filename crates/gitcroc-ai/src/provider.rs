//! The seam to a model: one request, one reply (section 3 "Provider").

use std::future::Future;
use std::pin::Pin;

use crate::answer::Usage;

/// Who wrote a message of a conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

/// A message after the first user message: a retry's earlier reply and the
/// validation error (AI-9).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

/// What a provider sends to its model.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderRequest {
    pub system: String,
    pub user: String,
    /// The messages after `user`, in order; empty on a first request.
    pub follow_up: Vec<Message>,
    /// `None` where the provider takes no temperature (AI-15).
    pub temperature: Option<f32>,
}

/// What a model answered: its raw text, not yet validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderReply {
    pub text: String,
    pub usage: Option<Usage>,
}

/// Why a request got no reply. It carries what the provider or the program
/// said, never a key (SEC-1).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error("could not reach the provider: {0}")]
    Transport(String),
    #[error("the provider did not answer within 5 minutes")]
    Timeout,
    #[error("the provider answered with HTTP {status}: {message}")]
    Status { status: u16, message: String },
    /// An error the provider reported after it started answering.
    #[error("the provider stopped with an error: {0}")]
    Failed(String),
    #[error("the provider's reply could not be read: {0}")]
    Malformed(String),
    /// A CLI provider's program could not be run.
    #[error("could not run {program}: {message}")]
    Start { program: String, message: String },
    #[error("{program} exited with {}: {stderr}", code.map_or_else(|| "a signal".to_owned(), |code| format!("code {code}")))]
    Exit {
        program: String,
        code: Option<i32>,
        stderr: String,
    },
    #[error("cancelled")]
    Cancelled,
}

/// The future a provider returns for a request.
pub type ProviderFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProviderReply, ProviderError>> + Send + 'a>>;

/// A configured way to reach a model (D4). Boxed futures keep it usable as
/// `dyn Provider`, so the settings can choose one at run time.
pub trait Provider: Send + Sync {
    /// Sends one request and waits for the model's whole reply.
    fn complete<'a>(&'a self, request: &'a ProviderRequest) -> ProviderFuture<'a>;
}
