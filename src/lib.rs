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

pub use client::{Client, ClientOptions, AnthropicConfig, OpenAiConfig};
pub use error::{Error, Result};
pub use message::{Message, Role};
pub use request::{CompletionRequest, RequestOptions};
pub use tool::ToolUse;
pub use response::{CompletionResponse, Usage, FinishReason};
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

// Re-export Tool from rmcp (replaces custom Tool struct)
pub use rmcp::model::Tool;

// Legacy gateway client (still exported for backward compatibility)
pub use mcp::{McpGateway, ListServersResponse};
