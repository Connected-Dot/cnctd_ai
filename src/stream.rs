use futures::StreamExt;
use eventsource_stream::Eventsource;

/// A chunk of streaming completion data
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// The incremental text content, if any
    pub delta: Option<String>,
    /// The finish reason, if this is the final chunk
    pub finish_reason: Option<crate::response::FinishReason>,
}

impl StreamChunk {
    /// Helper to get text from this chunk
    pub fn text(&self) -> Option<&str> {
        self.delta.as_deref()
    }
}

/// A stream of completion chunks from any provider
pub struct CompletionStream {
    inner: StreamType,
    model: String,
    accumulated_text: String,
    usage: Option<crate::response::Usage>,
    finish_reason: Option<crate::response::FinishReason>,
    tool_uses: Vec<crate::tool::ToolUse>,  
}

impl CompletionStream {
    /// Create a new stream from a raw HTTP byte stream (custom implementation)
    /// 
    /// This is used as a workaround for the anthropic-sdk-rust streaming bug.
    /// See the TODO comment in client/mod.rs::stream_anthropic()
    pub fn anthropic_custom(
        stream: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
        model: String,
    ) -> Self {
        let event_stream = stream.eventsource();
        Self {
            inner: StreamType::AnthropicCustom(Box::pin(event_stream)),
            model,
            accumulated_text: String::new(),
            usage: None,
            finish_reason: None,
            tool_uses: Vec::new(),
        }
    }

    pub fn openai(
        stream: async_openai::types::ChatCompletionResponseStream,
        model: String,
    ) -> Self {
        Self {
            inner: StreamType::OpenAi(stream),
            model,
            accumulated_text: String::new(),
            usage: None,
            finish_reason: None,
            tool_uses: Vec::new(),
        }
    }

    /// Get the next chunk from the stream
    pub async fn next(&mut self) -> Option<Result<StreamChunk, crate::error::Error>> {
        loop {
            let event = match &mut self.inner {
                StreamType::AnthropicCustom(stream) => {
                    match stream.next().await {
                        Some(Ok(event)) => event,
                        Some(Err(e)) => {
                            return Some(Err(crate::error::Error::Other(
                                format!("Stream error: {}", e)
                            )));
                        }
                        None => return None,
                    }
                }
                StreamType::OpenAi(stream) => {
                    match stream.next().await {
                        Some(Ok(response)) => {
                            if let Some(choice) = response.choices.get(0) {
                                // Handle tool calls
                                if let Some(tool_calls) = &choice.delta.tool_calls {
                                    for tool_call in tool_calls {
                                        // OpenAI streams tool calls with an index
                                        // We need to accumulate them properly
                                        if let Some(function) = &tool_call.function {
                                            // If we have a name, this is a new tool call
                                            if let Some(name) = &function.name {
                                                self.tool_uses.push(crate::tool::ToolUse {
                                                    id: tool_call.id.clone().unwrap_or_default(),
                                                    name: name.clone(),
                                                    input: serde_json::json!({}), // Will accumulate arguments next
                                                });
                                            }
                                            
                                            // If we have arguments, accumulate them to the last tool
                                            if let Some(arguments) = &function.arguments {
                                                if let Some(last_tool) = self.tool_uses.last_mut() {
                                                    // OpenAI streams JSON as strings, so we accumulate and parse later
                                                    // For now, just store the raw string
                                                    if let Some(current_args) = last_tool.input.as_str() {
                                                        let combined = format!("{}{}", current_args, arguments);
                                                        // Try to parse as JSON, if it fails keep as string
                                                        last_tool.input = serde_json::from_str(&combined)
                                                            .unwrap_or(serde_json::Value::String(combined));
                                                    } else {
                                                        last_tool.input = serde_json::Value::String(arguments.clone());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // Handle text content
                                if let Some(content) = &choice.delta.content {
                                    self.accumulated_text.push_str(content);
                                    return Some(Ok(StreamChunk {
                                        delta: Some(content.clone()),
                                        finish_reason: None,
                                    }));
                                }
                                
                                // Handle finish reason
                                if let Some(finish_reason) = &choice.finish_reason {
                                    self.finish_reason = Some(match finish_reason {
                                        async_openai::types::FinishReason::Stop => crate::response::FinishReason::Stop,
                                        async_openai::types::FinishReason::Length => crate::response::FinishReason::Length,
                                        async_openai::types::FinishReason::ContentFilter => crate::response::FinishReason::ContentFilter,
                                        async_openai::types::FinishReason::ToolCalls => crate::response::FinishReason::ToolUse,
                                        async_openai::types::FinishReason::FunctionCall => crate::response::FinishReason::ToolUse,
                                    });
                                }
                                
                                // Update usage if present
                                if let Some(usage) = &response.usage {
                                    self.usage = Some(crate::response::Usage {
                                        prompt_tokens: usage.prompt_tokens,
                                        completion_tokens: usage.completion_tokens,
                                        total_tokens: usage.total_tokens,
                                    });
                                }
                                
                                // If we have finish reason or tool uses but no text, still return a chunk
                                if self.finish_reason.is_some() || !self.tool_uses.is_empty() {
                                    return Some(Ok(StreamChunk {
                                        delta: None,
                                        finish_reason: self.finish_reason.clone(),
                                    }));
                                }
                            }
                            
                            // Continue to next chunk if nothing to return
                            continue;
                        }
                        Some(Err(e)) => {
                            return Some(Err(crate::error::Error::Other(
                                format!("Stream error: {}", e)
                            )));
                        }
                        None => return None,
                    }
                }
            };
            // Parse the SSE event
            if let Some(chunk) = self.handle_anthropic_sse_event(event).await? {
                return Some(Ok(chunk));
            }
            // If no chunk returned, continue to next event
        }
        
    }

    async fn handle_openai_chunk(
        &mut self,
        response: async_openai::types::CreateChatCompletionStreamResponse,
    ) -> Option<Result<StreamChunk, crate::error::Error>> {
        use async_openai::types::ChatCompletionStreamResponseDelta;

        // Get the first choice (OpenAI can return multiple choices but we'll use the first)
        let choice = response.choices.first()?;

        // Extract text delta
        let delta = &choice.delta;
        let text_delta = match delta {
            ChatCompletionStreamResponseDelta { content: Some(content), .. } => {
                self.accumulated_text.push_str(content);
                Some(content.clone())
            }
            _ => None,
        };

        // Update finish reason if present
        if let Some(finish_reason) = &choice.finish_reason {
            self.finish_reason = Some(match finish_reason {
                async_openai::types::FinishReason::Stop => crate::response::FinishReason::Stop,
                async_openai::types::FinishReason::Length => crate::response::FinishReason::Length,
                async_openai::types::FinishReason::ContentFilter => crate::response::FinishReason::ContentFilter,
                async_openai::types::FinishReason::ToolCalls => crate::response::FinishReason::ToolUse,
                async_openai::types::FinishReason::FunctionCall => crate::response::FinishReason::ToolUse,
            });
        }

        // Update usage if present (typically only in the last chunk)
        if let Some(usage) = &response.usage {
            self.usage = Some(crate::response::Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            });
        }

        // Return chunk if we have text, otherwise get next chunk
        if text_delta.is_some() || self.finish_reason.is_some() {
            Some(Ok(StreamChunk {
                delta: text_delta,
                finish_reason: self.finish_reason.clone(),
            }))
        } else {
            // No content yet, continue to next chunk
            Box::pin(self.next()).await
        }
    }

    async fn handle_anthropic_sse_event(
        &mut self,
        event: eventsource_stream::Event,
    ) -> Option<Option<StreamChunk>> {
        // Parse the event data as JSON
        let data = match serde_json::from_str::<serde_json::Value>(&event.data) {
            Ok(data) => data,
            Err(_) => return Some(None), // Skip unparseable events
        };

        let event_type = data["type"].as_str()?;

        match event_type {
            "message_start" => {
                // Extract initial usage info
                if let Some(usage) = data["message"]["usage"].as_object() {
                    let input_tokens = usage["input_tokens"].as_u64().unwrap_or(0) as u32;
                    let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0) as u32;
                    self.usage = Some(crate::response::Usage {
                        prompt_tokens: input_tokens,
                        completion_tokens: output_tokens,
                        total_tokens: input_tokens + output_tokens,
                    });
                }
                Some(None) // Continue to next event
            }
            "content_block_start" => {
                // Check if this is a tool use block
                if let Some(content_block) = data["content_block"].as_object() {
                    if content_block["type"].as_str() == Some("tool_use") {
                        // Create a new tool use with id and name
                        let id = content_block["id"].as_str().unwrap_or("").to_string();
                        let name = content_block["name"].as_str().unwrap_or("").to_string();
                        self.tool_uses.push(crate::tool::ToolUse {
                            id,
                            name,
                            input: serde_json::json!({}), // Will be filled by deltas
                        });
                    }
                }
                Some(None) // Continue to next event
            }
            "content_block_delta" => {
                // Check if this is a tool use input delta
                if let Some(delta) = data["delta"].as_object() {
                    if delta["type"].as_str() == Some("input_json_delta") {
                        // Accumulate JSON input to the last tool
                        if let (Some(partial_json), Some(last_tool)) = (delta["partial_json"].as_str(), self.tool_uses.last_mut()) {
                            // Anthropic streams JSON as strings, accumulate and parse
                            if let Some(current_input) = last_tool.input.as_str() {
                                let combined = format!("{}{}", current_input, partial_json);
                                // Try to parse, keep as string if invalid
                                last_tool.input = serde_json::from_str(&combined)
                                    .unwrap_or(serde_json::Value::String(combined));
                            } else if last_tool.input.is_object() && last_tool.input.as_object().unwrap().is_empty() {
                                // First chunk of input
                                last_tool.input = serde_json::Value::String(partial_json.to_string());
                            }
                        }
                        Some(None) // Continue, no text to return
                    } else if let Some(text) = delta["text"].as_str() {
                        // Existing text handling
                        self.accumulated_text.push_str(text);
                        Some(Some(StreamChunk {
                            delta: Some(text.to_string()),
                            finish_reason: None,
                        }))
                    } else {
                        Some(None)
                    }
                } else {
                    Some(None)
                }
            }
            "content_block_stop" => {
                // Finalize the last tool's JSON input if needed
                if let Some(last_tool) = self.tool_uses.last_mut() {
                    if let Some(json_str) = last_tool.input.as_str() {
                        // Try final parse of accumulated JSON
                        if let Ok(parsed) = serde_json::from_str(json_str) {
                            last_tool.input = parsed;
                        }
                    }
                }
                Some(None) // Continue to next event
            }
            "message_delta" => {
                // Update usage and finish reason
                if let Some(usage) = data["usage"].as_object() {
                    let output_tokens = usage["output_tokens"].as_u64().unwrap_or(0) as u32;
                    if let Some(existing_usage) = &mut self.usage {
                        existing_usage.completion_tokens = output_tokens;
                        existing_usage.total_tokens = existing_usage.prompt_tokens + output_tokens;
                    }
                }

                if let Some(stop_reason) = data["delta"]["stop_reason"].as_str() {
                    self.finish_reason = Some(match stop_reason {
                        "end_turn" => crate::response::FinishReason::Stop,
                        "max_tokens" => crate::response::FinishReason::Length,
                        "stop_sequence" => crate::response::FinishReason::Stop,
                        _ => crate::response::FinishReason::Other,
                    });
                }
                Some(None) // Continue to next event
            }
            "message_stop" => {
                // Stream complete
                None
            }
            _ => {
                // Unknown event type, skip
                Some(None)
            }
        }
    }

    /// Get the final response after streaming completes
    pub fn final_response(&self) -> Option<crate::response::CompletionResponse> {
        // Need either text or tool uses to have a response
        if self.accumulated_text.is_empty() && self.tool_uses.is_empty() {
            return None;
        }

        let tool_uses_opt = if !self.tool_uses.is_empty() {
            Some(self.tool_uses.clone())
        } else {
            None
        };

        Some(crate::response::CompletionResponse {
            message: crate::message::Message {
                role: crate::message::Role::Assistant,
                content: self.accumulated_text.clone(),
                tool_uses: tool_uses_opt.clone(),
                tool_call_id: None,
            },
            usage: self.usage.clone().unwrap_or(crate::response::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            }),
            finish_reason: self.finish_reason.clone().unwrap_or(crate::response::FinishReason::Other),
            model: self.model.clone(),
            tool_uses: tool_uses_opt,
        })
    }
}

enum StreamType {
    AnthropicCustom(
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = Result<eventsource_stream::Event, eventsource_stream::EventStreamError<reqwest::Error>>>
                    + Send
            >
        >
    ),
    OpenAi(async_openai::types::ChatCompletionResponseStream),
}