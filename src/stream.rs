use futures::StreamExt;
use eventsource_stream::Eventsource;
use crate::response::{GroundingMetadata, CodeExecutionResult};

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
    /// Grounding metadata from Gemini search (accumulated during streaming)
    grounding_metadata: Option<GroundingMetadata>,
    /// Code execution results from Gemini (accumulated during streaming)
    code_execution_results: Vec<CodeExecutionResult>,
    /// Google Maps widget token from Gemini
    google_maps_widget_token: Option<String>,
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
            grounding_metadata: None,
            code_execution_results: Vec::new(),
            google_maps_widget_token: None,
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
            grounding_metadata: None,
            code_execution_results: Vec::new(),
            google_maps_widget_token: None,
        }
    }

    /// Create a new stream from a raw HTTP byte stream for Gemini
    pub fn gemini_custom(
        stream: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
        model: String,
    ) -> Self {
        let event_stream = stream.eventsource();
        Self {
            inner: StreamType::GeminiCustom(Box::pin(event_stream)),
            model,
            accumulated_text: String::new(),
            usage: None,
            finish_reason: None,
            tool_uses: Vec::new(),
            grounding_metadata: None,
            code_execution_results: Vec::new(),
            google_maps_widget_token: None,
        }
    }

    /// Get the next chunk from the stream
    pub async fn next(&mut self) -> Option<Result<StreamChunk, crate::error::Error>> {
        loop {
            match &mut self.inner {
                StreamType::AnthropicCustom(stream) => {
                    let event = match stream.next().await {
                        Some(Ok(event)) => event,
                        Some(Err(e)) => {
                            return Some(Err(crate::error::Error::Other(
                                format!("Stream error: {}", e)
                            )));
                        }
                        None => return None,
                    };
                    // Parse the SSE event
                    if let Some(chunk) = self.handle_anthropic_sse_event(event).await? {
                        return Some(Ok(chunk));
                    }
                    // If no chunk returned, continue to next event
                }
                StreamType::OpenAi(stream) => {
                    match stream.next().await {
                        Some(Ok(response)) => {
                            let mut has_usage_update = false;
                            
                            // Update usage if present (check this first, before choices)
                            // OpenAI sends usage in a separate chunk at the end
                            if let Some(usage) = &response.usage {
                                self.usage = Some(crate::response::Usage {
                                    prompt_tokens: usage.prompt_tokens,
                                    completion_tokens: usage.completion_tokens,
                                    total_tokens: usage.total_tokens,
                                });
                                has_usage_update = true;
                            }
                            
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
                            }

                            // If we have something to report (finish reason, tool uses, or usage),
                            // return a chunk. Otherwise continue to next chunk.
                            if self.finish_reason.is_some() || !self.tool_uses.is_empty() || has_usage_update {
                                return Some(Ok(StreamChunk {
                                    delta: None,
                                    finish_reason: self.finish_reason.clone(),
                                }));
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
                StreamType::GeminiCustom(stream) => {
                    let event = match stream.next().await {
                        Some(Ok(event)) => event,
                        Some(Err(e)) => {
                            return Some(Err(crate::error::Error::GeminiError(
                                format!("Stream error: {}", e)
                            )));
                        }
                        None => return None,
                    };
                    // Parse the SSE event
                    if let Some(chunk) = self.handle_gemini_sse_event(event).await? {
                        return Some(Ok(chunk));
                    }
                    // If no chunk returned, continue to next event
                }
            }
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
                        // If parsing fails, fall back to empty object (API requires object, not string)
                        last_tool.input = serde_json::from_str(json_str)
                            .unwrap_or_else(|_| serde_json::json!({}));
                    }
                    // Ensure input is always an object - Anthropic API rejects string inputs
                    if !last_tool.input.is_object() {
                        last_tool.input = serde_json::json!({});
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
                        "tool_use" => crate::response::FinishReason::ToolUse,
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

    /// Handle Gemini SSE events
    /// 
    /// Gemini streaming format:
    /// - Each SSE data line contains a JSON object with candidates array
    /// - candidates[0].content.parts contains text or functionCall objects
    /// - candidates[0].groundingMetadata contains search grounding info
    /// - candidates[0].googleMapsWidgetContextToken for maps widget
    /// - usageMetadata contains token counts
    /// - candidates[0].finishReason indicates completion
    async fn handle_gemini_sse_event(
        &mut self,
        event: eventsource_stream::Event,
    ) -> Option<Option<StreamChunk>> {
        // Parse the event data as JSON
        let data = match serde_json::from_str::<serde_json::Value>(&event.data) {
            Ok(data) => data,
            Err(_) => return Some(None), // Skip unparseable events
        };

        // Extract candidate content
        let candidate = match data["candidates"].get(0) {
            Some(c) => c,
            None => return Some(None), // No candidate yet
        };

        // Extract grounding metadata if present (for search billing)
        if let Some(gm) = candidate.get("groundingMetadata") {
            if let Ok(metadata) = serde_json::from_value::<GroundingMetadata>(gm.clone()) {
                self.grounding_metadata = Some(metadata);
            }
        }

        // Extract Google Maps widget token if present
        if let Some(token) = candidate.get("googleMapsWidgetContextToken") {
            if let Some(token_str) = token.as_str() {
                self.google_maps_widget_token = Some(token_str.to_string());
            }
        }

        // Extract parts from content
        let parts = match candidate["content"]["parts"].as_array() {
            Some(p) => p,
            None => return Some(None), // No parts
        };

        let mut text_delta = String::new();
        
        for part in parts {
            // Handle text content
            if let Some(text) = part["text"].as_str() {
                text_delta.push_str(text);
            }
            
            // Handle function calls
            if let Some(function_call) = part["functionCall"].as_object() {
                let name = function_call["name"].as_str().unwrap_or("").to_string();
                let args = function_call.get("args").cloned().unwrap_or(serde_json::json!({}));
                
                // Generate a unique ID (Gemini doesn't provide one)
                let id = format!("gemini_call_{}", uuid::Uuid::new_v4());
                
                self.tool_uses.push(crate::tool::ToolUse {
                    id,
                    name,
                    input: args,
                });
            }

            // Handle executable code part (for code execution billing)
            if let Some(executable_code) = part.get("executableCode") {
                let code = executable_code["code"].as_str().map(|s| s.to_string());
                let language = executable_code["language"].as_str().map(|s| s.to_string());
                self.code_execution_results.push(CodeExecutionResult {
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
                if let Some(last) = self.code_execution_results.last_mut() {
                    if last.outcome.is_none() {
                        last.outcome = outcome;
                        last.output = output;
                        continue;
                    }
                }
                // Otherwise create a new result entry
                self.code_execution_results.push(CodeExecutionResult {
                    code: None,
                    language: None,
                    outcome,
                    output,
                });
            }
        }

        // Update usage if present
        if let Some(usage_metadata) = data.get("usageMetadata") {
            let prompt_tokens = usage_metadata["promptTokenCount"].as_u64().unwrap_or(0) as u32;
            let completion_tokens = usage_metadata["candidatesTokenCount"].as_u64().unwrap_or(0) as u32;
            let total_tokens = usage_metadata["totalTokenCount"].as_u64().unwrap_or(0) as u32;
            self.usage = Some(crate::response::Usage {
                prompt_tokens,
                completion_tokens,
                total_tokens,
            });
        }

        // Check for finish reason
        if let Some(finish_reason_str) = candidate["finishReason"].as_str() {
            self.finish_reason = Some(match finish_reason_str {
                "STOP" => crate::response::FinishReason::Stop,
                "MAX_TOKENS" => crate::response::FinishReason::Length,
                "SAFETY" => crate::response::FinishReason::ContentFilter,
                "RECITATION" => crate::response::FinishReason::ContentFilter,
                _ => {
                    // Check if we have tool uses
                    if !self.tool_uses.is_empty() {
                        crate::response::FinishReason::ToolUse
                    } else {
                        crate::response::FinishReason::Other
                    }
                }
            });
        }

        // Accumulate text
        if !text_delta.is_empty() {
            self.accumulated_text.push_str(&text_delta);
            return Some(Some(StreamChunk {
                delta: Some(text_delta),
                finish_reason: None,
            }));
        }

        // If we have tool uses or finish reason but no text, still return a chunk
        if !self.tool_uses.is_empty() || self.finish_reason.is_some() {
            return Some(Some(StreamChunk {
                delta: None,
                finish_reason: self.finish_reason.clone(),
            }));
        }

        Some(None) // Continue to next event
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

        let code_results_opt = if !self.code_execution_results.is_empty() {
            Some(self.code_execution_results.clone())
        } else {
            None
        };

        Some(crate::response::CompletionResponse {
            message: crate::message::Message {
                role: crate::message::Role::Assistant,
                content: self.accumulated_text.clone(),
                tool_uses: tool_uses_opt.clone(),
                tool_call_id: None,
            tool_results: None,
            },
            usage: self.usage.clone().unwrap_or(crate::response::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            }),
            finish_reason: self.finish_reason.clone().unwrap_or(crate::response::FinishReason::Other),
            model: self.model.clone(),
            tool_uses: tool_uses_opt,
            grounding_metadata: self.grounding_metadata.clone(),
            code_execution_results: code_results_opt,
            google_maps_widget_token: self.google_maps_widget_token.clone(),
        })
    }

    pub fn tool_use(&self) -> Option<&crate::tool::ToolUse> {
        self.tool_uses.first()
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
    GeminiCustom(
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = Result<eventsource_stream::Event, eventsource_stream::EventStreamError<reqwest::Error>>>
                    + Send
            >
        >
    ),
}
