//! Simple Agent Example - Minimal Setup
//!
//! This example shows the simplest way to use the agent framework
//! for autonomous task execution.
//!
//! Set these environment variables:
//! - ANTHROPIC_API_KEY: Your Anthropic API key
//! - GATEWAY_URL: URL of your MCP gateway (optional, defaults to https://mcp.cnctd.world)
//! - GATEWAY_TOKEN: Bearer token for gateway authentication (optional)

use anyhow::Result;
use cnctd_ai::{
    Agent, AnthropicConfig, Client, McpGateway,
};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    println!("=== Simple Agent Example ===\n");
    
    // Setup client
    let api_key = env::var("ANTHROPIC_API_KEY")?;
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    )?;
    
    // Setup MCP gateway with optional authentication
    let gateway_url = env::var("GATEWAY_URL")
        .unwrap_or_else(|_| "https://mcp.cnctd.world".to_string());
    
    let gateway = if let Ok(token) = env::var("GATEWAY_TOKEN") {
        McpGateway::with_auth(&gateway_url, token)
    } else {
        McpGateway::new(&gateway_url)
    };
    
    // Create agent - specify only brave-search server to keep token usage down
    let agent = Agent::new(&client)
        .with_gateway(&gateway)
        .with_servers(vec!["brave-search".to_string()]);
    
    // Run a simple task
    let task = "Search for the current weather in San Francisco and tell me if I need an umbrella today.";
    
    println!("Task: {}\n", task);
    
    let trace = agent.run_simple(task).await?;
    
    // Print detailed trace to see what happened
    trace.print_detailed();
    
    Ok(())
}
