use crate::error::Result;
use crate::request::{CompletionRequest, BuiltInTool};
use crate::response::{CompletionResponse, GroundingMetadata, CodeExecutionResult};
use crate::stream::CompletionStream;
use super::config::GeminiConfig;

/// Sanitize a JSON Schema for Gemini's function declaration format.
/// Gemini has stricter requirements than standard JSON Schema:
/// - No `$schema` field
/// - No `additionalProperties` field  
/// - `type` must be a string, not an array (convert ["string", "null"] to "string")
fn sanitize_schema_for_gemini(schema: &serde_json::Map<String, serde_json::Value>) -> serde_json::Map<String, serde_json::Value> {
    let mut result = serde_json::Map::new();
    
    for (key, value) in schema {
        // Skip unsupported fields
        if key == "$schema" || key == "additionalProperties" {
            continue;
        }
        
        match value {
            serde_json::Value::Object(obj) => {
                // Recursively sanitize nested objects
                result.insert(key.clone(), serde_json::Value::Object(sanitize_schema_for_gemini(obj)));
            }
            serde_json::Value::Array(arr) if key == "type" => {
                // Convert type arrays to single string
                // e.g., ["string", "null"] -> "string"
                let first_non_null = arr.iter()
                    .filter_map(|v| v.as_str())
                    .find(|s| *s != "null")
                    .unwrap_or("string");
                result.insert(key.clone(), serde_json::Value::String(first_non_null.to_string()));
            }
            serde_json::Value::Array(arr) => {
                // For other arrays (like "required", "enum"), sanitize each element if it's an object
                let sanitized: Vec<serde_json::Value> = arr.iter().map(|v| {
                    if let serde_json::Value::Object(obj) = v {
                        serde_json::Value::Object(sanitize_schema_for_gemini(obj))
                    } else {
                        v.clone()
                    }
                }).collect();
                result.insert(key.clone(), serde_json::Value::Array(sanitized));
            }
            _ => {
                result.insert(key.clone(), value.clone());
            }
        }
    }
    
    result
}


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
                // Check for multiple tool results first (new format with function_name)
                if let Some(tool_results) = &msg.tool_results {
                    let mut parts = Vec::new();
                    for result in tool_results {
                        // Use stored function_name, or try to extract from tool_call_id prefix
                        let function_name = result.function_name.clone()
                            .or_else(|| {
                                // Fallback: try to extract from ID if it starts with "gemini_call_"
                                // This won't work for Gemini since IDs are synthetic UUIDs
                                // But might help with manually constructed results
                                None
                            })
                            .unwrap_or_else(|| "function".to_string());
                        
                        parts.push(serde_json::json!({
                            "functionResponse": {
                                "name": function_name,
                                "response": {
                                    "result": result.content.clone()
                                }
                            }
                        }));
                    }
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": parts
                    }));
                }
                // Legacy single tool result
                else if let Some(_tool_call_id) = &msg.tool_call_id {
                    // Gemini uses functionResponse for tool results
                    // Since we don't have function_name stored in legacy format,
                    // use a generic name (this is the broken behavior we're fixing)
                    let function_name = "function".to_string();
                    
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
                        // Add thoughtSignature for Gemini 3 compatibility
                        // Using dummy signature to skip validation when original not available
                        parts.push(serde_json::json!({
                            "functionCall": {
                                "name": tool_use.name.clone(),
                                "args": tool_use.input.clone()
                            },
                            "thoughtSignature": "skip_thought_signature_validator"
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
    
    // Build tools array
    // Note: Gemini does NOT support combining function calling with built-in tools
    // If function declarations exist, we skip built-in tools (MCP tools take priority)
    let mut tools_array: Vec<serde_json::Value> = Vec::new();
    let has_function_declarations = request.tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
    
    // Add MCP function declarations if present
    if let Some(tools) = &request.tools {
        let function_declarations: Vec<_> = tools.iter().map(|tool| {
            serde_json::json!({
                "name": tool.name.to_string(),
                "description": tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                "parameters": serde_json::Value::Object(sanitize_schema_for_gemini(&tool.input_schema))
            })
        }).collect();
        
        tools_array.push(serde_json::json!({
            "functionDeclarations": function_declarations
        }));
    }
    
    // Add built-in tools if present (skip when function declarations exist - Gemini limitation)
    if !has_function_declarations {
    if let Some(built_in_tools) = &request.built_in_tools {
        for tool in built_in_tools {
            match tool {
                BuiltInTool::GoogleSearch => {
                    // Gemini 2.0+ format
                    tools_array.push(serde_json::json!({
                        "googleSearch": {}
                    }));
                }
                BuiltInTool::GoogleSearchRetrieval { dynamic_threshold } => {
                    // Gemini 1.5 legacy format
                    let mut config = serde_json::json!({
                        "mode": "MODE_DYNAMIC"
                    });
                    if let Some(threshold) = dynamic_threshold {
                        config["dynamicThreshold"] = serde_json::json!(threshold);
                    }
                    tools_array.push(serde_json::json!({
                        "googleSearchRetrieval": {
                            "dynamicRetrievalConfig": config
                        }
                    }));
                }
                BuiltInTool::CodeExecution => {
                    tools_array.push(serde_json::json!({
                        "codeExecution": {}
                    }));
                }
                BuiltInTool::UrlContext => {
                    tools_array.push(serde_json::json!({
                        "urlContext": {}
                    }));
                }
                BuiltInTool::GoogleMaps { enable_widget } => {
                    let mut maps_config = serde_json::Map::new();
                    if let Some(enable) = enable_widget {
                        maps_config.insert("enableWidget".to_string(), serde_json::json!(enable));
                    }
                    tools_array.push(serde_json::json!({
                        "googleMaps": maps_config
                    }));
                }
            }
        }
    }
    }
    
    if !tools_array.is_empty() {
        body["tools"] = serde_json::json!(tools_array);
    }
    
    // Add tool config if present (for location-aware queries)
    if let Some(tool_config) = &request.tool_config {
        if let Some(retrieval_config) = &tool_config.retrieval_config {
            let mut retrieval_json = serde_json::Map::new();
            
            if let Some(lat_lng) = &retrieval_config.lat_lng {
                retrieval_json.insert("latLng".to_string(), serde_json::json!({
                    "latitude": lat_lng.latitude,
                    "longitude": lat_lng.longitude
                }));
            }
            
            if let Some(lang) = &retrieval_config.language_code {
                retrieval_json.insert("languageCode".to_string(), serde_json::json!(lang));
            }
            
            if !retrieval_json.is_empty() {
                body["toolConfig"] = serde_json::json!({
                    "retrievalConfig": retrieval_json
                });
            }
        }
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
    
    // Extract text, function calls, and code execution results from parts
    let mut content = String::new();
    let mut tool_uses = Vec::new();
    let mut code_execution_results = Vec::new();
    
    for part in parts {
        if let Some(text) = part["text"].as_str() {
            content.push_str(text);
        }
        if let Some(function_call) = part["functionCall"].as_object() {
            let name = function_call["name"].as_str().unwrap_or("").to_string();
            let args = function_call.get("args").cloned().unwrap_or(serde_json::json!({}));
            
            // Generate a unique ID for the tool call (Gemini doesn't provide one)
            let id = format!("gemini_call_{}", uuid::Uuid::new_v4());
            
            tool_uses.push(crate::ToolUse { call_id: None,
                id,
                name,
                input: args,
            });
        }
        // Handle executable code part
        if let Some(executable_code) = part.get("executableCode") {
            let code = executable_code["code"].as_str().map(|s| s.to_string());
            let language = executable_code["language"].as_str().map(|s| s.to_string());
            code_execution_results.push(CodeExecutionResult {
                code,
                language,
                outcome: None,
                output: None,
            });
        }
        // Handle code execution result part
        if let Some(code_result) = part.get("codeExecutionResult") {
            let outcome = code_result["outcome"].as_str()
                .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok());
            let output = code_result["output"].as_str().map(|s| s.to_string());
            
            // If we have a pending code execution, update it with results
            if let Some(last) = code_execution_results.last_mut() {
                if last.outcome.is_none() {
                    last.outcome = outcome;
                    last.output = output;
                    continue;
                }
            }
            // Otherwise create a new result entry
            code_execution_results.push(CodeExecutionResult {
                code: None,
                language: None,
                outcome,
                output,
            });
        }
    }
    
    let tool_uses_opt = if !tool_uses.is_empty() {
        Some(tool_uses.clone())
    } else {
        None
    };
    
    let code_results_opt = if !code_execution_results.is_empty() {
        Some(code_execution_results)
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
    
    // Extract grounding metadata if present
    let grounding_metadata = candidate.get("groundingMetadata")
        .and_then(|gm| serde_json::from_value::<GroundingMetadata>(gm.clone()).ok());
    
    // Extract Google Maps widget token if present
    let google_maps_widget_token = candidate
        .get("googleMapsWidgetContextToken")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    
    let message = crate::message::Message {
        role: crate::message::Role::Assistant,
        content,
        tool_uses: tool_uses_opt.clone(),
        tool_call_id: None,
        tool_results: None,
            reasoning_items: None,
    };
    
    Ok(CompletionResponse {
        message,
        usage,
        finish_reason,
        model: config.model.clone(),
        tool_uses: tool_uses_opt,
        grounding_metadata,
        code_execution_results: code_results_opt,
        google_maps_widget_token,
        reasoning_items: None,
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
                // Check for multiple tool results first (new format with function_name)
                if let Some(tool_results) = &msg.tool_results {
                    let mut parts = Vec::new();
                    for result in tool_results {
                        // Use stored function_name, or fallback to generic
                        let function_name = result.function_name.clone()
                            .unwrap_or_else(|| "function".to_string());
                        
                        parts.push(serde_json::json!({
                            "functionResponse": {
                                "name": function_name,
                                "response": {
                                    "result": result.content.clone()
                                }
                            }
                        }));
                    }
                    contents.push(serde_json::json!({
                        "role": "user",
                        "parts": parts
                    }));
                }
                // Legacy single tool result
                else if let Some(_tool_call_id) = &msg.tool_call_id {
                    let function_name = "function".to_string();
                    
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
                        // Add thoughtSignature for Gemini 3 compatibility
                        // Using dummy signature to skip validation when original not available
                        parts.push(serde_json::json!({
                            "functionCall": {
                                "name": tool_use.name.clone(),
                                "args": tool_use.input.clone()
                            },
                            "thoughtSignature": "skip_thought_signature_validator"
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
    
    // Build tools array
    // Note: Gemini does NOT support combining function calling with built-in tools
    // If function declarations exist, we skip built-in tools (MCP tools take priority)
    let mut tools_array: Vec<serde_json::Value> = Vec::new();
    let has_function_declarations = request.tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
    
    // Add MCP function declarations if present
    if let Some(tools) = &request.tools {
        let function_declarations: Vec<_> = tools.iter().map(|tool| {
            serde_json::json!({
                "name": tool.name.to_string(),
                "description": tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                "parameters": serde_json::Value::Object(sanitize_schema_for_gemini(&tool.input_schema))
            })
        }).collect();
        
        tools_array.push(serde_json::json!({
            "functionDeclarations": function_declarations
        }));
    }
    
    // Add built-in tools if present (skip when function declarations exist - Gemini limitation)
    if !has_function_declarations {
    if let Some(built_in_tools) = &request.built_in_tools {
        for tool in built_in_tools {
            match tool {
                BuiltInTool::GoogleSearch => {
                    tools_array.push(serde_json::json!({
                        "googleSearch": {}
                    }));
                }
                BuiltInTool::GoogleSearchRetrieval { dynamic_threshold } => {
                    let mut config = serde_json::json!({
                        "mode": "MODE_DYNAMIC"
                    });
                    if let Some(threshold) = dynamic_threshold {
                        config["dynamicThreshold"] = serde_json::json!(threshold);
                    }
                    tools_array.push(serde_json::json!({
                        "googleSearchRetrieval": {
                            "dynamicRetrievalConfig": config
                        }
                    }));
                }
                BuiltInTool::CodeExecution => {
                    tools_array.push(serde_json::json!({
                        "codeExecution": {}
                    }));
                }
                BuiltInTool::UrlContext => {
                    tools_array.push(serde_json::json!({
                        "urlContext": {}
                    }));
                }
                BuiltInTool::GoogleMaps { enable_widget } => {
                    let mut maps_config = serde_json::Map::new();
                    if let Some(enable) = enable_widget {
                        maps_config.insert("enableWidget".to_string(), serde_json::json!(enable));
                    }
                    tools_array.push(serde_json::json!({
                        "googleMaps": maps_config
                    }));
                }
            }
        }
    }
    }
    
    if !tools_array.is_empty() {
        body["tools"] = serde_json::json!(tools_array);
    }
    
    // Add tool config if present (for location-aware queries)
    if let Some(tool_config) = &request.tool_config {
        if let Some(retrieval_config) = &tool_config.retrieval_config {
            let mut retrieval_json = serde_json::Map::new();
            
            if let Some(lat_lng) = &retrieval_config.lat_lng {
                retrieval_json.insert("latLng".to_string(), serde_json::json!({
                    "latitude": lat_lng.latitude,
                    "longitude": lat_lng.longitude
                }));
            }
            
            if let Some(lang) = &retrieval_config.language_code {
                retrieval_json.insert("languageCode".to_string(), serde_json::json!(lang));
            }
            
            if !retrieval_json.is_empty() {
                body["toolConfig"] = serde_json::json!({
                    "retrievalConfig": retrieval_json
                });
            }
        }
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
