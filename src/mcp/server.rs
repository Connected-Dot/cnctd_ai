use std::{sync::Arc, time::{Duration, Instant}};

use rmcp::{
    ServiceExt,
    model::{ClientCapabilities, Implementation, ProtocolVersion, Tool, CallToolRequestParam, ReadResourceRequestParam},
    service::{Peer, RoleClient, RunningService},
    transport::StreamableHttpClientTransport,
};
use serde::{Deserialize, Serialize};
use serde_json::Map;
use tokio::sync::Mutex;

use crate::{error::AiError, mcp::Auth};

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

        let initialized = Arc::new(Mutex::new(false));
        let request_id = Arc::new(Mutex::new(0));

        // Defaults are fine for first contact; initialize is negotiated by the service layer.
        let _client_caps = ClientCapabilities::default();
        let _client_info = Implementation::default();
        let _proto = ProtocolVersion::LATEST;

        // IMPORTANT: `serve(...)` returns a `RunningService<RoleClient, ()>`, not a `Peer`.
        let running: RunningService<RoleClient, ()> = match self.connection_type {
            ConnectionType::Http => {
                // Requires: rmcp = { version = "0.8.3", features = ["client", "transport-streamable-http-client-reqwest"] }
                let transport = StreamableHttpClientTransport::from_uri(self.url.as_str());
                ().serve(transport)
                    .await
                    .map_err(|e| AiError::McpError(format!("serve http client: {e}")))? 
            }
            ConnectionType::Stdio => {
                return Err(AiError::McpError("ConnectionType::Stdio not implemented yet".into()));
            }
            ConnectionType::WebSocket => {
                return Err(AiError::McpError("ConnectionType::WebSocket not implemented yet".into()));
            }
        };

        {
            let mut flag = initialized.lock().await;
            *flag = true;
        }

        Ok(McpConnection {
            server: self.clone(),
            initialized,
            request_id,
            running: Arc::new(Mutex::new(running)),
            tools_cache: Arc::new(Mutex::new(None)),
        })
    }
}

#[derive(Debug, Clone)]
pub struct McpConnection {
    pub server: McpServer,
    pub initialized: Arc<Mutex<bool>>,
    pub request_id: Arc<Mutex<i32>>,
    // Keep the running service alive; derive Peer<RoleClient> from it for calls.
    running: Arc<Mutex<RunningService<RoleClient, ()>>>,
    // Per-connection tools cache: (tools, last_refreshed)
    tools_cache: Arc<Mutex<Option<(Vec<Tool>, Instant)>>>,
}

impl McpConnection {
    /// Internal: get a clone of the Peer<RoleClient>.
    async fn peer(&self) -> Peer<RoleClient> {
        let running = self.running.lock().await;
        running.peer().clone()
    }

    /// Force-refresh the tool list from the server and update the cache.
    pub async fn refresh_tools(&self) -> Result<Vec<Tool>, AiError> {
        let peer = self.peer().await;
        let tools = peer
            .list_all_tools()
            .await
            .map_err(|e| AiError::McpError(format!("list_all_tools: {e}")))?;

        {
            let mut cache = self.tools_cache.lock().await;
            *cache = Some((tools.clone(), Instant::now()));
        }
        Ok(tools)
    }

    /// Get tools, honoring an optional staleness window.
    /// - If `max_age` is None and cache exists, return cached.
    /// - If cache is missing/stale, refresh.
    pub async fn get_tools(&self, max_age: Option<Duration>) -> Result<Vec<Tool>, AiError> {
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

    /// Example: get a single page of tools if you ever want pagination control.
    pub async fn list_tools_page(
        &self,
        page: Option<rmcp::model::PaginatedRequestParam>,
    ) -> Result<rmcp::model::ListToolsResult, AiError> {
        let peer = self.peer().await;
        peer.list_tools(page)
            .await
            .map_err(|e| AiError::McpError(format!("list_tools: {e}")))
    }

    pub async fn list_all_resources(&self) -> Result<Vec<String>, AiError> {
        let peer = self.peer().await;
        let resources = peer
            .list_all_resources()
            .await
            .map_err(|e| AiError::McpError(format!("list_all_resources: {e}")))?;
        
        // Extract URI from Annotated<RawResource>
        Ok(resources.into_iter().map(|r| r.raw.uri).collect())
    }

    pub async fn read_resource(&self, uri: &str) -> Result<rmcp::model::ReadResourceResult, AiError> {
        let peer = self.peer().await;
        
        // Create the proper request parameter
        let param = ReadResourceRequestParam {
            uri: uri.to_string(),
        };
        
        let result = peer
            .read_resource(param)
            .await
            .map_err(|e| AiError::McpError(format!("read_resource: {e}")))?;
        
        Ok(result)
    }

    pub async fn call_tool<T>(&self, name: &str, args: T) -> Result<rmcp::model::CallToolResult, AiError>
    where
        T: Serialize,
    {
        let peer = self.peer().await;
        
        let arguments_map = Map::from(serde_json::to_value(args)
            .map_err(|e| AiError::McpError(format!("serialize args: {e}")))?
            .as_object()
            .ok_or_else(|| AiError::McpError("args must serialize to a JSON object".into()))?
            .clone());
        // Create the proper request parameter
        let param = CallToolRequestParam {
            name: name.to_string().into(),
            arguments: Some(arguments_map)
        };
        
        let result = peer
            .call_tool(param)
            .await
            .map_err(|e| AiError::McpError(format!("call_tool: {e}")))?;
        
        Ok(result)
    }
}
