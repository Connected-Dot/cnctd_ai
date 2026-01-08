use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents a tool use/call in a conversation context.
/// This is used to track when an assistant invokes a tool and the subsequent result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolUse {
    /// Primary ID for the tool use. For OpenAI Responses API, this is the fc_... ID.
    pub id: String,
    /// Alternative call_id for OpenAI Responses API function_call_output matching.
    /// When present, this should be used in function_call_output.call_id field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    pub name: String,
    pub input: Value,
}
