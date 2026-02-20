use cnctd_ai::mcp::McpClient;
use cnctd_ai::McpGateway;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::obfuscation::SessionCache;

/// Shared state for active agent runs
#[derive(Clone, Debug, serde::Serialize)]
pub struct AgentRunState {
    pub status: String,
    pub result: Option<serde_json::Value>,
    pub events: Vec<serde_json::Value>,
    pub total_prompt_tokens: u32,
    pub total_completion_tokens: u32,
    pub iterations: usize,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

/// Application state shared across all routes
#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub gateway: Option<McpGateway>,
    pub mcp_client: Option<Arc<McpClient>>,
    pub agent_runs: Arc<RwLock<HashMap<String, AgentRunState>>>,
    pub session_cache: Option<Arc<SessionCache>>,
}

impl AppState {
    pub async fn new(config: Config) -> Self {
        let gateway = config
            .mcp_gateway_url
            .as_ref()
            .map(|url| McpGateway::new(url));

        let mcp_client = if let Some(url) = &config.mcp_server_url {
            let max_retries = 5;
            let mut attempt = 0;
            loop {
                attempt += 1;
                match McpClient::from_streamable_http(url).await {
                    Ok(client) => {
                        tracing::info!("Connected to MCP server at {url} (attempt {attempt})");
                        break Some(Arc::new(client));
                    }
                    Err(e) => {
                        if attempt >= max_retries {
                            tracing::warn!(
                                "Failed to connect to MCP server at {url} after {max_retries} attempts: {e}"
                            );
                            break None;
                        }
                        tracing::info!(
                            "Waiting for MCP server at {url} (attempt {attempt}/{max_retries}): {e}"
                        );
                        tokio::time::sleep(Duration::from_secs(2)).await;
                    }
                }
            }
        } else {
            None
        };

        let session_cache = if let (Some(key), Some(url), Some(token)) = (
            &config.obfuscation_key,
            &config.obfuscation_source_url,
            &config.obfuscation_source_token,
        ) {
            tracing::info!(
                "Obfuscation enabled (OBFUSCATION_KEY + OBFUSCATION_SOURCE_URL set)"
            );
            Some(Arc::new(SessionCache::new(
                key.clone(),
                url.clone(),
                token.clone(),
                Duration::from_secs(3600),
            )))
        } else {
            tracing::info!(
                "Obfuscation disabled (OBFUSCATION_KEY, OBFUSCATION_SOURCE_URL, or OBFUSCATION_SOURCE_TOKEN not set)"
            );
            None
        };

        Self {
            config,
            gateway,
            mcp_client,
            agent_runs: Arc::new(RwLock::new(HashMap::new())),
            session_cache,
        }
    }
}
