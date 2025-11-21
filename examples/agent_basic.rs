//! Agent Framework Example - Autonomous Task Execution
//!
//! This example demonstrates the new agent framework that handles autonomous
//! task execution with tool calling loops, error handling, and detailed tracing.
//!
//! Set these environment variables:
//! - ANTHROPIC_API_KEY: Your Anthropic API key
//! - GATEWAY_URL: URL of your MCP gateway (e.g., https://mcp.cnctd.world)
//! - GATEWAY_TOKEN: Bearer token for authentication (optional)

use anyhow::Result;
use cnctd_ai::{
    Agent, AnthropicConfig, Client, CompletionRequest, McpGateway, RequestOptions,
};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    println!("=== Agent Framework Example ===\n");
    
    // Initialize Anthropic client
    let api_key = env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY must be set");
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    )?;
    
    // Initialize MCP gateway
    let gateway_url = env::var("GATEWAY_URL")
        .unwrap_or_else(|_| "https://mcp.cnctd.world".to_string());
    
    let gateway = if let Ok(token) = env::var("GATEWAY_TOKEN") {
        McpGateway::with_auth(&gateway_url, token)
    } else {
        McpGateway::new(&gateway_url)
    };
    
    println!("Connected to gateway: {}\n", gateway_url);
    
    // Get available servers and tools
    let servers = gateway.list_servers().await?;
    println!("Available MCP servers:");
    for server in &servers {
        println!("  - {}", server.name);
    }
    println!();
    
    // Find brave-search server (if available)
    let search_server = servers.iter()
        .find(|s| s.name == "brave-search")
        .ok_or_else(|| anyhow::anyhow!("brave-search server not found"))?;
    
    // Get tools from the search server
    let mcp_tools = gateway.list_tools(&search_server.name).await?;
    println!("Loaded {} tools from brave-search\n", mcp_tools.len());
    
    // Build an agent with custom configuration
    let agent = Agent::builder(&client)
        .max_iterations(3)  // Reduced to 3 iterations to manage tokens
        .max_duration(std::time::Duration::from_secs(60))
        .max_tool_result_length(1500)  // More aggressive truncation
        .system_prompt("You are a helpful research assistant. Be concise and thorough.")
        .gateway(&gateway)
        .build();
    
    // Create a request with the MCP tools and lower max_tokens
    let mut request = CompletionRequest {
        messages: Vec::new(),
        tools: None,
        options: Some(RequestOptions {
            max_tokens: Some(1024),  // Reduced from 2048
            ..Default::default()
        }),
    };
    
    // Add all MCP tools
    for tool in &mcp_tools {
        request.add_tool(tool.clone());
    }
    
    // Define the task
    let task = "Research the latest developments in Rust async runtime performance. \
               Find recent benchmarks or discussions comparing tokio and async-std. \
               Summarize the key findings.";
    
    println!("Task: {}\n", task);
    println!("Starting agent execution...\n");
    
    // Run the agent
    let trace = agent.run(task, request).await?;
    
    // Print the detailed trace
    trace.print_detailed();
    
    // You can also access specific parts of the trace
    println!("\n=== Tool Executions ===");
    for (i, exec) in trace.tool_executions().iter().enumerate() {
        println!("\n[Tool {}] {}", i + 1, exec.tool_name);
        println!("  Server: {}", exec.server_name.as_deref().unwrap_or("unknown"));
        println!("  Success: {}", exec.is_success());
        println!("  Duration: {:.2}s", exec.duration.as_secs_f64());
        
        if let Some(error) = &exec.error {
            println!("  Error: {}", error);
        }
    }
    
    println!("\n=== Example Complete ===");
    
    Ok(())
}
