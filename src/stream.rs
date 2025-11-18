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
    // Accumulate response data as we stream
    accumulated_text: String,
    usage: Option<crate::response::Usage>,
    finish_reason: Option<crate::response::FinishReason>,
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
            };

            // Parse the SSE event
            if let Some(chunk) = self.handle_anthropic_sse_event(event).await? {
                return Some(Ok(chunk));
            }
            // If no chunk returned, continue to next event
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
            "content_block_delta" => {
                // Extract text delta
                if let Some(text) = data["delta"]["text"].as_str() {
                    self.accumulated_text.push_str(text);
                    Some(Some(StreamChunk {
                        delta: Some(text.to_string()),
                        finish_reason: None,
                    }))
                } else {
                    Some(None) // Non-text delta, continue
                }
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
        if self.accumulated_text.is_empty() {
            return None;
        }

        Some(crate::response::CompletionResponse {
            message: crate::message::Message {
                role: crate::message::Role::Assistant,
                content: self.accumulated_text.clone(),
            },
            usage: self.usage.clone().unwrap_or(crate::response::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            }),
            finish_reason: self.finish_reason.clone().unwrap_or(crate::response::FinishReason::Other),
            model: self.model.clone(),
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
}