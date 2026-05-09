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
    /// The active model's context window in tokens. When set together with
    /// `default_tool_result_max_chars`, the loop truncates tool outputs
    /// progressively as the prompt fills up — the goal is to keep multi-turn
    /// runs from blowing through the context limit when tools return long
    /// outputs. `None` disables the scaling (tools go through full-size).
    pub context_window: Option<u32>,
    /// Per-tool output cap that's used when context utilization is below 50%.
    /// Above 50%, the cap scales down toward 5K linearly through 70%, then
    /// hits a hard 5K floor. Default 50K.
    pub default_tool_result_max_chars: usize,
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
            .field("context_window", &self.context_window)
            .field("default_tool_result_max_chars", &self.default_tool_result_max_chars)
            .finish()
    }
}

impl Default for LoopConfig {
    fn default() -> Self {
        Self {
            max_turns: 10,
            max_consecutive_tool_errors: 2,
            should_continue: None,
            context_window: None,
            default_tool_result_max_chars: 50_000,
        }
    }
}

/// Hard floor for tool-output truncation when the prompt is past 70%
/// utilization. Tool outputs are clipped to this no matter what
/// `default_tool_result_max_chars` is set to.
const TRUNCATION_HARD_FLOOR: usize = 5_000;

/// Compute the per-tool output character cap given the current prompt-token
/// utilization (input_tokens / context_window) and the configured base.
///
/// - `utilization < 0.50`: use `base_max` as-is (no scaling)
/// - `0.50 ≤ utilization < 0.70`: linear scale from `base_max` at 0.50 down
///   to `0.25 * base_max` at 0.70
/// - `utilization ≥ 0.70`: hard floor at 5K (or `base_max` if smaller)
fn scale_truncation_limit(utilization: f64, base_max: usize) -> usize {
    let floor = TRUNCATION_HARD_FLOOR.min(base_max);
    if utilization >= 0.70 {
        return floor;
    }
    if utilization < 0.50 {
        return base_max;
    }
    let ratio = (utilization - 0.50) / 0.20; // 0.0 at 0.50, 1.0 at 0.70
    let multiplier = 1.0 - (0.75 * ratio); // 1.0 at 0.50, 0.25 at 0.70
    let scaled = (base_max as f64 * multiplier) as usize;
    scaled.max(floor)
}

/// Truncate a tool output to `max_chars`, appending a [truncated] notice if
/// it was over the limit. No-op when the output already fits.
fn truncate_tool_output(output: String, max_chars: usize) -> String {
    if output.len() <= max_chars {
        return output;
    }
    let original_len = output.len();
    // Char-boundary safe truncation
    let mut end = max_chars;
    while end > 0 && !output.is_char_boundary(end) {
        end -= 1;
    }
    let mut truncated = output[..end].to_string();
    truncated.push_str(&format!(
        "\n\n[truncated: original output was {} chars, kept first {}]",
        original_len, end
    ));
    truncated
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
    // Tracked across turns to compute context utilization for tool-output
    // truncation (B6 dynamic scaling). The provider's prompt_tokens for the
    // *previous* turn estimates the size of the prompt going into the *next*
    // turn (history grows monotonically); we use that to scale tool output
    // limits before re-feeding tool_result messages to the model.
    let mut last_prompt_tokens: Option<u32> = None;

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
        last_prompt_tokens = Some(response.usage.prompt_tokens);
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

        // B6 dynamic truncation: as the prompt fills up the model's window,
        // shrink per-tool output caps so the next turn doesn't blow through
        // the limit. Computed once per round from the response we just got.
        let max_chars = match (last_prompt_tokens, config.context_window) {
            (Some(used), Some(window)) if window > 0 => {
                let utilization = used as f64 / window as f64;
                scale_truncation_limit(utilization, config.default_tool_result_max_chars)
            }
            _ => config.default_tool_result_max_chars,
        };

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

            let truncated = truncate_tool_output(result.output.clone(), max_chars);
            tool_results.push((tu.id.clone(), truncated));
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
        assert!(cfg.context_window.is_none());
        assert_eq!(cfg.default_tool_result_max_chars, 50_000);
    }

    #[test]
    fn truncation_no_op_below_50_pct() {
        // 0%, 25%, 49% utilization → full base_max
        for u in [0.0, 0.25, 0.49] {
            assert_eq!(scale_truncation_limit(u, 50_000), 50_000);
        }
    }

    #[test]
    fn truncation_scales_linearly_50_to_70_pct() {
        // At 50%: full base. At 60%: midpoint between 100% and 25%, so 62.5%.
        // At 70%: 25% of base.
        assert_eq!(scale_truncation_limit(0.50, 50_000), 50_000);
        let mid = scale_truncation_limit(0.60, 50_000);
        // 50_000 * (1.0 - 0.75 * 0.5) = 50_000 * 0.625 = 31_250
        assert_eq!(mid, 31_250);
        let near_high = scale_truncation_limit(0.69, 50_000);
        assert!(near_high > 12_500 && near_high < 16_000);
    }

    #[test]
    fn truncation_hard_floor_above_70_pct() {
        for u in [0.70, 0.80, 0.95, 1.0] {
            assert_eq!(
                scale_truncation_limit(u, 50_000),
                TRUNCATION_HARD_FLOOR
            );
        }
    }

    #[test]
    fn truncation_floor_capped_by_base_max() {
        // If base_max is smaller than the hard floor, we don't expand to floor.
        assert_eq!(scale_truncation_limit(0.95, 1_000), 1_000);
    }

    #[test]
    fn truncate_tool_output_no_op_when_short() {
        let s = truncate_tool_output("hello".to_string(), 100);
        assert_eq!(s, "hello");
    }

    #[test]
    fn truncate_tool_output_clips_and_annotates() {
        let s = truncate_tool_output("0123456789abcdef".to_string(), 8);
        assert!(s.starts_with("01234567"));
        assert!(s.contains("[truncated"));
        assert!(s.contains("16 chars"));
    }

    #[test]
    fn truncate_tool_output_respects_char_boundaries() {
        // Multi-byte char at boundary
        let s = truncate_tool_output("aaaa🔥bbbb".to_string(), 5);
        // The fire emoji is 4 bytes; truncating at 5 should fall back to 4.
        assert!(s.starts_with("aaaa"));
        assert!(s.contains("[truncated"));
    }

    // Note: full integration tests against a fake `Client` would require a
    // mock provider stream — defer until the chat-engine consumer is wired up
    // and we can test end-to-end through it.
}
