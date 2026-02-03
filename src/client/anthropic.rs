use crate::error::Result;
use crate::request::CompletionRequest;
use crate::response::CompletionResponse;
use crate::stream::CompletionStream;
use super::config::AnthropicConfig;

pub(super) async fn complete(
    config: &AnthropicConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse> {
    use anthropic_sdk::{Anthropic, MessageCreateBuilder, MessageContent, ContentBlockParam};
    
    let client = Anthropic::new(&config.api_key)
        .map_err(|e| crate::error::Error::AnthropicError(e.to_string()))?;
    
    let mut builder = MessageCreateBuilder::new(
        &config.model,
        request.options.as_ref().and_then(|o| o.max_tokens).unwrap_or(4096),
    );
    
    // Add system message if present
    if let Some(system_msg) = request.messages.iter().find(|m| matches!(m.role, crate::message::Role::System)) {
        builder = builder.system(&system_msg.content);
    }
    
    // Add user/assistant messages - handle tool results, tool uses, and images
    for msg in request.messages.iter().filter(|m| !matches!(m.role, crate::message::Role::System)) {
        match msg.role {
            crate::message::Role::User => {
                // Check if this is a tool result message
                if let Some(tool_call_id) = &msg.tool_call_id {
                    // This is a tool result - use content blocks
                    builder = builder.user(MessageContent::Blocks(vec![
                        ContentBlockParam::ToolResult {
                            tool_use_id: tool_call_id.clone(),
                            content: Some(msg.content.clone()),
                            is_error: Some(false),
                        }
                    ]));
                } else if msg.has_images() {
                    // Message with images - build content blocks
                    let mut content_blocks = Vec::new();

                    // Add images first (Anthropic prefers images before text)
                    if let Some(images) = &msg.images {
                        for image in images {
                            content_blocks.push(ContentBlockParam::Image {
                                source: anthropic_sdk::types::ImageSource::Base64 {
                                    media_type: image.media_type.clone(),
                                    data: image.data.clone(),
                                },
                            });
                        }
                    }

                    // Add text content if present
                    if !msg.content.is_empty() {
                        content_blocks.push(ContentBlockParam::Text {
                            text: msg.content.clone(),
                        });
                    }

                    builder = builder.user(MessageContent::Blocks(content_blocks));
                } else {
                    // Regular user message
                    builder = builder.user(MessageContent::Text(msg.content.clone()));
                }
            }
            crate::message::Role::Assistant => {
                // Check if this has tool uses
                if let Some(tool_uses) = &msg.tool_uses {
                    // Assistant message with tool calls
                    let mut content_blocks = Vec::new();
                    
                    // Add text content if present
                    if !msg.content.is_empty() {
                        content_blocks.push(ContentBlockParam::Text {
                            text: msg.content.clone(),
                        });
                    }
                    
                    // Add tool use blocks
                    for tool_use in tool_uses {
                        content_blocks.push(ContentBlockParam::ToolUse {
                            id: tool_use.id.clone(),
                            name: tool_use.name.clone(),
                            input: tool_use.input.clone(),
                        });
                    }
                    
                    builder = builder.assistant(MessageContent::Blocks(content_blocks));
                } else {
                    // Regular assistant message
                    builder = builder.assistant(MessageContent::Text(msg.content.clone()));
                }
            }
            crate::message::Role::System => {}
        }
    }
    
    // Add tools if present
    if let Some(tools) = &request.tools {
        let anthropic_tools: Vec<anthropic_sdk::Tool> = tools
            .iter()
            .map(|tool| anthropic_sdk::Tool {
                name: tool.name.to_string(),
                description: tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                input_schema: serde_json::from_value(serde_json::Value::Object((*tool.input_schema).clone()))
                    .unwrap_or_else(|_| anthropic_sdk::types::ToolInputSchema {
                        schema_type: "object".to_string(),
                        properties: serde_json::Map::new(),
                        required: vec![],
                        additional: serde_json::Map::new(),
                    }),
            })
            .collect();
        builder = builder.tools(anthropic_tools);
    }
    
    // Apply options
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            builder = builder.temperature(temp);
        }
        if let Some(top_p) = opts.top_p {
            builder = builder.top_p(top_p);
        }
    }
    
    let anthropic_response = client.messages()
        .create(builder.build())
        .await
        .map_err(|e| crate::error::Error::AnthropicError(e.to_string()))?;
    
    // Extract text content AND tool uses from response
    let mut content = String::new();
    let mut tool_uses = Vec::new();
    
    for block in &anthropic_response.content {
        match block {
            anthropic_sdk::ContentBlock::Text { text } => {
                content.push_str(text);
            }
            anthropic_sdk::ContentBlock::ToolUse { id, name, input } => {
                tool_uses.push(crate::ToolUse { call_id: None,
                    id: id.clone(),
                    name: name.clone(),
                    input: input.clone(),
                });
            }
            _ => {}
        }
    }
    
    let tool_uses_opt = if !tool_uses.is_empty() {
        Some(tool_uses.clone())
    } else {
        None
    };
    
    let message = crate::message::Message {
        role: crate::message::Role::Assistant,
        content,
        images: None,
        videos: None,
        documents: None,
        cache_control: None,
        tool_uses: tool_uses_opt.clone(),
        tool_call_id: None,
        tool_results: None,
        reasoning_items: None,
    };

    let usage = crate::response::Usage {
        prompt_tokens: anthropic_response.usage.input_tokens,
        completion_tokens: anthropic_response.usage.output_tokens,
        total_tokens: anthropic_response.usage.input_tokens + anthropic_response.usage.output_tokens,
        // Note: Cache usage not available through the SDK - use streaming for full cache support
        cache_creation_tokens: None,
        cache_read_tokens: None,
    };
    
    let finish_reason = match anthropic_response.stop_reason {
        Some(anthropic_sdk::StopReason::EndTurn) => crate::response::FinishReason::Stop,
        Some(anthropic_sdk::StopReason::MaxTokens) => crate::response::FinishReason::Length,
        Some(anthropic_sdk::StopReason::StopSequence) => crate::response::FinishReason::Stop,
        Some(anthropic_sdk::StopReason::ToolUse) => crate::response::FinishReason::ToolUse,
        _ => crate::response::FinishReason::Other,
    };
    
    Ok(CompletionResponse {
        message,
        usage,
        finish_reason,
        model: anthropic_response.model,
        tool_uses: tool_uses_opt,
        grounding_metadata: None,
        code_execution_results: None,
        google_maps_widget_token: None,
        reasoning_items: None,
    })
}

// ============================================================================
// TEMPORARY WORKAROUND: Custom streaming implementation
// ============================================================================
// The anthropic-sdk-rust (v0.1.1) has a bug where create_stream() uses
// Bearer token authentication instead of x-api-key header, causing 401 errors.
// 
// Issue: https://github.com/dimichgh/anthropic-sdk-rust/issues/2
// Created: Sept 11, 2025
//
// TODO: Once the upstream bug is fixed and a new version is released:
//   1. Remove this custom implementation
//   2. Restore the original SDK-based streaming (see git history)
//   3. Update anthropic-sdk-rust dependency version
//   4. Test streaming works with SDK's create_stream() method
// ============================================================================

pub(super) async fn stream(
    config: &AnthropicConfig,
    request: &CompletionRequest,
) -> Result<CompletionStream> {
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
    
    // Build the request body manually
    let mut body = serde_json::json!({
        "model": config.model,
        "max_tokens": request.options.as_ref().and_then(|o| o.max_tokens).unwrap_or(4096),
        "messages": [],
        "stream": true,
    });

    // Add system message if present (with cache control support)
    if let Some(system_msg) = request.messages.iter().find(|m| matches!(m.role, crate::message::Role::System)) {
        // If cache control is set, use content block format
        if system_msg.cache_control.is_some() {
            body["system"] = serde_json::json!([{
                "type": "text",
                "text": system_msg.content,
                "cache_control": { "type": "ephemeral" }
            }]);
        } else {
            body["system"] = serde_json::json!(system_msg.content);
        }
    }

    // Add user/assistant messages - handle tool results, tool uses, and images properly
    let mut messages = Vec::new();
    for msg in request.messages.iter().filter(|m| !matches!(m.role, crate::message::Role::System)) {
        match msg.role {
            crate::message::Role::User => {
                // Check if this has multiple tool results (new format)
                if let Some(tool_results) = &msg.tool_results {
                    // Multiple tool results in a single message
                    let content_blocks: Vec<serde_json::Value> = tool_results
                        .iter()
                        .map(|tr| serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": tr.tool_call_id,
                            "content": tr.content,
                            "is_error": tr.is_error,
                        }))
                        .collect();
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": content_blocks,
                    }));
                } else if let Some(tool_call_id) = &msg.tool_call_id {
                    // Legacy single tool result format
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": msg.content.clone(),
                            "is_error": false,
                        }]
                    }));
                } else if msg.has_images() || msg.has_documents() {
                    // Message with images/documents - build content blocks
                    let mut content_blocks = Vec::new();

                    // Add documents first (Anthropic prefers documents before text)
                    if let Some(documents) = &msg.documents {
                        for doc in documents {
                            let mut doc_block = serde_json::json!({
                                "type": "document",
                                "source": {
                                    "type": "base64",
                                    "media_type": doc.media_type,
                                    "data": doc.data
                                }
                            });
                            // Add cache_control if message has it set
                            if msg.cache_control.is_some() {
                                doc_block["cache_control"] = serde_json::json!({ "type": "ephemeral" });
                            }
                            content_blocks.push(doc_block);
                        }
                    }

                    // Add images (Anthropic prefers images before text)
                    if let Some(images) = &msg.images {
                        for image in images {
                            content_blocks.push(serde_json::json!({
                                "type": "image",
                                "source": {
                                    "type": "base64",
                                    "media_type": image.media_type,
                                    "data": image.data
                                }
                            }));
                        }
                    }

                    // Add text content if present
                    if !msg.content.is_empty() {
                        let mut text_block = serde_json::json!({
                            "type": "text",
                            "text": msg.content.clone()
                        });
                        // Add cache_control to the last content block if set
                        if msg.cache_control.is_some() && msg.documents.is_none() {
                            text_block["cache_control"] = serde_json::json!({ "type": "ephemeral" });
                        }
                        content_blocks.push(text_block);
                    }

                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": content_blocks
                    }));
                } else if msg.cache_control.is_some() {
                    // Regular user message with cache control
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "text",
                            "text": msg.content.clone(),
                            "cache_control": { "type": "ephemeral" }
                        }]
                    }));
                } else {
                    // Regular user message
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content.clone(),
                    }));
                }
            }

            crate::message::Role::Assistant => {
                // Check if this has tool uses
                if let Some(tool_uses) = &msg.tool_uses {
                    // Assistant message with tool calls needs content blocks
                    let mut content_blocks = Vec::new();
                    
                    // Add text content if present
                    if !msg.content.is_empty() {
                        content_blocks.push(serde_json::json!({
                            "type": "text",
                            "text": msg.content.clone(),
                        }));
                    }
                    
                    // Add tool use blocks
                    for tool_use in tool_uses {
                        content_blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tool_use.id.clone(),
                            "name": tool_use.name.clone(),
                            "input": tool_use.input.clone(),
                        }));
                    }
                    
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": content_blocks,
                    }));
                } else {
                    // Regular assistant message
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": msg.content.clone(),
                    }));
                }
            }
            crate::message::Role::System => {}
        }
    }
    body["messages"] = serde_json::json!(messages);

    // Include tools if present
    if let Some(tools) = &request.tools {
        let tools_json: Vec<_> = tools.iter().map(|tool| {
            serde_json::json!({
                "name": tool.name.to_string(),
                "description": tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                "input_schema": serde_json::Value::Object((*tool.input_schema).clone()),
            })
        }).collect();
        body["tools"] = serde_json::json!(tools_json);
    }

    // Apply options
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = opts.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
    }
    
    // Build headers with correct x-api-key authentication
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&config.api_key)
            .map_err(|e| crate::error::Error::Other(format!("Invalid API key: {}", e)))?
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static("2023-06-01")
    );
    // Enable prompt caching beta feature
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("prompt-caching-2024-07-31")
    );
    
    // Make the HTTP request
    let client = reqwest::Client::new();
    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .headers(headers)
        .json(&body)
        .send()
        .await
        .map_err(|e| crate::error::Error::Other(format!("HTTP request failed: {}", e)))?;
    
    // Check for HTTP errors
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::error::Error::AnthropicError(
            format!("HTTP {}: {}", status, error_text)
        ));
    }
    
    // Convert response to SSE stream
    let stream = response.bytes_stream();
    
    Ok(CompletionStream::anthropic_custom(stream, config.model.clone()))
}
