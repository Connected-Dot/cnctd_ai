use serde::{Deserialize, Serialize};

/// Provider-agnostic response your app can rely on.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskResponse {
    pub text: String,                // final answer string (may be empty on tool calls)
    pub finish_reason: String,       // normalized: "stop" | "length" | "tool_call" | "content_filter" | "error"
    #[serde(default)]
    pub usage: Option<Usage>,        // token usage if available
    pub latency_ms: u128,            // end-to-end latency measured by caller
    #[serde(default)]
    pub provider_meta: serde_json::Value, // raw provider payload or fields for debugging
}

/// Normalized usage counters (best-effort; some providers may omit).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Usage {
    pub prompt_tokens: Option<u32>,
    pub completion_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}
