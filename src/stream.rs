use futures::StreamExt;
use eventsource_stream::Eventsource;
use crate::response::{GroundingMetadata, CodeExecutionResult};

/// Events related to tool use during streaming.
///
/// These events let callers observe tool use lifecycle in real-time
/// rather than waiting for `final_response()` after the stream ends.
#[derive(Debug, Clone)]
pub enum ToolUseEvent {
    /// A new tool call has started. Name and ID are known.
    Start {
        id: String,
        name: String,
    },
    /// Incremental JSON fragment for the tool input (Anthropic/OpenAI only).
    InputDelta {
        id: String,
        delta: String,
    },
    /// Tool call input is fully assembled and ready for execution.
    Complete(crate::tool::ToolUse),
}

/// A chunk of streaming completion data
#[derive(Debug, Clone)]
pub struct StreamChunk {
    /// The incremental text content, if any
    pub delta: Option<String>,
    /// The finish reason, if this is the final chunk
    pub finish_reason: Option<crate::response::FinishReason>,
    /// Tool use lifecycle event, if any
    pub tool_use_event: Option<ToolUseEvent>,
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
    /// Reasoning items that must be echoed back in continuation requests (GPT-5.2-pro)
    reasoning_items: Vec<serde_json::Value>,
    /// If set, abort with `Error::StreamInactivityTimeout` when no chunk is yielded
    /// within this duration. Default: 90s. Set to `None` via `with_inactivity_timeout(None)` to disable.
    inactivity_timeout: Option<std::time::Duration>,
    /// Once a terminal condition (timeout) fires, subsequent `next()` calls return `None`.
    terminated: bool,
    /// Anthropic: the response content blocks, by stream index, as they
    /// complete (thinking text + signature, redacted_thinking, text, tool_use).
    anthropic_blocks: Vec<serde_json::Value>,
    /// Anthropic: partial tool input JSON per block index.
    anthropic_tool_json: std::collections::HashMap<usize, String>,
    /// Gemini: the model turn's parts as received (thought signatures intact).
    gemini_parts: Vec<serde_json::Value>,
    /// Set when the request was declined.
    refusal: Option<crate::response::Refusal>,
    /// OpenAI reasoning summary text (kept out of the visible answer).
    reasoning_summary: String,
    /// A provider error event seen mid-stream, surfaced on the next `next()`.
    pending_error: Option<crate::error::Error>,
}

/// Default inactivity timeout applied to every new `CompletionStream`.
const DEFAULT_INACTIVITY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

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
        Self::with_inner(StreamType::AnthropicCustom(Box::pin(event_stream)), model)
    }

    pub fn openai(
        stream: async_openai::types::chat::ChatCompletionResponseStream,
        model: String,
    ) -> Self {
        Self::with_inner(StreamType::OpenAi(stream), model)
    }

    /// Create a new stream for OpenAI Responses API
    pub fn openai_responses(
        stream: async_openai::types::responses::ResponseStream,
        model: String,
    ) -> Self {
        Self::with_inner(StreamType::OpenAiResponses(stream), model)
    }

    /// Create a new stream from a raw HTTP byte stream for Gemini
    pub fn gemini_custom(
        stream: impl futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
        model: String,
    ) -> Self {
        let event_stream = stream.eventsource();
        Self::with_inner(StreamType::GeminiCustom(Box::pin(event_stream)), model)
    }

    fn with_inner(inner: StreamType, model: String) -> Self {
        Self {
            inner,
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
            reasoning_items: Vec::new(),
            inactivity_timeout: Some(DEFAULT_INACTIVITY_TIMEOUT),
            terminated: false,
            anthropic_blocks: Vec::new(),
            anthropic_tool_json: std::collections::HashMap::new(),
            gemini_parts: Vec::new(),
            refusal: None,
            reasoning_summary: String::new(),
            pending_error: None,
        }
    }

    /// Override the per-chunk inactivity timeout.
    ///
    /// Pass `Some(duration)` to set a custom window, or `None` to disable the timeout
    /// entirely. By default, every stream is constructed with a 90s timeout — if no
    /// chunk arrives within that window, `next()` yields `Error::StreamInactivityTimeout`
    /// and the stream becomes terminal (subsequent calls return `None`).
    pub fn with_inactivity_timeout(mut self, timeout: Option<std::time::Duration>) -> Self {
        self.inactivity_timeout = timeout;
        self
    }

    /// Get the next chunk from the stream.
    ///
    /// Returns `None` once the stream has ended (or a previous call hit an inactivity
    /// timeout). When `inactivity_timeout` is set, each `next()` call is bounded by that
    /// duration; if the underlying provider goes silent, the stream terminates with
    /// `Error::StreamInactivityTimeout`.
    pub async fn next(&mut self) -> Option<Result<StreamChunk, crate::error::Error>> {
        if self.terminated {
            return None;
        }
        match self.inactivity_timeout {
            Some(timeout) => match tokio::time::timeout(timeout, self.next_inner()).await {
                Ok(result) => result,
                Err(_) => {
                    self.terminated = true;
                    Some(Err(crate::error::Error::StreamInactivityTimeout {
                        elapsed_ms: timeout.as_millis() as u64,
                    }))
                }
            },
            None => self.next_inner().await,
        }
    }

    /// Inner stream-pump loop. Public callers go through `next()` which adds the
    /// inactivity-timeout wrapper.
    async fn next_inner(&mut self) -> Option<Result<StreamChunk, crate::error::Error>> {
        loop {
            if let Some(err) = self.pending_error.take() {
                self.terminated = true;
                return Some(Err(err));
            }
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
                                    cache_creation_tokens: None, // OpenAI doesn't expose cache tokens
                                    cache_read_tokens: None,
                                    cache_creation_1h_tokens: None,
                                });
                                has_usage_update = true;
                            }
                            
                            if let Some(choice) = response.choices.get(0) {
                                // Handle tool calls
                                let mut tool_event: Option<ToolUseEvent> = None;
                                if let Some(tool_calls) = &choice.delta.tool_calls {
                                    for tool_call in tool_calls {
                                        if let Some(function) = &tool_call.function {
                                            if let Some(name) = &function.name {
                                                let id = tool_call.id.clone().unwrap_or_default();
                                                self.tool_uses.push(crate::tool::ToolUse { call_id: None,
                                                    id: id.clone(),
                                                    name: name.clone(),
                                                    input: serde_json::json!({}),
                                                });
                                                tool_event = Some(ToolUseEvent::Start {
                                                    id,
                                                    name: name.clone(),
                                                });
                                            }

                                            if let Some(arguments) = &function.arguments {
                                                if let Some(last_tool) = self.tool_uses.last_mut() {
                                                    if tool_event.is_none() {
                                                        tool_event = Some(ToolUseEvent::InputDelta {
                                                            id: last_tool.id.clone(),
                                                            delta: arguments.clone(),
                                                        });
                                                    }
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
                                
                                // If we have a tool event but no text, yield it
                                if tool_event.is_some() {
                                    return Some(Ok(StreamChunk {
                                        delta: None,
                                        finish_reason: None,
                                        tool_use_event: tool_event,
                                    }));
                                }

                                if let Some(content) = &choice.delta.content {
                                    self.accumulated_text.push_str(content);
                                    return Some(Ok(StreamChunk {
                                        delta: Some(content.clone()),
                                        finish_reason: None,
                                        tool_use_event: None,
                                    }));
                                }
                                
                                if let Some(finish_reason) = &choice.finish_reason {
                                    self.finish_reason = Some(match finish_reason {
                                        async_openai::types::chat::FinishReason::Stop => crate::response::FinishReason::Stop,
                                        async_openai::types::chat::FinishReason::Length => crate::response::FinishReason::Length,
                                        async_openai::types::chat::FinishReason::ContentFilter => crate::response::FinishReason::ContentFilter,
                                        async_openai::types::chat::FinishReason::ToolCalls => crate::response::FinishReason::ToolUse,
                                        async_openai::types::chat::FinishReason::FunctionCall => crate::response::FinishReason::ToolUse,
                                    });
                                }
                            }

                            if self.finish_reason.is_some() || !self.tool_uses.is_empty() || has_usage_update {
                                // Emit Complete events for finalized tool uses
                                let complete_event = if self.finish_reason == Some(crate::response::FinishReason::ToolUse) {
                                    self.tool_uses.last().map(|tu| ToolUseEvent::Complete(tu.clone()))
                                } else {
                                    None
                                };
                                return Some(Ok(StreamChunk {
                                    delta: None,
                                    finish_reason: self.finish_reason.clone(),
                                    tool_use_event: complete_event,
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
                        return None;
                    }

                    match stream.next().await {
                        Some(Ok(event)) => {
                            if let Some(chunk) = self.handle_openai_responses_event(event) {
                                return Some(Ok(chunk));
                            }
                            continue;
                        }
                        Some(Err(e)) => {
                            let err_str = e.to_string();
                            // "Stream ended" is normal termination, not an error
                            if err_str.contains("Stream ended") {
                                return None;
                            }
                            return Some(Err(crate::error::Error::Other(
                                format!("Responses API stream error: {}", e)
                            )));
                        }
                        None => {
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
        event: async_openai::types::responses::ResponseStreamEvent,
    ) -> Option<StreamChunk> {
        use async_openai::types::responses::ResponseStreamEvent;
        
        match event {
            ResponseStreamEvent::ResponseCreated(_) => None,
            ResponseStreamEvent::ResponseInProgress(_) => None,
            ResponseStreamEvent::ResponseOutputItemAdded(e) => {
                // Capture function name for later use in Unknown event parsing
                if let async_openai::types::responses::OutputItem::FunctionCall(fc) = &e.item {
                    let id = fc.id.clone().unwrap_or_default();
                    self.pending_function_names.insert(id.clone(), fc.name.clone());
                    self.pending_call_ids.insert(id.clone(), fc.call_id.clone());
                    return Some(StreamChunk {
                        delta: None,
                        finish_reason: None,
                        tool_use_event: Some(ToolUseEvent::Start {
                            id,
                            name: fc.name.clone(),
                        }),
                    });
                }
                None
            }
            ResponseStreamEvent::ResponseOutputTextDelta(e) => {
                self.accumulated_text.push_str(&e.delta);
                Some(StreamChunk {
                    delta: Some(e.delta),
                    finish_reason: None,
                    tool_use_event: None,
                })
            }
            ResponseStreamEvent::ResponseOutputTextDone(e) => {
                // If we haven't accumulated any text via deltas, use the full text from done event
                if self.accumulated_text.is_empty() && !e.text.is_empty() {
                    self.accumulated_text.push_str(&e.text);
                    return Some(StreamChunk {
                        delta: Some(e.text),
                        finish_reason: None,
                        tool_use_event: None,
                    });
                }
                None
            }
            // Handle ContentPartAdded - may contain initial text for some models
            ResponseStreamEvent::ResponseContentPartAdded(e) => {
                if let async_openai::types::responses::OutputContent::OutputText(text_content) = &e.part {
                    if !text_content.text.is_empty() {
                        self.accumulated_text.push_str(&text_content.text);
                        return Some(StreamChunk {
                            delta: Some(text_content.text.clone()),
                            finish_reason: None,
                            tool_use_event: None,
                        });
                    }
                }
                None
            }
            // Handle ContentPartDone - contains complete text for some models (like GPT-5.2)
            ResponseStreamEvent::ResponseContentPartDone(e) => {
                if let async_openai::types::responses::OutputContent::OutputText(text_content) = &e.part {
                    // Only add if we haven't already accumulated this text
                    // (some models send both delta and done events)
                    if !text_content.text.is_empty() && self.accumulated_text.is_empty() {
                        self.accumulated_text.push_str(&text_content.text);
                        return Some(StreamChunk {
                            delta: Some(text_content.text.clone()),
                            finish_reason: None,
                            tool_use_event: None,
                        });
                    }
                }
                None
            }
            ResponseStreamEvent::ResponseFunctionCallArgumentsDelta(e) => {
                // Accumulate function call arguments
                let item_id = e.item_id.clone();
                let delta = e.delta.clone();
                let entry = self.accumulated_function_args.entry(e.item_id).or_default();
                entry.push_str(&delta);
                Some(StreamChunk {
                    delta: None,
                    finish_reason: None,
                    tool_use_event: Some(ToolUseEvent::InputDelta {
                        id: item_id,
                        delta,
                    }),
                })
            }
            ResponseStreamEvent::ResponseFunctionCallArgumentsDone(e) => {
                // Create tool use from accumulated arguments
                let args_value: serde_json::Value = serde_json::from_str(&e.arguments)
                    .unwrap_or_else(|_| serde_json::Value::String(e.arguments.clone()));
                // Get the call_id from OutputItemAdded (required for function_call_output)
                let call_id = self.pending_call_ids.get(&e.item_id).cloned();
                // The done event usually omits the name; it came with
                // OutputItemAdded. An empty name is rejected on replay.
                let name = e
                    .name
                    .filter(|n| !n.is_empty())
                    .or_else(|| self.pending_function_names.get(&e.item_id).cloned())
                    .unwrap_or_default();
                let tool_use = crate::tool::ToolUse {
                    id: e.item_id.clone(),
                    call_id,
                    name,
                    input: args_value,
                };
                let event = ToolUseEvent::Complete(tool_use.clone());
                self.tool_uses.push(tool_use);
                Some(StreamChunk {
                    delta: None,
                    finish_reason: None,
                    tool_use_event: Some(event),
                })
            }
            ResponseStreamEvent::ResponseOutputItemDone(e) => {
                // Capture reasoning items for continuation requests (GPT-5.2-pro requirement)
                // Must include encrypted_content for multi-turn stateless conversations
                if let async_openai::types::responses::OutputItem::Reasoning(ref reasoning) = e.item {
                    // Store as JSON for echoing back in continuation
                    // Include encrypted_content if present (required for multi-turn tool calls)
                    let mut reasoning_json = serde_json::json!({
                        "type": "reasoning",
                        "id": reasoning.id,
                        "summary": reasoning.summary.iter().map(|s| {
                            match s {
                                async_openai::types::responses::SummaryPart::SummaryText(content) => {
                                    serde_json::json!({
                                        "type": "summary_text",
                                        "text": content.text.clone()
                                    })
                                }
                            }
                        }).collect::<Vec<_>>()
                    });
                    // Add encrypted_content if present
                    if let Some(ref encrypted) = reasoning.encrypted_content {
                        reasoning_json["encrypted_content"] = serde_json::json!(encrypted);
                    }
                    self.reasoning_items.push(reasoning_json);
                }

                // Handle function calls from output item
                if let async_openai::types::responses::OutputItem::FunctionCall(fc) = e.item {
                    let args_value: serde_json::Value = serde_json::from_str(&fc.arguments)
                        .unwrap_or_else(|_| serde_json::Value::String(fc.arguments.clone()));
                    // Only add if not already present (check both ids)
                    let fc_id = fc.id.clone().unwrap_or_default();
                    // The finished item is authoritative for name and call_id.
                    if let Some(existing) = self.tool_uses.iter_mut().find(|tu| tu.id == fc_id) {
                        if existing.name.is_empty() {
                            existing.name = fc.name.clone();
                        }
                        if existing.call_id.is_none() {
                            existing.call_id = Some(fc.call_id.clone());
                        }
                    }
                    if !self.tool_uses.iter().any(|tu| tu.id == fc_id) {
                        let tool_use = crate::tool::ToolUse {
                            id: fc_id,
                            call_id: Some(fc.call_id),
                            name: fc.name,
                            input: args_value,
                        };
                        let event = ToolUseEvent::Complete(tool_use.clone());
                        self.tool_uses.push(tool_use);
                        return Some(StreamChunk {
                            delta: None,
                            finish_reason: None,
                            tool_use_event: Some(event),
                        });
                    }
                }
                None
            }
            ResponseStreamEvent::ResponseCompleted(e) => {
                // Set finish reason
                self.finish_reason = Some(if !self.tool_uses.is_empty() {
                    crate::response::FinishReason::ToolUse
                } else if self.refusal.is_some() {
                    crate::response::FinishReason::Refusal
                } else {
                    crate::response::FinishReason::Stop
                });
                
                if let Some(usage) = &e.response.usage {
                    self.usage = Some(crate::client::openai_responses::usage_from(usage));
                }
                
                Some(StreamChunk {
                    delta: None,
                    finish_reason: self.finish_reason.clone(),
                    tool_use_event: None,
                })
            }
            ResponseStreamEvent::ResponseFailed(_) => {
                self.finish_reason = Some(crate::response::FinishReason::Other);
                Some(StreamChunk {
                    delta: None,
                    finish_reason: Some(crate::response::FinishReason::Other),
                    tool_use_event: None,
                })
            }
            ResponseStreamEvent::ResponseIncomplete(e) => {
                if let Some(usage) = &e.response.usage {
                    self.usage = Some(crate::client::openai_responses::usage_from(usage));
                }
                let reason = e
                    .response
                    .incomplete_details
                    .as_ref()
                    .map(|d| d.reason.clone())
                    .unwrap_or_default();
                let finish = if reason == "content_filter" {
                    self.refusal.get_or_insert_with(Default::default).category =
                        Some("content_filter".to_string());
                    crate::response::FinishReason::Refusal
                } else {
                    crate::response::FinishReason::Length
                };
                self.finish_reason = Some(finish.clone());
                Some(StreamChunk {
                    delta: None,
                    finish_reason: Some(finish),
                    tool_use_event: None,
                })
            }
            // A refusal is user-facing text: show it, and mark the turn.
            ResponseStreamEvent::ResponseRefusalDelta(e) => {
                let refusal = self.refusal.get_or_insert_with(Default::default);
                refusal
                    .explanation
                    .get_or_insert_with(String::new)
                    .push_str(&e.delta);
                self.accumulated_text.push_str(&e.delta);
                Some(StreamChunk {
                    delta: Some(e.delta),
                    finish_reason: None,
                    tool_use_event: None,
                })
            }
            ResponseStreamEvent::ResponseError(_) => {
                self.finish_reason = Some(crate::response::FinishReason::Other);
                Some(StreamChunk {
                    delta: None,
                    finish_reason: Some(crate::response::FinishReason::Other),
                    tool_use_event: None,
                })
            }
            // Reasoning summaries are not part of the answer.
            ResponseStreamEvent::ResponseReasoningSummaryTextDelta(e) => {
                self.reasoning_summary.push_str(&e.delta);
                None
            }
            _ => None
        }
    }

    /// Handle one Anthropic SSE event. `None` ends the stream; `Some(None)`
    /// continues to the next event.
    async fn handle_anthropic_sse_event(
        &mut self,
        event: eventsource_stream::Event,
    ) -> Option<Option<StreamChunk>> {
        let data = match serde_json::from_str::<serde_json::Value>(&event.data) {
            Ok(data) => data,
            Err(_) => return Some(None),
        };
        let event_type = data["type"].as_str()?;
        let index = data["index"].as_u64().unwrap_or(0) as usize;

        match event_type {
            "message_start" => {
                self.usage = Some(crate::client::anthropic::parse_usage(data["message"].get("usage")));
                Some(None)
            }
            "content_block_start" => {
                let block = data["content_block"].clone();
                if self.anthropic_blocks.len() <= index {
                    self.anthropic_blocks.resize(index + 1, serde_json::Value::Null);
                }
                self.anthropic_blocks[index] = block.clone();
                if block["type"].as_str() == Some("tool_use") {
                    let id = block["id"].as_str().unwrap_or("").to_string();
                    let name = block["name"].as_str().unwrap_or("").to_string();
                    self.tool_uses.push(crate::tool::ToolUse {
                        call_id: None,
                        id: id.clone(),
                        name: name.clone(),
                        input: serde_json::json!({}),
                    });
                    return Some(Some(StreamChunk {
                        delta: None,
                        finish_reason: None,
                        tool_use_event: Some(ToolUseEvent::Start { id, name }),
                    }));
                }
                Some(None)
            }
            "content_block_delta" => {
                let delta = &data["delta"];
                let Some(block) = self.anthropic_blocks.get_mut(index) else {
                    return Some(None);
                };
                match delta["type"].as_str() {
                    Some("text_delta") => {
                        let text = delta["text"].as_str().unwrap_or_default();
                        append_str(block, "text", text);
                        self.accumulated_text.push_str(text);
                        Some(Some(StreamChunk {
                            delta: Some(text.to_string()),
                            finish_reason: None,
                            tool_use_event: None,
                        }))
                    }
                    Some("input_json_delta") => {
                        let fragment = delta["partial_json"].as_str().unwrap_or_default().to_string();
                        let id = block["id"].as_str().unwrap_or_default().to_string();
                        self.anthropic_tool_json
                            .entry(index)
                            .or_default()
                            .push_str(&fragment);
                        Some(Some(StreamChunk {
                            delta: None,
                            finish_reason: None,
                            tool_use_event: Some(ToolUseEvent::InputDelta { id, delta: fragment }),
                        }))
                    }
                    Some("thinking_delta") => {
                        append_str(block, "thinking", delta["thinking"].as_str().unwrap_or_default());
                        Some(None)
                    }
                    Some("signature_delta") => {
                        block["signature"] = delta["signature"].clone();
                        Some(None)
                    }
                    _ => Some(None),
                }
            }
            "content_block_stop" => {
                let Some(block) = self.anthropic_blocks.get_mut(index) else {
                    return Some(None);
                };
                if block["type"].as_str() != Some("tool_use") {
                    return Some(None);
                }
                // Tool input arrives as JSON fragments; the API rejects a
                // non-object input when the turn is replayed.
                let raw = self.anthropic_tool_json.remove(&index).unwrap_or_default();
                let input = if raw.trim().is_empty() {
                    serde_json::json!({})
                } else {
                    serde_json::from_str::<serde_json::Value>(&raw)
                        .ok()
                        .filter(|v| v.is_object())
                        .unwrap_or_else(|| serde_json::json!({}))
                };
                block["input"] = input.clone();
                let id = block["id"].as_str().unwrap_or_default().to_string();
                let Some(tool) = self.tool_uses.iter_mut().find(|t| t.id == id) else {
                    return Some(None);
                };
                tool.input = input;
                Some(Some(StreamChunk {
                    delta: None,
                    finish_reason: None,
                    tool_use_event: Some(ToolUseEvent::Complete(tool.clone())),
                }))
            }
            "message_delta" => {
                if let Some(usage) = data.get("usage").filter(|u| u.is_object()) {
                    if usage.get("input_tokens").is_some() {
                        // The final totals. They omit the 5m/1h split of the
                        // cache writes, which only message_start carries.
                        let mut totals = crate::client::anthropic::parse_usage(Some(usage));
                        if totals.cache_creation_1h_tokens.is_none() {
                            totals.cache_creation_1h_tokens =
                                self.usage.as_ref().and_then(|u| u.cache_creation_1h_tokens);
                        }
                        self.usage = Some(totals);
                    } else if let Some(output_tokens) = usage["output_tokens"].as_u64() {
                        if let Some(existing_usage) = &mut self.usage {
                            existing_usage.completion_tokens = output_tokens as u32;
                            existing_usage.total_tokens = existing_usage.prompt_tokens + output_tokens as u32;
                        }
                    }
                }
                if let Some(stop_reason) = data["delta"]["stop_reason"].as_str() {
                    let finish = crate::client::anthropic::map_stop_reason(
                        Some(stop_reason),
                        !self.tool_uses.is_empty(),
                    );
                    if finish == crate::response::FinishReason::Refusal {
                        let details = data["delta"]
                            .get("stop_details")
                            .or_else(|| data.get("stop_details"));
                        self.refusal = Some(crate::client::anthropic::parse_refusal(details));
                    }
                    self.finish_reason = Some(finish);
                }
                Some(None)
            }
            "message_stop" => None,
            "error" => {
                let message = data["error"]["message"].as_str().unwrap_or("unknown error");
                let kind = data["error"]["type"].as_str().unwrap_or("error");
                self.pending_error = Some(crate::error::Error::AnthropicError(format!(
                    "Stream error ({}): {}",
                    kind, message
                )));
                Some(None)
            }
            _ => Some(None),
        }
    }

    /// Handle Gemini SSE events
    async fn handle_gemini_sse_event(
        &mut self,
        event: eventsource_stream::Event,
    ) -> Option<Option<StreamChunk>> {
        let data = match serde_json::from_str::<serde_json::Value>(&event.data) {
            Ok(data) => data,
            Err(_) => return Some(None),
        };

        if let Some(usage_metadata) = data.get("usageMetadata") {
            self.usage = Some(crate::client::gemini::parse_usage(usage_metadata));
        }

        // A blocked prompt comes back with no candidates and a block reason.
        let Some(candidate) = data["candidates"].get(0) else {
            if let Some(reason) = data["promptFeedback"]["blockReason"].as_str() {
                self.refusal = Some(crate::response::Refusal {
                    category: Some(reason.to_string()),
                    explanation: data["promptFeedback"]["blockReasonMessage"]
                        .as_str()
                        .map(String::from),
                });
                self.finish_reason = Some(crate::response::FinishReason::Refusal);
                return Some(Some(StreamChunk {
                    delta: None,
                    finish_reason: self.finish_reason.clone(),
                    tool_use_event: None,
                }));
            }
            return Some(None);
        };

        if let Some(gm) = candidate.get("groundingMetadata") {
            if let Ok(metadata) = serde_json::from_value::<GroundingMetadata>(gm.clone()) {
                self.grounding_metadata = Some(metadata);
            }
        }
        if let Some(token) = candidate["googleMapsWidgetContextToken"].as_str() {
            self.google_maps_widget_token = Some(token.to_string());
        }

        let mut text_delta = String::new();
        let mut tool_event: Option<ToolUseEvent> = None;

        for part in candidate["content"]["parts"].as_array().into_iter().flatten() {
            crate::client::gemini::push_part(&mut self.gemini_parts, part);

            // Thought summaries are not part of the answer.
            let is_thought = part["thought"].as_bool().unwrap_or(false);
            if let Some(text) = part["text"].as_str() {
                if !is_thought {
                    text_delta.push_str(text);
                }
            }

            if let Some(tool_use) = crate::client::gemini::function_call_to_tool_use(part) {
                tool_event = Some(ToolUseEvent::Complete(tool_use.clone()));
                self.tool_uses.push(tool_use);
            }

            if let Some(executable_code) = part.get("executableCode") {
                self.code_execution_results.push(CodeExecutionResult {
                    code: executable_code["code"].as_str().map(|s| s.to_string()),
                    language: executable_code["language"].as_str().map(|s| s.to_string()),
                    outcome: None,
                    output: None,
                });
            }

            if let Some(code_result) = part.get("codeExecutionResult") {
                let outcome = code_result["outcome"]
                    .as_str()
                    .and_then(|s| serde_json::from_value(serde_json::json!(s)).ok());
                let output = code_result["output"].as_str().map(|s| s.to_string());
                if let Some(last) = self.code_execution_results.last_mut() {
                    if last.outcome.is_none() {
                        last.outcome = outcome;
                        last.output = output;
                        continue;
                    }
                }
                self.code_execution_results.push(CodeExecutionResult {
                    code: None,
                    language: None,
                    outcome,
                    output,
                });
            }
        }

        if let Some(finish_reason_str) = candidate["finishReason"].as_str() {
            let finish = crate::client::gemini::map_finish_reason(finish_reason_str, !self.tool_uses.is_empty());
            if finish == crate::response::FinishReason::Refusal {
                self.refusal = Some(crate::response::Refusal {
                    category: Some(finish_reason_str.to_string()),
                    explanation: candidate["finishMessage"].as_str().map(String::from),
                });
            }
            self.finish_reason = Some(finish);
        }

        if !text_delta.is_empty() {
            self.accumulated_text.push_str(&text_delta);
            return Some(Some(StreamChunk {
                delta: Some(text_delta),
                finish_reason: None,
                tool_use_event: tool_event,
            }));
        }

        if tool_event.is_some() || self.finish_reason.is_some() {
            return Some(Some(StreamChunk {
                delta: None,
                finish_reason: self.finish_reason.clone(),
                tool_use_event: tool_event,
            }));
        }

        Some(None)
    }

    /// Get the final response after streaming completes.
    ///
    /// `None` when the model produced nothing at all. A turn that ended on a
    /// refusal, a content filter, or the output limit is still returned (with
    /// whatever text arrived) so callers can see why it stopped.
    pub fn final_response(&self) -> Option<crate::response::CompletionResponse> {
        use crate::message::ProviderContent;
        use crate::response::FinishReason;

        let ended_with_reason = matches!(
            self.finish_reason,
            Some(FinishReason::Refusal) | Some(FinishReason::ContentFilter) | Some(FinishReason::Length)
        );
        if self.accumulated_text.is_empty() && self.tool_uses.is_empty() && !ended_with_reason {
            return None;
        }

        let tool_uses_opt = if self.tool_uses.is_empty() {
            None
        } else {
            Some(self.tool_uses.clone())
        };
        let reasoning_items = if self.reasoning_items.is_empty() {
            None
        } else {
            Some(self.reasoning_items.clone())
        };

        let anthropic_blocks: Vec<serde_json::Value> = self
            .anthropic_blocks
            .iter()
            .filter(|b| !b.is_null())
            .cloned()
            .collect();
        let provider_content = if !anthropic_blocks.is_empty() {
            Some(ProviderContent::new(ProviderContent::ANTHROPIC, self.model.clone(), anthropic_blocks))
        } else if !self.gemini_parts.is_empty() {
            Some(ProviderContent::new(ProviderContent::GEMINI, self.model.clone(), self.gemini_parts.clone()))
        } else {
            None
        };

        let mut message = crate::message::Message::assistant(self.accumulated_text.clone());
        message.tool_uses = tool_uses_opt.clone();
        message.reasoning_items = reasoning_items.clone();
        message.provider_content = provider_content;

        let finish_reason = self.finish_reason.clone().unwrap_or(FinishReason::Other);

        Some(crate::response::CompletionResponse {
            message,
            usage: self.usage.clone().unwrap_or_else(crate::response::Usage::zero),
            refusal: if finish_reason == FinishReason::Refusal {
                Some(self.refusal.clone().unwrap_or_default())
            } else {
                None
            },
            finish_reason,
            model: self.model.clone(),
            tool_uses: tool_uses_opt,
            grounding_metadata: self.grounding_metadata.clone(),
            code_execution_results: if self.code_execution_results.is_empty() {
                None
            } else {
                Some(self.code_execution_results.clone())
            },
            google_maps_widget_token: self.google_maps_widget_token.clone(),
            reasoning_items,
            reasoning_summary: if self.reasoning_summary.is_empty() {
                None
            } else {
                Some(self.reasoning_summary.clone())
            },
            citations: None,
        })
    }

    pub fn tool_use(&self) -> Option<&crate::tool::ToolUse> {
        self.tool_uses.first()
    }
}

/// Append to a string field of a JSON block in place.
fn append_str(block: &mut serde_json::Value, key: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    match block.get_mut(key) {
        Some(serde_json::Value::String(existing)) => existing.push_str(text),
        _ => block[key] = serde_json::Value::String(text.to_string()),
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
    OpenAi(async_openai::types::chat::ChatCompletionResponseStream),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// A stream that hangs forever wedges `next()` until the inactivity timeout fires.
    /// Verifies the timeout path returns `StreamInactivityTimeout` and marks the stream
    /// terminal so subsequent calls return `None`.
    #[tokio::test]
    async fn inactivity_timeout_fires_and_terminates_stream() {
        let pending = futures::stream::pending::<Result<bytes::Bytes, reqwest::Error>>();
        let mut stream = CompletionStream::anthropic_custom(pending, "test-model".to_string())
            .with_inactivity_timeout(Some(Duration::from_millis(50)));

        let started = std::time::Instant::now();
        let first = stream.next().await;
        let elapsed = started.elapsed();

        match first {
            Some(Err(crate::error::Error::StreamInactivityTimeout { elapsed_ms })) => {
                assert_eq!(elapsed_ms, 50);
            }
            other => panic!("expected StreamInactivityTimeout, got {:?}", other),
        }
        assert!(elapsed >= Duration::from_millis(50));
        assert!(elapsed < Duration::from_secs(1), "timeout should fire promptly, took {:?}", elapsed);

        // After timeout, the stream is terminal — subsequent calls return None.
        assert!(stream.next().await.is_none());
    }

    /// With the timeout disabled, a hung stream stays hung — verify by racing it
    /// against a short outer timeout that should win.
    #[tokio::test]
    async fn inactivity_timeout_can_be_disabled() {
        let pending = futures::stream::pending::<Result<bytes::Bytes, reqwest::Error>>();
        let mut stream = CompletionStream::anthropic_custom(pending, "test-model".to_string())
            .with_inactivity_timeout(None);

        // Race the stream against an outer 100ms timeout. The outer timeout should fire first
        // because we disabled the inner one.
        let outer = tokio::time::timeout(Duration::from_millis(100), stream.next()).await;
        assert!(outer.is_err(), "outer timeout should fire before inner (which is disabled)");
    }
}
