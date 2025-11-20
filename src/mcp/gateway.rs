use crate::{Error, Result};
use crate::mcp::ServerInfo;
use reqwest::Client;
use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Response from the gateway's /list endpoint
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ListServersResponse {
    pub servers: Vec<ServerInfo>,
}

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

/// Legacy client for interacting with an MCP gateway
/// 
/// This is the original gateway client implementation. For new code, consider using
/// the unified `McpClient` which supports both gateway and stdio transports with
/// a consistent interface.
///
/// The gateway acts as a proxy to multiple MCP servers, exposing them via HTTP.
/// This client provides methods to discover servers, list tools, and execute tools.
#[derive(Clone, Debug)]
pub struct McpGateway {
    base_url: String,
    client: Client,
    auth_header: Option<String>,
}

impl McpGateway {
    /// Create a new MCP gateway client
    /// 
    /// # Arguments
    /// * `base_url` - Base URL of the gateway (e.g., "https://api.cnctd.world")
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::new(),
            auth_header: None,
        }
    }

    /// Create a new MCP gateway client with authentication
    /// 
    /// # Arguments
    /// * `base_url` - Base URL of the gateway
    /// * `auth_token` - Bearer token for authentication
    pub fn with_auth(base_url: impl Into<String>, auth_token: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: Client::new(),
            auth_header: Some(format!("Bearer {}", auth_token.into())),
        }
    }

    /// List all available MCP servers from the gateway
    pub async fn list_servers(&self) -> Result<Vec<ServerInfo>> {
        let url = format!("{}/list", self.base_url);
        
        let mut request = self.client.get(&url);
        if let Some(auth) = &self.auth_header {
            request = request.header("Authorization", auth);
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
        
        Ok(list_response.servers)
    }

    /// List tools available from a specific server
    /// 
    /// # Arguments
    /// * `server_name` - Name of the server to query (e.g., "github", "brave-search")
    pub async fn list_tools(&self, server_name: &str) -> Result<Vec<rmcp::model::Tool>> {
        let url = format!("{}/service/{}/tools", self.base_url, server_name);
        
        let mut request = self.client.get(&url);
        if let Some(auth) = &self.auth_header {
            request = request.header("Authorization", auth);
        }
        
        let response = request
            .send()
            .await
            .map_err(|e| Error::Network(format!("Failed to list tools for {}: {}", server_name, e)))?;
        
        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "Gateway returned error status for {}: {}",
                server_name,
                response.status()
            )));
        }
        
        let tools: Vec<rmcp::model::Tool> = response
            .json()
            .await
            .map_err(|e| Error::Parse(format!("Failed to parse tools for {}: {}", server_name, e)))?;
        
        Ok(tools)
    }

    /// Call a tool on a specific server
    /// 
    /// # Arguments
    /// * `server_name` - Name of the server hosting the tool
    /// * `tool_name` - Name of the tool to call
    /// * `arguments` - Tool arguments as a JSON object
    pub async fn call_tool(
        &self,
        server_name: &str,
        tool_name: &str,
        arguments: Option<Value>,
    ) -> Result<CallToolResult> {
        let url = format!("{}/service/{}", self.base_url, server_name);
        
        // Create JSON-RPC request
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: "tools/call".to_string(),
            params: Some(json!({
                "name": tool_name,
                "arguments": arguments.unwrap_or(json!({}))
            })),
            id: Some(json!(chrono::Utc::now().timestamp_millis())),
        };
        
        let mut http_request = self.client.post(&url).json(&request);
        if let Some(auth) = &self.auth_header {
            http_request = http_request.header("Authorization", auth);
        }
        
        let response = http_request
            .send()
            .await
            .map_err(|e| Error::Network(format!("Failed to call tool {}: {}", tool_name, e)))?;
        
        if !response.status().is_success() {
            return Err(Error::Network(format!(
                "Gateway returned error status for tool call: {}",
                response.status()
            )));
        }
        
        let json_rpc_response: JsonRpcResponse = response
            .json()
            .await
            .map_err(|e| Error::Parse(format!("Failed to parse tool response: {}", e)))?;
        
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

    /// Send a raw JSON-RPC request to a server
    /// 
    /// This is a lower-level method for custom interactions with the gateway.
    pub async fn send_request(
        &self,
        server_name: &str,
        method: &str,
        params: Option<Value>,
    ) -> Result<Value> {
        let url = format!("{}/service/{}", self.base_url, server_name);
        
        let request = JsonRpcRequest {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
            id: Some(json!(chrono::Utc::now().timestamp_millis())),
        };
        
        let mut http_request = self.client.post(&url).json(&request);
        if let Some(auth) = &self.auth_header {
            http_request = http_request.header("Authorization", auth);
        }
        
        let response = http_request
            .send()
            .await
            .map_err(|e| Error::Network(format!("Failed to send request: {}", e)))?;
        
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
        
        if let Some(error) = json_rpc_response.error {
            return Err(Error::ToolExecution(format!(
                "Request failed: {} (code: {})",
                error.message, error.code
            )));
        }
        
        json_rpc_response
            .result
            .ok_or_else(|| Error::Parse("No result in JSON-RPC response".to_string()))
    }
}
