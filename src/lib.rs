pub mod error;
pub mod client;
pub mod request;
pub mod response;
pub mod message;
pub mod stream;
pub mod tool;
pub mod mcp;

pub use client::{Client, ClientOptions, AnthropicConfig, OpenAiConfig};
pub use error::{Error, Result};
pub use message::{Message, Role};
pub use request::{CompletionRequest, RequestOptions};
pub use tool::{Tool, ToolUse};
pub use response::{CompletionResponse, Usage, FinishReason};
pub use stream::{CompletionStream, StreamChunk};

// Re-export main MCP types
pub use mcp::{
    McpClient,
    GatewayConfig,
    StdioConfig,
    CallToolResult,
    tool_result_to_string,
};

// Legacy gateway client (still exported for backward compatibility)
pub use mcp::{McpGateway, ListServersResponse};
