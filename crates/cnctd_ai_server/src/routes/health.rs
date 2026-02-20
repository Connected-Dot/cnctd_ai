use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn health(State(state): State<AppState>) -> Json<Value> {
    let gateway_status = match &state.gateway {
        Some(gw) => match gw.list_servers().await {
            Ok(servers) => json!({
                "connected": true,
                "servers": servers.len(),
            }),
            Err(e) => json!({
                "connected": false,
                "error": e.to_string(),
            }),
        },
        None => json!({
            "connected": false,
            "error": "MCP_GATEWAY_URL not configured",
        }),
    };

    let mcp_client_status = match &state.mcp_client {
        Some(client) => match client.list_tools().await {
            Ok(tools) => json!({
                "connected": true,
                "tools": tools.len(),
            }),
            Err(e) => json!({
                "connected": false,
                "error": e.to_string(),
            }),
        },
        None => json!({
            "connected": false,
            "error": "MCP client not configured",
        }),
    };

    Json(json!({
        "status": "ok",
        "service": "cnctd_ai_server",
        "version": env!("CARGO_PKG_VERSION"),
        "providers": {
            "anthropic": state.config.anthropic_api_key.is_some(),
            "openai": state.config.openai_api_key.is_some(),
            "google": state.config.google_api_key.is_some(),
            "ollama": state.config.ollama_base_url.is_some(),
        },
        "mcp_gateway": gateway_status,
        "mcp_client": mcp_client_status,
    }))
}
