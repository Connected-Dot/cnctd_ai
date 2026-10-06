//! OpenAI Responses API implementation
//!
//! This module implements the newer Responses API (/v1/responses) which supports
//! all GPT-4, GPT-4.1, GPT-5, and reasoning models (o1, o3).

use crate::error::Result;
use crate::request::{CompletionRequest, Effort, Thinking};
use crate::response::{CompletionResponse, FinishReason, Refusal};
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

/// Whether to ask for encrypted reasoning content, which a stateless tool
/// loop must echo back. True for every reasoning model family, and for any
/// request that sets a reasoning effort other than `none`.
fn wants_encrypted_reasoning(model: &str, request: &CompletionRequest) -> bool {
    if let Some(effort) = effective_effort(request) {
        return effort != Effort::None;
    }
    let model_lower = model.to_lowercase();
    ["o1", "o3", "o4", "gpt-5", "gpt-6"]
        .iter()
        .any(|family| model_lower.starts_with(family))
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

/// async-openai 0.33's `ReasoningEffort` has no `max`, and the API echoes the
/// request's effort inside every response object. Read `max` as `xhigh` so
/// the typed response deserializes; nothing downstream reads the echo.
fn normalize_effort(raw: &mut serde_json::Value) {
    for path in ["/reasoning/effort", "/response/reasoning/effort"] {
        if let Some(effort) = raw.pointer_mut(path).filter(|e| *e == "max") {
            *effort = serde_json::json!("xhigh");
        }
    }
}

/// `reasoning.effort` for this request: the explicit effort, or `none` when
/// thinking is disabled.
fn effective_effort(request: &CompletionRequest) -> Option<Effort> {
    request.options.as_ref().and_then(|o| {
        o.effort.or(match o.thinking {
            Some(Thinking::Disabled) => Some(Effort::None),
            _ => None,
        })
    })
}

/// Responses usage -> [`Usage`]: `input_tokens` already includes the
/// automatically cached part, reported in `input_tokens_details`.
pub(crate) fn usage_from(u: &async_openai::types::responses::ResponseUsage) -> crate::response::Usage {
    crate::response::Usage {
        prompt_tokens: u.input_tokens,
        completion_tokens: u.output_tokens,
        total_tokens: u.total_tokens,
        cache_creation_tokens: None,
        cache_read_tokens: Some(u.input_tokens_details.cached_tokens),
        cache_creation_1h_tokens: None,
    }
}

/// Build the request with the typed builder, then add what async-openai 0.33
/// cannot express (reasoning effort `max`) on the JSON.
fn build_request_json(config: &OpenAiConfig, request: &CompletionRequest, stream: bool) -> Result<serde_json::Value> {
    let mut builder = CreateResponseArgs::default();
    builder.model(&config.model).input(build_input(request));
    if stream {
        // async-openai's create_stream skips auto-setting this when the
        // `byot` feature is enabled (included via `full`).
        builder.stream(true);
    }
    if wants_encrypted_reasoning(&config.model, request) {
        builder.include(vec![async_openai::types::responses::IncludeEnum::ReasoningEncryptedContent]);
    }
    if let Some(t) = build_tools(request) {
        builder.tools(t);
    }
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

    let mut json = serde_json::to_value(builder.build()?)
        .map_err(|e| crate::error::Error::Other(format!("Failed to serialize request: {}", e)))?;

    if let Some(effort) = effective_effort(request) {
        json["reasoning"] = serde_json::json!({ "effort": effort.as_str() });
    }
    if let Some(key) = request
        .options
        .as_ref()
        .and_then(|o| o.prompt_cache.as_ref())
        .and_then(|p| p.key.as_ref())
    {
        json["prompt_cache_key"] = serde_json::json!(key);
    }
    Ok(json)
}

/// Non-streaming completion using Responses API
pub(super) async fn complete(
    sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse> {
    let body = build_request_json(config, request, false)?;
    let mut raw: serde_json::Value = sdk_client.responses().create_byot(body).await?;
    normalize_effort(&mut raw);
    let response: async_openai::types::responses::Response = serde_json::from_value(raw.clone())
        .map_err(|e| async_openai::error::OpenAIError::JSONDeserialize(e, raw.to_string()))?;

    let mut content = String::new();
    let mut refusal_text = String::new();
    let mut reasoning_summary = String::new();
    let mut tool_uses: Vec<crate::ToolUse> = Vec::new();

    for output_item in &response.output {
        match output_item {
            OutputItem::Message(msg) => {
                for c in &msg.content {
                    match c {
                        async_openai::types::responses::OutputMessageContent::OutputText(text) => {
                            content.push_str(&text.text);
                        }
                        async_openai::types::responses::OutputMessageContent::Refusal(r) => {
                            refusal_text.push_str(&r.refusal);
                        }
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
            OutputItem::Reasoning(reasoning) => {
                for part in &reasoning.summary {
                    let async_openai::types::responses::SummaryPart::SummaryText(t) = part;
                    if !reasoning_summary.is_empty() {
                        reasoning_summary.push_str("\n\n");
                    }
                    reasoning_summary.push_str(&t.text);
                }
            }
            _ => {}
        }
    }

    let tool_uses_opt = if tool_uses.is_empty() { None } else { Some(tool_uses) };
    let incomplete_reason = response
        .incomplete_details
        .as_ref()
        .map(|d| d.reason.as_str())
        .unwrap_or_default();

    let mut refusal = None;
    let finish_reason = match response.status {
        _ if !refusal_text.is_empty() => {
            refusal = Some(Refusal { category: None, explanation: Some(refusal_text.clone()) });
            FinishReason::Refusal
        }
        async_openai::types::responses::Status::Incomplete if incomplete_reason == "content_filter" => {
            refusal = Some(Refusal { category: Some("content_filter".to_string()), explanation: None });
            FinishReason::Refusal
        }
        async_openai::types::responses::Status::Completed if tool_uses_opt.is_some() => FinishReason::ToolUse,
        async_openai::types::responses::Status::Completed => FinishReason::Stop,
        async_openai::types::responses::Status::Incomplete => FinishReason::Length,
        _ => FinishReason::Other,
    };
    if !refusal_text.is_empty() && content.is_empty() {
        content = refusal_text;
    }

    let mut message = crate::message::Message::assistant(content);
    message.tool_uses = tool_uses_opt.clone();

    let usage = response.usage.as_ref().map(usage_from).unwrap_or_else(crate::response::Usage::zero);

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
        reasoning_summary: if reasoning_summary.is_empty() { None } else { Some(reasoning_summary) },
        citations: None,
        refusal,
    })
}

/// Streaming completion using Responses API
pub(super) async fn stream(
    sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &CompletionRequest,
) -> Result<CompletionStream> {
    use futures::StreamExt;

    let body = build_request_json(config, request, true)?;
    let raw_stream = sdk_client
        .responses()
        .create_stream_byot::<serde_json::Value, serde_json::Value>(body)
        .await?;
    let stream = raw_stream.map(|item| {
        item.and_then(|mut raw| {
            normalize_effort(&mut raw);
            serde_json::from_value::<async_openai::types::responses::ResponseStreamEvent>(raw.clone())
                .map_err(|e| async_openai::error::OpenAIError::JSONDeserialize(e, raw.to_string()))
        })
    });
    Ok(CompletionStream::openai_responses(Box::pin(stream), config.model.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;
    use crate::request::RequestOptions;

    fn config(model: &str) -> OpenAiConfig {
        OpenAiConfig { api_key: "k".into(), model: model.into(), organization: None, transcription_model: None }
    }

    fn request(options: Option<RequestOptions>) -> CompletionRequest {
        CompletionRequest {
            messages: vec![Message::user("hi")],
            tools: None,
            built_in_tools: None,
            tool_config: None,
            options,
        }
    }

    #[test]
    fn max_effort_and_encrypted_reasoning_for_gpt_6() {
        let json = build_request_json(
            &config("gpt-6.1-sol"),
            &request(Some(RequestOptions { effort: Some(Effort::Max), ..Default::default() })),
            true,
        )
        .unwrap();
        assert_eq!(json["reasoning"], serde_json::json!({ "effort": "max" }));
        assert_eq!(json["include"], serde_json::json!(["reasoning.encrypted_content"]));
        assert_eq!(json["stream"], serde_json::json!(true));
    }

    #[test]
    fn non_reasoning_model_gets_no_reasoning_fields() {
        let json = build_request_json(&config("gpt-4o-mini"), &request(None), false).unwrap();
        assert!(json.get("reasoning").is_none());
        assert!(json.get("include").is_none());
    }

    #[test]
    fn effort_none_skips_encrypted_reasoning() {
        let json = build_request_json(
            &config("gpt-5.4"),
            &request(Some(RequestOptions { thinking: Some(Thinking::Disabled), temperature: Some(0.1), ..Default::default() })),
            false,
        )
        .unwrap();
        assert_eq!(json["reasoning"], serde_json::json!({ "effort": "none" }));
        assert!(json.get("include").is_none());
    }

    #[test]
    fn prompt_cache_key_reaches_the_body() {
        let opts = RequestOptions {
            prompt_cache: Some(crate::request::PromptCache { key: Some("dot-1".into()), ..Default::default() }),
            ..Default::default()
        };
        let json = build_request_json(&config("gpt-5.5"), &request(Some(opts)), false).unwrap();
        assert_eq!(json["prompt_cache_key"], serde_json::json!("dot-1"));
        let json = build_request_json(&config("gpt-5.5"), &request(None), false).unwrap();
        assert!(json.get("prompt_cache_key").is_none());
    }
}
