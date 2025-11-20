use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Represents a tool use/call in a conversation context.
/// This is used to track when an assistant invokes a tool and the subsequent result.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}
