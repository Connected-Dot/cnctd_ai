use crate::{Error, Result};
use reqwest::Client as HttpClient;
use rmcp::model::{CallToolResult, Tool};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::process::Stdio;

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

/// Information about an MCP server
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub available_tools: Vec<Tool>,
}

/// Response from gateway's /list endpoint
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ListServersResponse {
    servers: Vec<ServerInfoGateway>,
}

/// Gateway-specific server info (includes URL)
#[derive(Clone, Debug, Serialize, Deserialize)]
struct ServerInfoGateway {
    name: String,
    url: String,
    description: Option<String>,
    #[serde(default)]
    available_tools: Vec<Tool>,
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

/// Unified MCP client supporting both gateway and stdio transports
#[derive(Debug)]
pub enum McpClient {
    /// Gateway transport - communicates via HTTP with an MCP gateway
    Gateway {
        config: GatewayConfig,
        http_client: HttpClient,
    },
    /// Stdio transport - spawns and communicates with a local MCP server
    Stdio {
        client: rmcp::Client,
        config: StdioConfig,
    },
}

impl McpClient {
    /// Create a new MCP client using gateway transport
    ///
    /// # Arguments
    /// * `config` - Gateway configuration including base URL, server name, and optional auth
    ///
    /// # Example
    /// ```no_run
    /// use cnctd_ai::mcp::{McpClient, GatewayConfig};
    ///
    /// let config = GatewayConfig {
    ///     base_url: "https://mcp.cnctd.world".to_string(),
    ///     server_name: "github".to_string(),
    ///     auth_token: Some("your-token".to_string()),
    /// };
    ///
    /// let client = McpClient::from_gateway(config);
    /// ```
    pub fn from_gateway(config: GatewayConfig) -> Self {
        Self::Gateway {
            config,
            http_client: HttpClient::new(),
        }
    }

    /// Create a new MCP client using stdio transport
    ///
    /// This spawns a child process and communicates via stdin/stdout using the MCP protocol.
    ///
    /// # Arguments
    /// * `config` - Stdio configuration including command, args, and optional environment
    ///
    /// # Example
    /// ```no_run
    /// use cnctd_ai::mcp::{McpClient, StdioConfig};
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let config = StdioConfig {
    ///     command: "npx".to_string(),
    ///     args: vec!["-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
    ///         .into_iter()
    ///         .map(String::from)
    ///         .collect(),
    ///     env: None,
    /// };
    ///
    /// let client = McpClient::from_stdio(config).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn from_stdio(config: StdioConfig) -> Result<Self> {
        // Build the command
        let mut command = tokio::process::Command::new(&config.command);
        command.args(&config.args);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::inherit());

        // Add environment variables if provided
        if let Some(env_vars) = &config.env {
            for (key, value) in env_vars {
                command.env(key, value);
            }
        }

        // Spawn the process and create the MCP client
        let child = command
            .spawn()
            .map_err(|e| Error::Other(format!("Failed to spawn MCP server process: {}", e)))?;

        let transport = rmcp::transport::ChildProcess::new(child)
            .map_err(|e| Error::Other(format!("Failed to create child process transport: {}", e)))?;

        let client = rmcp::Client::new(transport)
            .await
            .map_err(|e| Error::Other(format!("Failed to initialize MCP client: {}", e)))?;

        Ok(Self::Stdio {
            client,
            config,
        })
    }

    /// List all tools available from this MCP server
    ///
    /// # Returns
    /// A vector of MCP tools with their schemas
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        match self {
            Self::Gateway { config, http_client } => {
                self.list_tools_gateway(config, http_client).await
            }
            Self::Stdio { client, .. } => {
                self.list_tools_stdio(client).await
            }
        }
    }

    /// Call a tool with the given arguments
    ///
    /// # Arguments
    /// * `tool_name` - Name of the tool to call
    /// * `arguments` - Tool arguments as a JSON object (or null for no arguments)
    ///
    /// # Returns
    /// The result from the tool execution
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        match self {
            Self::Gateway { config, http_client } => {
                self.call_tool_gateway(config, http_client, tool_name, arguments).await
            }
            Self::Stdio { client, .. } => {
                self.call_tool_stdio(client, tool_name, arguments).await
            }
        }
    }

    /// Get information about this MCP server
    ///
    /// For gateway clients, this queries the gateway for server info.
    /// For stdio clients, this returns basic info from the configuration.
    pub async fn server_info(&self) -> Result<ServerInfo> {
        match self {
            Self::Gateway { config, http_client } => {
                self.server_info_gateway(config, http_client).await
            }
            Self::Stdio { config, .. } => {
                // For stdio, we don't have a description available
                // The caller can query list_tools() to get available tools
                Ok(ServerInfo {
                    name: config.command.clone(),
                    description: None,
                    available_tools: vec![],
                })
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
        let server = list_response
            .servers
            .into_iter()
            .find(|s| s.name == config.server_name)
            .ok_or_else(|| {
                Error::Other(format!("Server '{}' not found in gateway", config.server_name))
            })?;

        Ok(ServerInfo {
            name: server.name,
            description: server.description,
            available_tools: server.available_tools,
        })
    }

    // ==================== Stdio Implementation ====================

    async fn list_tools_stdio(&self, client: &rmcp::Client) -> Result<Vec<Tool>> {
        let tools = client
            .list_tools()
            .await
            .map_err(|e| Error::Other(format!("Failed to list tools via stdio: {}", e)))?;

        Ok(tools.tools)
    }

    async fn call_tool_stdio(
        &self,
        client: &rmcp::Client,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        let args = arguments.unwrap_or(serde_json::json!({}));

        let result = client
            .call_tool(tool_name, args)
            .await
            .map_err(|e| Error::ToolExecution(format!("Tool execution failed: {}", e)))?;

        Ok(result)
    }
}
