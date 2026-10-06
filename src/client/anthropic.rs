//! Anthropic Messages API over raw HTTP.
//!
//! Streaming and non-streaming share one request builder ([`build_body`]) and
//! the non-streaming path parses with [`parse_message`]; the SSE parser lives
//! in `stream.rs`. Assistant turns that carry [`ProviderContent`] from an
//! Anthropic response are replayed block for block: thinking and
//! redacted_thinking blocks must go back unchanged in a tool loop.

use serde_json::{json, Value};

use super::config::AnthropicConfig;
use crate::error::Result;
use crate::message::{CacheControl, Message, ProviderContent, Role};
use crate::request::{CompletionRequest, Effort, Thinking};
use crate::response::{CompletionResponse, FinishReason, Refusal, Usage};
use crate::stream::CompletionStream;

const API_URL: &str = "https://api.anthropic.com/v1/messages";
const API_VERSION: &str = "2023-06-01";
const DEFAULT_MAX_TOKENS: u32 = 4096;

pub(super) async fn complete(
    config: &AnthropicConfig,
    request: &CompletionRequest,
) -> Result<CompletionResponse> {
    let body = build_body(config, request, false);
    let response = send(config, &body).await?;
    let json: Value = response.json().await.map_err(|e| {
        crate::error::Error::AnthropicError(format!("Failed to parse response: {}", e))
    })?;
    Ok(parse_message(&json, &config.model))
}

pub(super) async fn stream(
    config: &AnthropicConfig,
    request: &CompletionRequest,
) -> Result<CompletionStream> {
    let body = build_body(config, request, true);
    let response = send(config, &body).await?;
    Ok(CompletionStream::anthropic_custom(
        response.bytes_stream(),
        config.model.clone(),
    ))
}

async fn send(config: &AnthropicConfig, body: &Value) -> Result<reqwest::Response> {
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};

    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(&config.api_key)
            .map_err(|e| crate::error::Error::Other(format!("Invalid API key: {}", e)))?,
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_str(config.version.as_deref().unwrap_or(API_VERSION))
            .map_err(|e| crate::error::Error::Other(format!("Invalid API version: {}", e)))?,
    );

    let response = reqwest::Client::new()
        .post(API_URL)
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|e| crate::error::Error::Other(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::error::Error::AnthropicError(format!(
            "HTTP {}: {}",
            status, error_text
        )));
    }
    Ok(response)
}

/// Build the Messages API request body.
pub(crate) fn build_body(config: &AnthropicConfig, request: &CompletionRequest, stream: bool) -> Value {
    let opts = request.options.as_ref();
    let mut body = json!({
        "model": config.model,
        "max_tokens": opts.and_then(|o| o.max_tokens).unwrap_or(DEFAULT_MAX_TOKENS),
        "messages": [],
    });
    if stream {
        body["stream"] = json!(true);
    }

    // Every system message becomes one block, in order, carrying its own
    // breakpoint. A single unmarked system message stays a plain string.
    let system: Vec<&Message> = request
        .messages
        .iter()
        .filter(|m| m.role == Role::System && !m.content.is_empty())
        .collect();
    match system.as_slice() {
        [] => {}
        [only] if only.cache_control.is_none() => body["system"] = json!(only.content),
        many => {
            let blocks: Vec<Value> = many
                .iter()
                .map(|m| {
                    let mut block = json!({ "type": "text", "text": m.content });
                    if let Some(cc) = &m.cache_control {
                        block["cache_control"] = cc.to_anthropic_json();
                    }
                    block
                })
                .collect();
            body["system"] = json!(blocks);
        }
    }

    let mut messages = Vec::new();
    for msg in request.messages.iter().filter(|m| m.role != Role::System) {
        let rendered = match msg.role {
            Role::User => Some(user_message(msg)),
            Role::Assistant => assistant_message(msg),
            Role::System => None,
        };
        if let Some(mut rendered) = rendered {
            if let Some(cc) = &msg.cache_control {
                mark_last_block(&mut rendered, cc);
            }
            messages.push(rendered);
        }
    }
    let prompt_cache = opts.and_then(|o| o.prompt_cache.as_ref());
    if let (Some(cc), Some(last)) = (prompt_cache.and_then(|p| p.tail.as_ref()), messages.last_mut()) {
        mark_last_block(last, cc);
    }
    body["messages"] = json!(messages);

    if let Some(tools) = &request.tools {
        let tools_json: Vec<_> = tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name.to_string(),
                    "description": tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                    "input_schema": Value::Object((*tool.input_schema).clone()),
                })
            })
            .collect();
        body["tools"] = json!(tools_json);
        if let (Some(cc), Some(last)) = (
            prompt_cache.and_then(|p| p.tools.as_ref()),
            body["tools"].as_array_mut().and_then(|t| t.last_mut()),
        ) {
            last["cache_control"] = cc.to_anthropic_json();
        }
    }

    if let Some(opts) = opts {
        if let Some(temp) = opts.temperature {
            body["temperature"] = json!(temp);
        }
        if let Some(top_p) = opts.top_p {
            body["top_p"] = json!(top_p);
        }
        if let Some(stops) = opts.stop_sequences.as_ref().filter(|s| !s.is_empty()) {
            body["stop_sequences"] = json!(stops);
        }
        if let Some(thinking) = &opts.thinking {
            body["thinking"] = match thinking {
                Thinking::Adaptive => json!({ "type": "adaptive" }),
                Thinking::Budget { tokens } => json!({ "type": "enabled", "budget_tokens": tokens }),
                Thinking::BetweenTools => json!({ "type": "between_tools" }),
                Thinking::Disabled => json!({ "type": "disabled" }),
            };
        }
        if let Some(effort) = opts.effort {
            body["output_config"] = json!({ "effort": anthropic_effort(effort) });
        }
    }

    normalize_breakpoints(&mut body);
    body
}

/// The most cache breakpoints one request may carry.
const MAX_BREAKPOINTS: usize = 4;

/// Put a breakpoint on a rendered message's last block that can carry one
/// (thinking blocks and empty text cannot). String content becomes a text
/// block first.
fn mark_last_block(message: &mut Value, cc: &CacheControl) {
    if let Some(text) = message["content"].as_str().map(String::from) {
        if text.is_empty() {
            return;
        }
        message["content"] = json!([{ "type": "text", "text": text }]);
    }
    let Some(blocks) = message["content"].as_array_mut() else {
        return;
    };
    let target = blocks.iter_mut().rev().find(|b| {
        match b.get("type").and_then(Value::as_str) {
            Some("thinking") | Some("redacted_thinking") => false,
            Some("text") => b.get("text").and_then(Value::as_str).is_some_and(|t| !t.is_empty()),
            _ => true,
        }
    });
    if let Some(block) = target {
        block["cache_control"] = cc.to_anthropic_json();
    }
}

/// Enforce the API's breakpoint rules so a request never 400s on them:
/// at most four (the oldest message breakpoints go first, then the oldest
/// overall; the last one always stays), and no 1-hour entry after a
/// 5-minute one (a later 1-hour breakpoint drops to 5 minutes).
fn normalize_breakpoints(body: &mut Value) {
    let mut paths = Vec::new();
    for (key, nested) in [("tools", false), ("system", false), ("messages", true)] {
        let Some(items) = body[key].as_array() else { continue };
        for (i, item) in items.iter().enumerate() {
            if nested {
                let Some(blocks) = item["content"].as_array() else { continue };
                for (j, block) in blocks.iter().enumerate() {
                    if block.get("cache_control").is_some() {
                        paths.push(format!("/{key}/{i}/content/{j}"));
                    }
                }
            } else if item.get("cache_control").is_some() {
                paths.push(format!("/{key}/{i}"));
            }
        }
    }

    while paths.len() > MAX_BREAKPOINTS {
        let last = paths.len() - 1;
        let drop = paths[..last]
            .iter()
            .position(|p| p.starts_with("/messages/"))
            .unwrap_or(0);
        let path = paths.remove(drop);
        if let Some(block) = body.pointer_mut(&path).and_then(Value::as_object_mut) {
            block.remove("cache_control");
        }
    }

    let mut seen_short = false;
    for path in &paths {
        let Some(cc) = body.pointer_mut(&format!("{path}/cache_control")) else { continue };
        let long = cc.get("ttl").and_then(Value::as_str) == Some("1h");
        if long && seen_short {
            *cc = CacheControl::Ephemeral.to_anthropic_json();
        } else if !long {
            seen_short = true;
        }
    }
}

/// Anthropic's effort scale starts at `low`.
fn anthropic_effort(effort: Effort) -> &'static str {
    match effort {
        Effort::None | Effort::Minimal | Effort::Low => "low",
        other => other.as_str(),
    }
}

fn user_message(msg: &crate::message::Message) -> Value {
    if let Some(tool_results) = &msg.tool_results {
        let content_blocks: Vec<Value> = tool_results
            .iter()
            .map(|tr| {
                json!({
                    "type": "tool_result",
                    "tool_use_id": tr.tool_call_id,
                    "content": tr.content,
                    "is_error": tr.is_error,
                })
            })
            .collect();
        return json!({ "role": "user", "content": content_blocks });
    }

    if let Some(tool_call_id) = &msg.tool_call_id {
        return json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": tool_call_id,
                "content": msg.content.clone(),
                "is_error": false,
            }]
        });
    }

    if msg.has_images() || msg.has_documents() {
        let mut content_blocks = Vec::new();

        // Documents first. Anthropic's "document" block takes application/pdf only;
        // text-based files are decoded and sent as text blocks.
        if let Some(documents) = &msg.documents {
            for doc in documents {
                if doc.media_type == "application/pdf" {
                    content_blocks.push(json!({
                        "type": "document",
                        "source": {
                            "type": "base64",
                            "media_type": doc.media_type,
                            "data": doc.data
                        }
                    }));
                } else if doc.media_type.starts_with("text/")
                    || doc.media_type == "application/json"
                    || doc.media_type == "application/xml"
                {
                    if let Ok(decoded) =
                        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &doc.data)
                    {
                        let text_content = String::from_utf8_lossy(&decoded);
                        let label = doc.filename.as_deref().unwrap_or("document");
                        content_blocks.push(json!({
                            "type": "text",
                            "text": format!("[File: {label} ({})]\n{text_content}", doc.media_type)
                        }));
                    } else {
                        eprintln!(
                            "[cnctd_ai] Failed to decode base64 for text document: {}",
                            doc.filename.as_deref().unwrap_or("unknown")
                        );
                    }
                } else {
                    eprintln!(
                        "[cnctd_ai] Skipping unsupported document type for Anthropic: {}",
                        doc.media_type
                    );
                }
            }
        }

        if let Some(images) = &msg.images {
            for image in images {
                content_blocks.push(json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": image.media_type,
                        "data": image.data
                    }
                }));
            }
        }

        if !msg.content.is_empty() {
            content_blocks.push(json!({ "type": "text", "text": msg.content.clone() }));
        }

        return json!({ "role": "user", "content": content_blocks });
    }

    json!({ "role": "user", "content": msg.content.clone() })
}

/// An assistant turn. Anthropic-produced turns replay their original blocks;
/// anything else is rebuilt from text + tool uses. Empty turns are dropped
/// (the API rejects empty text content).
fn assistant_message(msg: &crate::message::Message) -> Option<Value> {
    if let Some(pc) = msg
        .provider_content
        .as_ref()
        .filter(|pc| pc.is_for(ProviderContent::ANTHROPIC))
    {
        let blocks = replay_blocks(&pc.blocks);
        if !blocks.is_empty() {
            return Some(json!({ "role": "assistant", "content": blocks }));
        }
    }

    if let Some(tool_uses) = &msg.tool_uses {
        let mut content_blocks = Vec::new();
        if !msg.content.is_empty() {
            content_blocks.push(json!({ "type": "text", "text": msg.content.clone() }));
        }
        for tool_use in tool_uses {
            content_blocks.push(json!({
                "type": "tool_use",
                "id": tool_use.id.clone(),
                "name": tool_use.name.clone(),
                "input": tool_use.input.clone(),
            }));
        }
        return Some(json!({ "role": "assistant", "content": content_blocks }));
    }

    if msg.content.is_empty() {
        return None;
    }
    Some(json!({ "role": "assistant", "content": msg.content.clone() }))
}

/// Reduce stored response blocks to their request shape. Thinking blocks keep
/// their signature and text exactly; a thinking block without a signature
/// (an interrupted stream) cannot be replayed and is dropped. Unknown block
/// types (server tools) pass through unchanged.
pub(crate) fn replay_blocks(blocks: &[Value]) -> Vec<Value> {
    blocks
        .iter()
        .filter_map(|b| match b.get("type").and_then(Value::as_str) {
            Some("thinking") => {
                let signature = b.get("signature").and_then(Value::as_str).filter(|s| !s.is_empty())?;
                Some(json!({
                    "type": "thinking",
                    "thinking": b.get("thinking").and_then(Value::as_str).unwrap_or_default(),
                    "signature": signature,
                }))
            }
            Some("redacted_thinking") => {
                let data = b.get("data").and_then(Value::as_str)?;
                Some(json!({ "type": "redacted_thinking", "data": data }))
            }
            Some("text") => {
                let text = b.get("text").and_then(Value::as_str).unwrap_or_default();
                if text.is_empty() {
                    None
                } else {
                    Some(json!({ "type": "text", "text": text }))
                }
            }
            Some("tool_use") => Some(json!({
                "type": "tool_use",
                "id": b.get("id").cloned().unwrap_or(Value::Null),
                "name": b.get("name").cloned().unwrap_or(Value::Null),
                "input": b.get("input").filter(|v| v.is_object()).cloned().unwrap_or_else(|| json!({})),
            })),
            _ => Some(b.clone()),
        })
        .collect()
}

pub(crate) fn map_stop_reason(stop_reason: Option<&str>, has_tool_uses: bool) -> FinishReason {
    match stop_reason {
        Some("end_turn") | Some("stop_sequence") => FinishReason::Stop,
        Some("max_tokens") | Some("model_context_window_exceeded") => FinishReason::Length,
        Some("tool_use") => FinishReason::ToolUse,
        Some("refusal") => FinishReason::Refusal,
        _ if has_tool_uses => FinishReason::ToolUse,
        _ => FinishReason::Other,
    }
}

pub(crate) fn parse_refusal(stop_details: Option<&Value>) -> Refusal {
    let details = stop_details.filter(|d| d.is_object());
    Refusal {
        category: details
            .and_then(|d| d.get("category"))
            .and_then(Value::as_str)
            .map(String::from),
        explanation: details
            .and_then(|d| d.get("explanation"))
            .and_then(Value::as_str)
            .map(String::from),
    }
}

/// Anthropic usage -> [`Usage`]. Anthropic's `input_tokens` excludes the
/// cached parts; `Usage::prompt_tokens` counts every input token.
pub(crate) fn parse_usage(usage: Option<&Value>) -> Usage {
    let count = |key: &str| {
        usage
            .and_then(|u| u.get(key))
            .and_then(Value::as_u64)
            .map(|v| v as u32)
    };
    let read = count("cache_read_input_tokens");
    let write = count("cache_creation_input_tokens");
    let prompt_tokens = count("input_tokens").unwrap_or(0) + read.unwrap_or(0) + write.unwrap_or(0);
    let completion_tokens = count("output_tokens").unwrap_or(0);
    Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
        cache_creation_tokens: write,
        cache_read_tokens: read,
        cache_creation_1h_tokens: usage
            .and_then(|u| u.pointer("/cache_creation/ephemeral_1h_input_tokens"))
            .and_then(Value::as_u64)
            .map(|v| v as u32),
    }
}

/// Parse a non-streaming Messages API response.
pub(crate) fn parse_message(json: &Value, fallback_model: &str) -> CompletionResponse {
    let model = json
        .get("model")
        .and_then(Value::as_str)
        .unwrap_or(fallback_model)
        .to_string();
    let blocks: Vec<Value> = json
        .get("content")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut content = String::new();
    let mut tool_uses = Vec::new();
    for block in &blocks {
        match block.get("type").and_then(Value::as_str) {
            Some("text") => {
                if let Some(text) = block.get("text").and_then(Value::as_str) {
                    content.push_str(text);
                }
            }
            Some("tool_use") => tool_uses.push(crate::ToolUse {
                call_id: None,
                id: block.get("id").and_then(Value::as_str).unwrap_or_default().to_string(),
                name: block.get("name").and_then(Value::as_str).unwrap_or_default().to_string(),
                input: block
                    .get("input")
                    .filter(|v| v.is_object())
                    .cloned()
                    .unwrap_or_else(|| json!({})),
            }),
            _ => {}
        }
    }

    let stop_reason = json.get("stop_reason").and_then(Value::as_str);
    let finish_reason = map_stop_reason(stop_reason, !tool_uses.is_empty());
    let refusal = (finish_reason == FinishReason::Refusal)
        .then(|| parse_refusal(json.get("stop_details")));

    let usage = parse_usage(json.get("usage"));

    let tool_uses_opt = if tool_uses.is_empty() { None } else { Some(tool_uses) };
    let mut message = crate::message::Message::assistant(content);
    message.tool_uses = tool_uses_opt.clone();
    if !blocks.is_empty() {
        message.provider_content = Some(ProviderContent::new(
            ProviderContent::ANTHROPIC,
            model.clone(),
            blocks,
        ));
    }

    CompletionResponse {
        message,
        usage,
        finish_reason,
        model,
        tool_uses: tool_uses_opt,
        grounding_metadata: None,
        code_execution_results: None,
        google_maps_widget_token: None,
        reasoning_items: None,
        reasoning_summary: None,
        citations: None,
        refusal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;
    use crate::request::{PromptCache, RequestOptions};

    fn config(model: &str) -> AnthropicConfig {
        AnthropicConfig {
            api_key: "k".into(),
            model: model.into(),
            version: None,
        }
    }

    fn request(messages: Vec<Message>, options: RequestOptions) -> CompletionRequest {
        CompletionRequest {
            messages,
            tools: None,
            built_in_tools: None,
            tool_config: None,
            options: Some(options),
        }
    }

    #[test]
    fn thinking_and_effort_reach_the_body() {
        let body = build_body(
            &config("claude-opus-5-5"),
            &request(
                vec![Message::user("hi")],
                RequestOptions {
                    thinking: Some(Thinking::Adaptive),
                    effort: Some(Effort::XHigh),
                    max_tokens: Some(32000),
                    ..Default::default()
                },
            ),
            true,
        );
        assert_eq!(body["thinking"], json!({ "type": "adaptive" }));
        assert_eq!(body["output_config"], json!({ "effort": "xhigh" }));
        assert_eq!(body["max_tokens"], json!(32000));
        assert_eq!(body["stream"], json!(true));
        assert!(body.get("temperature").is_none());
    }

    #[test]
    fn effort_below_low_maps_to_low() {
        assert_eq!(anthropic_effort(Effort::None), "low");
        assert_eq!(anthropic_effort(Effort::Minimal), "low");
        assert_eq!(anthropic_effort(Effort::Max), "max");
    }

    #[test]
    fn anthropic_turns_replay_their_blocks_in_order() {
        let blocks = vec![
            json!({ "type": "thinking", "thinking": "", "signature": "sig-1" }),
            json!({ "type": "text", "text": "Checking.", "citations": null }),
            json!({ "type": "thinking", "thinking": "", "signature": "sig-2" }),
            json!({ "type": "tool_use", "id": "toolu_1", "name": "lookup", "input": { "q": "x" } }),
            json!({ "type": "redacted_thinking", "data": "enc" }),
            json!({ "type": "thinking", "thinking": "partial" }),
        ];
        let msg = Message::assistant("Checking.").with_provider_content(ProviderContent::new(
            ProviderContent::ANTHROPIC,
            "claude-opus-5-5",
            blocks,
        ));
        let body = build_body(&config("claude-opus-5-5"), &request(vec![Message::user("q"), msg], RequestOptions::default()), false);
        let content = &body["messages"][1]["content"];
        assert_eq!(
            content,
            &json!([
                { "type": "thinking", "thinking": "", "signature": "sig-1" },
                { "type": "text", "text": "Checking." },
                { "type": "thinking", "thinking": "", "signature": "sig-2" },
                { "type": "tool_use", "id": "toolu_1", "name": "lookup", "input": { "q": "x" } },
                { "type": "redacted_thinking", "data": "enc" }
            ])
        );
    }

    #[test]
    fn other_provider_content_is_ignored_and_empty_turns_dropped() {
        let gemini = Message::assistant("from gemini").with_provider_content(ProviderContent::new(
            ProviderContent::GEMINI,
            "gemini-3.8-flash",
            vec![json!({ "text": "from gemini", "thoughtSignature": "x" })],
        ));
        let body = build_body(
            &config("claude-sonnet-5-5"),
            &request(vec![Message::user("a"), gemini, Message::user("b"), Message::assistant(""), Message::user("c")], RequestOptions::default()),
            false,
        );
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 4);
        assert_eq!(msgs[1], json!({ "role": "assistant", "content": "from gemini" }));
    }

    #[test]
    fn parses_refusal_with_details() {
        let resp = parse_message(
            &json!({
                "model": "claude-opus-5-5",
                "content": [],
                "stop_reason": "refusal",
                "stop_details": { "type": "refusal", "category": "cyber", "explanation": "no" },
                "usage": { "input_tokens": 10, "output_tokens": 0 }
            }),
            "fallback",
        );
        assert_eq!(resp.finish_reason, FinishReason::Refusal);
        assert_eq!(resp.refusal.as_ref().unwrap().category.as_deref(), Some("cyber"));
        assert!(resp.message.provider_content.is_none());
    }

    #[test]
    fn parses_thinking_tool_turn() {
        let resp = parse_message(
            &json!({
                "model": "claude-opus-5-5",
                "content": [
                    { "type": "thinking", "thinking": "", "signature": "s" },
                    { "type": "tool_use", "id": "t1", "name": "n", "input": {} }
                ],
                "stop_reason": "tool_use",
                "usage": { "input_tokens": 5, "output_tokens": 7, "cache_read_input_tokens": 3 }
            }),
            "fallback",
        );
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
        assert_eq!(resp.tool_uses.as_ref().unwrap()[0].id, "t1");
        assert_eq!(resp.usage.cache_read_tokens, Some(3));
        assert_eq!(resp.usage.prompt_tokens, 8);
        let pc = resp.message.provider_content.unwrap();
        assert_eq!(pc.blocks.len(), 2);
    }

    fn tool(name: &str) -> crate::Tool {
        crate::tool_helpers::create_tool(name, "d", json!({ "type": "object", "properties": {} })).unwrap()
    }

    fn cached(messages: Vec<Message>) -> CompletionRequest {
        CompletionRequest {
            messages,
            tools: Some(vec![tool("a"), tool("b")]),
            built_in_tools: None,
            tool_config: None,
            options: Some(RequestOptions {
                prompt_cache: Some(PromptCache {
                    tools: Some(CacheControl::Extended),
                    tail: Some(CacheControl::Ephemeral),
                    key: None,
                }),
                ..Default::default()
            }),
        }
    }

    #[test]
    fn chat_turn_breakpoints() {
        let body = build_body(
            &config("claude-opus-5-5"),
            &cached(vec![
                Message::system("stable").with_extended_cache(),
                Message::user("first"),
                Message::assistant("answer").with_cache(),
                Message::user("<context>memory</context> second"),
            ]),
            false,
        );
        let one_hour = json!({ "type": "ephemeral", "ttl": "1h" });
        let five_min = json!({ "type": "ephemeral" });
        assert!(body["tools"][0].get("cache_control").is_none());
        assert_eq!(body["tools"][1]["cache_control"], one_hour);
        assert_eq!(body["system"], json!([{ "type": "text", "text": "stable", "cache_control": one_hour }]));
        assert_eq!(body["messages"][0]["content"], json!("first"));
        assert_eq!(
            body["messages"][1]["content"],
            json!([{ "type": "text", "text": "answer", "cache_control": five_min }])
        );
        assert_eq!(body["messages"][2]["content"][0]["cache_control"], five_min);
    }

    #[test]
    fn single_unmarked_system_stays_a_string() {
        let body = build_body(
            &config("claude-opus-5-5"),
            &request(vec![Message::system("s"), Message::user("u")], RequestOptions::default()),
            false,
        );
        assert_eq!(body["system"], json!("s"));
        assert_eq!(body["messages"][0]["content"], json!("u"));
    }

    #[test]
    fn tail_skips_thinking_and_lands_on_tool_results() {
        let assistant = Message::assistant("").with_provider_content(ProviderContent::new(
            ProviderContent::ANTHROPIC,
            "claude-opus-5-5",
            vec![
                json!({ "type": "tool_use", "id": "t1", "name": "a", "input": {} }),
                json!({ "type": "thinking", "thinking": "", "signature": "s" }),
            ],
        ));
        let mut req = cached(vec![Message::user("go"), assistant.with_cache()]);
        let body = build_body(&config("claude-opus-5-5"), &req, false);
        let blocks = &body["messages"][1]["content"];
        assert!(blocks[0].get("cache_control").is_some());
        assert!(blocks[1].get("cache_control").is_none());

        req.messages.push(Message::tool_results(vec![crate::ToolResult::with_name("t1", "ok", "a")]));
        let body = build_body(&config("claude-opus-5-5"), &req, false);
        assert_eq!(body["messages"][2]["content"][0]["type"], json!("tool_result"));
        assert!(body["messages"][2]["content"][0].get("cache_control").is_some());
    }

    #[test]
    fn breakpoints_capped_at_four_and_long_before_short() {
        let body = build_body(
            &config("claude-opus-5-5"),
            &cached(vec![
                Message::system("stable").with_extended_cache(),
                Message::user("1").with_cache(),
                Message::assistant("2").with_cache(),
                Message::user("3").with_extended_cache(),
                Message::assistant("4"),
                Message::user("5"),
            ]),
            false,
        );
        let marks: Vec<String> = ["/tools/1", "/system/0", "/messages/0/content/0", "/messages/1/content/0", "/messages/2/content/0", "/messages/4/content/0"]
            .iter()
            .filter(|p| body.pointer(&format!("{p}/cache_control")).is_some())
            .map(|p| p.to_string())
            .collect();
        assert_eq!(marks, ["/tools/1", "/system/0", "/messages/2/content/0", "/messages/4/content/0"]);
        // The 1h breakpoint on message 3 follows a 5m one that was dropped, so it keeps its TTL.
        assert_eq!(body.pointer("/messages/2/content/0/cache_control/ttl"), Some(&json!("1h")));

        let body = build_body(
            &config("claude-opus-5-5"),
            &cached(vec![Message::user("1").with_cache(), Message::user("2").with_extended_cache()]),
            false,
        );
        // tools (1h), message 1 (5m), message 2 (1h -> 5m: it follows a 5m entry), tail on message 2.
        assert_eq!(body.pointer("/messages/1/content/0/cache_control"), Some(&json!({ "type": "ephemeral" })));
    }

    #[test]
    fn usage_counts_cached_input() {
        let usage = parse_usage(Some(&json!({
            "input_tokens": 10,
            "cache_creation_input_tokens": 200,
            "cache_read_input_tokens": 3000,
            "cache_creation": { "ephemeral_5m_input_tokens": 50, "ephemeral_1h_input_tokens": 150 },
            "output_tokens": 7
        })));
        assert_eq!(usage.prompt_tokens, 3210);
        assert_eq!(usage.cache_read_tokens, Some(3000));
        assert_eq!(usage.cache_creation_tokens, Some(200));
        assert_eq!(usage.cache_creation_1h_tokens, Some(150));
        assert_eq!(usage.effective_prompt_tokens(), 10);
        assert_eq!(usage.total_tokens, 3217);
    }
}
