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

pub use client::{McpClient, GatewayConfig, StdioConfig, StreamableHttpClient};
pub use gateway::{McpGateway, ListServersResponse};
pub use util::tool_result_to_string;

// Re-export rmcp types that consumers will need
pub use rmcp::model::{CallToolResult, Tool};

use serde::{Deserialize, Serialize};

/// Information about an MCP server
/// 
/// This consolidated structure is used for both stdio and gateway transports.
/// The `url` field is optional and only populated for gateway servers.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Server name/identifier
    pub name: String,
    /// Optional server URL (only for gateway servers)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Optional server description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Tools available from this server
    #[serde(default)]
    pub available_tools: Vec<Tool>,
}

impl ServerInfo {
    /// Create server info for a stdio server (no URL)
    pub fn stdio(name: String, available_tools: Vec<Tool>) -> Self {
        Self {
            name,
            url: None,
            description: None,
            available_tools,
        }
    }

    /// Create server info for a gateway server (with URL)
    pub fn gateway(
        name: String,
        url: String,
        description: Option<String>,
        available_tools: Vec<Tool>,
    ) -> Self {
        Self {
            name,
            url: Some(url),
            description,
            available_tools,
        }
    }
}
