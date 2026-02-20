# CLAUDE.md - cnctd_ai Library Reference

> This file provides context about the cnctd_ai library for Claude to reference in conversations.

## Project Summary

**cnctd_ai** is a Rust abstraction layer for AI/LLM providers with integrated MCP (Model Context Protocol) support. It is the core AI library used by cnctd.world and other Connected Dot projects.

## Key Features

- **Multi-Provider Support**: Unified interface for Anthropic Claude, OpenAI, Google Gemini, OpenRouter
- **Streaming & Non-Streaming**: Both completion modes supported
- **Tool Calling**: Full function/tool calling support across all providers
- **Agent Framework**: Autonomous task execution with tool calling loops
- **MCP Integration**: Native support for MCP servers (stdio and HTTP gateway)
- **Strong Typing**: Comprehensive error types with provider-specific handling

## Architecture

### Core Types

```rust
// Client - unified interface for all providers
pub struct Client { ... }

// Request/Response
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Tool>>,
    pub options: Option<CompletionOptions>,
}

pub struct CompletionResponse {
    pub content: Vec<Content>,
    pub model: String,
    pub usage: Option<Usage>,
    pub stop_reason: Option<StopReason>,
    // ... provider-specific fields
}

// Messages
pub struct Message {
    pub role: Role,
    pub content: Vec<Content>,
}

pub enum Content {
    Text { text: String },
    ToolUse { id: String, name: String, input: Value },
    ToolResult { tool_use_id: String, content: String, is_error: bool },
    // ... image, document types
}

// Tool definitions
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}
```

### Provider Configs

```rust
pub struct AnthropicConfig {
    pub api_key: String,
    pub model: String,
    pub version: Option<String>,
}

pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
    pub base_url: Option<String>,  // For OpenRouter/custom endpoints
}

pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,
}
```

### MCP Gateway

```rust
pub struct McpGateway {
    url: String,
    auth_token: Option<String>,
}

impl McpGateway {
    pub async fn list_servers(&self) -> Result<Vec<McpServerInfo>>;
    pub async fn list_tools(&self, server: &str) -> Result<Vec<Tool>>;
    pub async fn call_tool(&self, server: &str, tool: &str, args: Option<Value>) -> Result<Value>;
}
```

## Usage by cnctd.world

cnctd.world imports cnctd_ai and uses it for:

1. **Building AI clients** - Factory pattern creates provider-specific clients
2. **Chat completions** - Both streaming and non-streaming
3. **Tool execution** - Via MCP gateway integration
4. **Embeddings** - For memory/vector storage
5. **Cost tracking** - Token usage for billing

### Integration Code Locations

```
cnctd.world/server/src/
├── modules/
│   ├── inference/
│   │   └── client.rs        # Builds cnctd_ai::Client from model config
│   ├── conversation/
│   │   ├── mod.rs           # Uses CompletionRequest/Response
│   │   └── tool_executor.rs # Uses ToolResult, McpGateway
│   └── memory/
│       └── synthesis.rs     # Uses embeddings
└── router/
    └── resources/
        └── mcp.rs           # Uses McpGateway, McpServerInfo
```

## Project Structure

```
cnctd_ai/
├── Cargo.toml              # [package] cnctd_ai library
├── src/
│   ├── lib.rs              # Public exports
│   ├── client.rs           # Unified Client struct
│   ├── types/              # Core types (Message, Content, Tool, etc.)
│   ├── providers/
│   │   ├── anthropic.rs    # Anthropic Claude implementation
│   │   ├── openai.rs       # OpenAI implementation
│   │   └── gemini.rs       # Google Gemini implementation
│   ├── mcp/
│   │   ├── gateway.rs      # HTTP gateway client
│   │   └── client.rs       # Stdio MCP client
│   ├── agent/              # Agent framework
│   └── error.rs            # Error types
├── crates/
│   └── cnctd_ai_server/    # AI orchestration server
│       ├── Cargo.toml
│       ├── docs/
│       │   └── OBFUSCATION_SETUP.md  # Integration guide for calling apps
│       └── src/
│           ├── main.rs
│           ├── config.rs
│           ├── state.rs
│           ├── error.rs
│           ├── routes/
│           │   ├── chat.rs          # SSE streaming chat with tool loops
│           │   ├── agents.rs        # Agent execution endpoints
│           │   ├── models.rs        # Model listing
│           │   ├── health.rs        # Health check
│           │   └── obfuscation.rs   # Cache invalidation endpoint
│           └── obfuscation/
│               ├── source.rs        # HTTP source types + fetch
│               ├── entity_dictionary.rs  # String-keyed entity lookup
│               ├── tokenizer.rs     # HMAC tokenization + Aho-Corasick
│               ├── obfuscator.rs    # KeyInferenceEngine + Obfuscator
│               ├── numeric_scaler.rs # Per-metric scaling
│               └── session.rs       # Per-salt session cache
├── examples/               # Usage examples
├── docs/                   # Session summaries, post-mortems, design docs
│   ├── SESSION_*.md        # Auto-generated session summaries
│   ├── POST_MORTEM_*.md    # Incident reports
│   └── AGENT_FRAMEWORK.md
├── .claude/
│   ├── agents/             # session-summary-writer, post-mortem-writer
│   ├── hooks/              # session-start.sh, session-end.sh
│   ├── skills/             # session-summary workflow
│   └── settings.local.json # Permissions and hook config
└── tests/                  # Integration tests
```

### Subcrate: cnctd_ai_server

The `crates/cnctd_ai_server/` crate is an Axum-based REST API. It provides:

- **Streaming SSE chat** with full tool-calling loops
- **MCP integration** for tool discovery and execution
- **4-point data obfuscation** (user->LLM, LLM->tool, tool->LLM, LLM->user)
- **Agent execution** with background task management
- **Cache invalidation endpoint** for obfuscation session management

#### Obfuscation System

The obfuscation layer protects sensitive entity data during AI conversations. It is **fully dynamic** -- the server fetches its entity dictionary from an HTTP source URL hosted by the calling application. No entity types, IDs, or names are hardcoded.

**Configuration** (env vars on cnctd_ai_server):
- `OBFUSCATION_KEY` -- HMAC secret for deterministic tokenization
- `OBFUSCATION_SOURCE_URL` -- URL of the calling app's entity endpoint
- `OBFUSCATION_SOURCE_TOKEN` -- Bearer token for auth to the source URL

Obfuscation is enabled when all three vars are set. Otherwise, pass-through mode.

**How it works**:
1. Calling app hosts a `GET` endpoint returning `{ entities, key_inference_overrides?, numeric_rules? }`
2. Entities are `{ type: String, id: i32, name: String }` -- type names are arbitrary (e.g., "channel", "bidder", "advertiser")
3. The server auto-derives key inference patterns from type names (e.g., type "channel" -> patterns `channel_id`, `channelid`, `channelids`, `channel_ids`)
4. HMAC tokens like `channel_a1b2` replace real names/IDs before reaching the LLM
5. Numeric values are scaled per configurable rules (preserving relative ordering)
6. Client SSE stream receives `token_map` and `obfuscation_event` events for transparency

**Key files**:
- `obfuscation/source.rs` -- HTTP source types and fetch function
- `obfuscation/entity_dictionary.rs` -- String-keyed entity lookup
- `obfuscation/tokenizer.rs` -- HMAC-SHA256 tokenization with Aho-Corasick matching
- `obfuscation/obfuscator.rs` -- `KeyInferenceEngine` + 4-point `Obfuscator`
- `obfuscation/numeric_scaler.rs` -- Per-metric scaling with dynamic or default rules
- `obfuscation/session.rs` -- Per-salt session cache with HTTP fetch
- `routes/obfuscation.rs` -- `POST /obfuscation/invalidate` endpoint

See `crates/cnctd_ai_server/docs/OBFUSCATION_SETUP.md` for the full integration guide.

**Note**: The obfuscation system is designed for any application that needs to protect sensitive entity data during AI conversations.

## Common Patterns

### Creating a Client

```rust
// Anthropic
let client = Client::anthropic(
    AnthropicConfig {
        api_key: env::var("ANTHROPIC_API_KEY")?,
        model: "claude-sonnet-4-20250514".into(),
        version: None,
    },
    None,
)?;

// OpenAI
let client = Client::openai(
    OpenAiConfig {
        api_key: env::var("OPENAI_API_KEY")?,
        model: "gpt-4o".into(),
        base_url: None,
    },
)?;

// OpenRouter (uses OpenAI interface with custom base_url)
let client = Client::openai(
    OpenAiConfig {
        api_key: env::var("OPENROUTER_API_KEY")?,
        model: "anthropic/claude-3.5-sonnet".into(),
        base_url: Some("https://openrouter.ai/api/v1".into()),
    },
)?;
```

### Making Completions

```rust
// Non-streaming
let request = CompletionRequest {
    messages: vec![Message::user("Hello!")],
    tools: None,
    options: None,
};
let response = client.complete(request).await?;
let text = response.text();

// Streaming
let mut stream = client.complete_stream(request).await?;
while let Some(chunk) = stream.next().await {
    if let Some(text) = chunk?.text() {
        print!("{}", text);
    }
}
```

### Tool Calling

```rust
let tool = create_tool(
    "get_weather",
    "Get weather for a location",
    json!({
        "type": "object",
        "properties": {
            "location": { "type": "string" }
        },
        "required": ["location"]
    })
)?;

let mut request = CompletionRequest::new(vec![Message::user("What's the weather in NYC?")]);
request.add_tool(tool);

let response = client.complete(request).await?;
if let Some(tool_use) = response.tool_use() {
    // Execute tool, then continue conversation with ToolResult
}
```

## Error Handling

```rust
use cnctd_ai::Error;

match client.complete(request).await {
    Ok(response) => { /* success */ },
    Err(Error::AuthenticationFailed(msg)) => { /* bad API key */ },
    Err(Error::RateLimited { retry_after }) => { /* back off */ },
    Err(Error::ProviderError { provider, message, status_code }) => { /* API error */ },
    Err(e) => { /* other */ },
}
```

## Multi-Provider Tool Compatibility

Different providers handle tool calls differently. Key considerations:

- **Anthropic**: Native tool_use/tool_result content blocks
- **OpenAI**: Function calls with tool_call_id tracking (uses Responses API)
- **Gemini**: Function declarations with grounding metadata

The library normalizes these differences through the unified `Content` enum.

## OpenAI Responses API Details

cnctd_ai uses the newer **Responses API** (`/v1/responses`) for OpenAI models. Key implementation details:

### Files
- `src/client/openai_responses.rs` - Request building and response parsing
- `src/stream.rs` - Streaming response handling (captures `call_id`, `reasoning_items`)

### Multi-Turn Tool Calls
For multi-turn tool calls to work:

1. **call_id tracking**: `ToolUse.call_id` captures the `call_...` format ID from OpenAI
2. **function_call_output matching**: Use `ToolResult.effective_call_id()` when sending results back
3. **1:1 matching**: Every `function_call` in a request must have a matching `function_call_output`

### Reasoning Models (GPT-5.2-pro, o1, o3)
Reasoning models require additional handling:

1. `is_reasoning_model()` helper in `openai_responses.rs` detects o1/o3/gpt-5 models
2. Automatically includes `reasoning.encrypted_content` in requests for these models
3. Captures `encrypted_content` from reasoning items in streaming responses
4. `Message.reasoning_items` must be preserved and echoed back in continuations

### Application Responsibility
When persisting conversations to a database, applications must:
- Store `call_id` alongside `tool_use_id`
- Ensure every reconstructed `function_call` has a matching `function_call_output`
- Preserve `reasoning_items` for reasoning model conversations

## Session Protocol

### Session Start
1. Check `git status` for uncommitted changes
2. Review previous session summary (injected automatically via hook)
3. Pull latest if needed

### Session End
1. Commit and push all changes
2. **Run `/session-summary`** to generate a structured summary
3. NEVER write session summaries yourself - the session-summary-writer agent reads the transcript and writes the file

### Post-Mortems
If something went wrong, run the post-mortem-writer agent:
- Analyzes the transcript for what went wrong
- Produces a structured incident report with fix plan
- Written to `docs/POST_MORTEM_*.md`

## cnctd_ai_server Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ANTHROPIC_API_KEY` | Yes (if using Anthropic) | Anthropic API key |
| `MCP_GATEWAY_URL` | No | MCP gateway URL for tool execution |
| `MCP_GATEWAY_TOKEN` | No | Auth token for MCP gateway |
| `OBFUSCATION_KEY` | No* | HMAC secret for entity tokenization |
| `OBFUSCATION_SOURCE_URL` | No* | URL to fetch entity dictionary |
| `OBFUSCATION_SOURCE_TOKEN` | No* | Bearer token for source URL and invalidation endpoint |

*All three `OBFUSCATION_*` vars must be set to enable obfuscation. If any are missing, obfuscation is disabled.

## Development

```bash
# Run tests
cargo test

# Run example
cargo run --example basic_completion

# Check types (both crates)
cargo check -p cnctd_ai -p cnctd_ai_server

# Run cnctd_ai_server
cd crates/cnctd_ai_server && cargo run
```

## Related Documentation

- `README.md` - Quick start and examples
- `CHANGELOG.md` - Version history
- `MIGRATION.md` - Migration guide between versions
- `MULTI_PROVIDER_TOOL_PROGRESS.md` - Tool compatibility tracking
- `docs/AGENT_FRAMEWORK.md` - Agent framework details
- `crates/cnctd_ai_server/docs/OBFUSCATION_SETUP.md` - Obfuscation integration guide for calling apps

## Parent Project

This library is part of the **cnctd** monorepo. See `../../../CLAUDE.md` for the full ecosystem documentation.

---

*This document serves as persistent context for Claude conversations about the cnctd_ai library.*
