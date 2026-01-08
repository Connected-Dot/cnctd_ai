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
    /// Accumulated function call arguments (for OpenAI Responses API)
    accumulated_function_args: std::collections::HashMap<String, String>,
    /// Pending function names from OutputItemAdded (before arguments arrive)
    pending_function_names: std::collections::HashMap<String, String>,
    /// Pending call_ids from OutputItemAdded (for OpenAI Responses API)
    pending_call_ids: std::collections::HashMap<String, String>,
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
            accumulated_function_args: std::collections::HashMap::new(),
            pending_function_names: std::collections::HashMap::new(),
            pending_call_ids: std::collections::HashMap::new(),
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
            accumulated_function_args: std::collections::HashMap::new(),
            pending_function_names: std::collections::HashMap::new(),
            pending_call_ids: std::collections::HashMap::new(),
        }
    }

    /// Create a new stream for OpenAI Responses API
    pub fn openai_responses(
        stream: async_openai::types::responses::ResponseStream,
        model: String,
    ) -> Self {
        Self {
            inner: StreamType::OpenAiResponses(stream),
            model,
            accumulated_text: String::new(),
            usage: None,
            finish_reason: None,
            tool_uses: Vec::new(),
            grounding_metadata: None,
            code_execution_results: Vec::new(),
            google_maps_widget_token: None,
            accumulated_function_args: std::collections::HashMap::new(),
            pending_function_names: std::collections::HashMap::new(),
            pending_call_ids: std::collections::HashMap::new(),
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
            accumulated_function_args: std::collections::HashMap::new(),
            pending_function_names: std::collections::HashMap::new(),
            pending_call_ids: std::collections::HashMap::new(),
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
                            eprintln!("DEBUG OpenAI stream chunk: choices={}, usage={:?}", 
                                response.choices.len(),
                                response.usage.as_ref().map(|u| (u.prompt_tokens, u.completion_tokens))
                            );
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
                                eprintln!("DEBUG OpenAI choice: content={:?}, tool_calls={:?}, finish={:?}",
                                    choice.delta.content.as_ref().map(|s| s.chars().take(50).collect::<String>()),
                                    choice.delta.tool_calls.as_ref().map(|tc| tc.len()),
                                    choice.finish_reason
                                );
                                // Handle tool calls
                                if let Some(tool_calls) = &choice.delta.tool_calls {
                                    for tool_call in tool_calls {
                                        if let Some(function) = &tool_call.function {
                                            if let Some(name) = &function.name {
                                                self.tool_uses.push(crate::tool::ToolUse { call_id: None,
                                                    id: tool_call.id.clone().unwrap_or_default(),
                                                    name: name.clone(),
                                                    input: serde_json::json!({}),
                                                });
                                            }
                                            
                                            if let Some(arguments) = &function.arguments {
                                                if let Some(last_tool) = self.tool_uses.last_mut() {
                                                    if let Some(current_args) = last_tool.input.as_str() {
                                                        let combined = format!("{}{}", current_args, arguments);
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
                                
                                if let Some(content) = &choice.delta.content {
                                    self.accumulated_text.push_str(content);
                                    return Some(Ok(StreamChunk {
                                        delta: Some(content.clone()),
                                        finish_reason: None,
                                    }));
                                }
                                
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

                            if self.finish_reason.is_some() || !self.tool_uses.is_empty() || has_usage_update {
                                return Some(Ok(StreamChunk {
                                    delta: None,
                                    finish_reason: self.finish_reason.clone(),
                                }));
                            }

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
                StreamType::OpenAiResponses(stream) => {
                    // If we've already completed, don't poll again
                    if self.finish_reason.is_some() {
                        eprintln!("DEBUG: Responses stream already completed, returning None");
                        return None;
                    }
                    
                    eprintln!("DEBUG: About to poll OpenAI Responses stream...");
                    let poll_result = stream.next().await;
                    eprintln!("DEBUG: Poll result is_some={}", poll_result.is_some());
                    match poll_result {
                        Some(Ok(event)) => {
                            eprintln!("DEBUG: Got event from Responses stream");
                            if let Some(chunk) = self.handle_openai_responses_event(event) {
                                return Some(Ok(chunk));
                            }
                            continue;
                        }
                        Some(Err(e)) => {
                            let err_str = e.to_string();
                            // "Stream ended" is normal termination, not an error
                            if err_str.contains("Stream ended") {
                                eprintln!("DEBUG: Responses stream ended normally");
                                return None;
                            }
                            eprintln!("DEBUG: Responses stream error: {}", e);
                            return Some(Err(crate::error::Error::Other(
                                format!("Responses API stream error: {}", e)
                            )));
                        }
                        None => {
                            eprintln!("DEBUG: Responses stream ended (None)");
                            return None;
                        }
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
                    if let Some(chunk) = self.handle_gemini_sse_event(event).await? {
                        return Some(Ok(chunk));
                    }
                }
            }
        }
    }

    /// Handle OpenAI Responses API stream events
    fn handle_openai_responses_event(
        &mut self,
        event: async_openai::types::responses::ResponseEvent,
    ) -> Option<StreamChunk> {
        use async_openai::types::responses::ResponseEvent;
        
        match event {
            ResponseEvent::ResponseCreated(e) => {
                eprintln!("DEBUG Responses: Created, id={}", e.response.id);
                None
            }
            ResponseEvent::ResponseInProgress(_) => {
                eprintln!("DEBUG Responses: In progress");
                None
            }
            ResponseEvent::ResponseOutputItemAdded(e) => {
                eprintln!("DEBUG Responses: Output item added, index={}", e.output_index);
                // Capture function name for later use in Unknown event parsing
                if let async_openai::types::responses::OutputItem::FunctionCall(fc) = &e.item {
                    eprintln!("DEBUG Responses: OutputItemAdded FunctionCall id={}, call_id={}, name={}", fc.id, fc.call_id, fc.name);
                    self.pending_function_names.insert(fc.id.clone(), fc.name.clone());
                    self.pending_call_ids.insert(fc.id.clone(), fc.call_id.clone());
                }
                None
            }
            ResponseEvent::ResponseOutputTextDelta(e) => {
                eprintln!("DEBUG Responses: Text delta len={}", e.delta.len());
                self.accumulated_text.push_str(&e.delta);
                Some(StreamChunk {
                    delta: Some(e.delta),
                    finish_reason: None,
                })
            }
            ResponseEvent::ResponseOutputTextDone(e) => {
                eprintln!("DEBUG Responses: Text done, total len={}, accumulated={}", e.text.len(), self.accumulated_text.len());
                // If we haven't accumulated any text via deltas, use the full text from done event
                if self.accumulated_text.is_empty() && !e.text.is_empty() {
                    eprintln!("DEBUG Responses: Using text from OutputTextDone since accumulated was empty");
                    self.accumulated_text.push_str(&e.text);
                    return Some(StreamChunk {
                        delta: Some(e.text),
                        finish_reason: None,
                    });
                }
                None
            }
            // Handle ContentPartAdded - may contain initial text for some models
            ResponseEvent::ResponseContentPartAdded(e) => {
                eprintln!("DEBUG Responses: Content part added, type={}", e.part.part_type);
                if let Some(text) = &e.part.text {
                    if !text.is_empty() {
                        eprintln!("DEBUG Responses: ContentPartAdded has text len={}", text.len());
                        self.accumulated_text.push_str(text);
                        return Some(StreamChunk {
                            delta: Some(text.clone()),
                            finish_reason: None,
                        });
                    }
                }
                None
            }
            // Handle ContentPartDone - contains complete text for some models (like GPT-5.2)
            ResponseEvent::ResponseContentPartDone(e) => {
                eprintln!("DEBUG Responses: Content part done, type={}", e.part.part_type);
                if let Some(text) = &e.part.text {
                    // Only add if we haven't already accumulated this text
                    // (some models send both delta and done events)
                    if !text.is_empty() && self.accumulated_text.is_empty() {
                        eprintln!("DEBUG Responses: ContentPartDone has text len={}, accumulated was empty", text.len());
                        self.accumulated_text.push_str(text);
                        return Some(StreamChunk {
                            delta: Some(text.clone()),
                            finish_reason: None,
                        });
                    } else if !text.is_empty() {
                        eprintln!("DEBUG Responses: ContentPartDone has text but accumulated already has {} chars", self.accumulated_text.len());
                    }
                }
                None
            }
            ResponseEvent::ResponseFunctionCallArgumentsDelta(e) => {
                eprintln!("DEBUG Responses: Function args delta, item_id={}", e.item_id);
                // Accumulate function call arguments
                let entry = self.accumulated_function_args.entry(e.item_id).or_default();
                entry.push_str(&e.delta);
                None
            }
            ResponseEvent::ResponseFunctionCallArgumentsDone(e) => {
                eprintln!("DEBUG Responses: Function call done, name={}, args={}", e.name, e.arguments);
                // Create tool use from accumulated arguments
                let args_value: serde_json::Value = serde_json::from_str(&e.arguments)
                    .unwrap_or_else(|_| serde_json::Value::String(e.arguments.clone()));
                // Get the call_id from OutputItemAdded (required for function_call_output)
                let call_id = self.pending_call_ids.get(&e.item_id).cloned();
                eprintln!("DEBUG Responses: Creating ToolUse: id={}, call_id={:?}", e.item_id, call_id);
                self.tool_uses.push(crate::tool::ToolUse {
                    id: e.item_id.clone(),
                    call_id,
                    name: e.name,
                    input: args_value,
                });
                None
            }
            ResponseEvent::ResponseOutputItemDone(e) => {
                eprintln!("DEBUG Responses: Output item done, index={}", e.output_index);
                // Handle function calls from output item
                if let async_openai::types::responses::OutputItem::FunctionCall(fc) = e.item {
                    let args_value: serde_json::Value = serde_json::from_str(&fc.arguments)
                        .unwrap_or_else(|_| serde_json::Value::String(fc.arguments.clone()));
                    eprintln!("DEBUG Responses: FunctionCall id={}, call_id={}, name={}", fc.id, fc.call_id, fc.name);
                    // Only add if not already present (check both ids)
                    if !self.tool_uses.iter().any(|tu| tu.id == fc.id) {
                        self.tool_uses.push(crate::tool::ToolUse {
                            id: fc.id,           // Use fc.id (fc_...) for primary ID
                            call_id: Some(fc.call_id), // Store call_id for function_call_output
                            name: fc.name,
                            input: args_value,
                        });
                    }
                }
                None
            }
            ResponseEvent::ResponseCompleted(e) => {
                eprintln!("DEBUG Responses: Completed, status={:?}", e.response.status);
                // Set finish reason
                self.finish_reason = Some(if !self.tool_uses.is_empty() {
                    crate::response::FinishReason::ToolUse
                } else {
                    crate::response::FinishReason::Stop
                });
                
                // Extract usage if present
                if let Some(usage) = e.response.usage {
                    self.usage = Some(crate::response::Usage {
                        prompt_tokens: usage.input_tokens,
                        completion_tokens: usage.output_tokens,
                        total_tokens: usage.total_tokens,
                    });
                }
                
                Some(StreamChunk {
                    delta: None,
                    finish_reason: self.finish_reason.clone(),
                })
            }
            ResponseEvent::ResponseFailed(e) => {
                eprintln!("DEBUG Responses: Failed");
                self.finish_reason = Some(crate::response::FinishReason::Other);
                if let Some(err) = e.response.error {
                    eprintln!("DEBUG Responses: Error - {}: {}", err.code, err.message);
                }
                Some(StreamChunk {
                    delta: None,
                    finish_reason: Some(crate::response::FinishReason::Other),
                })
            }
            ResponseEvent::ResponseIncomplete(e) => {
                eprintln!("DEBUG Responses: Incomplete");
                self.finish_reason = Some(crate::response::FinishReason::Length);
                if let Some(details) = e.response.incomplete_details {
                    eprintln!("DEBUG Responses: Incomplete reason - {}", details.reason);
                }
                Some(StreamChunk {
                    delta: None,
                    finish_reason: Some(crate::response::FinishReason::Length),
                })
            }
            ResponseEvent::ResponseError(e) => {
                eprintln!("DEBUG Responses: Error event - {}", e.message);
                self.finish_reason = Some(crate::response::FinishReason::Other);
                Some(StreamChunk {
                    delta: None,
                    finish_reason: Some(crate::response::FinishReason::Other),
                })
            }
            // Reasoning events
            ResponseEvent::ResponseReasoningSummaryTextDelta(e) => {
                eprintln!("DEBUG Responses: Reasoning delta");
                // We could optionally expose reasoning tokens, for now just log
                Some(StreamChunk {
                    delta: Some(e.delta),
                    finish_reason: None,
                })
            }
            ResponseEvent::Unknown(val) => {
                // Try to parse unknown events - async-openai might have version mismatches
                eprintln!("DEBUG Responses: Unknown event: {}", val);
                
                // Check if this is a function_call_arguments.done that failed to parse
                // (async-openai expects 'name' field but OpenAI may not send it)
                if let Some(event_type) = val.get("type").and_then(|v| v.as_str()) {
                    if event_type == "response.function_call_arguments.done" {
                        if let (Some(item_id), Some(arguments)) = (
                            val.get("item_id").and_then(|v| v.as_str()),
                            val.get("arguments").and_then(|v| v.as_str())
                        ) {
                            eprintln!("DEBUG Responses: Parsed function_call_arguments.done from Unknown: item_id={}, args_len={}", item_id, arguments.len());
                            // Get function name from accumulated_function_args or use placeholder
                            let args_value: serde_json::Value = serde_json::from_str(arguments)
                                .unwrap_or_else(|_| serde_json::Value::String(arguments.to_string()));
                            
                            // Look for existing tool use with this item_id and update it
                            // Or add if not present (name will be from OutputItemAdded event)
                            if !self.tool_uses.iter().any(|tu| tu.id == item_id) {
                                // We need the name - check if we stored it from OutputItemAdded
                                if let Some(name) = self.pending_function_names.get(item_id) {
                                    // Get the call_id from OutputItemAdded (required for function_call_output)
                                    let call_id = self.pending_call_ids.get(item_id).cloned();
                                    eprintln!("DEBUG Responses: Creating ToolUse from Unknown event: id={}, call_id={:?}, name={}", item_id, call_id, name);
                                    self.tool_uses.push(crate::tool::ToolUse {
                                        id: item_id.to_string(),
                                        call_id,
                                        name: name.clone(),
                                        input: args_value,
                                    });
                                } else {
                                    eprintln!("DEBUG Responses: No function name found for item_id={}", item_id);
                                }
                            }
                        }
                    }
                }
                None
            }
            other => {
                // Log unhandled events for debugging
                eprintln!("DEBUG Responses: Unhandled event variant: {:?}", other);
                None
            }
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
                        self.tool_uses.push(crate::tool::ToolUse { call_id: None,
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
                
                self.tool_uses.push(crate::tool::ToolUse { call_id: None,
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
        eprintln!("DEBUG final_response: accumulated_text={} chars, tool_uses={}, finish_reason={:?}", 
            self.accumulated_text.len(), self.tool_uses.len(), self.finish_reason);
        // Need either text or tool uses to have a response
        if self.accumulated_text.is_empty() && self.tool_uses.is_empty() {
            eprintln!("DEBUG final_response: Returning None - no text or tool uses!");
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
    OpenAiResponses(async_openai::types::responses::ResponseStream),
    GeminiCustom(
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = Result<eventsource_stream::Event, eventsource_stream::EventStreamError<reqwest::Error>>>
                    + Send
            >
        >
    ),
}
