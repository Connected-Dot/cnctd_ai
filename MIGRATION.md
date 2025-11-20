# Migration Guide: Tool API Changes

This document explains the changes to the Tool API in cnctd_ai v0.1.5 and how to migrate your code.

## What Changed

The library now uses `rmcp::model::Tool` instead of a custom Tool struct. This provides better compatibility with MCP servers and standardizes the tool format.

### Old Tool Structure (Pre-0.1.5)

```rust
struct Tool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}
```

### New Tool Structure (0.1.5+)

```rust
use std::borrow::Cow;
use std::sync::Arc;

struct Tool {
    name: Cow<'a, str>,
    description: Option<Cow<'a, str>>,
    input_schema: Arc<serde_json::Map<String, Value>>,
}
```

## Migration Strategies

### Strategy 1: Use Helper Functions (Recommended)

The easiest way to migrate is to use the new helper functions:

```rust
use cnctd_ai::create_tool;
use serde_json::json;

// Old way (won't compile)
let tool = Tool {
    name: "my_tool".into(),
    description: "Does something".into(),
    input_schema: json!({"type": "object"}),
};

// New way (recommended)
let tool = create_tool(
    "my_tool",
    "Does something",
    json!({"type": "object"})
)?;
```

### Strategy 2: Manual Construction

If you need more control, you can construct Tools manually:

```rust
use cnctd_ai::Tool;
use std::borrow::Cow;
use std::sync::Arc;
use serde_json::json;

// For static strings (compile-time constants)
let tool = Tool {
    name: Cow::Borrowed("my_tool"),
    description: Some(Cow::Borrowed("Does something")),
    input_schema: Arc::new(
        serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(
            json!({"type": "object"})
        )?
    ),
};

// For runtime strings
let tool = Tool {
    name: Cow::Owned(name_string),
    description: Some(Cow::Owned(desc_string)),
    input_schema: Arc::new(schema_map),
};
```

## Common Migration Examples

### Example 1: Basic Tool

**Before:**
```rust
let weather_tool = Tool {
    name: "get_weather".into(),
    description: "Get weather info".into(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "location": {"type": "string"}
        }
    }),
};
```

**After:**
```rust
use cnctd_ai::create_tool;

let weather_tool = create_tool(
    "get_weather",
    "Get weather info",
    json!({
        "type": "object",
        "properties": {
            "location": {"type": "string"}
        }
    })
)?;
```

### Example 2: Tool from MCP Gateway

Tools fetched from MCP gateway are already in the correct format:

```rust
let gateway = McpGateway::new("https://mcp.cnctd.world");
let tools = gateway.list_tools("server-name").await?;

// These tools can be used directly
for tool in tools {
    request.add_tool(tool);
}
```

### Example 3: Dynamic Tool Creation

If you're creating tools dynamically from data:

```rust
use cnctd_ai::create_tool;

for tool_spec in tool_specifications {
    let tool = create_tool(
        &tool_spec.name,
        &tool_spec.description,
        tool_spec.schema.clone()
    )?;
    request.add_tool(tool);
}
```

## Helper Functions Reference

### `create_tool`

For owned strings (runtime data):

```rust
pub fn create_tool(
    name: &str,
    description: &str,
    schema: Value,
) -> Result<Tool, serde_json::Error>
```

### `create_tool_borrowed`

For static strings (compile-time constants):

```rust
pub fn create_tool_borrowed(
    name: &'static str,
    description: &'static str,
    schema: Value,
) -> Result<Tool, serde_json::Error>
```

## Error Handling

Both helper functions return `Result<Tool, serde_json::Error>`. Make sure to handle the error:

```rust
let tool = create_tool(name, desc, schema)?; // Propagate error

// Or handle explicitly
match create_tool(name, desc, schema) {
    Ok(tool) => { /* use tool */ },
    Err(e) => { /* handle error */ },
}
```

## Benefits of the New API

1. **Better Memory Efficiency**: Uses `Cow` for strings and `Arc` for schemas
2. **MCP Compatibility**: Direct compatibility with MCP server tools
3. **Type Safety**: Stronger typing with proper schema validation
4. **Flexibility**: Choose between borrowed or owned strings based on use case

## Need Help?

If you encounter issues during migration:

1. Check the examples in the `examples/` directory
2. Review the API documentation
3. Look at the test cases for usage patterns
