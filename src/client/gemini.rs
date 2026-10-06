//! Gemini generateContent over raw HTTP.
//!
//! Streaming and non-streaming share [`build_body`]. Model turns produced by
//! Gemini carry their `parts` as [`ProviderContent`] and are replayed as-is:
//! Gemini 3 rejects a tool step whose first functionCall part is missing its
//! thoughtSignature, and function responses must carry the call's `id`.

use serde_json::{json, Map, Value};

use super::config::GeminiConfig;
use crate::error::Result;
use crate::message::{Message, ProviderContent, Role};
use crate::request::{BuiltInTool, CompletionRequest, MediaResolution, Thinking};
use crate::response::{CodeExecutionResult, CompletionResponse, FinishReason, GroundingMetadata, Refusal, Usage};
use crate::stream::CompletionStream;

/// Google's documented value for history that has no real signature
/// (turns from another model, or stored before signatures were kept).
const DUMMY_THOUGHT_SIGNATURE: &str = "skip_thought_signature_validator";
/// Prefix of the ids cnctd_ai mints when Gemini returns a call without one.
const SYNTHETIC_ID_PREFIX: &str = "gemini_call_";

/// Sanitize a JSON Schema for Gemini's function declaration format.
/// Gemini has stricter requirements than standard JSON Schema:
/// - No `$schema` field
/// - No `additionalProperties` field
/// - `type` must be a string, not an array (convert ["string", "null"] to "string")
fn sanitize_schema_for_gemini(schema: &Map<String, Value>) -> Map<String, Value> {
    let mut result = Map::new();
    for (key, value) in schema {
        if key == "$schema" || key == "additionalProperties" {
            continue;
        }
        match value {
            Value::Object(obj) => {
                result.insert(key.clone(), Value::Object(sanitize_schema_for_gemini(obj)));
            }
            Value::Array(arr) if key == "type" => {
                let first_non_null = arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .find(|s| *s != "null")
                    .unwrap_or("string");
                result.insert(key.clone(), Value::String(first_non_null.to_string()));
            }
            Value::Array(arr) => {
                let sanitized: Vec<Value> = arr
                    .iter()
                    .map(|v| match v {
                        Value::Object(obj) => Value::Object(sanitize_schema_for_gemini(obj)),
                        other => other.clone(),
                    })
                    .collect();
                result.insert(key.clone(), Value::Array(sanitized));
            }
            _ => {
                result.insert(key.clone(), value.clone());
            }
        }
    }
    result
}

pub(super) async fn complete(config: &GeminiConfig, request: &CompletionRequest) -> Result<CompletionResponse> {
    let body = build_body(request);
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        config.model, config.api_key
    );
    let response = send(&url, &body).await?;
    let response_json: Value = response
        .json()
        .await
        .map_err(|e| crate::error::Error::GeminiError(format!("Failed to parse response: {}", e)))?;
    parse_response(&response_json, &config.model)
}

pub(super) async fn stream(config: &GeminiConfig, request: &CompletionRequest) -> Result<CompletionStream> {
    let body = build_body(request);
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse&key={}",
        config.model, config.api_key
    );
    let response = send(&url, &body).await?;
    Ok(CompletionStream::gemini_custom(response.bytes_stream(), config.model.clone()))
}

async fn send(url: &str, body: &Value) -> Result<reqwest::Response> {
    use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let response = reqwest::Client::new()
        .post(url)
        .headers(headers)
        .json(body)
        .send()
        .await
        .map_err(|e| crate::error::Error::GeminiError(format!("HTTP request failed: {}", e)))?;
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(crate::error::Error::from_gemini_error(format!("HTTP {}: {}", status, error_text)));
    }
    Ok(response)
}

/// Build the generateContent request body (shared by both endpoints).
pub(crate) fn build_body(request: &CompletionRequest) -> Value {
    let mut body = json!({ "contents": [] });

    // Gemini caches implicitly; with_cache() is a no-op here.
    if let Some(system_msg) = request.messages.iter().find(|m| m.role == Role::System) {
        body["systemInstruction"] = json!({ "parts": [{ "text": system_msg.content.clone() }] });
    }

    let mut contents = Vec::new();
    for msg in request.messages.iter().filter(|m| m.role != Role::System) {
        match msg.role {
            Role::User => contents.push(user_content(msg)),
            Role::Assistant => {
                if let Some(content) = model_content(msg) {
                    contents.push(content);
                }
            }
            Role::System => {}
        }
    }
    body["contents"] = json!(contents);

    // Function declarations and built-in tools. Built-in tools are skipped
    // when function declarations exist (function calling takes priority).
    let mut tools_array: Vec<Value> = Vec::new();
    let has_function_declarations = request.tools.as_ref().map(|t| !t.is_empty()).unwrap_or(false);
    if let Some(tools) = &request.tools {
        let function_declarations: Vec<_> = tools
            .iter()
            .map(|tool| {
                json!({
                    "name": tool.name.to_string(),
                    "description": tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                    "parameters": Value::Object(sanitize_schema_for_gemini(&tool.input_schema))
                })
            })
            .collect();
        tools_array.push(json!({ "functionDeclarations": function_declarations }));
    }
    if !has_function_declarations {
        for tool in request.built_in_tools.iter().flatten() {
            match tool {
                BuiltInTool::GoogleSearch => tools_array.push(json!({ "googleSearch": {} })),
                BuiltInTool::GoogleSearchRetrieval { dynamic_threshold } => {
                    let mut config = json!({ "mode": "MODE_DYNAMIC" });
                    if let Some(threshold) = dynamic_threshold {
                        config["dynamicThreshold"] = json!(threshold);
                    }
                    tools_array.push(json!({ "googleSearchRetrieval": { "dynamicRetrievalConfig": config } }));
                }
                BuiltInTool::CodeExecution => tools_array.push(json!({ "codeExecution": {} })),
                BuiltInTool::UrlContext => tools_array.push(json!({ "urlContext": {} })),
                BuiltInTool::GoogleMaps { enable_widget } => {
                    let mut maps_config = Map::new();
                    if let Some(enable) = enable_widget {
                        maps_config.insert("enableWidget".to_string(), json!(enable));
                    }
                    tools_array.push(json!({ "googleMaps": maps_config }));
                }
                BuiltInTool::OpenAiCodeInterpreter
                | BuiltInTool::OpenAiWebSearch
                | BuiltInTool::OpenAiImageGeneration => {}
            }
        }
    }
    if !tools_array.is_empty() {
        body["tools"] = json!(tools_array);
    }

    if let Some(retrieval_config) = request.tool_config.as_ref().and_then(|c| c.retrieval_config.as_ref()) {
        let mut retrieval_json = Map::new();
        if let Some(lat_lng) = &retrieval_config.lat_lng {
            retrieval_json.insert(
                "latLng".to_string(),
                json!({ "latitude": lat_lng.latitude, "longitude": lat_lng.longitude }),
            );
        }
        if let Some(lang) = &retrieval_config.language_code {
            retrieval_json.insert("languageCode".to_string(), json!(lang));
        }
        if !retrieval_json.is_empty() {
            body["toolConfig"] = json!({ "retrievalConfig": retrieval_json });
        }
    }

    let mut generation_config = Map::new();
    if let Some(opts) = &request.options {
        if let Some(temp) = opts.temperature {
            generation_config.insert("temperature".to_string(), json!(temp));
        }
        if let Some(max_tokens) = opts.max_tokens {
            generation_config.insert("maxOutputTokens".to_string(), json!(max_tokens));
        }
        if let Some(top_p) = opts.top_p {
            generation_config.insert("topP".to_string(), json!(top_p));
        }
        if let Some(stops) = opts.stop_sequences.as_ref().filter(|s| !s.is_empty()) {
            generation_config.insert("stopSequences".to_string(), json!(stops));
        }

        // thinkingBudget (Gemini 2.5) and thinkingLevel (Gemini 3) cannot be
        // sent together; an explicit budget wins.
        let mut thinking_config = Map::new();
        match &opts.thinking {
            Some(Thinking::Budget { tokens }) => {
                thinking_config.insert("thinkingBudget".to_string(), json!(tokens));
            }
            Some(Thinking::Disabled) => {
                thinking_config.insert("thinkingBudget".to_string(), json!(0));
            }
            _ => {
                let level = opts
                    .thinking_level
                    .clone()
                    .or_else(|| opts.effort.map(|e| e.to_thinking_level()));
                if let Some(level) = level {
                    thinking_config.insert("thinkingLevel".to_string(), json!(level.as_str()));
                }
            }
        }
        if !thinking_config.is_empty() {
            generation_config.insert("thinkingConfig".to_string(), Value::Object(thinking_config));
        }

        if let Some(resolution) = &opts.media_resolution {
            let value = match resolution {
                MediaResolution::Low => "MEDIA_RESOLUTION_LOW",
                MediaResolution::Medium => "MEDIA_RESOLUTION_MEDIUM",
                // ultra_high exists only per content part; the request-wide
                // setting tops out at high.
                MediaResolution::High | MediaResolution::UltraHigh => "MEDIA_RESOLUTION_HIGH",
            };
            generation_config.insert("mediaResolution".to_string(), json!(value));
        }
    }
    if !generation_config.is_empty() {
        body["generationConfig"] = Value::Object(generation_config);
    }

    body
}

fn is_synthetic_id(id: &str) -> bool {
    id.is_empty() || id.starts_with(SYNTHETIC_ID_PREFIX)
}

fn user_content(msg: &Message) -> Value {
    if let Some(tool_results) = &msg.tool_results {
        let parts: Vec<Value> = tool_results
            .iter()
            .map(|result| {
                let mut function_response = json!({
                    "name": result.function_name.clone().unwrap_or_else(|| "function".to_string()),
                    "response": { "result": result.content.clone() }
                });
                if !is_synthetic_id(&result.tool_call_id) {
                    function_response["id"] = json!(result.tool_call_id);
                }
                json!({ "functionResponse": function_response })
            })
            .collect();
        return json!({ "role": "user", "parts": parts });
    }

    if msg.tool_call_id.is_some() {
        // Legacy single tool result: no function name was stored.
        return json!({
            "role": "user",
            "parts": [{
                "functionResponse": {
                    "name": "function",
                    "response": { "result": msg.content.clone() }
                }
            }]
        });
    }

    if msg.has_images() || msg.has_videos() || msg.has_documents() {
        let mut parts = Vec::new();
        for doc in msg.documents.iter().flatten() {
            parts.push(json!({ "inlineData": { "mimeType": doc.media_type, "data": doc.data } }));
        }
        for image in msg.images.iter().flatten() {
            parts.push(json!({ "inlineData": { "mimeType": image.media_type, "data": image.data } }));
        }
        for video in msg.videos.iter().flatten() {
            parts.push(json!({ "inlineData": { "mimeType": video.media_type, "data": video.data } }));
        }
        if !msg.content.is_empty() {
            parts.push(json!({ "text": msg.content.clone() }));
        }
        return json!({ "role": "user", "parts": parts });
    }

    json!({ "role": "user", "parts": [{ "text": msg.content.clone() }] })
}

/// A model turn: Gemini-produced turns replay their parts; anything else is
/// rebuilt from text + tool uses with the documented placeholder signature.
fn model_content(msg: &Message) -> Option<Value> {
    if let Some(pc) = msg
        .provider_content
        .as_ref()
        .filter(|pc| pc.is_for(ProviderContent::GEMINI))
    {
        return Some(json!({ "role": "model", "parts": pc.blocks }));
    }

    if let Some(tool_uses) = &msg.tool_uses {
        let mut parts = Vec::new();
        if !msg.content.is_empty() {
            parts.push(json!({ "text": msg.content.clone() }));
        }
        for tool_use in tool_uses {
            let mut function_call = json!({ "name": tool_use.name.clone(), "args": tool_use.input.clone() });
            if !is_synthetic_id(&tool_use.id) {
                function_call["id"] = json!(tool_use.id);
            }
            parts.push(json!({ "functionCall": function_call, "thoughtSignature": DUMMY_THOUGHT_SIGNATURE }));
        }
        return Some(json!({ "role": "model", "parts": parts }));
    }

    if msg.content.is_empty() {
        return None;
    }
    Some(json!({ "role": "model", "parts": [{ "text": msg.content.clone() }] }))
}

/// Keep a response part for replay. Consecutive plain text parts (no
/// signature, not a thought) are merged so a long streamed answer does not
/// become thousands of parts; a part carrying a signature is never merged.
pub(crate) fn push_part(parts: &mut Vec<Value>, part: &Value) {
    let plain_text = |p: &Value| {
        p.as_object()
            .map(|o| o.len() == 1 && o.get("text").map(Value::is_string).unwrap_or(false))
            .unwrap_or(false)
    };
    if plain_text(part) {
        if let Some(last) = parts.last_mut().filter(|l| plain_text(l)) {
            if let (Some(Value::String(existing)), Some(addition)) =
                (last.get_mut("text"), part["text"].as_str())
            {
                existing.push_str(addition);
                return;
            }
        }
    }
    parts.push(part.clone());
}

/// The tool use for a functionCall part, keeping Gemini's call id when given.
pub(crate) fn function_call_to_tool_use(part: &Value) -> Option<crate::ToolUse> {
    let function_call = part.get("functionCall")?.as_object()?;
    let id = function_call
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .unwrap_or_else(|| format!("{}{}", SYNTHETIC_ID_PREFIX, uuid::Uuid::new_v4()));
    Some(crate::ToolUse {
        call_id: None,
        id,
        name: function_call.get("name").and_then(Value::as_str).unwrap_or("").to_string(),
        input: function_call.get("args").cloned().unwrap_or_else(|| json!({})),
    })
}

pub(crate) fn map_finish_reason(reason: &str, has_tool_uses: bool) -> FinishReason {
    match reason {
        "STOP" if has_tool_uses => FinishReason::ToolUse,
        "STOP" => FinishReason::Stop,
        "MAX_TOKENS" => FinishReason::Length,
        "SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII" | "IMAGE_SAFETY"
        | "IMAGE_PROHIBITED_CONTENT" | "IMAGE_RECITATION" => FinishReason::Refusal,
        _ if has_tool_uses => FinishReason::ToolUse,
        _ => FinishReason::Other,
    }
}

pub(crate) fn parse_usage(usage_data: &Value) -> Usage {
    let count = |key: &str| usage_data.get(key).and_then(Value::as_u64).unwrap_or(0) as u32;
    // Thinking tokens are billed as output but reported separately.
    let completion_tokens = count("candidatesTokenCount") + count("thoughtsTokenCount");
    Usage {
        prompt_tokens: count("promptTokenCount"),
        completion_tokens,
        total_tokens: count("totalTokenCount"),
        cache_creation_tokens: None,
        cache_read_tokens: usage_data
            .get("cachedContentTokenCount")
            .and_then(Value::as_u64)
            .map(|v| v as u32),
        cache_creation_1h_tokens: None,
    }
}

/// Parse a non-streaming generateContent response.
pub(crate) fn parse_response(response_json: &Value, model: &str) -> Result<CompletionResponse> {
    let usage = response_json.get("usageMetadata").map(parse_usage).unwrap_or_else(Usage::zero);

    let Some(candidate) = response_json["candidates"].get(0) else {
        // A blocked prompt has no candidates, only a block reason.
        if let Some(reason) = response_json["promptFeedback"]["blockReason"].as_str() {
            return Ok(refusal_response(
                model,
                usage,
                Refusal {
                    category: Some(reason.to_string()),
                    explanation: response_json["promptFeedback"]["blockReasonMessage"].as_str().map(String::from),
                },
            ));
        }
        return Err(crate::error::Error::GeminiError("No candidates in response".into()));
    };

    let mut content = String::new();
    let mut tool_uses = Vec::new();
    let mut code_execution_results: Vec<CodeExecutionResult> = Vec::new();
    let mut parts: Vec<Value> = Vec::new();

    for part in candidate["content"]["parts"].as_array().into_iter().flatten() {
        push_part(&mut parts, part);
        if let Some(text) = part["text"].as_str() {
            if !part["thought"].as_bool().unwrap_or(false) {
                content.push_str(text);
            }
        }
        if let Some(tool_use) = function_call_to_tool_use(part) {
            tool_uses.push(tool_use);
        }
        if let Some(executable_code) = part.get("executableCode") {
            code_execution_results.push(CodeExecutionResult {
                code: executable_code["code"].as_str().map(|s| s.to_string()),
                language: executable_code["language"].as_str().map(|s| s.to_string()),
                outcome: None,
                output: None,
            });
        }
        if let Some(code_result) = part.get("codeExecutionResult") {
            let outcome = code_result["outcome"]
                .as_str()
                .and_then(|s| serde_json::from_value(json!(s)).ok());
            let output = code_result["output"].as_str().map(|s| s.to_string());
            if let Some(last) = code_execution_results.last_mut().filter(|l| l.outcome.is_none()) {
                last.outcome = outcome;
                last.output = output;
            } else {
                code_execution_results.push(CodeExecutionResult { code: None, language: None, outcome, output });
            }
        }
    }

    let finish_reason_str = candidate["finishReason"].as_str().unwrap_or("OTHER");
    let finish_reason = map_finish_reason(finish_reason_str, !tool_uses.is_empty());
    let refusal = (finish_reason == FinishReason::Refusal).then(|| Refusal {
        category: Some(finish_reason_str.to_string()),
        explanation: candidate["finishMessage"].as_str().map(String::from),
    });

    let tool_uses_opt = if tool_uses.is_empty() { None } else { Some(tool_uses) };
    let mut message = Message::assistant(content);
    message.tool_uses = tool_uses_opt.clone();
    if !parts.is_empty() {
        message.provider_content = Some(ProviderContent::new(ProviderContent::GEMINI, model, parts));
    }

    Ok(CompletionResponse {
        message,
        usage,
        finish_reason,
        model: response_json["modelVersion"].as_str().unwrap_or(model).to_string(),
        tool_uses: tool_uses_opt,
        grounding_metadata: candidate
            .get("groundingMetadata")
            .and_then(|gm| serde_json::from_value::<GroundingMetadata>(gm.clone()).ok()),
        code_execution_results: if code_execution_results.is_empty() { None } else { Some(code_execution_results) },
        google_maps_widget_token: candidate
            .get("googleMapsWidgetContextToken")
            .and_then(Value::as_str)
            .map(String::from),
        reasoning_items: None,
        reasoning_summary: None,
        citations: None,
        refusal,
    })
}

fn refusal_response(model: &str, usage: Usage, refusal: Refusal) -> CompletionResponse {
    CompletionResponse {
        message: Message::assistant(""),
        usage,
        finish_reason: FinishReason::Refusal,
        model: model.to_string(),
        tool_uses: None,
        grounding_metadata: None,
        code_execution_results: None,
        google_maps_widget_token: None,
        reasoning_items: None,
        reasoning_summary: None,
        citations: None,
        refusal: Some(refusal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::request::{Effort, RequestOptions, ThinkingLevel};
    use crate::{ToolResult, ToolUse};

    fn request(messages: Vec<Message>, options: Option<RequestOptions>) -> CompletionRequest {
        CompletionRequest { messages, tools: None, built_in_tools: None, tool_config: None, options }
    }

    #[test]
    fn effort_maps_to_thinking_level_and_budget_wins() {
        let body = build_body(&request(
            vec![Message::user("hi")],
            Some(RequestOptions { effort: Some(Effort::Max), ..Default::default() }),
        ));
        assert_eq!(body["generationConfig"]["thinkingConfig"], json!({ "thinkingLevel": "high" }));

        let body = build_body(&request(
            vec![Message::user("hi")],
            Some(RequestOptions {
                thinking_level: Some(ThinkingLevel::Minimal),
                effort: Some(Effort::High),
                ..Default::default()
            }),
        ));
        assert_eq!(body["generationConfig"]["thinkingConfig"], json!({ "thinkingLevel": "minimal" }));

        let body = build_body(&request(
            vec![Message::user("hi")],
            Some(RequestOptions {
                thinking: Some(Thinking::Disabled),
                effort: Some(Effort::High),
                ..Default::default()
            }),
        ));
        assert_eq!(body["generationConfig"]["thinkingConfig"], json!({ "thinkingBudget": 0 }));
    }

    #[test]
    fn gemini_turns_replay_parts_and_results_carry_ids() {
        let parts = vec![
            json!({ "functionCall": { "id": "fc-1", "name": "a", "args": {} }, "thoughtSignature": "SIG" }),
            json!({ "functionCall": { "id": "fc-2", "name": "b", "args": {} } }),
        ];
        let assistant = Message::assistant_with_tool_uses(vec![
            ToolUse { id: "fc-1".into(), call_id: None, name: "a".into(), input: json!({}) },
            ToolUse { id: "fc-2".into(), call_id: None, name: "b".into(), input: json!({}) },
        ])
        .with_provider_content(ProviderContent::new(ProviderContent::GEMINI, "gemini-3.8-flash", parts.clone()));
        let results = Message::tool_results(vec![
            ToolResult::with_name("fc-1", "r1", "a"),
            ToolResult::with_name("fc-2", "r2", "b"),
        ]);
        let body = build_body(&request(vec![Message::user("go"), assistant, results], None));
        assert_eq!(body["contents"][1]["parts"], json!(parts));
        assert_eq!(body["contents"][2]["parts"][0]["functionResponse"]["id"], json!("fc-1"));
        assert_eq!(body["contents"][2]["parts"][1]["functionResponse"]["id"], json!("fc-2"));
    }

    #[test]
    fn rebuilt_turns_use_placeholder_signature_and_skip_synthetic_ids() {
        let assistant = Message::assistant_with_tool_uses(vec![ToolUse {
            id: "gemini_call_abc".into(),
            call_id: None,
            name: "a".into(),
            input: json!({}),
        }]);
        let body = build_body(&request(vec![Message::user("go"), assistant], None));
        let part = &body["contents"][1]["parts"][0];
        assert_eq!(part["thoughtSignature"], json!(DUMMY_THOUGHT_SIGNATURE));
        assert!(part["functionCall"].get("id").is_none());
    }

    #[test]
    fn push_part_merges_plain_text_only() {
        let mut parts = Vec::new();
        push_part(&mut parts, &json!({ "text": "Hel" }));
        push_part(&mut parts, &json!({ "text": "lo" }));
        push_part(&mut parts, &json!({ "text": "", "thoughtSignature": "S" }));
        push_part(&mut parts, &json!({ "text": "!" }));
        assert_eq!(parts, vec![json!({ "text": "Hello" }), json!({ "text": "", "thoughtSignature": "S" }), json!({ "text": "!" })]);
    }

    #[test]
    fn blocked_prompt_is_a_refusal() {
        let resp = parse_response(
            &json!({ "promptFeedback": { "blockReason": "PROHIBITED_CONTENT" }, "usageMetadata": { "promptTokenCount": 4 } }),
            "gemini-3.8-flash",
        )
        .unwrap();
        assert_eq!(resp.finish_reason, FinishReason::Refusal);
        assert_eq!(resp.refusal.unwrap().category.as_deref(), Some("PROHIBITED_CONTENT"));
    }

    #[test]
    fn parses_ids_signatures_and_thought_tokens() {
        let resp = parse_response(
            &json!({
                "candidates": [{
                    "content": { "parts": [
                        { "functionCall": { "id": "fc-9", "name": "lookup", "args": { "q": 1 } }, "thoughtSignature": "SIG" }
                    ]},
                    "finishReason": "STOP"
                }],
                "usageMetadata": { "promptTokenCount": 10, "candidatesTokenCount": 5, "thoughtsTokenCount": 20, "totalTokenCount": 35 },
                "modelVersion": "gemini-3.8-flash"
            }),
            "gemini-3.8-flash",
        )
        .unwrap();
        assert_eq!(resp.finish_reason, FinishReason::ToolUse);
        assert_eq!(resp.tool_uses.as_ref().unwrap()[0].id, "fc-9");
        assert_eq!(resp.usage.completion_tokens, 25);
        assert_eq!(resp.message.provider_content.unwrap().blocks[0]["thoughtSignature"], json!("SIG"));
    }
}
