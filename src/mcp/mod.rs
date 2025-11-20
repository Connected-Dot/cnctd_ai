//! MCP (Model Context Protocol) integration
//!
//! This module provides a unified client for interacting with MCP servers using
//! either HTTP gateway transport or stdio (child process) transport.
//!
//! # Transports
//!
//! ## Gateway Transport
//! Communicates with MCP servers through an HTTP gateway that proxies multiple
//! MCP servers. Useful for remote servers or when you have a centralized gateway.
//!
//! ## Stdio Transport
//! Spawns and communicates directly with an MCP server as a child process using
//! stdin/stdout. This is the standard MCP transport used by apps like Claude Desktop.
//!
//! # Example
//!
//! ```no_run
//! use cnctd_ai::mcp::{McpClient, GatewayConfig, StdioConfig};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Gateway transport
//! let gateway = McpClient::from_gateway(GatewayConfig {
//!     base_url: "https://mcp.cnctd.world".to_string(),
//!     server_name: "github".to_string(),
//!     auth_token: Some("token".to_string()),
//! });
//!
//! // Stdio transport
//! let stdio = McpClient::from_stdio(StdioConfig {
//!     command: "npx".to_string(),
//!     args: vec!["-y".to_string(), "@modelcontextprotocol/server-filesystem".to_string()],
//!     env: None,
//! }).await?;
//!
//! // Both clients have the same interface
//! let tools = gateway.list_tools().await?;
//! let result = stdio.call_tool("read_file", Some(serde_json::json!({
//!     "path": "/tmp/test.txt"
//! }))).await?;
//! # Ok(())
//! # }
//! ```

mod client;
mod gateway;
mod util;

pub use client::{McpClient, GatewayConfig, StdioConfig, ServerInfo};
pub use gateway::{McpGateway, ListServersResponse};
pub use util::tool_result_to_string;

// Re-export rmcp types that consumers will need
pub use rmcp::model::{CallToolResult, Tool};
