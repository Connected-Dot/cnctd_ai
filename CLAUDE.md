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
├── examples/               # Usage examples
├── docs/                   # Additional documentation
└── tests/                  # Integration tests
```

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
- **OpenAI**: Function calls with tool_call_id tracking
- **Gemini**: Function declarations with grounding metadata

The library normalizes these differences through the unified `Content` enum.

## Development

```bash
# Run tests
cargo test

# Run example
cargo run --example basic_completion

# Check types
cargo check
```

## Related Documentation

- `README.md` - Quick start and examples
- `CHANGELOG.md` - Version history
- `MIGRATION.md` - Migration guide between versions
- `MULTI_PROVIDER_TOOL_PROGRESS.md` - Tool compatibility tracking
- `docs/AGENT_FRAMEWORK.md` - Agent framework details

## Parent Project

This library is part of the **cnctd** monorepo. See `../../../CLAUDE.md` for the full ecosystem documentation.

---

*This document serves as persistent context for Claude conversations about the cnctd_ai library.*
