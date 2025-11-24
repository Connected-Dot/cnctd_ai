use crate::error::Result;
use crate::request::CompletionRequest;
use crate::response::CompletionResponse;
use crate::stream::CompletionStream;
use super::config::GeminiConfig;

pub(super) async fn complete(
    config: &GeminiConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse> {
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
    
    // Build the Gemini request body
    let mut body = serde_json::json!({
        "contents": [],
    });
    
    // Add system instruction if present
    if let Some(system_msg) = request.messages.iter().find(|m| matches!(m.role, crate::message::Role::System)) {
        body["systemInstruction"] = serde_json::json!({
            "parts": [{ "text": system_msg.content.clone() }]
        });
    }
    
    // Build contents array - handle tool calls and results
    let mut contents = Vec::new();
    for msg in request.messages.iter().filter(|m| !matches!(m.role, crate::message::Role::System)) {
        match msg.role {
            crate::message::Role::User => {
                // Check if this is a tool result message
                if let Some(_tool_call_id) = &msg.tool_call_id {
                    // Gemini uses functionResponse for tool results
                    // The tool name should be extracted from the message or stored separately
                    // For now, we'll try to parse it from the message or use a placeholder
                    // In practice, the caller should set up the message with the tool name
                    
                    // Try to parse the content as JSON to extract the tool name
                    // If the content contains the tool name, use it; otherwise use a generic approach
                    let function_name = msg.tool_uses.as_ref()
                        .and_then(|uses| uses.first())
                        .map(|u| u.name.clone())
                        .unwrap_or_else(|| "function".to_string());
                    
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": function_name,
                                "response": {
                                    "result": msg.content.clone()
                                }
                            }
                        }]
                    }));
                } else {
                    // Regular user message
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{ "text": msg.content.clone() }]
                    }));
                }
            }
            crate::message::Role::Assistant => {
                // Check if this has tool uses (function calls)
                if let Some(tool_uses) = &msg.tool_uses {
                    let mut parts = Vec::new();
                    
                    // Add text content if present
                    if !msg.content.is_empty() {
                        parts.push(serde_json::json!({
                            "text": msg.content.clone()
                        }));
                    }
                    
                    // Add function calls
                    for tool_use in tool_uses {
                        parts.push(serde_json::json!({
                            "functionCall": {
                                "name": tool_use.name.clone(),
                                "args": tool_use.input.clone()
                            }
                        }));
                    }
                    
                    contents.push(serde_json::json!({
                        "role": "model",
                        "parts": parts
                    }));
                } else {
                    // Regular model message
                    contents.push(serde_json::json!({
                        "role": "model",
                        "parts": [{ "text": msg.content.clone() }]
                    }));
                }
            }
            crate::message::Role::System => {}
        }
    }
    body["contents"] = serde_json::json!(contents);
    
    // Add tools if present
    if let Some(tools) = &request.tools {
        let function_declarations: Vec<_> = tools.iter().map(|tool| {
            serde_json::json!({
                "name": tool.name.to_string(),
                "description": tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                "parameters": serde_json::Value::Object((*tool.input_schema).clone())
            })
        }).collect();
        
        body["tools"] = serde_json::json!([{
            "functionDeclarations": function_declarations
        }]);
    }
    
    // Add generation config
    let mut generation_config = serde_json::Map::new();
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            generation_config.insert("temperature".to_string(), serde_json::json!(temp));
        }
        if let Some(max_tokens) = opts.max_tokens {
            generation_config.insert("maxOutputTokens".to_string(), serde_json::json!(max_tokens));
        }
        if let Some(top_p) = opts.top_p {
            generation_config.insert("topP".to_string(), serde_json::json!(top_p));
        }
    }
    if !generation_config.is_empty() {
        body["generationConfig"] = serde_json::Value::Object(generation_config);
    }
    
    // Build headers
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    
    // Build URL with API key in query param
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        config.model,
        config.api_key
    );
    
    // Make the HTTP request
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| crate::error::Error::GeminiError(format!("HTTP request failed: {}", e)))?;
    
    // Check for HTTP errors
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::error::Error::from_gemini_error(
            format!("HTTP {}: {}", status, error_text)
        ));
    }
    
    // Parse the response
    let response_json: serde_json::Value = response.json().await
        .map_err(|e| crate::error::Error::GeminiError(format!("Failed to parse response: {}", e)))?;
    
    // Extract content from response
    let candidate = response_json["candidates"]
        .get(0)
        .ok_or_else(|| crate::error::Error::GeminiError("No candidates in response".into()))?;
    
    let parts = candidate["content"]["parts"]
        .as_array()
        .ok_or_else(|| crate::error::Error::GeminiError("No parts in response".into()))?;
    
    // Extract text and function calls from parts
    let mut content = String::new();
    let mut tool_uses = Vec::new();
    
    for part in parts {
        if let Some(text) = part["text"].as_str() {
            content.push_str(text);
        }
        if let Some(function_call) = part["functionCall"].as_object() {
            let name = function_call["name"].as_str().unwrap_or("").to_string();
            let args = function_call.get("args").cloned().unwrap_or(serde_json::json!({}));
            
            // Generate a unique ID for the tool call (Gemini doesn't provide one)
            let id = format!("gemini_call_{}", uuid::Uuid::new_v4());
            
            tool_uses.push(crate::ToolUse {
                id,
                name,
                input: args,
            });
        }
    }
    
    let tool_uses_opt = if !tool_uses.is_empty() {
        Some(tool_uses.clone())
    } else {
        None
    };
    
    // Extract finish reason
    let finish_reason_str = candidate["finishReason"].as_str().unwrap_or("OTHER");
    let finish_reason = match finish_reason_str {
        "STOP" => crate::response::FinishReason::Stop,
        "MAX_TOKENS" => crate::response::FinishReason::Length,
        "SAFETY" => crate::response::FinishReason::ContentFilter,
        "RECITATION" => crate::response::FinishReason::ContentFilter,
        _ => {
            // Check if we have tool uses - Gemini doesn't have a specific TOOL_USE finish reason
            if !tool_uses.is_empty() {
                crate::response::FinishReason::ToolUse
            } else {
                crate::response::FinishReason::Other
            }
        }
    };
    
    // Extract usage
    let usage_metadata = response_json.get("usageMetadata");
    let usage = if let Some(usage_data) = usage_metadata {
        crate::response::Usage {
            prompt_tokens: usage_data["promptTokenCount"].as_u64().unwrap_or(0) as u32,
            completion_tokens: usage_data["candidatesTokenCount"].as_u64().unwrap_or(0) as u32,
            total_tokens: usage_data["totalTokenCount"].as_u64().unwrap_or(0) as u32,
        }
    } else {
        crate::response::Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
        }
    };
    
    let message = crate::message::Message {
        role: crate::message::Role::Assistant,
        content,
        tool_uses: tool_uses_opt.clone(),
        tool_call_id: None,
    };
    
    Ok(CompletionResponse {
        message,
        usage,
        finish_reason,
        model: config.model.clone(),
        tool_uses: tool_uses_opt,
    })
}

pub(super) async fn stream(
    config: &GeminiConfig,
    request: &CompletionRequest,
) -> Result<CompletionStream> {
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
    
    // Build the Gemini request body (same as non-streaming)
    let mut body = serde_json::json!({
        "contents": [],
    });
    
    // Add system instruction if present
    if let Some(system_msg) = request.messages.iter().find(|m| matches!(m.role, crate::message::Role::System)) {
        body["systemInstruction"] = serde_json::json!({
            "parts": [{ "text": system_msg.content.clone() }]
        });
    }
    
    // Build contents array
    let mut contents = Vec::new();
    for msg in request.messages.iter().filter(|m| !matches!(m.role, crate::message::Role::System)) {
        match msg.role {
            crate::message::Role::User => {
                if let Some(_tool_call_id) = &msg.tool_call_id {
                    let function_name = msg.tool_uses.as_ref()
                        .and_then(|uses| uses.first())
                        .map(|u| u.name.clone())
                        .unwrap_or_else(|| "function".to_string());
                    
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{
                            "functionResponse": {
                                "name": function_name,
                                "response": {
                                    "result": msg.content.clone()
                                }
                            }
                        }]
                    }));
                } else {
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": [{ "text": msg.content.clone() }]
                    }));
                }
            }
            crate::message::Role::Assistant => {
                if let Some(tool_uses) = &msg.tool_uses {
                    let mut parts = Vec::new();
                    
                    if !msg.content.is_empty() {
                        parts.push(serde_json::json!({
                            "text": msg.content.clone()
                        }));
                    }
                    
                    for tool_use in tool_uses {
                        parts.push(serde_json::json!({
                            "functionCall": {
                                "name": tool_use.name.clone(),
                                "args": tool_use.input.clone()
                            }
                        }));
                    }
                    
                    contents.push(serde_json::json!({
                        "role": "model",
                        "parts": parts
                    }));
                } else {
                    contents.push(serde_json::json!({
                        "role": "model",
                        "parts": [{ "text": msg.content.clone() }]
                    }));
                }
            }
            crate::message::Role::System => {}
        }
    }
    body["contents"] = serde_json::json!(contents);
    
    // Add tools if present
    if let Some(tools) = &request.tools {
        let function_declarations: Vec<_> = tools.iter().map(|tool| {
            serde_json::json!({
                "name": tool.name.to_string(),
                "description": tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                "parameters": serde_json::Value::Object((*tool.input_schema).clone())
            })
        }).collect();
        
        body["tools"] = serde_json::json!([{
            "functionDeclarations": function_declarations
        }]);
    }
    
    // Add generation config
    let mut generation_config = serde_json::Map::new();
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            generation_config.insert("temperature".to_string(), serde_json::json!(temp));
        }
        if let Some(max_tokens) = opts.max_tokens {
            generation_config.insert("maxOutputTokens".to_string(), serde_json::json!(max_tokens));
        }
        if let Some(top_p) = opts.top_p {
            generation_config.insert("topP".to_string(), serde_json::json!(top_p));
        }
    }
    if !generation_config.is_empty() {
        body["generationConfig"] = serde_json::Value::Object(generation_config);
    }
    
    // Build headers
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    
    // Build streaming URL with API key in query param
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
        config.model,
        config.api_key
    );
    
    // Make the HTTP request
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| crate::error::Error::GeminiError(format!("HTTP request failed: {}", e)))?;
    
    // Check for HTTP errors
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::error::Error::from_gemini_error(
            format!("HTTP {}: {}", status, error_text)
        ));
    }
    
    // Convert response to SSE stream
    let stream = response.bytes_stream();
    
    Ok(CompletionStream::gemini_custom(stream, config.model.clone()))
}
