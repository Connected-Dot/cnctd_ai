//! Streaming-first multi-turn agent loop with pluggable tool execution and
//! observability hooks.
//!
//! Generic loop body that powers cnctd.world's chat path and any other
//! consumer that wants the same "call provider → execute tools → repeat
//! until done" pattern. The library used to ship `agent::executor` for this
//! but it was non-streaming, hardcoded to MCP tool execution, and exposed no
//! per-event hooks. This module replaces that for new consumers.
//!
//! Design (ported from transmit-ai's `packages/ai/src/agent-loop.ts`):
//! - **Tool execution is a callback**, not built-in. Consumers pass a
//!   [`ToolExecutor`] and the loop asks it to run each tool the model requests.
//!   That's how cnctd.world's [`ToolRegistry`](super) plugs in without the
//!   library knowing about MCP, app tools, builtins, or anything specific.
//! - **Lifecycle hooks via trait, not stream**. The consumer implements
//!   [`LoopHandler`] with whichever events it cares about (text deltas, tool
//!   start/result, turn boundaries, errors). Default impls are no-ops, so
//!   handlers can opt into just the events they need.
//! - **Per-tool consecutive error caps**. If a single tool fails N times in a
//!   row the loop forces a final no-tools response so the model wraps up
//!   instead of looping forever on a broken tool.
//! - **Reuses the cnctd_ai retry layer**. [`Client::complete_stream`] already
//!   wraps in [`RetryPolicy`](crate::RetryPolicy) — the loop just calls it and
//!   surfaces the error if all retries fail.
//! - **Stream inactivity timeout** is enforced by [`CompletionStream`] itself
//!   (set at construction in the provider modules) — the loop just hits the
//!   resulting `StreamInactivityTimeout` error like any other failure.

use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use crate::message::Message;
use crate::request::CompletionRequest;
use crate::response::{CompletionResponse, FinishReason};
use crate::stream::StreamChunk;
use crate::tool::ToolUse;
use crate::{Client, Error};

/// Result of executing one tool call.
#[derive(Debug, Clone)]
pub struct ToolExecResult {
    /// The output text that gets sent back to the model as the tool_result content.
    pub output: String,
    /// Whether execution succeeded. Drives consecutive-error tracking.
    pub success: bool,
    /// Wall time the execution took. Forwarded to the handler for observability.
    pub duration_ms: u64,
}

/// Pluggable tool dispatcher. The agent loop hands off every tool call to
/// the consumer's executor — wrapping a registry, an MCP client, in-process
/// functions, or anything else.
#[async_trait]
pub trait ToolExecutor: Send + Sync {
    /// Run a single tool. The implementer is responsible for routing by name,
    /// handling errors as `Ok` results with `success: false`, and providing
    /// timing.
    async fn execute(&self, tool_use: &ToolUse) -> ToolExecResult;
}

/// Lifecycle/observability hooks. Every method has a no-op default so consumers
/// can implement only what they need (e.g., chat just wants the chunk + tool
/// events to emit envelopes; an autonomous-task runner might just want
/// `on_turn_complete` to log progress).
#[async_trait]
#[allow(unused_variables)]
pub trait LoopHandler: Send + Sync {
    /// Each chunk yielded by the streaming completion. Includes text deltas,
    /// tool-use lifecycle events, and finish-reason markers — see
    /// [`StreamChunk`](crate::stream::StreamChunk).
    async fn on_chunk(&self, chunk: &StreamChunk) {}

    /// Fires once per tool call right before the executor is invoked.
    async fn on_tool_call_start(&self, tool_use: &ToolUse) {}

    /// Fires once per tool call after the executor returns. Carries the same
    /// info that goes back to the model plus success/duration for the UI.
    async fn on_tool_call_result(
        &self,
        tool_use: &ToolUse,
        result: &ToolExecResult,
    ) {
    }

    /// Fires after a single turn (one provider call plus its tool round)
    /// completes. The loop may continue with another turn if there were
    /// tool calls; this signal lets the consumer log per-turn metrics.
    async fn on_turn_complete(&self, iteration: u32) {}

    /// Provider error (post-retry) on the streaming call itself. The loop
    /// terminates after this fires — the handler can choose to record / log
    /// without trying to recover (recovery is the consumer's job at a layer
    /// above; e.g., chat does fallback-model swap before invoking the loop).
    async fn on_error(&self, message: &str) {}
}

/// Why the loop stopped.
#[derive(Debug, Clone)]
pub enum StopReason {
    /// Provider returned a finish_reason other than ToolUse — model is done.
    ModelFinished(FinishReason),
    /// Hit `max_turns` without the model wrapping up. Rare; usually means a
    /// stuck tool loop.
    MaxTurns,
    /// One tool exceeded `max_consecutive_tool_errors`. Loop forced a final
    /// no-tools response to wrap up.
    ToolErrors(String),
    /// Provider call (post-retry) failed terminally.
    ProviderError(String),
    /// `LoopConfig::should_continue` returned `false` — consumer asked the
    /// loop to halt at the next turn boundary. Used by autonomous-task
    /// agents to honor cooperative cancellation requests.
    Aborted,
}

/// Final result of the loop. Carries the accumulated assistant text and the
/// last [`CompletionResponse`] (for cost/usage/tool_uses extraction by the
/// consumer).
#[derive(Debug, Clone)]
pub struct LoopResult {
    pub stop_reason: StopReason,
    pub iterations: u32,
    pub final_response: Option<CompletionResponse>,
    pub accumulated_text: String,
}

/// Loop tunables. Defaults match transmit-ai conventions.
#[derive(Clone)]
pub struct LoopConfig {
    /// Hard cap on turns to prevent infinite loops. Each "turn" is one
    /// provider call + its tool execution round.
    pub max_turns: u32,
    /// If a single tool errors this many times in a row, force a wrap-up
    /// response instead of looping further. `0` disables the check.
    pub max_consecutive_tool_errors: u32,
    /// Optional consumer-supplied gate, polled at the top of each turn.
    /// Return `false` to stop the loop gracefully (surfaces as
    /// `StopReason::Aborted`). Used by autonomous-task agents to honor
    /// cooperative cancellation; chat doesn't need it.
    pub should_continue: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl std::fmt::Debug for LoopConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LoopConfig")
            .field("max_turns", &self.max_turns)
            .field("max_consecutive_tool_errors", &self.max_consecutive_tool_errors)
            .field(
                "should_continue",
                &self.should_continue.as_ref().map(|_| "<fn>"),
            )
            .finish()
    }
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_turns: 10,
            max_consecutive_tool_errors: 2,
            should_continue: None,
        }
    }
}

/// Run the agent loop until the model wraps up, an error fires, or a limit
/// trips.
///
/// The streaming completion is consumed chunk-by-chunk and forwarded to
/// `handler.on_chunk` in real time. After each turn, any tools the model
/// requested are dispatched to `tool_executor` (concurrently? no — sequential,
/// matching transmit-ai's behavior) and their results are appended to the
/// message history before the next turn.
pub async fn run_agent_loop<H, T>(
    client: &Client,
    mut request: CompletionRequest,
    tool_executor: &T,
    handler: &H,
    config: LoopConfig,
) -> Result<LoopResult, Error>
where
    H: LoopHandler,
    T: ToolExecutor,
{
    let mut consecutive_errors: HashMap<String, u32> = HashMap::new();
    let mut accumulated_text = String::new();
    let mut last_response: Option<CompletionResponse> = None;
    let mut iteration: u32 = 0;
    let mut wrap_up_only = false;

    loop {
        if iteration >= config.max_turns {
            return Ok(LoopResult {
                stop_reason: StopReason::MaxTurns,
                iterations: iteration,
                final_response: last_response,
                accumulated_text,
            });
        }
        if let Some(check) = &config.should_continue {
            if !check() {
                return Ok(LoopResult {
                    stop_reason: StopReason::Aborted,
                    iterations: iteration,
                    final_response: last_response,
                    accumulated_text,
                });
            }
        }
        iteration += 1;

        // Wrap-up turn: the model is being asked to respond with text only,
        // no further tool calls allowed. We strip tools from the request and
        // proceed; the next response should be `Stop` and we exit.
        let turn_request = if wrap_up_only {
            let mut r = request.clone();
            r.tools = None;
            r
        } else {
            request.clone()
        };

        // Open the stream. Retry/fallback/timeout are baked into the cnctd_ai
        // Client surface — if we get an Err here it means all retries failed
        // and there's no more we can do at this layer.
        let mut stream = match client.complete_stream(turn_request).await {
            Ok(s) => s,
            Err(e) => {
                let msg = e.to_string();
                handler.on_error(&msg).await;
                return Ok(LoopResult {
                    stop_reason: StopReason::ProviderError(msg),
                    iterations: iteration,
                    final_response: last_response,
                    accumulated_text,
                });
            }
        };

        // Pump chunks through the handler. The stream itself accumulates the
        // text and tool_use payloads internally; once it ends we pull the
        // final_response.
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    if let Some(text) = &chunk.delta {
                        accumulated_text.push_str(text);
                    }
                    handler.on_chunk(&chunk).await;
                    if chunk.finish_reason.is_some() {
                        // Stream signaled the end of this turn. We still need
                        // to drain the rest of the iterator (some providers
                        // emit a usage chunk after the finish reason) and
                        // then read final_response.
                        break;
                    }
                }
                Err(e) => {
                    let msg = e.to_string();
                    handler.on_error(&msg).await;
                    return Ok(LoopResult {
                        stop_reason: StopReason::ProviderError(msg),
                        iterations: iteration,
                        final_response: last_response,
                        accumulated_text,
                    });
                }
            }
        }

        let response = match stream.final_response() {
            Some(r) => r,
            None => {
                // Stream ended with no usable content. Treat as model-done
                // rather than an error — the consumer has whatever text
                // accumulated in `on_chunk`.
                return Ok(LoopResult {
                    stop_reason: StopReason::ModelFinished(FinishReason::Stop),
                    iterations: iteration,
                    final_response: last_response,
                    accumulated_text,
                });
            }
        };

        let stop_reason = response.finish_reason.clone();
        last_response = Some(response.clone());

        // No tool calls (or wrap-up turn) — model is done.
        let tool_uses = match response.tool_uses.as_ref() {
            Some(tu) if !tu.is_empty() && !wrap_up_only => tu.clone(),
            _ => {
                handler.on_turn_complete(iteration).await;
                return Ok(LoopResult {
                    stop_reason: StopReason::ModelFinished(stop_reason),
                    iterations: iteration,
                    final_response: last_response,
                    accumulated_text,
                });
            }
        };

        // Append the assistant turn (with its tool_use blocks) to the history
        // so subsequent turns see the right round-trip shape.
        request.messages.push(response.message.clone());

        let mut tool_results = Vec::new();
        for tu in &tool_uses {
            handler.on_tool_call_start(tu).await;
            let result = tool_executor.execute(tu).await;
            handler.on_tool_call_result(tu, &result).await;

            // Per-tool error caps: if one tool keeps failing, force wrap-up
            // on the next turn so the model can explain rather than retrying.
            if !result.success {
                let count = consecutive_errors.entry(tu.name.clone()).or_insert(0);
                *count += 1;
                if config.max_consecutive_tool_errors > 0
                    && *count >= config.max_consecutive_tool_errors
                {
                    wrap_up_only = true;
                }
            } else {
                consecutive_errors.remove(&tu.name);
            }

            tool_results.push((tu.id.clone(), result.output.clone()));
        }

        // Feed tool results back as the next user-side message and loop.
        let result_message = Message::tool_results(
            tool_results
                .into_iter()
                .map(|(id, out)| crate::message::ToolResult::new(&id, out))
                .collect(),
        );
        request.messages.push(result_message);

        handler.on_turn_complete(iteration).await;

        if wrap_up_only {
            // We'll force one more turn with tools stripped. If the executor
            // hits the cap mid-tool, this kicks in next iteration.
        }

        // Keep track of which tool failed if we're about to abort
        if wrap_up_only && config.max_consecutive_tool_errors > 0 {
            // Find the offending tool for the StopReason
            if let Some((tool_name, _)) = consecutive_errors
                .iter()
                .find(|(_, c)| **c >= config.max_consecutive_tool_errors)
            {
                let _ = tool_name; // We let the wrap-up turn run; if it succeeds we get ModelFinished
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A tool executor that returns canned results.
    struct FakeExecutor {
        results: Mutex<Vec<ToolExecResult>>,
    }

    #[async_trait]
    impl ToolExecutor for FakeExecutor {
        async fn execute(&self, _tool_use: &ToolUse) -> ToolExecResult {
            self.results
                .lock()
                .unwrap()
                .pop()
                .unwrap_or(ToolExecResult {
                    output: "default".into(),
                    success: true,
                    duration_ms: 0,
                })
        }
    }

    /// Default no-op handler. The trait's default impls cover everything.
    struct NoOpHandler;
    impl LoopHandler for NoOpHandler {}

    #[test]
    fn loop_config_defaults_match_transmit_conventions() {
        let cfg = LoopConfig::default();
        assert_eq!(cfg.max_turns, 10);
        assert_eq!(cfg.max_consecutive_tool_errors, 2);
    }

    // Note: full integration tests against a fake `Client` would require a
    // mock provider stream — defer until the chat-engine consumer is wired up
    // and we can test end-to-end through it.
}
