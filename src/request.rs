use serde::{Deserialize, Serialize};
use crate::{Tool, message::Message};

/// Built-in tools provided by AI providers (not MCP tools)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuiltInTool {
    /// Gemini 2.0+ Google Search grounding
    GoogleSearch,
    /// Gemini 1.5 legacy search with dynamic retrieval
    GoogleSearchRetrieval {
        /// Threshold (0.0-1.0) for dynamic search triggering
        /// Only searches if model confidence exceeds this threshold
        #[serde(skip_serializing_if = "Option::is_none")]
        dynamic_threshold: Option<f32>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Tool>>,
    /// Provider-specific built-in tools (e.g., Google Search for Gemini)
    pub built_in_tools: Option<Vec<BuiltInTool>>,
    pub options: Option<RequestOptions>,
}

impl CompletionRequest {
    pub fn add_tool(&mut self, tool: Tool) {
        if let Some(ref mut tools) = self.tools {
            tools.push(tool);
        } else {
            self.tools = Some(vec![tool]);
        }
    }

    /// Add a built-in provider tool (e.g., GoogleSearch for Gemini)
    pub fn add_built_in_tool(&mut self, tool: BuiltInTool) {
        if let Some(ref mut tools) = self.built_in_tools {
            tools.push(tool);
        } else {
            self.built_in_tools = Some(vec![tool]);
        }
    }

    /// Enable Gemini Google Search grounding (2.0+ models)
    pub fn with_google_search(mut self) -> Self {
        self.add_built_in_tool(BuiltInTool::GoogleSearch);
        self
    }

    /// Enable Gemini Google Search Retrieval (1.5 models) with optional threshold
    pub fn with_google_search_retrieval(mut self, dynamic_threshold: Option<f32>) -> Self {
        self.add_built_in_tool(BuiltInTool::GoogleSearchRetrieval { dynamic_threshold });
        self
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RequestOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
}
