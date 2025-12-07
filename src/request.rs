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
    /// Gemini Code Execution - runs Python code server-side
    /// Supports numpy, pandas, matplotlib, sympy, etc.
    CodeExecution,
    /// Gemini URL Context - fetches and analyzes full webpage content
    UrlContext,
    /// Gemini Google Maps grounding - location-aware queries
    GoogleMaps {
        /// Enable interactive Maps widget (returns context token)
        #[serde(skip_serializing_if = "Option::is_none")]
        enable_widget: Option<bool>,
    },
}

/// Location coordinates for Google Maps queries
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LatLng {
    pub latitude: f64,
    pub longitude: f64,
}

/// Retrieval configuration for location-aware tools
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalConfig {
    /// Geographic coordinates for location-aware queries
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lat_lng: Option<LatLng>,
    /// Language code for localized results (e.g., "en_US")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language_code: Option<String>,
}

/// Tool configuration options
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolConfig {
    /// Retrieval configuration for location-aware tools
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieval_config: Option<RetrievalConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Tool>>,
    /// Provider-specific built-in tools (e.g., Google Search for Gemini)
    pub built_in_tools: Option<Vec<BuiltInTool>>,
    /// Tool configuration (e.g., location for Google Maps)
    pub tool_config: Option<ToolConfig>,
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

    /// Enable Gemini Code Execution (Python with numpy, pandas, matplotlib, etc.)
    pub fn with_code_execution(mut self) -> Self {
        self.add_built_in_tool(BuiltInTool::CodeExecution);
        self
    }

    /// Enable Gemini URL Context (fetch and analyze webpage content)
    pub fn with_url_context(mut self) -> Self {
        self.add_built_in_tool(BuiltInTool::UrlContext);
        self
    }

    /// Enable Gemini Google Maps grounding
    pub fn with_google_maps(mut self, enable_widget: Option<bool>) -> Self {
        self.add_built_in_tool(BuiltInTool::GoogleMaps { enable_widget });
        self
    }

    /// Set location for Google Maps queries
    pub fn with_location(mut self, latitude: f64, longitude: f64) -> Self {
        let retrieval_config = RetrievalConfig {
            lat_lng: Some(LatLng { latitude, longitude }),
            language_code: None,
        };
        if let Some(ref mut config) = self.tool_config {
            config.retrieval_config = Some(retrieval_config);
        } else {
            self.tool_config = Some(ToolConfig {
                retrieval_config: Some(retrieval_config),
            });
        }
        self
    }

    /// Set language code for localized results
    pub fn with_language(mut self, language_code: &str) -> Self {
        if let Some(ref mut config) = self.tool_config {
            if let Some(ref mut retrieval) = config.retrieval_config {
                retrieval.language_code = Some(language_code.to_string());
            } else {
                config.retrieval_config = Some(RetrievalConfig {
                    lat_lng: None,
                    language_code: Some(language_code.to_string()),
                });
            }
        } else {
            self.tool_config = Some(ToolConfig {
                retrieval_config: Some(RetrievalConfig {
                    lat_lng: None,
                    language_code: Some(language_code.to_string()),
                }),
            });
        }
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
