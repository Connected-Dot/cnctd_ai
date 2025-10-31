use anyhow::Result;
use reqwest::Client;
use rmcp::model::{CallToolResult, Tool};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    Bearer(String),
    None,
}

/// MCP client with session management
pub struct McpClient {
    client: Client,
    url: String,
    auth: Auth,
    initialized: Arc<Mutex<bool>>,
    request_id: Arc<Mutex<i32>>,
}

impl McpClient {
    /// Create a new MCP client
    pub fn new(url: String, auth: Auth) -> Self {
        Self {
            client: Client::new(),
            url,
            auth,
            initialized: Arc::new(Mutex::new(false)),
            request_id: Arc::new(Mutex::new(0)),
        }
    }

    /// Connect to the MCP server (initialize handshake)
    pub async fn connect(&self) -> Result<()> {
        let mut is_initialized = self.initialized.lock().await;
        
        if *is_initialized {
            return Ok(());
        }

        eprintln!("Connecting to MCP server at {}...", self.url);

        // Step 1: Initialize
        let init_response = self.send_request(
            "initialize",
            Some(json!({
                "capabilities": {},
                "clientInfo": {
                    "name": "my-rust-client",
                    "version": "0.1.0"
                },
                "protocolVersion": "2024-11-05"
            })),
            true,
        ).await?;

        eprintln!("Initialize response: {:?}", init_response);

        // Step 2: Send initialized notification
        self.send_request(
            "notifications/initialized",
            None,
            false, // No ID for notifications
        ).await?;

        *is_initialized = true;
        eprintln!("Connected successfully");

        Ok(())
    }

    /// List all available tools
    pub async fn list_tools(&self) -> Result<Vec<Tool>> {
        self.ensure_connected().await?;

        let response = self.send_request(
            "tools/list",
            Some(json!({})),
            true,
        ).await?;

        let tools = response["result"]["tools"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("No tools in response"))?;

        serde_json::from_value(json!(tools)).map_err(Into::into)
    }

    /// Call a tool by name with optional arguments
    pub async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Option<&serde_json::Map<String, Value>>,
    ) -> Result<CallToolResult> {
        self.ensure_connected().await?;

        let response = self.send_request(
            "tools/call",
            Some(json!({
                "name": tool_name,
                "arguments": arguments
            })),
            true,
        ).await?;

        // Parse the result into CallToolResult
        serde_json::from_value(response["result"].clone()).map_err(Into::into)
    }

    /// Ensure the client is connected, connect if not
    async fn ensure_connected(&self) -> Result<()> {
        let is_initialized = self.initialized.lock().await;
        if !*is_initialized {
            drop(is_initialized); // Release lock before calling connect
            self.connect().await?;
        }
        Ok(())
    }

    /// Send a JSON-RPC request to the server
    async fn send_request(
        &self,
        method: &str,
        params: Option<Value>,
        with_id: bool,
    ) -> Result<Value> {
        let mut body = json!({
            "jsonrpc": "2.0",
            "method": method,
        });

        if let Some(params) = params {
            body["params"] = params;
        }

        if with_id {
            let mut id = self.request_id.lock().await;
            body["id"] = json!(*id);
            *id += 1;
        }

        let mut request = self.client.post(&self.url).json(&body);

        if let Auth::Bearer(token) = &self.auth {
            request = request.header("Authorization", format!("Bearer {}", token));
        }

        let response = request.send().await?;
        let status = response.status();
        let text = response.text().await?;

        if !status.is_success() {
            return Err(anyhow::anyhow!("HTTP error {}: {}", status, text));
        }

        serde_json::from_str(&text).map_err(Into::into)
    }

    /// Check if the client is connected
    pub async fn is_connected(&self) -> bool {
        *self.initialized.lock().await
    }

    /// Disconnect and reset the client state
    pub async fn disconnect(&self) {
        let mut is_initialized = self.initialized.lock().await;
        *is_initialized = false;
        eprintln!("Disconnected from MCP server");
    }
}

// Convenience functions for backwards compatibility

/// Connect over HTTP(S) and list tools once (creates a temporary client)
pub async fn list_tools_http(url: &str, auth: Auth) -> Result<Vec<Tool>> {
    let client = McpClient::new(url.to_string(), auth);
    client.connect().await?;
    client.list_tools().await
}

/// Call a tool by name with optional JSON object args (creates a temporary client)
pub async fn call_tool_http(
    url: &str,
    auth: Auth,
    tool_name: &str,
    args_obj: Option<&serde_json::Map<String, Value>>,
) -> Result<CallToolResult> {
    let client = McpClient::new(url.to_string(), auth);
    client.connect().await?;
    client.call_tool(tool_name, args_obj).await
}