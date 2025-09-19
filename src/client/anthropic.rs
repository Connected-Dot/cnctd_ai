use anthropic_sdk::Client;
use serde_json::{json, Value};

use crate::config::AiConfig;
use crate::error::AiError;
use crate::types::{AskRequest, Msg, Role, UniversalResponse};

pub async fn ask(
    _http: &reqwest::Client,
    cfg: &AiConfig,
    req: &AskRequest,
) -> Result<UniversalResponse, AiError> {
    // --- client/config ---
    let key = cfg.anthropic_api_key.as_ref().ok_or(AiError::Auth)?;
    
    // --- prepare messages ---
    let mut messages = Vec::new();
    
    for m in &req.messages {
        match m {
            Msg { role: Role::User, content, .. } => {
                messages.push(json!({
                    "role": "user",
                    "content": content
                }));
            }
            Msg { role: Role::Assistant, content, .. } => {
                messages.push(json!({
                    "role": "assistant",
                    "content": content
                }));
            }
            // Skip tool and system messages - system handled separately
            Msg { role: Role::Tool, .. } | Msg { role: Role::System, .. } => {
                continue;
            }
        }
    }
    
    // --- build client ---
    let mut client = Client::new()
        .auth(key)
        .model(&req.model.clone().unwrap_or_else(|| cfg.default_model.clone()))
        .max_tokens(req.options.max_output_tokens.unwrap_or(1024) as i32)
        .messages(&json!(messages));
    
    if let Some(system) = &req.system {
        client = client.system(system);
    }
    
    if let Some(temperature) = req.options.temperature {
        client = client.temperature(temperature);
    }
    
    // --- make request ---
    let request = client.build().map_err(|e| AiError::Provider(e.to_string()))?;

    let result_text = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let result_text_clone = result_text.clone();

    request.execute(move |content| {
        let result_text = result_text_clone.clone();
        async move {
            if let Ok(mut text) = result_text.lock() {
                text.push_str(&content);
            }
        }
    }).await.map_err(|e| AiError::Provider(e.to_string()))?;

    let final_text = result_text.lock().unwrap().clone();
    
    // --- build response ---
    Ok(UniversalResponse {
        text: final_text,
        finish_reason: "stop".to_string(),
        usage: None, // This SDK doesn't provide usage info easily
        latency_ms: 0, // facade fills this
        provider_meta: Value::Null,
    })
}
