use std::{sync::Arc, time::{Duration, Instant}};
use reqwest::{Client, header::{HeaderMap, HeaderValue, AUTHORIZATION}};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tokio::sync::Mutex;

use crate::{error::AiError, mcp::{Auth, requests::JsonRpcRequest}};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ConnectionType {
    Stdio,
    WebSocket,
    Http,
}

impl ConnectionType {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConnectionType::Stdio => "stdio",
            ConnectionType::WebSocket => "websocket",
            ConnectionType::Http => "http",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "stdio" => Some(ConnectionType::Stdio),
            "websocket" => Some(ConnectionType::WebSocket),
            "http" => Some(ConnectionType::Http),
            _ => None,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpServer {
    pub name: String,
    pub description: Option<String>,
    pub url: String,
    pub auth: Auth,
    pub connection_type: ConnectionType,
}

impl McpServer {
    pub fn new<N, U, D, C>(name: N, url: U, auth: Auth, description: Option<D>, connection_type: C) -> Self
    where
        N: Into<String>,
        U: Into<String>,
        D: Into<String>,
        C: Into<ConnectionType>,
    {
        Self {
            name: name.into(),
            url: url.into(),
            auth,
            description: description.map(Into::into),
            connection_type: connection_type.into(),
        }
    }

    /// Establish a client connection to this server (HTTP transport implemented; others TODO).
    pub async fn connect(&self) -> Result<McpConnection, AiError> {
        eprintln!("Connecting to MCP server at {}...", self.url);

        let mut headers = HeaderMap::new();
        
        // Apply auth headers
        match &self.auth {
            Auth::Bearer(token) => {
                headers.insert(
                    AUTHORIZATION,
                    HeaderValue::from_str(&format!("Bearer {}", token))
                        .map_err(|e| AiError::McpError(format!("invalid bearer token: {e}")))?
                );
            }
            // Auth::ApiKey { header, key } => {
            //     headers.insert(
            //         header.parse().map_err(|e| AiError::McpError(format!("invalid header name: {e}")))?,
            //         HeaderValue::from_str(key)
            //             .map_err(|e| AiError::McpError(format!("invalid api key: {e}")))?
            //     );
            // }
            // Auth::Basic { username, password } => {
            //     let credentials = base64::encode(format!("{}:{}", username, password));
            //     headers.insert(
            //         AUTHORIZATION,
            //         HeaderValue::from_str(&format!("Basic {}", credentials))
            //             .map_err(|e| AiError::McpError(format!("invalid basic auth: {e}")))?
            //     );
            // }
            Auth::None => {}
        }

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .map_err(|e| AiError::McpError(format!("build http client: {e}")))?;

        Ok(McpConnection {
            server: self.clone(),
            client: Arc::new(client),
            tools_cache: Arc::new(Mutex::new(None)),
        })
    }
}

#[derive(Debug, Clone)]
pub struct McpConnection {
    pub server: McpServer,
    client: Arc<Client>,
    tools_cache: Arc<Mutex<Option<(Vec<Value>, Instant)>>>,
}

impl McpConnection {
    /// Generic request executor
    async fn execute(&self, request: JsonRpcRequest) -> Result<Value, AiError> {
        let response = self.client
            .post(&self.server.url)
            .json(&request.build())
            .send()
            .await
            .map_err(|e| AiError::McpError(format!("request failed: {e}")))?;

        let result: Value = response.json().await
            .map_err(|e| AiError::McpError(format!("parse response: {e}")))?;

        if let Some(error) = result.get("error") {
            return Err(AiError::McpError(format!("rpc error: {}", error)));
        }

        Ok(result)
    }

    pub async fn refresh_tools(&self) -> Result<Vec<Value>, AiError> {
        let result = self.execute(JsonRpcRequest::tools_list()).await?;

        let tools = result["result"]["tools"]
            .as_array()
            .ok_or_else(|| AiError::McpError("tools/list response missing tools array".into()))?
            .clone();

        let mut cache = self.tools_cache.lock().await;
        *cache = Some((tools.clone(), Instant::now()));
        
        Ok(tools)
    }

    pub async fn get_tools(&self, max_age: Option<Duration>) -> Result<Vec<Value>, AiError> {
        if let Some(max_age) = max_age {
            if let Some((cached, t0)) = &*self.tools_cache.lock().await {
                if t0.elapsed() <= max_age {
                    return Ok(cached.clone());
                }
            }
        } else if let Some((cached, _)) = &*self.tools_cache.lock().await {
            return Ok(cached.clone());
        }
        self.refresh_tools().await
    }

    pub async fn call_tool(&self, name: &str, args: Value) -> Result<Value, AiError> {
        let result = self.execute(JsonRpcRequest::tools_call(name, args)).await?;
        Ok(result["result"].clone())
    }

    pub async fn list_all_resources(&self) -> Result<Vec<String>, AiError> {
        let result = self.execute(JsonRpcRequest::resources_list()).await?;

        let resources = result["result"]["resources"]
            .as_array()
            .ok_or_else(|| AiError::McpError("resources/list response missing resources array".into()))?
            .iter()
            .filter_map(|r| r["uri"].as_str().map(String::from))
            .collect();

        Ok(resources)
    }

    pub async fn read_resource(&self, uri: &str) -> Result<Value, AiError> {
        let result = self.execute(JsonRpcRequest::resources_read(uri)).await?;
        Ok(result["result"].clone())
    }
}