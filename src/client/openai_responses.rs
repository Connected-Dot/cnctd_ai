//! OpenAI Responses API implementation
//! 
//! This module implements the newer Responses API (/v1/responses) which supports
//! all GPT-4, GPT-4.1, GPT-5, and reasoning models (o1, o3).

use crate::error::Result;
use crate::request::CompletionRequest;
use crate::response::CompletionResponse;
use crate::stream::CompletionStream;
use super::config::OpenAiConfig;

use async_openai::types::responses::{
    CreateResponseArgs, Input, InputItem, InputMessage, InputContent,
    Role as ResponsesRole, ToolDefinition, Function,
    OutputContent, Content,
};

/// Convert our Role to Responses API Role
fn convert_role(role: &crate::message::Role) -> ResponsesRole {
    match role {
        crate::message::Role::System => ResponsesRole::System,
        crate::message::Role::User => ResponsesRole::User,
        crate::message::Role::Assistant => ResponsesRole::Assistant,
    }
}

/// Build input items from our messages
fn build_input(request: &CompletionRequest) -> Input {
    let mut items: Vec<InputItem> = Vec::new();
    
    for msg in &request.messages {
        match msg.role {
            crate::message::Role::System | crate::message::Role::User => {
                // Handle tool results specially
                if let Some(tool_results) = &msg.tool_results {
                    // For tool results, we need to provide function call outputs
                    for result in tool_results {
                        // Build a function_call_output item
                        // Use tool_call_id which should now contain the call_id (call_...) format
                        let output_item = serde_json::json!({
                            "type": "function_call_output",
                            "call_id": result.tool_call_id,
                            "output": result.content
                        });
                        eprintln!("DEBUG Responses: Building function_call_output with call_id={}", result.tool_call_id);
                        items.push(InputItem::Custom(output_item));
                    }
                    continue;
                }
                // Legacy single tool result
                else if let Some(tool_call_id) = &msg.tool_call_id {
                    let output_item = serde_json::json!({
                        "type": "function_call_output",
                        "call_id": tool_call_id,
                        "output": msg.content
                    });
                    items.push(InputItem::Custom(output_item));
                    continue;
                }
                
                // Regular message
                let input_msg = InputMessage {
                    kind: Default::default(),
                    role: convert_role(&msg.role),
                    content: InputContent::TextInput(msg.content.clone()),
                };
                items.push(InputItem::Message(input_msg));
            }
            crate::message::Role::Assistant => {
                // Include reasoning items FIRST (required for GPT-5.2-pro before function calls)
                if let Some(reasoning_items) = &msg.reasoning_items {
                    for reasoning in reasoning_items {
                        eprintln!("DEBUG Responses: Including reasoning item in request: {:?}", reasoning.get("id"));
                        items.push(InputItem::Custom(reasoning.clone()));
                    }
                }
                
                // For assistant messages with tool uses, include the function calls
                if let Some(tool_uses) = &msg.tool_uses {
                    for tu in tool_uses {
                        // For OpenAI Responses API:
                        // - id field must be the fc_... format (tu.id)
                        // - call_id field should be the call_... format (tu.call_id)
                        let call_id = tu.call_id.as_ref().unwrap_or(&tu.id);
                        eprintln!("DEBUG Responses: Building function_call with id={}, call_id={}", tu.id, call_id);
                        let func_call = serde_json::json!({
                            "type": "function_call",
                            "id": tu.id,
                            "call_id": call_id,
                            "name": tu.name,
                            "arguments": tu.input.to_string(),
                            "status": "completed"
                        });
                        items.push(InputItem::Custom(func_call));
                    }
                }
                
                // Also include text content if present
                if !msg.content.is_empty() {
                    let input_msg = InputMessage {
                        kind: Default::default(),
                        role: ResponsesRole::Assistant,
                        content: InputContent::TextInput(msg.content.clone()),
                    };
                    items.push(InputItem::Message(input_msg));
                }
            }
        }
    }
    
    Input::Items(items)
}

/// Build tools for the Responses API
/// Ensure all object schemas have a "properties" field (OpenAI Responses API requirement)
fn ensure_properties(schema: &mut serde_json::Map<String, serde_json::Value>) {
    // If this is an object type without properties, add empty properties
    if let Some(serde_json::Value::String(t)) = schema.get("type") {
        if t == "object" && !schema.contains_key("properties") {
            schema.insert("properties".to_string(), serde_json::json!({}));
        }
    }
    
    // Recursively process nested schemas
    if let Some(serde_json::Value::Object(props)) = schema.get_mut("properties") {
        for (_, prop_schema) in props.iter_mut() {
            if let serde_json::Value::Object(prop_obj) = prop_schema {
                ensure_properties(prop_obj);
            }
        }
    }
    
    // Handle items in arrays
    if let Some(serde_json::Value::Object(items)) = schema.get_mut("items") {
        ensure_properties(items);
    }
    
    // Handle additionalProperties if it's an object schema
    if let Some(serde_json::Value::Object(additional)) = schema.get_mut("additionalProperties") {
        ensure_properties(additional);
    }
}

fn build_tools(request: &CompletionRequest) -> Option<Vec<ToolDefinition>> {
    request.tools.as_ref().map(|tools| {
        tools.iter().map(|tool| {
            let mut schema = (*tool.input_schema).clone();
            ensure_properties(&mut schema);
            
            ToolDefinition::Function(Function {
                name: tool.name.to_string(),
                description: tool.description.as_ref().map(|d| d.to_string()),
                parameters: serde_json::Value::Object(schema),
                strict: false,
            })
        }).collect()
    })
}

/// Non-streaming completion using Responses API
pub(super) async fn complete(
    sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse> {
    let input = build_input(request);
    let tools = build_tools(request);
    
    let mut builder = CreateResponseArgs::default();
    builder
        .model(&config.model)
        .input(input);
    
    if let Some(t) = tools {
        builder.tools(t);
    }
    
    // Apply options
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            builder.temperature(temp);
        }
        if let Some(max_tokens) = opts.max_tokens {
            builder.max_output_tokens(max_tokens);
        }
        if let Some(top_p) = opts.top_p {
            builder.top_p(top_p);
        }
    }
    
    let create_request = builder.build()?;
    eprintln!("DEBUG: Responses API request: {:?}", serde_json::to_string(&create_request));
    
    eprintln!("DEBUG: Sending Responses API request to model: {}", config.model);
    
    let response = sdk_client
        .responses()
        .create(create_request)
        .await?;
    
    eprintln!("DEBUG: Received Responses API response, status: {:?}", response.status);
    
    // Extract text content and tool calls from output
    let mut content = String::new();
    let mut tool_uses: Vec<crate::ToolUse> = Vec::new();
    
    for output_item in &response.output {
        match output_item {
            OutputContent::Message(msg) => {
                for c in &msg.content {
                    if let Content::OutputText(text) = c {
                        content.push_str(&text.text);
                    }
                }
            }
            OutputContent::FunctionCall(fc) => {
                tool_uses.push(crate::ToolUse {
                    id: fc.id.clone(),
                    call_id: Some(fc.call_id.clone()),
                    name: fc.name.clone(),
                    input: serde_json::from_str(&fc.arguments)
                        .unwrap_or_else(|_| serde_json::Value::String(fc.arguments.clone())),
                });
            }
            _ => {}
        }
    }
    
    let tool_uses_opt = if tool_uses.is_empty() { None } else { Some(tool_uses.clone()) };
    
    let message = crate::message::Message {
        role: crate::message::Role::Assistant,
        content,
        tool_uses: tool_uses_opt.clone(),
        tool_call_id: None,
        tool_results: None,
            reasoning_items: None,
    };
    
    let usage = if let Some(u) = &response.usage {
        crate::response::Usage {
            prompt_tokens: u.input_tokens,
            completion_tokens: u.output_tokens,
            total_tokens: u.total_tokens,
        }
    } else {
        crate::response::Usage { prompt_tokens: 0, completion_tokens: 0, total_tokens: 0 }
    };
    
    let finish_reason = match response.status {
        async_openai::types::responses::Status::Completed => {
            if tool_uses_opt.is_some() {
                crate::response::FinishReason::ToolUse
            } else {
                crate::response::FinishReason::Stop
            }
        }
        async_openai::types::responses::Status::Failed => crate::response::FinishReason::Other,
        async_openai::types::responses::Status::Incomplete => crate::response::FinishReason::Length,
        _ => crate::response::FinishReason::Other,
    };
    
    Ok(CompletionResponse {
        message,
        usage,
        finish_reason,
        model: response.model,
        tool_uses: tool_uses_opt,
        grounding_metadata: None,
        code_execution_results: None,
        google_maps_widget_token: None,
        reasoning_items: None,
    })
}

/// Streaming completion using Responses API
pub(super) async fn stream(
    sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &CompletionRequest,
) -> Result<CompletionStream> {
    let input = build_input(request);
    let tools = build_tools(request);
    
    let mut builder = CreateResponseArgs::default();
    builder
        .model(&config.model)
        .input(input);
    
    if let Some(t) = tools {
        builder.tools(t);
    }
    
    // Apply options
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            builder.temperature(temp);
        }
        if let Some(max_tokens) = opts.max_tokens {
            builder.max_output_tokens(max_tokens);
        }
        if let Some(top_p) = opts.top_p {
            builder.top_p(top_p);
        }
    }
    
    let create_request = builder.build()?;
    eprintln!("DEBUG: Responses API request: {:?}", serde_json::to_string(&create_request));
    
    eprintln!("DEBUG: Creating Responses API stream for model: {}", config.model);
    
    let stream = sdk_client
        .responses()
        .create_stream(create_request)
        .await?;
    
    eprintln!("DEBUG: Responses API stream created successfully");
    
    Ok(CompletionStream::openai_responses(stream, config.model.clone()))
}
