// src/types.rs
//! Minimal shared types for cnctd_ai.
//! Keep this file small and stable; add optional fields over time.

use serde::{Deserialize, Serialize};

/// Provider selector (keep ids stable for client/server).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    OpenAI,       // OpenAI (or Azure OpenAI if your base_url points there)
    OpenRouter,   // OpenRouter (OpenAI-compatible gateway)
    Anthropic,    // Claude (adapter to add later)
    // Add others later (e.g., Gemini)
}

/// A simple chat role set. Expand later if you add multi-part content.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// One message in your canonical transcript.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Msg {
    pub role: Role,
    pub content: String,
    #[serde(default)]
    pub name: Option<String>, // optional sender label
}

/// Per-call generation knobs (leave unset to use server defaults).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AskOptions {
    pub temperature: Option<f32>,       // e.g., Some(0.2)
    pub max_output_tokens: Option<u32>, // e.g., Some(512)
    pub json_mode: Option<bool>,        // request strict JSON if supported
    pub stream: Option<bool>,           // stream tokens (adapter may ignore for now)
}

/// The request shape your universal client expects.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AskRequest {
    pub provider: ProviderId,   // which adapter to use
    pub model: Option<String>,  // provider-native model id; fallback to config default
    #[serde(default)]
    pub system: Option<String>, // optional system instruction
    pub messages: Vec<Msg>,     // recent window + summary, already packed
    #[serde(default)]
    pub options: AskOptions,    // per-call overrides
    #[serde(default)]
    pub context_refs: Vec<String>, // optional ids (conversation, project, doc)
}

/// Normalized usage counters (best-effort; some providers may omit).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

/// Provider-agnostic response your app can rely on.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct UniversalResponse {
    pub text: String,                // final answer string (may be empty on tool calls)
    pub finish_reason: String,       // normalized: "stop" | "length" | "tool_call" | "content_filter" | "error"
    #[serde(default)]
    pub usage: Option<Usage>,        // token usage if available
    pub latency_ms: u128,            // end-to-end latency measured by caller
    #[serde(default)]
    pub provider_meta: serde_json::Value, // raw provider payload or fields for debugging
}

/// Optional: minimal metadata structs if you want to surface providers/models to the client.
/// Keep these as plain data; no behavior.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProviderInfo {
    pub id: ProviderId,
    pub name: String,
    #[serde(default)]
    pub models: Vec<ModelInfo>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,            // provider-native id (e.g., "gpt-4.1-mini")
    pub label: String,         // human-friendly name
    #[serde(default)]
    pub context_tokens: Option<u32>,
    #[serde(default)]
    pub supports_json: Option<bool>,
    #[serde(default)]
    pub supports_tools: Option<bool>,
}
