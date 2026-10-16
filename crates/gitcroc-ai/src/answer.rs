//! What a model answers for a file's hunks (section 7 "What comes back").

use serde::{Deserialize, Serialize};

/// How sure the model is of a resolution (AI-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    High,
    Medium,
    Low,
}

/// Whether the model resolved a hunk (AI-8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum HunkStatus {
    Resolved,
    Unresolved,
}

/// The model's answer for one hunk. A resolved hunk has a resolution, an
/// unresolved one has none; both have an explanation (AI-8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct HunkAnswer {
    /// The hunk's id in the request, counted from 1.
    pub id: u32,
    pub status: HunkStatus,
    /// The exact replacement text for the hunk.
    #[serde(default)]
    pub resolution: Option<String>,
    pub explanation: String,
    pub confidence: Confidence,
}

/// The model's answer for a file: every requested hunk once (AI-9).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(deny_unknown_fields)]
pub struct FileAnswer {
    pub hunks: Vec<HunkAnswer>,
}

/// The tokens of one request, when the provider reports them (AI-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
}
