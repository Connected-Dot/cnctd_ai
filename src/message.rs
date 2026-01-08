
use serde::{Deserialize, Serialize};

use crate::ToolUse;

/// Represents a single tool result (tool_call_id + content)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
    /// Function name - required for Gemini, optional for others
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function_name: Option<String>,
    /// OpenAI Responses API call_id (call_...) for function_call_output matching
    /// When present, this is used instead of tool_call_id for OpenAI Responses API
    #[serde(skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
}

impl ToolResult {
    pub fn new(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error: false,
            function_name: None,
            call_id: None,
        }
    }
    
    pub fn error(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error: true,
            function_name: None,
            call_id: None,
        }
    }
    
    /// Create a tool result with function name (required for Gemini)
    pub fn with_name(
        tool_call_id: impl Into<String>, 
        content: impl Into<String>,
        function_name: impl Into<String>,
    ) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error: false,
            function_name: Some(function_name.into()),
            call_id: None,
        }
    }
    
    /// Create an error tool result with function name
    pub fn error_with_name(
        tool_call_id: impl Into<String>, 
        content: impl Into<String>,
        function_name: impl Into<String>,
    ) -> Self {
        Self {
            tool_call_id: tool_call_id.into(),
            content: content.into(),
            is_error: true,
            function_name: Some(function_name.into()),
            call_id: None,
        }
    }
    
    /// Builder method to set the function name
    pub fn set_name(mut self, function_name: impl Into<String>) -> Self {
        self.function_name = Some(function_name.into());
        self
    }
    
    /// Builder method to set the OpenAI Responses API call_id
    pub fn set_call_id(mut self, call_id: impl Into<String>) -> Self {
        self.call_id = Some(call_id.into());
        self
    }
    
    /// Get the effective call_id for OpenAI Responses API
    /// Returns call_id if present, otherwise falls back to tool_call_id
    pub fn effective_call_id(&self) -> &str {
        self.call_id.as_deref().unwrap_or(&self.tool_call_id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
    // Internal fields for tool tracking
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_uses: Option<Vec<ToolUse>>,
    /// Single tool result (legacy, for backward compatibility)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_call_id: Option<String>,
    /// Multiple tool results in one message (Anthropic API requirement)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) tool_results: Option<Vec<ToolResult>>,
    /// Reasoning items for OpenAI Responses API (GPT-5.2-pro)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_items: Option<Vec<serde_json::Value>>,
}

impl Message {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_uses: None,
            tool_call_id: None,
            tool_results: None,
            reasoning_items: None,
        }
    }
    
    pub fn assistant(content: impl Into<String>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_uses: None,
            tool_call_id: None,
            tool_results: None,
            reasoning_items: None,
        }
    }
    
    pub fn system(content: impl Into<String>) -> Self {
        Self {
            role: Role::System,
            content: content.into(),
            tool_uses: None,
            tool_call_id: None,
            tool_results: None,
            reasoning_items: None,
        }
    }
    
    /// Create an assistant message with a single tool use (empty content)
    pub fn assistant_with_tool_use(tool_use: ToolUse) -> Self {
        Self {
            role: Role::Assistant,
            content: String::new(),
            tool_uses: Some(vec![tool_use]),
            tool_call_id: None,
            tool_results: None,
            reasoning_items: None,
        }
    }
    
    /// Create an assistant message with multiple tool uses (empty content)
    pub fn assistant_with_tool_uses(tool_uses: Vec<ToolUse>) -> Self {
        Self {
            role: Role::Assistant,
            content: String::new(),
            tool_uses: if tool_uses.is_empty() { None } else { Some(tool_uses) },
            tool_call_id: None,
            tool_results: None,
            reasoning_items: None,
        }
    }
    
    /// Create an assistant message with content and tool uses
    pub fn assistant_with_content_and_tools(content: impl Into<String>, tool_uses: Vec<ToolUse>) -> Self {
        Self {
            role: Role::Assistant,
            content: content.into(),
            tool_uses: if tool_uses.is_empty() { None } else { Some(tool_uses) },
            tool_call_id: None,
            tool_results: None,
            reasoning_items: None,
        }
    }
    
    /// Create a tool result user message (single result, legacy)
    pub fn tool_result(tool_call_id: String, content: impl Into<String>) -> Self {
        Self {
            role: Role::User,
            content: content.into(),
            tool_uses: None,
            tool_call_id: Some(tool_call_id),
            tool_results: None,
            reasoning_items: None,
        }
    }
    
    /// Create a user message with multiple tool results
    /// This is the correct way to respond to an assistant message with multiple tool_uses
    pub fn tool_results(results: Vec<ToolResult>) -> Self {
        Self {
            role: Role::User,
            content: String::new(),
            tool_uses: None,
            tool_call_id: None,
            tool_results: if results.is_empty() { None } else { Some(results) },
            reasoning_items: None,
        }
    }
    
    /// Check if this message contains tool results
    pub fn has_tool_results(&self) -> bool {
        self.tool_call_id.is_some() || self.tool_results.is_some()
    }
    
    /// Get all tool results from this message (combines legacy and new format)
    pub fn get_tool_results(&self) -> Vec<ToolResult> {
        let mut results = Vec::new();
        
        // Legacy single tool result
        if let Some(ref tool_call_id) = self.tool_call_id {
            results.push(ToolResult::new(tool_call_id.clone(), self.content.clone()));
        }
        
        // Multiple tool results
        if let Some(ref tool_results) = self.tool_results {
            results.extend(tool_results.clone());
        }
        
        results
    }

    /// Set reasoning items for OpenAI Responses API (GPT-5.2-pro)
    /// Required for multi-turn tool calls with reasoning models
    pub fn with_reasoning_items(mut self, items: Vec<serde_json::Value>) -> Self {
        self.reasoning_items = Some(items);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}
