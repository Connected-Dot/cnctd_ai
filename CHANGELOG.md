# Changelog

All notable changes to this project will be documented in this file.

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
