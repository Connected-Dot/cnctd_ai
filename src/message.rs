
use serde::{Deserialize, Serialize};

use crate::ToolUse;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    // Internal fields for tool tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_uses: Option<Vec<ToolUse>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_uses: None,
            tool_call_id: None,
        }
    }
    
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_uses: None,
            tool_call_id: None,
        }
    }
    
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_uses: None,
            tool_call_id: None,
        }
    }
    
    // New helper methods from your example:
    pub fn assistant_with_tool_use(tool_use: ToolUse) -> Self {
        Self {
            role: Role::Assistant,
            content: String::new(),
            tool_uses: Some(vec![tool_use]),
            tool_call_id: None,
        }
    }
    
    pub fn tool_result(tool_call_id: String, content: impl Into<String>) -> Self {
        Self {
            role: Role::User,  // Note: tool results typically have User role
            content: content.into(),
            tool_uses: None,
            tool_call_id: Some(tool_call_id),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}