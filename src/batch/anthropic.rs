use crate::client::config::AnthropicConfig;
use crate::error::{Error, Result};
use crate::message::Role;
use crate::response::{CompletionResponse, FinishReason, Usage};
use crate::message::Message;
use crate::ToolUse;
use super::types::*;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const ANTHROPIC_API_BASE: &str = "https://api.anthropic.com/v1";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const ANTHROPIC_BETA: &str = "message-batches-2024-09-24";

fn build_headers(api_key: &str) -> Result<HeaderMap> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        "x-api-key",
        HeaderValue::from_str(api_key)
            .map_err(|e| Error::Other(format!("Invalid API key: {}", e)))?,
    );
    headers.insert(
        "anthropic-version",
        HeaderValue::from_static(ANTHROPIC_VERSION),
    );
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static(ANTHROPIC_BETA),
    );
    Ok(headers)
}

/// Anthropic API request format for batch items
#[derive(Serialize)]
struct AnthropicBatchRequest {
    requests: Vec<AnthropicBatchItem>,
}

#[derive(Serialize)]
struct AnthropicBatchItem {
    custom_id: String,
    params: AnthropicMessageParams,
}

#[derive(Serialize)]
struct AnthropicMessageParams {
    model: String,
    max_tokens: u32,
    messages: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<serde_json::Value>>,
}

/// Anthropic API response types
#[derive(Deserialize, Debug)]
struct AnthropicBatchResponse {
    id: String,
    #[serde(rename = "type")]
    _type: String,
    processing_status: String,
    request_counts: AnthropicRequestCounts,
    ended_at: Option<String>,
    created_at: String,
    expires_at: String,
    results_url: Option<String>,
}

#[derive(Deserialize, Debug)]
struct AnthropicRequestCounts {
    processing: u32,
    succeeded: u32,
    errored: u32,
    canceled: u32,
    expired: u32,
}

#[derive(Deserialize, Debug)]
struct AnthropicBatchResultLine {
    custom_id: String,
    result: AnthropicResultPayload,
}

#[derive(Deserialize, Debug)]
struct AnthropicResultPayload {
    #[serde(rename = "type")]
    result_type: String,
    message: Option<AnthropicMessageResponse>,
    error: Option<AnthropicErrorResponse>,
}

#[derive(Deserialize, Debug)]
struct AnthropicMessageResponse {
    id: String,
    model: String,
    content: Vec<AnthropicContentBlock>,
    stop_reason: Option<String>,
    usage: AnthropicUsage,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: serde_json::Value },
}

#[derive(Deserialize, Debug)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

#[derive(Deserialize, Debug)]
struct AnthropicErrorResponse {
    #[serde(rename = "type")]
    error_type: String,
    message: String,
}

#[derive(Deserialize, Debug)]
struct AnthropicListResponse {
    data: Vec<AnthropicBatchResponse>,
    has_more: bool,
}

fn convert_batch_item(item: &BatchItem, config: &AnthropicConfig) -> AnthropicBatchItem {
    let mut messages = Vec::new();
    let mut system_msg = None;

    for msg in &item.request.messages {
        match msg.role {
            Role::System => {
                system_msg = Some(msg.content.clone());
            }
            Role::User => {
                if let Some(tool_call_id) = &msg.tool_call_id {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tool_call_id,
                            "content": msg.content.clone(),
                        }]
                    }));
                } else {
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": msg.content.clone(),
                    }));
                }
            }
            Role::Assistant => {
                if let Some(tool_uses) = &msg.tool_uses {
                    let mut content_blocks = Vec::new();
                    if !msg.content.is_empty() {
                        content_blocks.push(serde_json::json!({
                            "type": "text",
                            "text": msg.content.clone(),
                        }));
                    }
                    for tool_use in tool_uses {
                        content_blocks.push(serde_json::json!({
                            "type": "tool_use",
                            "id": tool_use.id.clone(),
                            "name": tool_use.name.clone(),
                            "input": tool_use.input.clone(),
                        }));
                    }
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": content_blocks,
                    }));
                } else {
                    messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": msg.content.clone(),
                    }));
                }
            }
        }
    }

    let tools = item.request.tools.as_ref().map(|tools| {
        tools
            .iter()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name.to_string(),
                    "description": tool.description.as_ref().map(|d| d.to_string()).unwrap_or_default(),
                    "input_schema": serde_json::Value::Object((*tool.input_schema).clone()),
                })
            })
            .collect()
    });

    let opts = item.request.options.as_ref();

    AnthropicBatchItem {
        custom_id: item.custom_id.clone(),
        params: AnthropicMessageParams {
            model: config.model.clone(),
            max_tokens: opts.and_then(|o| o.max_tokens).unwrap_or(4096),
            messages,
            system: system_msg,
            temperature: opts.and_then(|o| o.temperature),
            top_p: opts.and_then(|o| o.top_p),
            tools,
        },
    }
}

fn parse_status(status: &str) -> BatchStatus {
    match status {
        "in_progress" => BatchStatus::InProgress,
        "ended" => BatchStatus::Completed,
        "canceling" => BatchStatus::Cancelling,
        _ => BatchStatus::InProgress,
    }
}

fn parse_datetime(s: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc).timestamp())
        .ok()
}

fn convert_batch_response(resp: AnthropicBatchResponse) -> BatchInfo {
    let counts = &resp.request_counts;
    let total = counts.processing + counts.succeeded + counts.errored + counts.canceled + counts.expired;

    BatchInfo {
        id: resp.id,
        status: parse_status(&resp.processing_status),
        created_at: parse_datetime(&resp.created_at),
        completed_at: resp.ended_at.as_ref().and_then(|s| parse_datetime(s)),
        expires_at: parse_datetime(&resp.expires_at),
        counts: Some(BatchCounts {
            total,
            completed: counts.succeeded,
            failed: counts.errored + counts.canceled + counts.expired,
        }),
        error_message: None,
    }
}

pub(crate) async fn create_batch(
    config: &AnthropicConfig,
    items: Vec<BatchItem>,
) -> Result<BatchInfo> {
    let client = reqwest::Client::new();
    let headers = build_headers(&config.api_key)?;

    let request_body = AnthropicBatchRequest {
        requests: items.iter().map(|item| convert_batch_item(item, config)).collect(),
    };

    let response = client
        .post(format!("{}/messages/batches", ANTHROPIC_API_BASE))
        .headers(headers)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::AnthropicError(format!("HTTP {}: {}", status, error_text)));
    }

    let batch_response: AnthropicBatchResponse = response
        .json()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;

    Ok(convert_batch_response(batch_response))
}

pub(crate) async fn get_batch(config: &AnthropicConfig, batch_id: &str) -> Result<BatchInfo> {
    let client = reqwest::Client::new();
    let headers = build_headers(&config.api_key)?;

    let response = client
        .get(format!("{}/messages/batches/{}", ANTHROPIC_API_BASE, batch_id))
        .headers(headers)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::AnthropicError(format!("HTTP {}: {}", status, error_text)));
    }

    let batch_response: AnthropicBatchResponse = response
        .json()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;

    Ok(convert_batch_response(batch_response))
}

pub(crate) async fn cancel_batch(config: &AnthropicConfig, batch_id: &str) -> Result<BatchInfo> {
    let client = reqwest::Client::new();
    let headers = build_headers(&config.api_key)?;

    let response = client
        .post(format!("{}/messages/batches/{}/cancel", ANTHROPIC_API_BASE, batch_id))
        .headers(headers)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::AnthropicError(format!("HTTP {}: {}", status, error_text)));
    }

    let batch_response: AnthropicBatchResponse = response
        .json()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;

    Ok(convert_batch_response(batch_response))
}

pub(crate) async fn list_batches(
    config: &AnthropicConfig,
    limit: Option<u32>,
) -> Result<Vec<BatchInfo>> {
    let client = reqwest::Client::new();
    let headers = build_headers(&config.api_key)?;

    let mut url = format!("{}/messages/batches", ANTHROPIC_API_BASE);
    if let Some(limit) = limit {
        url = format!("{}?limit={}", url, limit);
    }

    let response = client
        .get(&url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::AnthropicError(format!("HTTP {}: {}", status, error_text)));
    }

    let list_response: AnthropicListResponse = response
        .json()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;

    Ok(list_response.data.into_iter().map(convert_batch_response).collect())
}

pub(crate) async fn get_batch_results(
    config: &AnthropicConfig,
    batch_id: &str,
) -> Result<Vec<BatchResult>> {
    let client = reqwest::Client::new();
    let headers = build_headers(&config.api_key)?;

    // First get the batch to find the results URL
    let batch_response = client
        .get(format!("{}/messages/batches/{}", ANTHROPIC_API_BASE, batch_id))
        .headers(headers.clone())
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    if !batch_response.status().is_success() {
        let status = batch_response.status();
        let error_text = batch_response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::AnthropicError(format!("HTTP {}: {}", status, error_text)));
    }

    let batch: AnthropicBatchResponse = batch_response
        .json()
        .await
        .map_err(|e| Error::Parse(e.to_string()))?;

    let results_url = batch.results_url.ok_or_else(|| {
        Error::Other("Batch results not yet available".to_string())
    })?;

    // Fetch the JSONL results file
    let results_response = client
        .get(&results_url)
        .headers(headers)
        .send()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    if !results_response.status().is_success() {
        let status = results_response.status();
        let error_text = results_response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::AnthropicError(format!("HTTP {}: {}", status, error_text)));
    }

    let results_text = results_response
        .text()
        .await
        .map_err(|e| Error::Network(e.to_string()))?;

    // Parse JSONL
    let mut results = Vec::new();
    for line in results_text.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let result_line: AnthropicBatchResultLine = serde_json::from_str(line)
            .map_err(|e| Error::Parse(format!("Failed to parse result line: {}", e)))?;

        let batch_result = convert_result_line(result_line);
        results.push(batch_result);
    }

    Ok(results)
}

fn convert_result_line(line: AnthropicBatchResultLine) -> BatchResult {
    let result = if line.result.result_type == "succeeded" {
        if let Some(msg) = line.result.message {
            let mut content = String::new();
            let mut tool_uses = Vec::new();

            for block in msg.content {
                match block {
                    AnthropicContentBlock::Text { text } => {
                        content.push_str(&text);
                    }
                    AnthropicContentBlock::ToolUse { id, name, input } => {
                        tool_uses.push(ToolUse { id, name, input });
                    }
                }
            }

            let tool_uses_opt = if tool_uses.is_empty() { None } else { Some(tool_uses.clone()) };

            let finish_reason = match msg.stop_reason.as_deref() {
                Some("end_turn") => FinishReason::Stop,
                Some("max_tokens") => FinishReason::Length,
                Some("stop_sequence") => FinishReason::Stop,
                Some("tool_use") => FinishReason::ToolUse,
                _ => FinishReason::Other,
            };

            BatchResultType::Success(CompletionResponse {
                message: Message {
                    role: Role::Assistant,
                    content,
                    tool_uses: tool_uses_opt.clone(),
                    tool_call_id: None,
                },
                usage: Usage {
                    prompt_tokens: msg.usage.input_tokens,
                    completion_tokens: msg.usage.output_tokens,
                    total_tokens: msg.usage.input_tokens + msg.usage.output_tokens,
                },
                finish_reason,
                model: msg.model,
                tool_uses: tool_uses_opt,
                grounding_metadata: None,
            })
        } else {
            BatchResultType::Error(BatchItemError {
                error_type: "unknown".to_string(),
                message: "Success response missing message".to_string(),
            })
        }
    } else if let Some(err) = line.result.error {
        BatchResultType::Error(BatchItemError {
            error_type: err.error_type,
            message: err.message,
        })
    } else {
        BatchResultType::Error(BatchItemError {
            error_type: "unknown".to_string(),
            message: "Unknown error".to_string(),
        })
    };

    BatchResult {
        custom_id: line.custom_id,
        result,
    }
}
