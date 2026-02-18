use crate::{Error, Result};
use crate::mcp::ServerInfo;
use reqwest::Client as HttpClient;
use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::{RoleClient, RunningService, ServiceExt};
use rmcp::transport::{ConfigureCommandExt, StreamableHttpClientTransport, TokioChildProcess};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;
use std::sync::Arc;
use tokio::process::Command;

/// JSON-RPC 2.0 request structure
#[derive(Clone, Debug, Serialize, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    params: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
}

/// JSON-RPC 2.0 response structure
#[derive(Clone, Debug, Serialize, Deserialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

/// JSON-RPC 2.0 error structure
#[derive(Clone, Debug, Serialize, Deserialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

/// Response from gateway's /list endpoint
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ListServersResponse {
    servers: Vec<ServerInfo>,
}

/// Configuration for connecting to an MCP server via gateway
#[derive(Clone, Debug)]
pub struct GatewayConfig {
    /// Base URL of the gateway (e.g., "https://mcp.cnctd.world")
    pub base_url: String,
    /// Server name within the gateway (e.g., "github", "brave-search")
    pub server_name: String,
    /// Optional bearer token for authentication
    pub auth_token: Option<String>,
}

/// Configuration for connecting to an MCP server via stdio
#[derive(Clone, Debug)]
pub struct StdioConfig {
    /// Command to execute (e.g., "npx", "node", "/path/to/binary")
    pub command: String,
    /// Arguments to pass to the command
    pub args: Vec<String>,
    /// Optional environment variables
    pub env: Option<Vec<(String, String)>>,
}

/// Stdio MCP client wrapping the rmcp service
pub struct StdioClient {
    service: RunningService<RoleClient, ()>,
    config: StdioConfig,
}

impl StdioClient {
    /// List all tools available from this MCP server
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let result = self.service
            .list_tools(None)
            .await
            .map_err(|e| Error::Other(format!("Failed to list tools via stdio: {}", e)))?;

        Ok(result.tools)
    }

    /// Call a tool with the given arguments
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        // Convert Value to Map<String, Value>
        let args_map = match arguments {
            Some(Value::Object(map)) => Some(map),
            Some(other) => {
                return Err(Error::Other(format!(
                    "Tool arguments must be a JSON object, got: {:?}",
                    other
                )));
            }
            None => None,
        };
        let result = self.service
            .call_tool(CallToolRequestParams {
                meta: None,
                name: Cow::Owned(tool_name.to_string()),
                arguments: args_map,
                task: None,
            })
            .await
            .map_err(|e| Error::ToolExecution(format!("Tool execution failed: {}", e)))?;

        Ok(result)
    }

    /// Get server info
    pub async fn server_info(&self) -> Result<ServerInfo> {
        let tools = self.list_tools().await?;
        Ok(ServerInfo::stdio(self.config.command.clone(), tools))
    }
}

/// Streamable HTTP MCP client wrapping the rmcp service
pub struct StreamableHttpClient {
    service: RunningService<RoleClient, ()>,
    url: String,
}

impl StreamableHttpClient {
    /// List all tools available from this MCP server
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        let result = self.service
            .list_tools(None)
            .await
            .map_err(|e| Error::Other(format!("Failed to list tools via streamable HTTP: {}", e)))?;

        Ok(result.tools)
    }

    /// Call a tool with the given arguments
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        let args_map = match arguments {
            Some(Value::Object(map)) => Some(map),
            Some(other) => {
                return Err(Error::Other(format!(
                    "Tool arguments must be a JSON object, got: {:?}",
                    other
                )));
            }
            None => None,
        };
        let result = self.service
            .call_tool(CallToolRequestParams {
                meta: None,
                name: Cow::Owned(tool_name.to_string()),
                arguments: args_map,
                task: None,
            })
            .await
            .map_err(|e| Error::ToolExecution(format!("Tool execution failed: {}", e)))?;

        Ok(result)
    }

    /// Get server info
    pub async fn server_info(&self) -> Result<ServerInfo> {
        let tools = self.list_tools().await?;
        Ok(ServerInfo {
            name: self.url.clone(),
            url: Some(self.url.clone()),
            description: None,
            available_tools: tools,
        })
    }
}

/// Unified MCP client supporting gateway, stdio, and streamable HTTP transports
pub enum McpClient {
    /// Gateway transport - communicates via HTTP with an MCP gateway
    Gateway {
        config: GatewayConfig,
        http_client: HttpClient,
    },
    /// Stdio transport - spawns and communicates with a local MCP server
    Stdio(StdioClient),
    /// Streamable HTTP transport - connects to an MCP server via HTTP streaming
    StreamableHttp(StreamableHttpClient),
}

impl McpClient {
    /// Create a new MCP client using gateway transport
    pub fn from_gateway(config: GatewayConfig) -> Self {
        Self::Gateway {
            config,
            http_client: HttpClient::new(),
        }
    }

    /// Create a new MCP client using streamable HTTP transport
    pub async fn from_streamable_http(url: &str) -> Result<Self> {
        let transport = StreamableHttpClientTransport::from_uri(Arc::<str>::from(url));

        let service = ().serve(transport)
            .await
            .map_err(|e| Error::Other(format!("Failed to initialize streamable HTTP MCP service: {}", e)))?;

        Ok(Self::StreamableHttp(StreamableHttpClient {
            service,
            url: url.to_string(),
        }))
    }

    /// Create a new MCP client using stdio transport
    pub async fn from_stdio(config: StdioConfig) -> Result<Self> {
        // Build the command
        let mut command = Command::new(&config.command);
        
        // Add arguments
        for arg in &config.args {
            command.arg(arg);
        }

        // Add environment variables if provided
        if let Some(env_vars) = &config.env {
            for (key, value) in env_vars {
                command.env(key, value);
            }
        }

        // Create transport and service using rmcp's API
        let transport = TokioChildProcess::new(command.configure(|_| {}))
            .map_err(|e| Error::Other(format!("Failed to create child process transport: {}", e)))?;

        let service = ().serve(transport)
            .await
            .map_err(|e| Error::Other(format!("Failed to initialize MCP service: {}", e)))?;

        Ok(Self::Stdio(StdioClient {
            service,
            config,
        }))
    }

    /// List all tools available from this MCP server
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        match self {
            Self::Gateway { config, http_client } => {
                self.list_tools_gateway(config, http_client).await
            }
            Self::Stdio(client) => {
                client.list_tools().await
            }
            Self::StreamableHttp(client) => {
                client.list_tools().await
            }
        }
    }

    /// Call a tool with the given arguments
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        match self {
            Self::Gateway { config, http_client } => {
                self.call_tool_gateway(config, http_client, tool_name, arguments).await
            }
            Self::Stdio(client) => {
                client.call_tool(tool_name, arguments).await
            }
            Self::StreamableHttp(client) => {
                client.call_tool(tool_name, arguments).await
            }
        }
    }

    /// Get information about this MCP server
    pub async fn server_info(&self) -> Result<ServerInfo> {
        match self {
            Self::Gateway { config, http_client } => {
                self.server_info_gateway(config, http_client).await
            }
            Self::Stdio(client) => {
                client.server_info().await
            }
            Self::StreamableHttp(client) => {
                client.server_info().await
            }
        }
    }

    // ==================== Gateway Implementation ====================

    async fn list_tools_gateway(
        &self,
        config: &GatewayConfig,
        http_client: &HttpClient,
    ) -> Result<Vec<Tool>> {
        let url = format!("{}/service/{}/tools", config.base_url, config.server_name);

        let mut request = http_client.get(&url);
        if let Some(token) = &config.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::Network(format!("Failed to list tools: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "Gateway returned error status: {}",
                response.status()
            )));
        }

        let tools: Vec<Tool> = response
            .json()
            .await
            .map_err(|e| Error::Parse(format!("Failed to parse tools: {}", e)))?;

        Ok(tools)
    }

    async fn call_tool_gateway(
        &self,
        config: &GatewayConfig,
        http_client: &HttpClient,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        let url = format!("{}/service/{}", config.base_url, config.server_name);

        // Create JSON-RPC request with unique ID
        let request_id = uuid::Uuid::new_v4().to_string();
        let rpc_request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/call".to_string(),
            params: Some(serde_json::json!({
                "name": tool_name,
                "arguments": arguments.unwrap_or(serde_json::json!({}))
            })),
            id: Some(Value::String(request_id)),
        };

        let mut http_request = http_client.post(&url).json(&rpc_request);
        if let Some(token) = &config.auth_token {
            http_request = http_request.header("Authorization", format!("Bearer {}", token));
        }

        let response = http_request
            .send()
            .await
            .map_err(|e| Error::Network(format!("Failed to call tool: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "Gateway returned error status: {}",
                response.status()
            )));
        }

        let json_rpc_response: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| Error::Parse(format!("Failed to parse response: {}", e)))?;

        // Handle JSON-RPC errors
        if let Some(error) = json_rpc_response.error {
            return Err(Error::ToolExecution(format!(
                "Tool execution failed: {} (code: {})",
                error.message, error.code
            )));
        }

        // Parse the result as CallToolResult
        let result = json_rpc_response
            .result
            .ok_or_else(|| Error::Parse("No result in JSON-RPC response".to_string()))?;

        serde_json::from_value(result)
            .map_err(|e| Error::Parse(format!("Failed to parse CallToolResult: {}", e)))
    }

    async fn server_info_gateway(
        &self,
        config: &GatewayConfig,
        http_client: &HttpClient,
    ) -> Result<ServerInfo> {
        let url = format!("{}/list", config.base_url);

        let mut request = http_client.get(&url);
        if let Some(token) = &config.auth_token {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request
            .send()
            .await
            .map_err(|e| Error::Network(format!("Failed to list servers: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "Gateway returned error status: {}",
                response.status()
            )));
        }

        let list_response: ListServersResponse = response
            .json()
            .await
            .map_err(|e| Error::Parse(format!("Failed to parse server list: {}", e)))?;

        // Find our server in the list
        list_response
            .servers
            .into_iter()
            .find(|s| s.name == config.server_name)
            .ok_or_else(|| {
                Error::Other(format!("Server '{}' not found in gateway", config.server_name))
            })
    }
}
