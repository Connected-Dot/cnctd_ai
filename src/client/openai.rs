use crate::error::Result;
use crate::request::CompletionRequest;
use crate::response::CompletionResponse;
use crate::stream::CompletionStream;
use super::config::OpenAiConfig;

pub(super) async fn complete(
    sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse> {
    use async_openai::types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestToolMessageArgs, ChatCompletionTool, ChatCompletionTools,
        ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
        FunctionObject, CreateChatCompletionRequestArgs,
    };

    // Convert our messages to OpenAI format
    let mut messages = Vec::new();
    for msg in &request.messages {
        match msg.role {
            crate::message::Role::System => {
                messages.push(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(msg.content.clone())
                        .build()?
                ));
            }
            crate::message::Role::User => {
                // Check for multiple tool results first (new format)
                if let Some(tool_results) = &msg.tool_results {
                    // OpenAI requires one Tool message per result
                    for result in tool_results {
                        messages.push(ChatCompletionRequestMessage::Tool(
                            ChatCompletionRequestToolMessageArgs::default()
                                .content(result.content.clone())
                                .tool_call_id(result.tool_call_id.clone())
                                .build()?
                        ));
                    }
                    // Skip the outer message - we've expanded it into multiple Tool messages
                    continue;
                }
                // Legacy single tool result
                else if let Some(tool_call_id) = &msg.tool_call_id {
                    messages.push(ChatCompletionRequestMessage::Tool(
                        ChatCompletionRequestToolMessageArgs::default()
                            .content(msg.content.clone())
                            .tool_call_id(tool_call_id.clone())
                            .build()?
                    ));
                } else {
                    // Regular user message
                    messages.push(ChatCompletionRequestMessage::User(
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    ));
                }
            }
            crate::message::Role::Assistant => {
                let mut builder = ChatCompletionRequestAssistantMessageArgs::default();

                // Add content if present
                if !msg.content.is_empty() {
                    builder.content(msg.content.clone());
                }

                // Add tool calls if present
                if let Some(tool_uses) = &msg.tool_uses {
                    let tool_calls: Vec<ChatCompletionMessageToolCalls> = tool_uses.iter().map(|tu| {
                        ChatCompletionMessageToolCalls::Function(ChatCompletionMessageToolCall {
                            id: tu.id.clone(),
                            function: async_openai::types::chat::FunctionCall {
                                name: tu.name.clone(),
                                arguments: tu.input.to_string(),
                            },
                        })
                    }).collect();
                    builder.tool_calls(tool_calls);
                }

                messages.push(ChatCompletionRequestMessage::Assistant(builder.build()?));
            }
        }
    }

    // Build the request
    let mut request_builder = CreateChatCompletionRequestArgs::default();
    request_builder.model(&config.model).messages(messages);

    // Add tools if present
    if let Some(tools) = &request.tools {
        let openai_tools: Vec<ChatCompletionTools> = tools
            .iter()
            .map(|tool| ChatCompletionTools::Function(ChatCompletionTool {
                function: FunctionObject {
                    name: tool.name.to_string(),
                    description: tool.description.as_ref().map(|d| d.to_string()),
                    parameters: Some(serde_json::Value::Object((*tool.input_schema).clone())),
                    strict: None,
                },
            }))
            .collect();
        request_builder.tools(openai_tools);
    }
    
    // Apply options if provided
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            request_builder.temperature(temp);
        }
        if let Some(max_tokens) = opts.max_tokens {
            // Use max_completion_tokens (newer API) instead of deprecated max_tokens
            // This is required for o1/o3 reasoning models and works for all other models
            request_builder.max_completion_tokens(max_tokens);
        }
        if let Some(top_p) = opts.top_p {
            request_builder.top_p(top_p);
        }
        if let Some(stop) = &opts.stop_sequences {
            request_builder.stop(stop.clone());
        }
    }
    
    let openai_request = request_builder.build()?;
    
    let response = sdk_client
        .chat()
        .create(openai_request)
        .await?;
    
    let choice = response
        .choices
        .first()
        .ok_or_else(|| crate::error::Error::Other("No response from OpenAI".into()))?;
    
    // Extract tool calls if present
    let tool_uses_opt = choice.message.tool_calls.as_ref().map(|calls| {
        calls.iter().filter_map(|call| {
            match call {
                async_openai::types::chat::ChatCompletionMessageToolCalls::Function(fc) => {
                    Some(crate::ToolUse {
                        call_id: None,
                        id: fc.id.clone(),
                        name: fc.function.name.clone(),
                        input: serde_json::from_str(&fc.function.arguments)
                            .unwrap_or_else(|_| serde_json::Value::String(fc.function.arguments.clone())),
                    })
                }
                _ => None,
            }
        }).collect()
    });
    
    let message = crate::message::Message {
        role: crate::message::Role::Assistant,
        content: choice.message.content.clone().unwrap_or_default(),
        images: None,
        videos: None,
        documents: None,
        cache_control: None,
        tool_uses: tool_uses_opt.clone(),
        tool_call_id: None,
        tool_results: None,
        reasoning_items: None,
    };

    let usage = if let Some(usage) = &response.usage {
        crate::response::Usage {
            prompt_tokens: usage.prompt_tokens,
            completion_tokens: usage.completion_tokens,
            total_tokens: usage.total_tokens,
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
    
    let finish_reason = match &choice.finish_reason {
        Some(async_openai::types::chat::FinishReason::Stop) => crate::response::FinishReason::Stop,
        Some(async_openai::types::chat::FinishReason::Length) => crate::response::FinishReason::Length,
        Some(async_openai::types::chat::FinishReason::ContentFilter) => crate::response::FinishReason::ContentFilter,
        Some(async_openai::types::chat::FinishReason::ToolCalls) => crate::response::FinishReason::ToolUse,
        Some(async_openai::types::chat::FinishReason::FunctionCall) => crate::response::FinishReason::ToolUse,
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
        reasoning_summary: None,
        citations: None,
    })
}

pub(super) async fn stream(
    sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &CompletionRequest,
) -> Result<CompletionStream> {
    use async_openai::types::chat::{
        ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
        ChatCompletionRequestUserMessageArgs, ChatCompletionRequestAssistantMessageArgs,
        ChatCompletionRequestToolMessageArgs, ChatCompletionTool, ChatCompletionTools,
        ChatCompletionMessageToolCall, ChatCompletionMessageToolCalls,
        FunctionObject, CreateChatCompletionRequestArgs, ChatCompletionStreamOptions,
    };

    // Convert our messages to OpenAI format - handle tool results and tool uses
    let mut messages = Vec::new();
    for msg in &request.messages {
        match msg.role {
            crate::message::Role::System => {
                messages.push(ChatCompletionRequestMessage::System(
                    ChatCompletionRequestSystemMessageArgs::default()
                        .content(msg.content.clone())
                        .build()?
                ));
            }
            crate::message::Role::User => {
                // Check for multiple tool results first (new format)
                if let Some(tool_results) = &msg.tool_results {
                    // OpenAI requires one Tool message per result
                    for result in tool_results {
                        messages.push(ChatCompletionRequestMessage::Tool(
                            ChatCompletionRequestToolMessageArgs::default()
                                .content(result.content.clone())
                                .tool_call_id(result.tool_call_id.clone())
                                .build()?
                        ));
                    }
                    // Skip the outer message - we've expanded it into multiple Tool messages
                    continue;
                }
                // Legacy single tool result
                else if let Some(tool_call_id) = &msg.tool_call_id {
                    messages.push(ChatCompletionRequestMessage::Tool(
                        ChatCompletionRequestToolMessageArgs::default()
                            .content(msg.content.clone())
                            .tool_call_id(tool_call_id.clone())
                            .build()?
                    ));
                } else {
                    // Regular user message
                    messages.push(ChatCompletionRequestMessage::User(
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    ));
                }
            }
            crate::message::Role::Assistant => {
                let mut builder = ChatCompletionRequestAssistantMessageArgs::default();

                // Add content if present
                if !msg.content.is_empty() {
                    builder.content(msg.content.clone());
                }

                // Add tool calls if present
                if let Some(tool_uses) = &msg.tool_uses {
                    let tool_calls: Vec<ChatCompletionMessageToolCalls> = tool_uses.iter().map(|tu| {
                        ChatCompletionMessageToolCalls::Function(ChatCompletionMessageToolCall {
                            id: tu.id.clone(),
                            function: async_openai::types::chat::FunctionCall {
                                name: tu.name.clone(),
                                arguments: tu.input.to_string(),
                            },
                        })
                    }).collect();
                    builder.tool_calls(tool_calls);
                }

                messages.push(ChatCompletionRequestMessage::Assistant(builder.build()?));
            }
        }
    }

    // Build the request
    let mut request_builder = CreateChatCompletionRequestArgs::default();
    request_builder
        .model(&config.model)
        .messages(messages)
        .stream(true)
        .stream_options(ChatCompletionStreamOptions {
            include_usage: Some(true),
            include_obfuscation: None,
        });

    // Add tools if present
    if let Some(tools) = &request.tools {
        let openai_tools: Vec<ChatCompletionTools> = tools
            .iter()
            .map(|tool| ChatCompletionTools::Function(ChatCompletionTool {
                function: FunctionObject {
                    name: tool.name.to_string(),
                    description: tool.description.as_ref().map(|d| d.to_string()),
                    parameters: Some(serde_json::Value::Object((*tool.input_schema).clone())),
                    strict: None,
                },
            }))
            .collect();
        request_builder.tools(openai_tools);
    }

    // Apply options if provided
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            request_builder.temperature(temp);
        }
        if let Some(max_tokens) = opts.max_tokens {
            // Use max_completion_tokens (newer API) instead of deprecated max_tokens
            // This is required for o1/o3 reasoning models and works for all other models
            request_builder.max_completion_tokens(max_tokens);
        }
        if let Some(top_p) = opts.top_p {
            request_builder.top_p(top_p);
        }
        if let Some(stop) = &opts.stop_sequences {
            request_builder.stop(stop.clone());
        }
    }

    let openai_request = request_builder.build()?;
    
    // Make the streaming API call
    let stream = sdk_client
        .chat()
        .create_stream(openai_request)
        .await?;
    
    Ok(CompletionStream::openai(stream, config.model.clone()))
}
