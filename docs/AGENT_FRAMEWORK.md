# Agent Framework

The agent framework provides autonomous task execution with tool calling loops, comprehensive error handling, and detailed execution tracing.

## Features

- **Autonomous Tool Calling**: Automatically executes tool calling loops until task completion
- **Configurable Limits**: Set max iterations, timeouts, and result truncation
- **Error Handling**: Retry failed tools, continue on errors, or stop immediately
- **Execution Tracing**: Complete trace of all events, tools called, and tokens used
- **MCP Integration**: Native support for MCP gateway tool execution
- **Builder Pattern**: Fluent API for easy agent configuration

## Quick Start

### Simple Usage

```rust
use cnctd_ai::{Agent, Client, AnthropicConfig, McpGateway};

let client = Client::anthropic(
    AnthropicConfig {
        api_key: "your-key".into(),
        model: "claude-sonnet-4-20250514".into(),
        version: None,
    },
    None,
)?;

let gateway = McpGateway::new("https://mcp.cnctd.world");
let agent = Agent::new(&client).with_gateway(&gateway);

// Run a task - the agent will autonomously use tools as needed
let trace = agent.run_simple("Find the weather in Tokyo").await?;

// Print execution summary
trace.print_summary();
```

### Advanced Configuration

```rust
use cnctd_ai::{Agent, CompletionRequest, RequestOptions};
use std::time::Duration;

// Build agent with custom configuration
let agent = Agent::builder(&client)
    .max_iterations(10)
    .max_duration(Duration::from_secs(300))
    .max_tool_result_length(2000)
    .system_prompt("You are a research assistant. Be thorough but concise.")
    .retry_failed_tools(true)
    .max_tool_retries(3)
    .gateway(&gateway)
    .build();

// Create request with tools
let mut request = CompletionRequest {
    messages: Vec::new(),
    tools: None,
    options: Some(RequestOptions {
        max_tokens: Some(2048),
        ..Default::default()
    }),
};

// Add MCP tools
let tools = gateway.list_tools("brave-search").await?;
for tool in tools {
    request.add_tool(tool);
}

// Run the agent
let trace = agent.run("Research AI trends", request).await?;
```

## Configuration Options

### AgentConfig

- `max_iterations`: Maximum reasoning loops (default: 10)
- `max_duration`: Total execution timeout (default: 5 minutes)
- `stop_on_error`: Whether to halt on first error (default: false)
- `max_tool_result_length`: Truncate long results (default: 4000 chars)
- `include_reasoning`: Capture thinking/reasoning (default: true)
- `system_prompt`: Custom system prompt to prepend (default: None)
- `retry_failed_tools`: Auto-retry failed tools (default: true)
- `max_tool_retries`: Max retry attempts (default: 2)

### Builder Methods

```rust
Agent::builder(&client)
    .max_iterations(5)
    .max_duration(Duration::from_secs(120))
    .no_time_limit()  // Remove timeout
    .stop_on_error(true)
    .max_tool_result_length(1500)
    .unlimited_tool_results()  // No truncation
    .system_prompt("Custom instructions...")
    .retry_failed_tools(true)
    .max_tool_retries(3)
    .gateway(&gateway)
    .build()
```

## Execution Traces

The agent returns a comprehensive `AgentTrace` with:

```rust
pub struct AgentTrace {
    pub events: Vec<TraceEvent>,       // All events during execution
    pub result: Option<String>,        // Final result
    pub stop_reason: StopReason,       // Why agent stopped
    pub duration: Duration,            // Total time
    pub total_tokens: u32,             // Tokens used
    pub iterations: usize,             // Loop iterations
    pub errors: usize,                 // Error count
    pub successful_tool_calls: usize,  // Successful tools
}
```

### Trace Methods

```rust
// Print human-readable summary
trace.print_summary();

// Print detailed event log
trace.print_detailed();

// Access specific data
let tool_calls = trace.tool_executions();
let errors = trace.errors_trace();
let responses = trace.model_responses();

// Check outcome
match trace.stop_reason {
    StopReason::Success => println!("Task completed!"),
    StopReason::MaxIterations => println!("Hit iteration limit"),
    StopReason::Timeout => println!("Timed out"),
    StopReason::Error => println!("Stopped on error"),
    // ...
}
```

### Trace Events

Events captured during execution:

- `Start`: Agent started with task
- `Iteration`: New reasoning loop
- `ModelThinking`: Reasoning/thinking (if captured)
- `ModelResponse`: Model's response with token usage
- `ToolExecution`: Tool called with input/output/timing
- `Error`: Error occurred
- `Complete`: Agent finished with reason

## Tool Execution

### MCP Gateway Integration

The agent automatically discovers and executes MCP tools:

```rust
// Tools can be specified as "server:tool" or just "tool"
// Agent will auto-discover the correct server

let gateway = McpGateway::new("https://mcp.cnctd.world");
let agent = Agent::new(&client).with_gateway(&gateway);

// Agent will find and use appropriate tools
let trace = agent.run_simple("Search for Rust news").await?;
```

### Tool Execution Details

Each `ToolExecution` includes:

```rust
pub struct ToolExecution {
    pub tool_name: String,          // Tool that was called
    pub tool_call_id: String,       // Unique ID for this call
    pub input: Option<Value>,       // Arguments passed
    pub output: Option<String>,     // Result (if success)
    pub error: Option<String>,      // Error (if failed)
    pub duration: Duration,         // Execution time
    pub server_name: Option<String>, // MCP server used
}
```

### Error Handling

The agent has sophisticated error handling:

- **Retry Logic**: Automatically retries failed tool calls (configurable)
- **Continue on Error**: Can keep going when tools fail (default)
- **Stop on Error**: Halt immediately on first failure (optional)
- **Error Tracking**: All errors captured in trace

```rust
let agent = Agent::builder(&client)
    .stop_on_error(false)      // Continue on errors
    .retry_failed_tools(true)  // Retry failed calls
    .max_tool_retries(3)       // Up to 3 retry attempts
    .build();
```

## Stop Reasons

Agents stop for these reasons:

- `Success`: Task completed successfully
- `ModelStopped`: Model indicated completion
- `MaxIterations`: Hit iteration limit
- `Timeout`: Exceeded time limit
- `Error`: Error occurred and stop_on_error=true
- `NoToolsRequested`: Unusual - no tools requested
- `Cancelled`: External cancellation (future feature)

## Examples

See the examples directory:

- `agent_simple.rs` - Minimal setup and usage
- `agent_basic.rs` - Full-featured example with configuration
- `agent_custom_tools.rs` - Using custom tool executors (coming soon)

Run examples:

```bash
cargo run --example agent_simple
cargo run --example agent_basic
```

## Use Cases

Perfect for:

- Research tasks requiring web searches
- Multi-step analysis workflows
- Data gathering and synthesis
- Iterative problem solving
- Autonomous tool usage scenarios

## Future Enhancements

Planned features:

- Custom tool executors (beyond MCP)
- Streaming trace updates
- Human-in-the-loop breakpoints
- Parallel tool execution
- Conversation persistence
- Memory integration
