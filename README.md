# cnctd_ai

A Rust abstraction layer for AI/LLM providers (Anthropic Claude, OpenAI) with integrated MCP (Model Context Protocol) support and autonomous agent framework.

## Features

- **Multi-Provider Support**: Unified interface for Anthropic Claude and OpenAI
- **Streaming & Non-Streaming**: Support for both regular completions and streaming responses
- **Tool Calling**: Full support for function/tool calling with both providers
- **Agent Framework**: Autonomous task execution with tool calling loops
- **MCP Integration**: Native support for MCP servers (stdio and HTTP gateway)
- **Error Handling**: Comprehensive error types with provider-specific handling
- **Type Safety**: Strong typing with proper error handling throughout

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
cnctd_ai = "0.1.5"
```

## Quick Start

### Agent Framework (NEW!)

The easiest way to build autonomous AI applications:

```rust
use cnctd_ai::{Agent, Client, AnthropicConfig, McpGateway};

// Setup client and gateway
let client = Client::anthropic(
    AnthropicConfig {
        api_key: "your-key".into(),
        model: "claude-sonnet-4-20250514".into(),
        version: None,
    },
    None,
)?;

let gateway = McpGateway::new("https://mcp.cnctd.world");

// Create agent with default settings
let agent = Agent::new(&client).with_gateway(&gateway);

// Run autonomous task - agent will use tools as needed
let trace = agent.run_simple(
    "Research the latest Rust async trends and summarize key findings"
).await?;

// View results
trace.print_summary();
```

For advanced configuration:

```rust
let agent = Agent::builder(&client)
    .max_iterations(10)
    .max_duration(Duration::from_secs(300))
    .system_prompt("You are a helpful research assistant.")
    .gateway(&gateway)
    .build();
```

See [Agent Framework Documentation](docs/AGENT_FRAMEWORK.md) for more details.

### Basic Completion

```rust
use cnctd_ai::{Client, AnthropicConfig, Message, CompletionRequest};

let client = Client::anthropic(
    AnthropicConfig {
        api_key: "your-api-key".into(),
        model: "claude-sonnet-4-20250514".into(),
        version: None,
    },
    None,
)?;

let request = CompletionRequest {
    messages: vec![Message::user("Hello, how are you?")],
    tools: None,
    options: None,
};

let response = client.complete(request).await?;
println!("Response: {}", response.text());
```

### Streaming

```rust
use cnctd_ai::{Client, AnthropicConfig, Message, CompletionRequest};

let mut stream = client.complete_stream(request).await?;

while let Some(chunk) = stream.next().await {
    let chunk = chunk?;
    if let Some(text) = chunk.text() {
        print!("{}", text);
    }
}
```

### Tool Calling

```rust
use cnctd_ai::{Client, Message, CompletionRequest, create_tool};
use serde_json::json;

// Create a tool using the helper function
let weather_tool = create_tool(
    "get_weather",
    "Get the current weather for a location",
    json!({
        "type": "object",
        "properties": {
            "location": {
                "type": "string",
                "description": "The city and state, e.g. San Francisco, CA"
            },
            "unit": {
                "type": "string",
                "enum": ["celsius", "fahrenheit"]
            }
        },
        "required": ["location"]
    })
)?;

let mut request = CompletionRequest {
    messages: vec![Message::user("What's the weather in SF?")],
    tools: None,
    options: None,
};
request.add_tool(weather_tool);

let response = client.complete(request).await?;

// Check if model wants to use a tool
if let Some(tool_use) = response.tool_use() {
    println!("Tool: {}", tool_use.name);
    println!("Arguments: {}", tool_use.input);
    
    // Execute tool and continue conversation
    // See examples/tool_calling.rs for full implementation
}
```

### MCP Gateway Integration

```rust
use cnctd_ai::McpGateway;

let gateway = McpGateway::new("https://mcp.cnctd.world");

// List available servers
let servers = gateway.list_servers().await?;

// List tools from a specific server
let tools = gateway.list_tools("brave-search").await?;

// Execute a tool
let result = gateway.call_tool(
    "brave-search",
    "brave_web_search",
    Some(json!({"query": "Rust programming"})),
).await?;
```

## Examples

The repository includes several examples:

**Agent Framework:**
- `agent_simple.rs` - Minimal agent setup
- `agent_basic.rs` - Full-featured agent with configuration

**Core Functionality:**
- `basic_completion.rs` - Simple completion example
- `streaming.rs` - Streaming responses
- `tool_calling.rs` - Function/tool calling
- `tool_calling_streaming.rs` - Tool calling with streaming
- `conversation.rs` - Multi-turn conversations
- `error_handling.rs` - Error handling patterns
- `mcp_gateway.rs` - MCP gateway integration

Run examples with:

```bash
cargo run --example agent_simple
cargo run --example basic_completion
```

## Environment Variables

Set these for the examples:

```bash
ANTHROPIC_API_KEY=your-anthropic-key
OPENAI_API_KEY=your-openai-key
GATEWAY_URL=https://mcp.cnctd.world  # Optional
GATEWAY_TOKEN=your-token  # Optional
```

## Tool Creation Helpers

The library provides helper functions for easier tool creation:

```rust
use cnctd_ai::{create_tool, create_tool_borrowed};

// For owned strings (runtime data)
let tool = create_tool(name, description, schema)?;

// For static strings (compile-time constants)
let tool = create_tool_borrowed(name, description, schema)?;
```

## Error Handling

The library provides comprehensive error types:

```rust
use cnctd_ai::Error;

match client.complete(request).await {
    Ok(response) => { /* handle success */ },
    Err(Error::AuthenticationFailed(msg)) => { /* handle auth */ },
    Err(Error::RateLimited { retry_after }) => { /* handle rate limit */ },
    Err(Error::ProviderError { provider, message, status_code }) => { /* handle provider error */ },
    Err(e) => { /* handle other errors */ },
}
```

## Documentation

- [Agent Framework Guide](docs/AGENT_FRAMEWORK.md) - Complete agent framework documentation
- [API Documentation](https://docs.rs/cnctd_ai) - Full API reference (coming soon)

## License

MIT License - see LICENSE file for details.
