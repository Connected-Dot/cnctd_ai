use serde::{Deserialize, Serialize};

use crate::ask::{msg::Msg, response::AskResponse};

/// Per-call generation knobs (leave unset to use server defaults).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AskOptions {
    pub temperature: Option<f32>,       // e.g., Some(0.2)
    pub max_output_tokens: Option<u32>, // e.g., Some(512)
    pub json_mode: Option<bool>,        // request strict JSON if supported
    pub stream: Option<bool>,           // stream tokens (adapter may ignore for now)
}

/// The request shape your universal client expects.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskRequest {
    #[serde(default)]
    pub system: Option<String>, // optional system instruction
    pub messages: Vec<Msg>,     // recent window + summary, already packed
    #[serde(default)]
    pub options: AskOptions,    // per-call overrides
    #[serde(default)]
    pub context_refs: Vec<String>, // optional ids (conversation, project, doc)
    pub provider: String,       // e.g., "openai" or "anthropic"
    pub model: String,          // e.g., "gpt-4o" or
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum AskChunk {
    /// Token/text delta for chat.
    Delta { text: String },

    /// Tool call deltas are streamed; you must assemble them.
    ToolCallDelta {
        tool_call_id: String,
        name: Option<String>,
        args_delta: Option<String>,
    },

    /// Provider emitted a role/content change (rare but possible).
    Role(String),

    /// End-of-stream summary with usage/finish_reason and full text we assembled.
    Complete(AskResponse),
}
