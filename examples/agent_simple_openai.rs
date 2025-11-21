//! Simple Agent Example - OpenAI Version
//!
//! This example shows how to use the agent framework with OpenAI models
//! for autonomous task execution.
//!
//! Set these environment variables:
//! - OPENAI_API_KEY: Your OpenAI API key
//! - GATEWAY_URL: URL of your MCP gateway (optional, defaults to https://mcp.cnctd.world)
//! - GATEWAY_TOKEN: Bearer token for gateway authentication (optional)

use anyhow::Result;
use cnctd_ai::{
    Agent, Client, McpGateway, OpenAiConfig,
};
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    println!("=== Simple Agent Example (OpenAI) ===\n");
    
    // Setup OpenAI client
    let api_key = env::var("OPENAI_API_KEY")?;
    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(),  // or "gpt-4o-mini" for cheaper option
            organization: None,
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
