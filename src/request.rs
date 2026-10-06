use crate::{
    message::{CacheControl, Message},
    Tool,
};
use serde::{Deserialize, Serialize};

/// Built-in tools provided by AI providers (not MCP tools)
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BuiltInTool {
    // Gemini tools
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

    // OpenAI tools
    /// OpenAI Code Interpreter - runs Python in sandboxed containers ($0.03/container)
    OpenAiCodeInterpreter,
    /// OpenAI Web Search - searches the web for current information
    OpenAiWebSearch,
    /// OpenAI Image Generation - generates images via DALL-E
    OpenAiImageGeneration,
}

/// Thinking level for Gemini 3 models (controls reasoning depth)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ThinkingLevel {
    /// Close to no thinking (Gemini 3 Flash / Flash-Lite; not 3.1 Pro, 3.7/3.8 Flash)
    Minimal,
    /// Minimizes latency and cost, best for simple tasks
    Low,
    /// Balanced (the default on Gemini 3.5+ Flash)
    Medium,
    /// Maximizes reasoning depth, may take longer
    High,
}

impl ThinkingLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            ThinkingLevel::Minimal => "minimal",
            ThinkingLevel::Low => "low",
            ThinkingLevel::Medium => "medium",
            ThinkingLevel::High => "high",
        }
    }
}

/// How much work the model puts into a response, thinking included.
///
/// One scale for every provider; each maps the levels it has:
/// - Anthropic: `output_config.effort` (`low`..`max`; `none`/`minimal` send `low`)
/// - OpenAI Responses: `reasoning.effort` (`none`..`max`, passed through)
/// - Gemini 3: `thinkingConfig.thinkingLevel` when `thinking_level` is not set
///   (`none`/`minimal` -> minimal, `xhigh`/`max` -> high)
///
/// The library does not know which levels a given model accepts; the caller
/// picks a level the model supports.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Effort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl Effort {
    pub fn as_str(&self) -> &'static str {
        match self {
            Effort::None => "none",
            Effort::Minimal => "minimal",
            Effort::Low => "low",
            Effort::Medium => "medium",
            Effort::High => "high",
            Effort::XHigh => "xhigh",
            Effort::Max => "max",
        }
    }

    /// Parse the lowercase wire names used by every provider.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "none" => Some(Effort::None),
            "minimal" => Some(Effort::Minimal),
            "low" => Some(Effort::Low),
            "medium" => Some(Effort::Medium),
            "high" => Some(Effort::High),
            "xhigh" => Some(Effort::XHigh),
            "max" => Some(Effort::Max),
            _ => None,
        }
    }

    /// Gemini 3 thinking level for this effort.
    pub fn to_thinking_level(self) -> ThinkingLevel {
        match self {
            Effort::None | Effort::Minimal => ThinkingLevel::Minimal,
            Effort::Low => ThinkingLevel::Low,
            Effort::Medium => ThinkingLevel::Medium,
            Effort::High | Effort::XHigh | Effort::Max => ThinkingLevel::High,
        }
    }
}

/// Whether and how the model thinks before answering.
///
/// - Anthropic: the `thinking` parameter (`adaptive`, `enabled` + `budget_tokens`,
///   `between_tools`, `disabled`). Which values a model accepts varies by model.
/// - Gemini: `Budget` -> `thinkingBudget`, `Disabled` -> `thinkingBudget: 0`
///   (Gemini 2.5 only; Gemini 3 cannot fully disable thinking). `Adaptive`
///   leaves the model's dynamic default.
/// - OpenAI: `Disabled` -> `reasoning.effort: none` when `effort` is unset.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Thinking {
    Adaptive,
    Budget {
        tokens: u32,
    },
    /// Anthropic Claude Sonnet 5.5: no up-front thinking, progress notes between tools.
    BetweenTools,
    Disabled,
}

/// Media resolution for Gemini 3 vision (controls token usage vs detail)
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MediaResolution {
    /// Lowest token usage (~280 tokens/image)
    Low,
    /// Balanced (~560 tokens/image)
    Medium,
    /// Higher detail (~1120 tokens/image)
    High,
    /// Maximum detail (highest token usage)
    UltraHigh,
}

/// Configuration for Anthropic Citations API
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CitationConfig {
    /// Enable citations in responses (requires source documents in messages)
    pub enabled: bool,
}

/// Configuration for OpenAI native MCP server support
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// URL of the remote MCP server
    pub server_url: String,
    /// Label for the server (displayed to users)
    pub server_label: String,
    /// Whether to require approval before tool execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub require_approval: Option<McpApprovalMode>,
}

/// Approval mode for MCP tool execution
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum McpApprovalMode {
    /// Always require approval before execution
    Always,
    /// Never require approval (auto-execute)
    Never,
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
            lat_lng: Some(LatLng {
                latitude,
                longitude,
            }),
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

    /// Enable Anthropic Citations API for source document attribution
    pub fn with_citations(mut self) -> Self {
        let options = self.options.get_or_insert_with(RequestOptions::default);
        options.citations = Some(CitationConfig { enabled: true });
        self
    }

    /// Set Gemini 3 thinking level (Low = fast/cheap, High = deep reasoning)
    pub fn with_thinking_level(mut self, level: ThinkingLevel) -> Self {
        let options = self.options.get_or_insert_with(RequestOptions::default);
        options.thinking_level = Some(level);
        self
    }

    /// Set Gemini 3 media resolution for vision (controls token usage vs detail)
    pub fn with_media_resolution(mut self, resolution: MediaResolution) -> Self {
        let options = self.options.get_or_insert_with(RequestOptions::default);
        options.media_resolution = Some(resolution);
        self
    }

    /// Add an OpenAI native MCP server
    pub fn with_mcp_server(
        mut self,
        url: &str,
        label: &str,
        require_approval: Option<McpApprovalMode>,
    ) -> Self {
        let options = self.options.get_or_insert_with(RequestOptions::default);
        let servers = options.mcp_servers.get_or_insert_with(Vec::new);
        servers.push(McpServerConfig {
            server_url: url.to_string(),
            server_label: label.to_string(),
            require_approval,
        });
        self
    }

    /// Enable OpenAI Code Interpreter
    pub fn with_openai_code_interpreter(mut self) -> Self {
        self.add_built_in_tool(BuiltInTool::OpenAiCodeInterpreter);
        self
    }

    /// Enable OpenAI Web Search
    pub fn with_openai_web_search(mut self) -> Self {
        self.add_built_in_tool(BuiltInTool::OpenAiWebSearch);
        self
    }

    /// Enable OpenAI Image Generation
    pub fn with_openai_image_generation(mut self) -> Self {
        self.add_built_in_tool(BuiltInTool::OpenAiImageGeneration);
        self
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RequestOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,

    /// Enable Anthropic Citations API for source document attribution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<CitationConfig>,

    /// Thinking level for Gemini 3 models. Takes precedence over `effort` there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,

    /// Whether and how the model thinks (see [`Thinking`]). `None` = the
    /// model's own default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking: Option<Thinking>,

    /// How much work the model puts in (see [`Effort`]). `None` = the model's
    /// own default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effort: Option<Effort>,

    /// Media resolution for Gemini 3 vision (controls token usage vs detail)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_resolution: Option<MediaResolution>,

    /// OpenAI native MCP server configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_servers: Option<Vec<McpServerConfig>>,

    /// Where to place prompt-cache breakpoints beyond the ones messages
    /// carry themselves (see [`PromptCache`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache: Option<PromptCache>,
}

/// Request-level prompt-cache breakpoints.
///
/// Anthropic caches explicitly: a breakpoint writes the prefix up to it, and a
/// later request reads it when its prefix is byte-identical up to an earlier
/// write (render order tools -> system -> messages, at most 4 breakpoints,
/// 1-hour entries before 5-minute ones). System and conversation messages add
/// their own breakpoints with [`Message::with_cache`](crate::Message::with_cache) /
/// [`Message::with_extended_cache`](crate::Message::with_extended_cache).
/// OpenAI and Gemini cache stable prefixes automatically and ignore the
/// breakpoints; OpenAI uses [`key`](PromptCache::key) to route requests that
/// share a prefix to the same cache.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromptCache {
    /// Breakpoint on the last tool definition: the tool list is the start of
    /// the prompt and usually the largest stable part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<CacheControl>,
    /// Breakpoint on the last block of the last message. Each call in a tool
    /// loop then reads everything the previous call wrote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<CacheControl>,
    /// OpenAI `prompt_cache_key`: requests with the same key and prefix land
    /// on the same cache (a conversation or agent id works well).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}
