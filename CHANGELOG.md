# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2026-10-06

Prompt caching that works end to end. Breaking: `Usage::prompt_tokens` now
counts cached input on Anthropic, and public structs gain fields.

### Added
- **`RequestOptions::prompt_cache`** ([`PromptCache`]): a breakpoint on the
  last tool definition (`tools`), one on the last block of the last message
  (`tail`, which moves with each round of a tool loop), and an OpenAI
  `prompt_cache_key` (`key`).
- **1-hour cache entries**: `CacheControl::Extended` is now sent as
  `{"type":"ephemeral","ttl":"1h"}` (it was sent as a 5-minute entry).
- **`Usage::cache_creation_1h_tokens`**: the part of the cache writes billed
  at the 1-hour rate.
- `examples/live_cache_check.rs`: a two-turn chat with tool rounds on
  Anthropic (explicit breakpoints) plus OpenAI and Gemini (automatic caching),
  reporting reads and writes per call.

### Changed
- **`Usage::prompt_tokens` is every input token, cached or not**, on all
  providers. Anthropic reports `input_tokens` without the cached parts; it is
  now added up (OpenAI and Gemini already included them).
  `effective_prompt_tokens()` is the uncached remainder.
- Anthropic: every system message becomes its own system block carrying its
  own breakpoint (a single unmarked system message is still a plain string).
  A message's `cache_control` lands on its last block that can carry one
  (tool results and replayed assistant turns included; never thinking blocks
  or empty text). The request is normalized to the API's rules: at most four
  breakpoints (the oldest message breakpoints go first, the last one always
  stays) and no 1-hour entry after a 5-minute one.

### Fixed
- OpenAI cached input (`input_tokens_details.cached_tokens`) is now reported
  as `cache_read_tokens` from Responses calls, streamed or not.
- Streamed Anthropic usage keeps the cache figures from `message_start` when
  `message_delta` brings the final totals.

## [0.2.0] - 2026-10-06

Model-aware requests for the 2026 model generations (Claude Opus 5.5 /
Sonnet 5.5 / Fable 5.1, GPT-6 / GPT-5.6, Gemini 3.5-3.8). Breaking: new
fields on public structs and a new `FinishReason` variant.

### Added
- **Thinking and effort controls**: `RequestOptions::thinking` ([`Thinking`]:
  `Adaptive`, `Budget`, `BetweenTools`, `Disabled`) and `RequestOptions::effort`
  ([`Effort`]: `none`..`max`). Anthropic sends `thinking` + `output_config.effort`,
  OpenAI `reasoning.effort` (including `max`, which async-openai 0.33 lacks),
  Gemini `thinkingConfig` (`thinkingLevel` from effort, `thinkingBudget` from
  `Budget`/`Disabled`). `ThinkingLevel` gains `Minimal` and `Medium`.
- **Exact replay of reasoning**: `Message::provider_content` ([`ProviderContent`])
  carries the provider's raw assistant blocks. Anthropic thinking /
  redacted_thinking blocks (with signatures) and Gemini parts (with
  thoughtSignature) are captured from streaming and non-streaming responses
  and replayed unchanged to the same provider. Required inside a tool loop.
- **Refusals**: `FinishReason::Refusal` and `CompletionResponse::refusal`
  ([`Refusal`]: category + explanation) from Anthropic `stop_reason: refusal`
  + `stop_details`, Gemini safety finish reasons and blocked prompts
  (`promptFeedback.blockReason`), OpenAI refusal content and
  `content_filter` incompletes. A refused stream still yields a final response.
- **`LoopResult::messages`** (every message the loop added, in order) and
  **`LoopResult::usage`** (summed across all rounds).
- **Transcription**: `gemini-3.5-transcribe` (Interactions API, diarization +
  word timestamps grouped into speaker segments; now the Gemini default) and
  `gpt-transcribe` (multipart, `languages[]`, detected language).
- Gemini `media_resolution` and `stop_sequences` are now sent; Anthropic
  `stop_sequences` too.
- `examples/live_model_check.rs`: live tool-loop / replay / transcription check
  across all three providers.

### Fixed
- **Agent loop tool results** now carry the function name (Gemini matched
  results to "function") and the OpenAI `call_id`, and set `is_error` on
  failed tools.
- **OpenAI streamed tool calls** had an empty name (the arguments-done event
  omits it), which the API rejected on replay. OpenAI tool loops work again.
- **Gemini function calls** keep Gemini's call `id` and send it back on the
  `functionResponse`; Gemini usage now counts thinking tokens as output.
- **Anthropic stream**: the tool "complete" event fired again on every later
  block stop (duplicate events with interleaved thinking); mid-stream `error`
  events now surface as errors; cache read/write tokens are reported.
- OpenAI reasoning-summary deltas no longer leak into the answer text.
- Reasoning-model detection covers `gpt-6*` and `o4*`.

### Changed
- Anthropic non-streaming completions use the same raw-HTTP request builder
  as streaming; the `anthropic-sdk-rust` dependency is removed. The obsolete
  `prompt-caching-2024-07-31` beta header is no longer sent (caching is GA).
- Empty assistant turns are dropped from Anthropic and Gemini requests.

## [0.1.20] - 2026-02-03

### Added
- **Anthropic Citations API Support**: New `Citation` type and `citations` field in `CompletionResponse`
  - `CitationConfig` in `RequestOptions` to enable citations
  - `CompletionRequest::with_citations()` builder method
  - Helper methods: `has_citations()`, `get_citations()`
- **Extended Cache Duration**: Added `CacheControl::Extended` for 1-hour cache TTL (Anthropic)
  - `Message::with_extended_cache()` builder method
- **OpenAI Reasoning Summaries**: Added `reasoning_summary` field in `CompletionResponse`
  - Helper method: `get_reasoning_summary()`
- **Gemini 3 Thinking Level**: New `ThinkingLevel` enum (`Low`, `High`) for controlling reasoning depth
  - `CompletionRequest::with_thinking_level()` builder method
- **Gemini 3 Media Resolution**: New `MediaResolution` enum (`Low`, `Medium`, `High`, `UltraHigh`)
  - `CompletionRequest::with_media_resolution()` builder method
- **OpenAI Native MCP Support**: New `McpServerConfig` and `McpApprovalMode` types
  - `CompletionRequest::with_mcp_server()` builder method
- **OpenAI Built-in Tools**: Added `OpenAiCodeInterpreter`, `OpenAiWebSearch`, `OpenAiImageGeneration` to `BuiltInTool`
  - Builder methods: `with_openai_code_interpreter()`, `with_openai_web_search()`, `with_openai_image_generation()`
- Optional `tracing` feature for configurable logging

### Changed
- Removed all debug `eprintln!` statements from production code
- Bumped version to 0.1.19

### Fixed
- Cleaned up streaming code in `stream.rs` and `client/openai_responses.rs`

## [Unreleased]

### Added
- **Agent Framework**: Complete autonomous task execution system
  - `Agent` struct with builder pattern for easy configuration
  - `AgentConfig` for controlling behavior (iterations, timeouts, retries)
  - `AgentExecutor` handles the autonomous tool calling loop
  - `AgentTrace` provides comprehensive execution history
  - `AgentState` tracks current execution state
  - Automatic MCP tool discovery and execution
  - Configurable error handling and retry logic
  - Detailed event tracing for debugging and analysis
  - Support for custom system prompts
  - Result truncation to manage context limits
- Added `agent` module with full framework implementation
- Added `agent_basic.rs` example demonstrating agent usage
- Added `agent_simple.rs` example showing minimal setup
- Added comprehensive agent framework documentation in `docs/AGENT_FRAMEWORK.md`
- Re-exported agent types in main library: `Agent`, `AgentConfig`, `AgentTrace`, etc.

### Changed
- Updated `lib.rs` to export new agent module

## [0.1.5] - 2025-11-20

### Fixed
- Fixed tool calling examples to work with `rmcp::model::Tool` type
- Fixed error handling example to properly construct Tool schemas
- Fixed streaming tool calling example to use correct Tool structure
- Added all required rmcp Tool fields (annotations, icons, meta, cache_policy, self_ref)
- All examples now compile and run correctly

### Added
- Added `tool_helpers` module with `create_tool()` and `create_tool_borrowed()` helper functions
- Helper functions handle all required Tool fields automatically
- Added comprehensive README with usage examples
- Added CHANGELOG to track changes
- Added MIGRATION guide for users upgrading
- Added FIXES_SUMMARY documenting all changes
- Added complete MCP gateway agent example

### Changed
- Simplified tool creation with helper functions instead of manual Arc/Cow construction
- Examples now use cleaner, more ergonomic API for tool creation
- Updated all tool-related examples to use the new helper functions
- Error handling example now uses helper functions instead of manual Tool construction

## [0.1.4] - Previous Version

### Changed
- Migrated from custom Tool struct to `rmcp::model::Tool`
- This breaking change required updates to all tool-related code
