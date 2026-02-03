use serde::{Deserialize, Serialize};
use crate::{ToolUse, message::Message};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub message: Message,
    pub usage: Usage,
    pub finish_reason: FinishReason,
    pub model: String,
    #[serde(skip)]
    pub tool_uses: Option<Vec<ToolUse>>,
    /// Grounding metadata from search-enabled responses (Gemini)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_metadata: Option<GroundingMetadata>,
    /// Code execution results from Gemini
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code_execution_results: Option<Vec<CodeExecutionResult>>,
    /// Google Maps widget context token for rendering interactive widgets
    #[serde(skip_serializing_if = "Option::is_none")]
    pub google_maps_widget_token: Option<String>,
    /// Reasoning items that must be echoed back in continuation requests (GPT-5.2-pro)
    /// Stored as raw JSON values to preserve exact format
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_items: Option<Vec<serde_json::Value>>,
    /// Natural-language reasoning summary (OpenAI o-series models, free feature)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_summary: Option<String>,
    /// Citations from source documents (Anthropic Citations API)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citations: Option<Vec<Citation>>,
}

impl CompletionResponse {
    /// Convenience method to get the text content
    pub fn text(&self) -> &str {
        &self.message.content
    }

    pub fn tool_use(&self) -> Option<&ToolUse> {
        self.tool_uses.as_ref()?.first()
    }

    /// Check if response was grounded with search results
    pub fn is_grounded(&self) -> bool {
        // Check for actual search data, not just metadata presence
        self.grounding_metadata.as_ref()
            .map(|m| m.web_search_queries.is_some() || m.grounding_chunks.is_some())
            .unwrap_or(false)
    }

    /// Get search queries used for grounding (if any)
    pub fn search_queries(&self) -> Option<&Vec<String>> {
        self.grounding_metadata.as_ref()?.web_search_queries.as_ref()
    }

    /// Get grounding sources/citations (if any)
    pub fn sources(&self) -> Option<&Vec<GroundingChunk>> {
        self.grounding_metadata.as_ref()?.grounding_chunks.as_ref()
    }

    /// Check if response contains code execution results
    pub fn has_code_execution(&self) -> bool {
        self.code_execution_results.is_some()
    }

    /// Get code execution results (if any)
    pub fn code_results(&self) -> Option<&Vec<CodeExecutionResult>> {
        self.code_execution_results.as_ref()
    }

    /// Check if response has a Google Maps widget token
    pub fn has_maps_widget(&self) -> bool {
        self.google_maps_widget_token.is_some()
    }

    /// Check if response contains citations (Anthropic Citations API)
    pub fn has_citations(&self) -> bool {
        self.citations.as_ref().map(|c| !c.is_empty()).unwrap_or(false)
    }

    /// Get citations from the response (Anthropic Citations API)
    pub fn get_citations(&self) -> Option<&Vec<Citation>> {
        self.citations.as_ref()
    }

    /// Get the reasoning summary (OpenAI o-series models)
    pub fn get_reasoning_summary(&self) -> Option<&str> {
        self.reasoning_summary.as_deref()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    /// Tokens written to cache (Anthropic prompt caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_tokens: Option<u32>,
    /// Tokens read from cache (Anthropic prompt caching)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_tokens: Option<u32>,
}

impl Usage {
    /// Check if any caching was used in this request
    pub fn used_cache(&self) -> bool {
        self.cache_creation_tokens.is_some() || self.cache_read_tokens.is_some()
    }

    /// Get the effective prompt tokens (non-cached portion)
    /// Returns prompt_tokens minus cache_read_tokens if available
    pub fn effective_prompt_tokens(&self) -> u32 {
        self.prompt_tokens.saturating_sub(self.cache_read_tokens.unwrap_or(0))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    ToolUse,
    #[serde(other)]
    Other,
}

/// Metadata about search grounding from Gemini
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingMetadata {
    /// Search queries that were executed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web_search_queries: Option<Vec<String>>,
    /// Search entry point with HTML/CSS for rendering
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search_entry_point: Option<SearchEntryPoint>,
    /// Source chunks used for grounding
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_chunks: Option<Vec<GroundingChunk>>,
    /// Mapping of response text to source chunks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_supports: Option<Vec<GroundingSupport>>,
}

/// Search entry point for rendering search suggestions
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchEntryPoint {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rendered_content: Option<String>,
}

/// A source chunk from web search
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GroundingChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub web: Option<WebChunk>,
}

/// Web source information
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WebChunk {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Links response text segments to source chunks
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundingSupport {
    /// Start index in response text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_index: Option<u32>,
    /// End index in response text
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_index: Option<u32>,
    /// Indices into grounding_chunks array
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grounding_chunk_indices: Option<Vec<u32>>,
    /// Confidence scores for each chunk
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence_scores: Option<Vec<f32>>,
}

/// Code execution result from Gemini
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodeExecutionResult {
    /// The executed code
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Programming language (usually "PYTHON")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    /// Execution outcome
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<CodeExecutionOutcome>,
    /// Output from code execution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// Outcome of code execution
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodeExecutionOutcome {
    OutcomeOk,
    OutcomeFailed,
    OutcomeDeadlineExceeded,
    #[serde(other)]
    OutcomeUnspecified,
}

/// A citation from a source document (Anthropic Citations API)
/// Provides precise references to the exact sentences/passages used to generate a response
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Citation {
    /// The exact text that was cited from the source document
    pub cited_text: String,
    /// Index of the document this citation references (0-based)
    pub document_index: usize,
    /// Title of the source document, if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_title: Option<String>,
    /// Start character index in the source document
    pub start_char_index: u32,
    /// End character index in the source document
    pub end_char_index: u32,
}
