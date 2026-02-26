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
    CreateResponseArgs, InputParam, InputItem, EasyInputMessage, EasyInputContent,
    Role as ResponsesRole, Tool as ResponsesTool, FunctionTool,
    OutputItem, Item, MessageItem, InputMessage, InputRole, InputContent,
    InputTextContent, InputImageContent, ImageDetail,
    FunctionCallOutputItemParam, FunctionCallOutput,
    FunctionToolCall as ResponsesFunctionToolCall, OutputStatus,
};

/// Check if a model is a reasoning model that requires encrypted_content for multi-turn
fn is_reasoning_model(model: &str) -> bool {
    let model_lower = model.to_lowercase();
    model_lower.contains("o1")
        || model_lower.contains("o3")
        || model_lower.contains("gpt-5")
}

/// Convert our Role to Responses API Role
fn convert_role(role: &crate::message::Role) -> ResponsesRole {
    match role {
        crate::message::Role::System => ResponsesRole::System,
        crate::message::Role::User => ResponsesRole::User,
        crate::message::Role::Assistant => ResponsesRole::Assistant,
    }
}

/// Build input items from our messages
fn build_input(request: &CompletionRequest) -> InputParam {
    let mut items: Vec<InputItem> = Vec::new();

    for msg in &request.messages {
        match msg.role {
            crate::message::Role::System | crate::message::Role::User => {
                // Handle tool results specially
                if let Some(tool_results) = &msg.tool_results {
                    // For tool results, we need to provide function call outputs
                    for result in tool_results {
                        let output_item = FunctionCallOutputItemParam {
                            call_id: result.effective_call_id().to_string(),
                            output: FunctionCallOutput::Text(result.content.clone()),
                            id: None,
                            status: None,
                        };
                        items.push(InputItem::Item(Item::FunctionCallOutput(output_item)));
                    }
                    continue;
                }
                // Legacy single tool result
                else if let Some(tool_call_id) = &msg.tool_call_id {
                    let output_item = FunctionCallOutputItemParam {
                        call_id: tool_call_id.clone(),
                        output: FunctionCallOutput::Text(msg.content.clone()),
                        id: None,
                        status: None,
                    };
                    items.push(InputItem::Item(Item::FunctionCallOutput(output_item)));
                    continue;
                }

                // Check if message has images (vision support)
                if msg.has_images() {
                    let mut content_parts: Vec<InputContent> = Vec::new();

                    // Add text content if present
                    if !msg.content.is_empty() {
                        content_parts.push(InputContent::InputText(InputTextContent {
                            text: msg.content.clone(),
                        }));
                    }

                    // Add images as base64 data URLs
                    if let Some(images) = &msg.images {
                        for image in images {
                            let data_url = format!("data:{};base64,{}", image.media_type, image.data);
                            content_parts.push(InputContent::InputImage(InputImageContent {
                                detail: ImageDetail::default(),
                                file_id: None,
                                image_url: Some(data_url),
                            }));
                        }
                    }

                    let role = match msg.role {
                        crate::message::Role::System => InputRole::System,
                        crate::message::Role::User => InputRole::User,
                        _ => InputRole::User,
                    };
                    let input_msg = InputMessage {
                        content: content_parts,
                        role,
                        status: None,
                    };
                    items.push(InputItem::Item(Item::Message(MessageItem::Input(input_msg))));
                    continue;
                }

                // Regular text-only message
                let input_msg = EasyInputMessage {
                    r#type: Default::default(),
                    role: convert_role(&msg.role),
                    content: EasyInputContent::Text(msg.content.clone()),
                };
                items.push(InputItem::EasyMessage(input_msg));
            }
            crate::message::Role::Assistant => {
                // Include reasoning items FIRST (required for GPT-5.2-pro before function calls)
                // Reasoning items are stored as serde_json::Value — deserialize into InputItem
                if let Some(reasoning_items) = &msg.reasoning_items {
                    for reasoning in reasoning_items {
                        if let Ok(input_item) = serde_json::from_value::<InputItem>(reasoning.clone()) {
                            items.push(input_item);
                        }
                    }
                }

                // For assistant messages with tool uses, include the function calls
                if let Some(tool_uses) = &msg.tool_uses {
                    for tu in tool_uses {
                        let call_id = tu.call_id.as_ref().unwrap_or(&tu.id).clone();
                        let func_call = ResponsesFunctionToolCall {
                            arguments: tu.input.to_string(),
                            call_id,
                            name: tu.name.clone(),
                            id: Some(tu.id.clone()),
                            status: Some(OutputStatus::Completed),
                        };
                        items.push(InputItem::Item(Item::FunctionCall(func_call)));
                    }
                }

                // Also include text content if present
                if !msg.content.is_empty() {
                    let input_msg = EasyInputMessage {
                        r#type: Default::default(),
                        role: ResponsesRole::Assistant,
                        content: EasyInputContent::Text(msg.content.clone()),
                    };
                    items.push(InputItem::EasyMessage(input_msg));
                }
            }
        }
    }

    InputParam::Items(items)
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

fn build_tools(request: &CompletionRequest) -> Option<Vec<ResponsesTool>> {
    request.tools.as_ref().map(|tools| {
        tools.iter().map(|tool| {
            let mut schema = (*tool.input_schema).clone();
            ensure_properties(&mut schema);

            ResponsesTool::Function(FunctionTool {
                name: tool.name.to_string(),
                description: tool.description.as_ref().map(|d| d.to_string()),
                parameters: Some(serde_json::Value::Object(schema)),
                strict: Some(false),
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

    // Include encrypted reasoning content for multi-turn tool calls with reasoning models (GPT-5.2-pro, o1, o3)
    // This is required for stateless multi-turn conversations - only for reasoning models
    if is_reasoning_model(&config.model) {
        builder.include(vec![async_openai::types::responses::IncludeEnum::ReasoningEncryptedContent]);
    }

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

    let response = sdk_client
        .responses()
        .create(create_request)
        .await?;
    
    // Extract text content and tool calls from output
    let mut content = String::new();
    let mut tool_uses: Vec<crate::ToolUse> = Vec::new();
    
    for output_item in &response.output {
        match output_item {
            OutputItem::Message(msg) => {
                for c in &msg.content {
                    if let async_openai::types::responses::OutputMessageContent::OutputText(text) = c {
                        content.push_str(&text.text);
                    }
                }
            }
            OutputItem::FunctionCall(fc) => {
                tool_uses.push(crate::ToolUse {
                    id: fc.id.clone().unwrap_or_default(),
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
        images: None,
        videos: None,
        documents: None,
        cache_control: None,
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
            cache_creation_tokens: None, // OpenAI caching is automatic
            cache_read_tokens: None,
        }
    } else {
        crate::response::Usage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            cache_creation_tokens: None,
            cache_read_tokens: None,
        }
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
        reasoning_summary: None, // TODO: Extract from response if available
        citations: None, // OpenAI doesn't support citations
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
        .input(input)
        .stream(true); // Required: async-openai's create_stream skips auto-setting this when the `byot` feature is enabled (included via `full`)

    // Include encrypted reasoning content for multi-turn tool calls with reasoning models (GPT-5.2-pro, o1, o3)
    // This is required for stateless multi-turn conversations - only for reasoning models
    if is_reasoning_model(&config.model) {
        builder.include(vec![async_openai::types::responses::IncludeEnum::ReasoningEncryptedContent]);
    }

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

    let stream = sdk_client
        .responses()
        .create_stream(create_request)
        .await?;

    Ok(CompletionStream::openai_responses(stream, config.model.clone()))
}
