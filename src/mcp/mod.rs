use serde::{Deserialize, Serialize};

pub mod gateway;
pub mod server;
pub mod requests;

// Shared auth type used across cnctd ecosystem
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    Bearer(String),
    None,
}

// Re-export rmcp protocol types for shared use across cnctd_ai and mcp-gateway
pub use rmcp::model::{
    // Core protocol types
    Tool,
    CallToolResult,
    CallToolRequestParam,
    Content,
    
    // Request/Response types
    ListToolsResult,
    ListToolsRequestParam,
    
    // Resource types
    Resource,
    ResourceContents,
    ListResourcesResult,
    ReadResourceResult,
    
    // Common types
    TextContent,
    ImageContent,
    EmbeddedResource,
};

// Re-export main types from submodules for convenience
pub use gateway::{McpGateway, GatewayInfo, McpServerInfo};
pub use server::{McpServer, McpConnection, ConnectionType};
