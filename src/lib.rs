pub mod error;
pub mod client;
pub mod request;
pub mod response;
pub mod message;
pub mod stream;
pub mod tool;
pub mod mcp;
pub mod tool_helpers;
pub mod agent;
pub mod batch;
pub mod embeddings;
pub mod transcription;
pub mod video;
pub mod image_gen;
pub mod tts;

pub use client::{Client, ClientOptions, AnthropicConfig, OpenAiConfig, GeminiConfig};
pub use error::{Error, Result};
pub use message::{Message, Role, ToolResult, ImageContent, ContentPart};
pub use request::{
    CompletionRequest, RequestOptions, BuiltInTool,
    LatLng, RetrievalConfig, ToolConfig,
};
pub use tool::ToolUse;
pub use response::{
    CompletionResponse, Usage, FinishReason,
    GroundingMetadata, GroundingChunk, GroundingSupport, WebChunk, SearchEntryPoint,
    CodeExecutionResult, CodeExecutionOutcome,
};
pub use stream::{CompletionStream, StreamChunk};
pub use tool_helpers::{create_tool, create_tool_borrowed};

// Re-export MCP types
pub use mcp::{
    McpClient,
    GatewayConfig,
    StdioConfig,
    ServerInfo,
    CallToolResult,
    tool_result_to_string,
};

// Re-export agent types
pub use agent::{
    Agent,
    AgentConfig,
    AgentConfigBuilder,
    AgentTrace,
    TraceEvent,
    ToolExecution,
    AgentState,
    StopReason,
};

// Re-export batch types
pub use batch::{
    BatchItem,
    BatchInfo,
    BatchStatus,
    BatchCounts,
    BatchResult,
    BatchResultType,
    BatchItemError,
    BatchAwaitOptions,
};

// Re-export embedding types
pub use embeddings::{
    EmbeddingRequest,
    EmbeddingInput,
    EmbeddingResponse,
    Embedding,
    EmbeddingUsage,
    embed_small,
    embed_large,
};

// Re-export transcription types
pub use transcription::{
    TranscriptionRequest,
    TranscriptionResponse,
    TranscriptSegment,
    AudioInput,
    Speaker,
};

// Re-export video analysis types
pub use video::{
    VideoInput,
    VideoContent,
    VideoAnalysisRequest,
    VideoAnalysisResponse,
};

// Re-export image generation types
pub use image_gen::{
    ImageGenerationRequest,
    ImageGenerationResponse,
    GeneratedImage,
    AspectRatio,
    ImageQuality,
    ImageFormat,
};

// Re-export TTS types
pub use tts::{
    SpeechRequest,
    SpeechResponse,
    Voice,
    AudioFormat,
};

// Re-export Tool from rmcp (replaces custom Tool struct)
pub use rmcp::model::Tool;

// Legacy gateway client (still exported for backward compatibility)
pub use mcp::{McpGateway, ListServersResponse};
