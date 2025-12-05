//! OpenAI Batch API implementation
//!
//! OpenAI's batch API uses a file-based workflow:
//! 1. Create JSONL file with requests
//! 2. Upload file with purpose "batch"
//! 3. Create batch referencing the file
//! 4. Poll for completion
//! 5. Download results from output file

use async_openai::config::OpenAIConfig;
use async_openai::types::{
    Batch as OpenAiBatch,
    BatchCompletionWindow,
    BatchEndpoint,
    BatchRequest,
    BatchRequestInput,
    BatchRequestInputMethod,
    BatchRequestOutput,
    BatchStatus as OpenAiBatchStatus,
    CreateFileRequest,
    FileInput,
    FilePurpose,
    InputSource,
};
use serde::Serialize;

use super::types::{
    BatchCounts,
    BatchInfo,
    BatchItem,
    BatchItemError,
    BatchResult,
    BatchResultType,
    BatchStatus,
};
use crate::client::config::OpenAiConfig;
use crate::error::{Error, Result};
use crate::message::Role;

/// Query parameters for listing batches
#[derive(Debug, Serialize, Default)]
struct ListBatchesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    after: Option<String>,
}

/// Create a batch of completion requests.
///
/// This implementation:
/// 1. Converts BatchItems to OpenAI's JSONL format
/// 2. Uploads the JSONL as a file
/// 3. Creates a batch referencing that file
pub async fn create_batch(
    sdk_client: &async_openai::Client<OpenAIConfig>,
    _config: &OpenAiConfig,
    items: Vec<BatchItem>,
) -> Result<BatchInfo> {
    if items.is_empty() {
        return Err(Error::InvalidRequest("Batch items cannot be empty".to_string()));
    }

    // Convert items to JSONL format
    let jsonl_content = items_to_jsonl(&items)?;

    // Upload the JSONL file
    let file = sdk_client
        .files()
        .create(CreateFileRequest {
            file: FileInput {
                source: InputSource::VecU8 {
                    filename: "batch_input.jsonl".to_string(),
                    vec: jsonl_content.into_bytes(),
                },
            },
            purpose: FilePurpose::Batch,
            expires_after: None,
        })
        .await
        .map_err(Error::OpenAiError)?;

    // Create the batch
    let batch = sdk_client
        .batches()
        .create(BatchRequest {
            input_file_id: file.id,
            endpoint: BatchEndpoint::V1ChatCompletions,
            completion_window: BatchCompletionWindow::W24H,
            metadata: None,
        })
        .await
        .map_err(Error::OpenAiError)?;

    Ok(openai_batch_to_info(batch))
}

/// Get the current status of a batch.
pub async fn get_batch(
    sdk_client: &async_openai::Client<OpenAIConfig>,
    batch_id: &str,
) -> Result<BatchInfo> {
    let batch = sdk_client
        .batches()
        .retrieve(batch_id)
        .await
        .map_err(Error::OpenAiError)?;

    Ok(openai_batch_to_info(batch))
}

/// Cancel a batch.
pub async fn cancel_batch(
    sdk_client: &async_openai::Client<OpenAIConfig>,
    batch_id: &str,
) -> Result<BatchInfo> {
    let batch = sdk_client
        .batches()
        .cancel(batch_id)
        .await
        .map_err(Error::OpenAiError)?;

    Ok(openai_batch_to_info(batch))
}

/// List batches.
pub async fn list_batches(
    sdk_client: &async_openai::Client<OpenAIConfig>,
    limit: Option<u32>,
) -> Result<Vec<BatchInfo>> {
    let query = ListBatchesQuery {
        limit,
        after: None,
    };

    let response = sdk_client
        .batches()
        .list(&query)
        .await
        .map_err(Error::OpenAiError)?;

    Ok(response.data.into_iter().map(openai_batch_to_info).collect())
}

/// Get results from a completed batch.
pub async fn get_batch_results(
    sdk_client: &async_openai::Client<OpenAIConfig>,
    batch_id: &str,
) -> Result<Vec<BatchResult>> {
    // First get the batch to find the output file
    let batch = sdk_client
        .batches()
        .retrieve(batch_id)
        .await
        .map_err(Error::OpenAiError)?;

    let output_file_id = batch.output_file_id.ok_or_else(|| {
        Error::InvalidRequest("Batch has no output file (may not be complete)".to_string())
    })?;

    // Download the output file content
    let file_content = sdk_client
        .files()
        .content(&output_file_id)
        .await
        .map_err(Error::OpenAiError)?;

    // Parse the JSONL output
    let content_str = String::from_utf8(file_content.to_vec())
        .map_err(|e| Error::Parse(format!("Invalid UTF-8 in output file: {}", e)))?;

    parse_results_jsonl(&content_str)
}

// ============================================================================
// Helper functions
// ============================================================================

/// Convert Role enum to OpenAI role string
fn role_to_string(role: &Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
    }
}

/// Convert BatchItems to OpenAI's JSONL format for batch input.
fn items_to_jsonl(items: &[BatchItem]) -> Result<String> {
    let lines: Result<Vec<String>> = items
        .iter()
        .map(|item| {
            let input = BatchRequestInput {
                custom_id: item.custom_id.clone(),
                method: BatchRequestInputMethod::POST,
                url: BatchEndpoint::V1ChatCompletions,
                body: Some(request_to_openai_body(item)?),
            };
            serde_json::to_string(&input)
                .map_err(|e| Error::Parse(format!("Failed to serialize batch item: {}", e)))
        })
        .collect();

    Ok(lines?.join("\n"))
}

/// Convert our BatchItem to OpenAI's chat completion body format.
fn request_to_openai_body(item: &BatchItem) -> Result<serde_json::Value> {
    let request = &item.request;
    
    // Build messages array, separating system messages
    let mut messages: Vec<serde_json::Value> = Vec::new();
    
    for msg in &request.messages {
        // Check if this is a system message
        if matches!(msg.role, Role::System) {
            // Prepend system messages
            messages.insert(0, serde_json::json!({
                "role": role_to_string(&msg.role),
                "content": &msg.content
            }));
        } else {
            messages.push(serde_json::json!({
                "role": role_to_string(&msg.role),
                "content": &msg.content
            }));
        }
    }
    
    let mut body = serde_json::json!({
        "model": &item.model,
        "messages": messages
    });

    if let Some(opts) = &request.options {
        if let Some(max_tokens) = opts.max_tokens {
            body["max_tokens"] = serde_json::json!(max_tokens);
        }
        if let Some(temp) = opts.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = opts.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if let Some(ref stop) = opts.stop_sequences {
            body["stop"] = serde_json::json!(stop);
        }
    }

    Ok(body)
}

/// Parse JSONL results from OpenAI batch output.
fn parse_results_jsonl(content: &str) -> Result<Vec<BatchResult>> {
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let output: BatchRequestOutput = serde_json::from_str(line)
                .map_err(|e| Error::Parse(format!("Failed to parse result line: {}", e)))?;

            Ok(BatchResult {
                custom_id: output.custom_id,
                result: if let Some(response) = output.response {
                    if response.status_code == 200 {
                        // Parse the completion response from body
                        match parse_completion_from_body(&response.body) {
                            Ok(completion) => BatchResultType::Success(completion),
                            Err(e) => BatchResultType::Error(BatchItemError {
                                error_type: "parse_error".to_string(),
                                message: format!("Failed to parse response body: {}", e),
                            }),
                        }
                    } else {
                        BatchResultType::Error(BatchItemError {
                            error_type: format!("http_{}", response.status_code),
                            message: response.body.to_string(),
                        })
                    }
                } else if let Some(error) = output.error {
                    BatchResultType::Error(BatchItemError {
                        error_type: error.code,
                        message: error.message,
                    })
                } else {
                    BatchResultType::Error(BatchItemError {
                        error_type: "unknown".to_string(),
                        message: "No response or error in result".to_string(),
                    })
                },
            })
        })
        .collect()
}

/// Parse a CompletionResponse from OpenAI's response body JSON.
fn parse_completion_from_body(body: &serde_json::Value) -> Result<crate::response::CompletionResponse> {
    // OpenAI chat completion response format
    let content = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("")
        .to_string();

    let finish_reason = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
        .map(|r| match r {
            "stop" => crate::response::FinishReason::Stop,
            "length" => crate::response::FinishReason::Length,
            "tool_calls" => crate::response::FinishReason::ToolUse,
            _ => crate::response::FinishReason::Stop,
        })
        .unwrap_or(crate::response::FinishReason::Stop);

    let prompt_tokens = body.get("usage")
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    let completion_tokens = body.get("usage")
        .and_then(|u| u.get("completion_tokens"))
        .and_then(|t| t.as_u64())
        .unwrap_or(0) as u32;
    let usage = crate::response::Usage {
        prompt_tokens,
        completion_tokens,
        total_tokens: prompt_tokens + completion_tokens,
    };

    // Parse tool calls if present
    let tool_uses = body
        .get("choices")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("message"))
        .and_then(|m| m.get("tool_calls"))
        .and_then(|tc| tc.as_array())
        .map(|calls| {
            calls
                .iter()
                .filter_map(|call| {
                    let id = call.get("id")?.as_str()?;
                    let function = call.get("function")?;
                    let name = function.get("name")?.as_str()?;
                    let arguments = function.get("arguments")?.as_str()?;

                    Some(crate::ToolUse {
                        id: id.to_string(),
                        name: name.to_string(),
                        input: serde_json::from_str(arguments).ok()?,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let tool_uses_opt = if tool_uses.is_empty() { None } else { Some(tool_uses.clone()) };
    
    Ok(crate::response::CompletionResponse {
        message: crate::message::Message {
            role: crate::message::Role::Assistant,
            content,
            tool_uses: tool_uses_opt.clone(),
            tool_call_id: None,
        },
        usage,
        finish_reason,
        model: body
            .get("model")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown")
            .to_string(),
        tool_uses: tool_uses_opt,
    })
}

/// Convert OpenAI Batch to our BatchInfo type.
fn openai_batch_to_info(batch: OpenAiBatch) -> BatchInfo {
    BatchInfo {
        id: batch.id,
        status: match batch.status {
            OpenAiBatchStatus::Validating => BatchStatus::Validating,
            OpenAiBatchStatus::InProgress => BatchStatus::InProgress,
            OpenAiBatchStatus::Finalizing => BatchStatus::Finalizing,
            OpenAiBatchStatus::Completed => BatchStatus::Completed,
            OpenAiBatchStatus::Failed => BatchStatus::Failed,
            OpenAiBatchStatus::Expired => BatchStatus::Expired,
            OpenAiBatchStatus::Cancelling => BatchStatus::Cancelling,
            OpenAiBatchStatus::Cancelled => BatchStatus::Cancelled,
        },
        counts: batch.request_counts.map(|c| BatchCounts {
            total: c.total,
            completed: c.completed,
            failed: c.failed,
        }),
        created_at: Some(batch.created_at as i64),
        expires_at: batch.expires_at.map(|t| t as i64),
        completed_at: batch.completed_at.map(|t| t as i64),
        error_message: batch.errors.and_then(|e| {
            e.data.first().map(|err| format!("{}: {}", err.code, err.message))
        }),
    }
}
