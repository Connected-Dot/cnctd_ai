use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use cnctd_ai::client::{AnthropicConfig, GeminiConfig, OpenAiConfig};
use cnctd_ai::mcp::tool_result_to_string;
use cnctd_ai::{Client, CompletionRequest, Message, RequestOptions, Tool, ToolResult, ToolUse, ToolUseEvent};
use futures::stream::Stream;
use serde::Deserialize;
use serde_json::json;
use std::convert::Infallible;
use std::time::Duration;

use crate::error::AppError;
use crate::obfuscation::Obfuscator;
use crate::state::AppState;

/// Buffers streaming text deltas so that obfuscation tokens split across
/// chunks are properly deobfuscated. Holds back a tail that could be the
/// start of a partial token until more text arrives or the stream ends.
struct StreamingDeobfuscator {
    buffer: String,
    /// Max length of any token (e.g. "advertiser_" + suffix). We hold this
    /// many trailing bytes back in case they're the start of a split token.
    max_token_len: usize,
}

impl StreamingDeobfuscator {
    fn new() -> Self {
        // Longest entity type prefix is "advertiser" (10) + "_" + suffix_length hex chars.
        // suffix_length=4 -> 15 chars. Add margin for longer suffixes.
        Self {
            buffer: String::new(),
            max_token_len: 24,
        }
    }

    /// Push a new delta chunk. Returns text that is safe to emit (fully deobfuscated).
    fn push(&mut self, delta: &str, obfuscator: &Obfuscator) -> String {
        self.buffer.push_str(delta);

        if self.buffer.len() <= self.max_token_len {
            // Not enough text accumulated yet; hold everything
            return String::new();
        }

        // Split: everything up to the safe boundary can be processed,
        // the tail is held back in case a token straddles the boundary.
        let safe_end = self.buffer.len() - self.max_token_len;

        // Find a clean split point — don't cut in the middle of a word
        let split_at = self.buffer[..safe_end]
            .rfind(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .map(|pos| pos + 1)
            .unwrap_or(0);

        if split_at == 0 {
            // Entire buffer is one long word; hold it all
            return String::new();
        }

        let safe_text = self.buffer[..split_at].to_string();
        self.buffer = self.buffer[split_at..].to_string();

        obfuscator.deobfuscate_llm_response(&safe_text)
    }

    /// Flush remaining buffer at end of stream.
    fn flush(&mut self, obfuscator: &Obfuscator) -> String {
        if self.buffer.is_empty() {
            return String::new();
        }
        let remaining = std::mem::take(&mut self.buffer);
        obfuscator.deobfuscate_llm_response(&remaining)
    }
}

// ── List available tools ──────────────────────────────────────────────

pub async fn list_tools(State(state): State<AppState>) -> Json<serde_json::Value> {
    let tools = load_tools(&state, &None).await;
    let items: Vec<serde_json::Value> = tools
        .iter()
        .map(|t| {
            let desc = t
                .description
                .as_ref()
                .map(|d| d.as_ref())
                .unwrap_or("");
            json!({
                "name": t.name,
                "description": desc,
            })
        })
        .collect();
    Json(json!({ "tools": items }))
}

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub tools: Option<Vec<String>>,
    #[serde(default = "default_stream")]
    pub stream: bool,
    #[serde(default)]
    pub execute_tools: Option<bool>,
    #[serde(default)]
    pub max_tool_iterations: Option<usize>,
    #[serde(default)]
    pub session_salt: Option<String>,
}

fn default_stream() -> bool {
    true
}

fn default_max_iterations() -> usize {
    10
}

#[derive(Debug, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    /// Tool uses from an assistant message (for multi-turn tool calling)
    #[serde(default)]
    pub tool_uses: Option<Vec<ChatToolUse>>,
    /// Tool results from a user message (for multi-turn tool calling)
    #[serde(default)]
    pub tool_results: Option<Vec<ChatToolResult>>,
}

#[derive(Debug, Deserialize)]
pub struct ChatToolUse {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct ChatToolResult {
    pub tool_call_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub function_name: Option<String>,
}

pub fn resolve_provider(model: &str) -> &str {
    if model.starts_with("claude-") {
        "anthropic"
    } else if model.starts_with("gpt-")
        || model.starts_with("o1")
        || model.starts_with("o3")
        || model.starts_with("o4")
    {
        "openai"
    } else if model.starts_with("gemini-") {
        "google"
    } else {
        "ollama"
    }
}

pub fn build_client(state: &AppState, model: &str) -> Result<Client, AppError> {
    let provider = resolve_provider(model);

    match provider {
        "anthropic" => {
            let api_key = state
                .config
                .anthropic_api_key
                .as_ref()
                .ok_or_else(|| AppError::ProviderNotConfigured("anthropic".into()))?;
            Client::anthropic(
                AnthropicConfig {
                    api_key: api_key.clone(),
                    model: model.to_string(),
                    version: None,
                },
                None,
            )
            .map_err(AppError::from)
        }
        "openai" => {
            let api_key = state
                .config
                .openai_api_key
                .as_ref()
                .ok_or_else(|| AppError::ProviderNotConfigured("openai".into()))?;
            Client::openai(
                OpenAiConfig {
                    api_key: api_key.clone(),
                    model: model.to_string(),
                    organization: None,
                    transcription_model: None,
                },
                None,
            )
            .map_err(AppError::from)
        }
        "google" => {
            let api_key = state
                .config
                .google_api_key
                .as_ref()
                .ok_or_else(|| AppError::ProviderNotConfigured("google".into()))?;
            Client::gemini(
                GeminiConfig {
                    api_key: api_key.clone(),
                    model: model.to_string(),
                },
                None,
            )
            .map_err(AppError::from)
        }
        "ollama" => {
            let base_url = state
                .config
                .ollama_base_url
                .as_ref()
                .cloned()
                .unwrap_or_else(|| "http://localhost:11434/v1".to_string());
            Client::openai(
                OpenAiConfig {
                    api_key: "ollama".to_string(),
                    model: model.to_string(),
                    organization: None,
                    transcription_model: None,
                },
                Some(cnctd_ai::client::ClientOptions {
                    timeout: Some(Duration::from_secs(300)),
                    max_retries: 1,
                    base_url: Some(base_url),
                }),
            )
            .map_err(AppError::from)
        }
        _ => Err(AppError::BadRequest(format!("Unknown provider for model: {model}"))),
    }
}

fn convert_messages(msgs: &[ChatMessage]) -> Vec<Message> {
    msgs.iter()
        .map(|m| match m.role.as_str() {
            "system" => Message::system(&m.content),
            "assistant" => {
                // If assistant message has tool_uses, include them
                if let Some(ref tool_uses) = m.tool_uses {
                    let tus: Vec<ToolUse> = tool_uses
                        .iter()
                        .map(|tu| ToolUse {
                            id: tu.id.clone(),
                            call_id: None,
                            name: tu.name.clone(),
                            input: tu.input.clone(),
                        })
                        .collect();
                    if m.content.is_empty() {
                        Message::assistant_with_tool_uses(tus)
                    } else {
                        Message::assistant_with_content_and_tools(&m.content, tus)
                    }
                } else {
                    Message::assistant(&m.content)
                }
            }
            "user" | _ => {
                // If user message has tool_results, include them
                if let Some(ref tool_results) = m.tool_results {
                    let results: Vec<ToolResult> = tool_results
                        .iter()
                        .map(|tr| {
                            let mut result = if tr.is_error {
                                ToolResult::error(&tr.tool_call_id, &tr.content)
                            } else {
                                ToolResult::new(&tr.tool_call_id, &tr.content)
                            };
                            if let Some(ref name) = tr.function_name {
                                result = result.set_name(name);
                            }
                            result
                        })
                        .collect();
                    Message::tool_results(results)
                } else {
                    Message::user(&m.content)
                }
            }
        })
        .collect()
}

pub async fn load_tools(state: &AppState, filter: &Option<Vec<String>>) -> Vec<Tool> {
    // Prefer direct MCP client over gateway
    let mut all_tools = if let Some(mcp_client) = &state.mcp_client {
        match mcp_client.list_tools().await {
            Ok(tools) => tools,
            Err(e) => {
                tracing::warn!("Failed to list tools via MCP client: {e}");
                Vec::new()
            }
        }
    } else if let Some(gateway) = &state.gateway {
        let servers = match gateway.list_servers().await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("Failed to list MCP servers: {e}");
                return Vec::new();
            }
        };
        let mut tools = Vec::new();
        for server in &servers {
            match gateway.list_tools(&server.name).await {
                Ok(t) => tools.extend(t),
                Err(e) => tracing::warn!("Failed to list tools for {}: {e}", server.name),
            }
        }
        tools
    } else {
        Vec::new()
    };

    // Filter tools if a specific list was requested
    match filter {
        Some(names) if !names.is_empty() && names[0] != "*" => {
            all_tools.retain(|t| names.contains(&t.name.to_string()));
        }
        _ => {} // "*" or None = all tools
    }

    all_tools
}

pub async fn chat_stream(
    State(state): State<AppState>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, AppError> {
    let client = build_client(&state, &req.model)?;
    let messages = convert_messages(&req.messages);
    let tools = load_tools(&state, &req.tools).await;

    let execute_tools = req.execute_tools.unwrap_or(false);
    let max_iterations = req.max_tool_iterations.unwrap_or_else(default_max_iterations);

    // Prepend system prompt as a system message if provided
    let mut all_messages = Vec::new();
    if let Some(sp) = &req.system_prompt {
        all_messages.push(Message::system(sp));
    }
    all_messages.extend(messages);

    let tools_for_req = if tools.is_empty() { None } else { Some(tools.clone()) };

    // Initialize obfuscator if configured
    let obfuscator: Option<Obfuscator> = if let Some(cache) = &state.session_cache {
        let salt = req
            .session_salt
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| {
                tracing::debug!("No session_salt provided, generating random");
                // We'll use a generated UUID below since we can't return a &str from a closure
                ""
            });
        let salt = if salt.is_empty() {
            uuid::Uuid::new_v4().to_string()
        } else {
            salt.to_string()
        };
        match cache.get_or_create(&salt).await {
            Ok(session) => {
                tracing::info!("Obfuscation session ready [salt={}...]", &salt[..salt.len().min(8)]);
                Some(Obfuscator::new(session))
            }
            Err(e) => {
                tracing::warn!("Failed to init obfuscation session: {e}");
                None
            }
        }
    } else {
        None
    };

    let sse_stream = async_stream::stream! {
        let mut iteration = 0;
        let mut total_prompt_tokens: u32 = 0;
        let mut total_completion_tokens: u32 = 0;
        let mut final_model = String::new();
        let mut final_finish_reason = String::new();
        let mut final_tool_uses: Vec<serde_json::Value> = Vec::new();
        let mut deobfuscate_buffer = StreamingDeobfuscator::new();

        // Emit token_map event if obfuscation is active
        if let Some(ref obf) = obfuscator {
            let token_map = obf.export_token_map();
            let event = json!({
                "type": "token_map",
                "data": token_map,
            });
            yield Ok(Event::default().data(event.to_string()));
        }

        loop {
            // Interception 1: Obfuscate user message content before sending to LLM
            let mut obf_events_i1: Vec<serde_json::Value> = Vec::new();
            let messages_for_llm = if let Some(ref obf) = obfuscator {
                let mut msgs = Vec::with_capacity(all_messages.len());
                for m in &all_messages {
                    if m.role == cnctd_ai::Role::User && !m.content.is_empty() && !m.has_tool_results() {
                        let obfuscated = obf.obfuscate_user_message(&m.content);
                        if obfuscated != m.content {
                            obf_events_i1.push(json!({
                                "type": "obfuscation_event",
                                "data": {
                                    "stage": "user_to_llm",
                                    "tool_name": null,
                                    "tool_call_id": null,
                                    "before": m.content,
                                    "after": obfuscated,
                                }
                            }));
                        }
                        msgs.push(Message::user(&obfuscated));
                    } else {
                        msgs.push(m.clone());
                    }
                }
                msgs
            } else {
                all_messages.clone()
            };
            for evt in obf_events_i1 {
                yield Ok(Event::default().data(evt.to_string()));
            }

            let completion_req = CompletionRequest {
                messages: messages_for_llm,
                tools: tools_for_req.clone(),
                built_in_tools: None,
                tool_config: None,
                options: Some(RequestOptions {
                    max_tokens: Some(4096),
                    ..Default::default()
                }),
            };

            tracing::info!("Starting LLM stream (iteration {iteration}, {} messages)", completion_req.messages.len());
            let mut stream = match client.complete_stream(completion_req).await {
                Ok(s) => {
                    tracing::info!("LLM stream opened (iteration {iteration})");
                    s
                }
                Err(e) => {
                    tracing::error!("LLM stream error (iteration {iteration}): {e}");
                    let event = json!({
                        "type": "error",
                        "data": { "message": e.to_string() }
                    });
                    yield Ok(Event::default().data(event.to_string()));
                    break;
                }
            };

            let mut errored = false;
            let mut chunk_count = 0u32;
            // Accumulate raw LLM text (with tokens) for Interception 4 obfuscation event
            let mut raw_llm_text = String::new();

            // Stream the LLM response
            loop {
                match stream.next().await {
                    Some(Ok(chunk)) => {
                        chunk_count += 1;
                        if chunk_count == 1 {
                            tracing::info!("First chunk received (iteration {iteration})");
                        }
                        if let Some(delta) = &chunk.delta {
                            // Capture raw LLM text before deobfuscation
                            if obfuscator.is_some() {
                                raw_llm_text.push_str(delta);
                            }
                            // Interception 4: Deobfuscate tokens in LLM text before sending to client.
                            // Use the streaming buffer to handle tokens split across chunks.
                            let display_delta = if let Some(ref obf) = obfuscator {
                                deobfuscate_buffer.push(delta, obf)
                            } else {
                                delta.clone()
                            };
                            if !display_delta.is_empty() {
                                let event = json!({
                                    "type": "text_delta",
                                    "data": { "text": display_delta }
                                });
                                yield Ok(Event::default().data(event.to_string()));
                            }
                        }

                        // Emit real-time tool use events
                        if let Some(tool_event) = &chunk.tool_use_event {
                            let event = match tool_event {
                                ToolUseEvent::Start { id, name } => json!({
                                    "type": "tool_use_start",
                                    "data": { "id": id, "name": name }
                                }),
                                ToolUseEvent::InputDelta { id, delta } => json!({
                                    "type": "tool_use_delta",
                                    "data": { "id": id, "delta": delta }
                                }),
                                ToolUseEvent::Complete(tool_use) => json!({
                                    "type": "tool_use_complete",
                                    "data": {
                                        "id": tool_use.id,
                                        "name": tool_use.name,
                                        "input": tool_use.input,
                                    }
                                }),
                            };
                            yield Ok(Event::default().data(event.to_string()));
                        }

                        if chunk.finish_reason.is_some() {
                            tracing::info!("Stream finished (iteration {iteration}, {chunk_count} chunks, reason={:?})", chunk.finish_reason);
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        let event = json!({
                            "type": "error",
                            "data": { "message": e.to_string() }
                        });
                        yield Ok(Event::default().data(event.to_string()));
                        errored = true;
                        break;
                    }
                    None => break,
                }
            }

            // Flush any remaining buffered text from the deobfuscation buffer
            if let Some(ref obf) = obfuscator {
                let remaining = deobfuscate_buffer.flush(obf);
                if !remaining.is_empty() {
                    let event = json!({
                        "type": "text_delta",
                        "data": { "text": remaining }
                    });
                    yield Ok(Event::default().data(event.to_string()));
                }

                // Interception 4 event: Emit the full raw LLM text vs deobfuscated text
                if !raw_llm_text.is_empty() {
                    let deobfuscated = obf.deobfuscate_llm_response(&raw_llm_text);
                    if deobfuscated != raw_llm_text {
                        let obf_event = json!({
                            "type": "obfuscation_event",
                            "data": {
                                "stage": "llm_to_user",
                                "tool_name": null,
                                "tool_call_id": null,
                                "before": raw_llm_text,
                                "after": deobfuscated,
                            }
                        });
                        yield Ok(Event::default().data(obf_event.to_string()));
                    }
                }
                raw_llm_text.clear();
            }

            if errored {
                break;
            }

            // Collect final response metadata
            let final_resp = stream.final_response();
            if let Some(ref resp) = final_resp {
                total_prompt_tokens += resp.usage.prompt_tokens;
                total_completion_tokens += resp.usage.completion_tokens;
                final_model = resp.model.clone();
                final_finish_reason = format!("{:?}", resp.finish_reason);
            }

            // Check if the LLM wants to call tools
            let tool_uses_from_llm = final_resp.as_ref()
                .and_then(|r| r.tool_uses.clone())
                .unwrap_or_default();

            // Build tool_uses JSON for the done event
            let iteration_tool_uses: Vec<serde_json::Value> = tool_uses_from_llm.iter()
                .map(|tu| json!({ "id": tu.id, "name": tu.name, "input": tu.input }))
                .collect();
            final_tool_uses.extend(iteration_tool_uses);

            // If no tool calls, or not executing tools, or max iterations reached: done
            if tool_uses_from_llm.is_empty()
                || !execute_tools
                || iteration >= max_iterations
            {
                break;
            }

            // Execute tools via MCP client
            let mcp_client = if let Some(c) = &state.mcp_client {
                c.clone()
            } else {
                tracing::warn!("execute_tools=true but no MCP client configured");
                let err_event = json!({
                    "type": "error",
                    "data": { "message": "Tool execution unavailable: MCP server not connected" }
                });
                yield Ok(Event::default().data(err_event.to_string()));
                break;
            };

            // Append the assistant message with tool uses to conversation
            all_messages.push(Message::assistant_with_tool_uses(tool_uses_from_llm.clone()));

            let mut tool_results: Vec<ToolResult> = Vec::new();

            for tu in &tool_uses_from_llm {
                // Emit tool_executing event
                let exec_event = json!({
                    "type": "tool_executing",
                    "data": { "id": tu.id, "name": tu.name, "arguments": tu.input }
                });
                yield Ok(Event::default().data(exec_event.to_string()));

                // Interception 2: Deobfuscate tool args (tokens -> real IDs/names) before MCP call
                let real_args = if let Some(ref obf) = obfuscator {
                    let deobfuscated = obf.deobfuscate_tool_args(&tu.input);
                    if deobfuscated != tu.input {
                        let obf_event = json!({
                            "type": "obfuscation_event",
                            "data": {
                                "stage": "llm_to_tool",
                                "tool_name": tu.name,
                                "tool_call_id": tu.id,
                                "before": tu.input,
                                "after": deobfuscated,
                            }
                        });
                        yield Ok(Event::default().data(obf_event.to_string()));
                    }
                    deobfuscated
                } else {
                    tu.input.clone()
                };

                // Call the tool with real (deobfuscated) arguments
                tracing::info!("Calling MCP tool: {} with args: {}", tu.name, real_args);
                let result = mcp_client.call_tool(&tu.name, Some(real_args)).await;
                tracing::info!("MCP tool {} returned (is_ok={})", tu.name, result.is_ok());

                let (result_text, is_error) = match result {
                    Ok(call_result) => {
                        let err = call_result.is_error.unwrap_or(false);
                        let text = tool_result_to_string(&call_result);
                        (text, err)
                    }
                    Err(e) => {
                        tracing::error!("MCP tool {} error: {e}", tu.name);
                        (format!("Tool execution error: {e}"), true)
                    }
                };

                // Emit tool_result event with REAL (unobfuscated) data for the client
                let result_event = json!({
                    "type": "tool_result",
                    "data": {
                        "id": tu.id,
                        "name": tu.name,
                        "result": result_text,
                        "is_error": is_error,
                    }
                });
                yield Ok(Event::default().data(result_event.to_string()));

                // Interception 3: Obfuscate tool result before feeding back to LLM
                tracing::info!("Obfuscating tool result for {} (len={})", tu.name, result_text.len());
                let llm_result_text = if let Some(ref obf) = obfuscator {
                    let obfuscated = obf.obfuscate_tool_result(&result_text);
                    if obfuscated != result_text {
                        let obf_event = json!({
                            "type": "obfuscation_event",
                            "data": {
                                "stage": "tool_to_llm",
                                "tool_name": tu.name,
                                "tool_call_id": tu.id,
                                "before": result_text,
                                "after": obfuscated,
                            }
                        });
                        yield Ok(Event::default().data(obf_event.to_string()));
                    }
                    obfuscated
                } else {
                    result_text.clone()
                };
                tracing::info!("Obfuscation complete for {} (len={})", tu.name, llm_result_text.len());

                // Build ToolResult with obfuscated data for the next LLM call
                let tr = if is_error {
                    ToolResult::error(&tu.id, &llm_result_text)
                } else {
                    ToolResult::new(&tu.id, &llm_result_text)
                };
                tool_results.push(tr.set_name(&tu.name));
            }

            // Append tool results to conversation for next iteration
            all_messages.push(Message::tool_results(tool_results));

            iteration += 1;
            tracing::info!("Tool loop iteration {iteration} complete, starting next LLM call");
        }

        // Send final done event with aggregated usage
        let done = json!({
            "type": "done",
            "data": {
                "model": final_model,
                "usage": {
                    "prompt_tokens": total_prompt_tokens,
                    "completion_tokens": total_completion_tokens,
                    "total_tokens": total_prompt_tokens + total_completion_tokens,
                },
                "finish_reason": final_finish_reason,
                "tool_uses": final_tool_uses,
            }
        });
        yield Ok(Event::default().data(done.to_string()));
    };

    Ok(Sse::new(sse_stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15))))
}
