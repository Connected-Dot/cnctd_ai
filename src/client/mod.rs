pub mod config;
pub mod options;

pub use config::{AnthropicConfig, OpenAiConfig};
pub use options::ClientOptions;

#[derive(Clone)]
pub struct Client {
    provider: ProviderType,
    options: ClientOptions,
}

impl Client {
    pub fn anthropic(
    config: AnthropicConfig,
    options: Option<ClientOptions>,
) -> Result<Self, crate::error::Error> {
    let options = options.unwrap_or_default();
    
    Ok(Self {
        provider: ProviderType::Anthropic { config },
        options,
    })
}
    
    pub fn openai(
        config: OpenAiConfig,
        options: Option<ClientOptions>,
    ) -> Result<Self, crate::error::Error> {
        let options = options.unwrap_or_default();
        
        // Create the OpenAI config
        let mut openai_config = async_openai::config::OpenAIConfig::new()
            .with_api_key(&config.api_key);
        
        if let Some(org) = &config.organization {
            openai_config = openai_config.with_org_id(org);
        }
        
        if let Some(base_url) = &options.base_url {
            openai_config = openai_config.with_api_base(base_url);
        }
        
        let sdk_client = async_openai::Client::with_config(openai_config);
        
        Ok(Self {
            provider: ProviderType::OpenAi {
                sdk_client,
                config,
            },
            options,
        })
    }

     pub async fn complete(
        &self,
        request: crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::response::CompletionResponse> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                self.complete_anthropic(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                self.complete_openai(sdk_client, config, &request).await
            }
        }
    }
    
    async fn complete_anthropic(
        &self,
        config: &AnthropicConfig,
        request: &crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::response::CompletionResponse> {
        use anthropic_sdk::{Anthropic, MessageCreateBuilder, MessageContent, ContentBlockParam};
        
        let client = Anthropic::new(&config.api_key)
            .map_err(|e| crate::error::Error::AnthropicError(e.to_string()))?;
        
        let mut builder = MessageCreateBuilder::new(&config.model, 
            request.options.as_ref().and_then(|o| o.max_tokens).unwrap_or(4096));
        
        // Add system message if present
        if let Some(system_msg) = request.messages.iter().find(|m| matches!(m.role, crate::message::Role::System)) {
            builder = builder.system(&system_msg.content);
        }
        
        // Add user/assistant messages - handle tool results and tool uses
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
                                is_error: Some(false),  // <-- Change None to Some(false)
                            }
                        ]));

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
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    input_schema: serde_json::from_value(tool.input_schema.clone())
                        .unwrap_or_else(|_| anthropic_sdk::types::ToolInputSchema { 
                            // Provide a default schema if parsing fails
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
                    tool_uses.push(crate::ToolUse {
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
            tool_uses: tool_uses_opt.clone(),
            tool_call_id: None,
        };
        
        let usage = crate::response::Usage {
            prompt_tokens: anthropic_response.usage.input_tokens,
            completion_tokens: anthropic_response.usage.output_tokens,
            total_tokens: anthropic_response.usage.input_tokens + anthropic_response.usage.output_tokens,
        };
        
        let finish_reason = match anthropic_response.stop_reason {
            Some(anthropic_sdk::StopReason::EndTurn) => crate::response::FinishReason::Stop,
            Some(anthropic_sdk::StopReason::MaxTokens) => crate::response::FinishReason::Length,
            Some(anthropic_sdk::StopReason::StopSequence) => crate::response::FinishReason::Stop,
            Some(anthropic_sdk::StopReason::ToolUse) => crate::response::FinishReason::ToolUse,
            _ => crate::response::FinishReason::Other,
        };
        
        Ok(crate::response::CompletionResponse {
            message,
            usage,
            finish_reason,
            model: anthropic_response.model,
            tool_uses: tool_uses_opt,
        })
    }
    
    async fn complete_openai(
        &self,
        sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
        config: &OpenAiConfig,
        request: &crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::response::CompletionResponse> {
        use async_openai::types::{
            ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
            ChatCompletionRequestUserMessageArgs, ChatCompletionRequestAssistantMessageArgs,
            ChatCompletionRequestToolMessageArgs, ChatCompletionTool, ChatCompletionToolType,
            FunctionObject, CreateChatCompletionRequestArgs,
        };
        
        // Convert our messages to OpenAI format
        let mut messages = Vec::new();
        for msg in &request.messages {
            let openai_msg = match msg.role {
                crate::message::Role::System => {
                    ChatCompletionRequestMessage::System(
                        ChatCompletionRequestSystemMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    )
                }
                crate::message::Role::User => {
                    // Check if this is a tool result
                    if let Some(tool_call_id) = &msg.tool_call_id {
                        ChatCompletionRequestMessage::Tool(
                            ChatCompletionRequestToolMessageArgs::default()
                                .content(msg.content.clone())
                                .tool_call_id(tool_call_id.clone())
                                .build()?
                        )
                    } else {
                        ChatCompletionRequestMessage::User(
                            ChatCompletionRequestUserMessageArgs::default()
                                .content(msg.content.clone())
                                .build()?
                        )
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
                        let tool_calls: Vec<_> = tool_uses.iter().map(|tu| {
                            async_openai::types::ChatCompletionMessageToolCall {
                                id: tu.id.clone(),
                                r#type: async_openai::types::ChatCompletionToolType::Function,
                                function: async_openai::types::FunctionCall {
                                    name: tu.name.clone(),
                                    arguments: tu.input.to_string(),
                                },
                            }
                        }).collect();
                        builder.tool_calls(tool_calls);
                    }
                    
                    ChatCompletionRequestMessage::Assistant(builder.build()?)
                }
            };
            messages.push(openai_msg);
        }
        
        // Build the request
        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(&config.model).messages(messages);
        
        // Add tools if present
        if let Some(tools) = &request.tools {
            let openai_tools: Vec<ChatCompletionTool> = tools
                .iter()
                .map(|tool| ChatCompletionTool {
                    r#type: ChatCompletionToolType::Function,
                    function: FunctionObject {
                        name: tool.name.clone(),
                        description: Some(tool.description.clone()),
                        parameters: Some(tool.input_schema.clone()),
                        strict: None,
                    },
                })
                .collect();
            request_builder.tools(openai_tools);
        }
        
        // Apply options if provided
        if let Some(opts) = &request.options {
            if let Some(temp) = opts.temperature {
                request_builder.temperature(temp);
            }
            if let Some(max_tokens) = opts.max_tokens {
                request_builder.max_tokens(max_tokens);
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
            calls.iter().map(|call| crate::ToolUse {
                id: call.id.clone(),
                name: call.function.name.clone(),
                input: serde_json::from_str(&call.function.arguments)
                    .unwrap_or_else(|_| serde_json::Value::String(call.function.arguments.clone())),
            }).collect()
        });
        
        let message = crate::message::Message {
            role: crate::message::Role::Assistant,
            content: choice.message.content.clone().unwrap_or_default(),
            tool_uses: tool_uses_opt.clone(),
            tool_call_id: None,
        };
        
        let usage = if let Some(usage) = &response.usage {
            crate::response::Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            }
        } else {
            crate::response::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            }
        };
        
        let finish_reason = match &choice.finish_reason {
            Some(async_openai::types::FinishReason::Stop) => crate::response::FinishReason::Stop,
            Some(async_openai::types::FinishReason::Length) => crate::response::FinishReason::Length,
            Some(async_openai::types::FinishReason::ContentFilter) => crate::response::FinishReason::ContentFilter,
            Some(async_openai::types::FinishReason::ToolCalls) => crate::response::FinishReason::ToolUse,
            Some(async_openai::types::FinishReason::FunctionCall) => crate::response::FinishReason::ToolUse,
            _ => crate::response::FinishReason::Other,
        };
        
        Ok(crate::response::CompletionResponse {
            message,
            usage,
            finish_reason,
            model: response.model,
            tool_uses: tool_uses_opt,
        })
    }

    pub async fn complete_stream(
        &self,
        request: crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::stream::CompletionStream> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                self.stream_anthropic(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                self.stream_openai(sdk_client, config, &request).await
            }
        }
    }

    async fn stream_anthropic(
        &self,
        config: &AnthropicConfig,
        request: &crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::stream::CompletionStream> {
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
        
        use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
        
        // Build the request body manually
        let mut body = serde_json::json!({
            "model": config.model,
            "max_tokens": request.options.as_ref().and_then(|o| o.max_tokens).unwrap_or(4096),
            "messages": [],
            "stream": true,
        });

        // Add system message if present
        if let Some(system_msg) = request.messages.iter().find(|m| matches!(m.role, crate::message::Role::System)) {
            body["system"] = serde_json::json!(system_msg.content);
        }

        // Add user/assistant messages
        let mut messages = Vec::new();
        for msg in request.messages.iter().filter(|m| !matches!(m.role, crate::message::Role::System)) {
            let role = match msg.role {
                crate::message::Role::User => "user",
                crate::message::Role::Assistant => "assistant",
                crate::message::Role::System => continue,
            };
            messages.push(serde_json::json!({
                "role": role,
                "content": msg.content,
            }));
        }
        body["messages"] = serde_json::json!(messages);

        // ADD THIS: Include tools if present
        if let Some(tools) = &request.tools {
            let tools_json: Vec<_> = tools.iter().map(|tool| {
                serde_json::json!({
                    "name": tool.name,
                    "description": tool.description,
                    "input_schema": tool.input_schema,
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
        
        Ok(crate::stream::CompletionStream::anthropic_custom(stream, config.model.clone()))
    }

    async fn stream_openai(
        &self,
        sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
        config: &OpenAiConfig,
        request: &crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::stream::CompletionStream> {
        use async_openai::types::{
            ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
            ChatCompletionRequestUserMessageArgs, ChatCompletionRequestAssistantMessageArgs,
            CreateChatCompletionRequestArgs,
        };
        
        // Convert our messages to OpenAI format (same as complete_openai)
        let mut messages = Vec::new();
        for msg in &request.messages {
            let openai_msg = match msg.role {
                crate::message::Role::System => {
                    ChatCompletionRequestMessage::System(
                        ChatCompletionRequestSystemMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    )
                }
                crate::message::Role::User => {
                    ChatCompletionRequestMessage::User(
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    )
                }
                crate::message::Role::Assistant => {
                    ChatCompletionRequestMessage::Assistant(
                        ChatCompletionRequestAssistantMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    )
                }
            };
            messages.push(openai_msg);
        }
        
        // Build the request
        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder
            .model(&config.model)
            .messages(messages)
            .stream(true); // Enable streaming!

        // ADD THIS: Add tools if present
        if let Some(tools) = &request.tools {
            use async_openai::types::{ChatCompletionTool, ChatCompletionToolType, FunctionObject};
            
            let openai_tools: Vec<ChatCompletionTool> = tools
                .iter()
                .map(|tool| ChatCompletionTool {
                    r#type: ChatCompletionToolType::Function,
                    function: FunctionObject {
                        name: tool.name.clone(),
                        description: Some(tool.description.clone()),
                        parameters: Some(tool.input_schema.clone()),
                        strict: None,
                    },
                })
                .collect();
            request_builder.tools(openai_tools);
        }

        // Apply options if provided
        if let Some(opts) = &request.options {
            if let Some(temp) = opts.temperature {
                request_builder.temperature(temp);
            }
            if let Some(max_tokens) = opts.max_tokens {
                request_builder.max_tokens(max_tokens);
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
        
        Ok(crate::stream::CompletionStream::openai(stream, config.model.clone()))
    }
}

#[derive(Clone)]
enum ProviderType {
    Anthropic {
        config: AnthropicConfig,
    },
    OpenAi {
        sdk_client: async_openai::Client<async_openai::config::OpenAIConfig>,
        config: OpenAiConfig,
    },
}
