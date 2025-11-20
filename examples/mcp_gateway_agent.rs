//! Example: Complete Agent with MCP Gateway Integration
//!
//! This example demonstrates a complete agentic workflow:
//! 1. Connect to MCP gateway to get available tools
//! 2. Use Claude to decide which tools to call
//! 3. Execute tools via the gateway
//! 4. Return results to Claude for final response
//!
//! Set these environment variables:
//! - ANTHROPIC_API_KEY: Your Anthropic API key
//! - GATEWAY_URL: URL of your MCP gateway (e.g., https://mcp.cnctd.world)
//! - GATEWAY_TOKEN: Bearer token for authentication (optional)

use anyhow::Result;
use cnctd_ai::{
    Client, AnthropicConfig, Message, CompletionRequest,
    McpGateway, tool_result_to_string,
};
use serde_json::json;
use std::env;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    println!("=== MCP Gateway Agent Example ===\n");
    
    // Initialize Claude client
    let api_key = env::var("ANTHROPIC_API_KEY")
        .expect("ANTHROPIC_API_KEY must be set");
    
    let claude = Client::anthropic(
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
        println!("  - {} ({} tools)", server.name, server.available_tools.len());
    }
    println!();
    
    // Find brave-search server (if available)
    let search_server = servers.iter()
        .find(|s| s.name == "brave-search")
        .ok_or_else(|| anyhow::anyhow!("brave-search server not found"))?;
    
    // Get tools from the search server
    let mcp_tools = gateway.list_tools(&search_server.name).await?;
    println!("Tools from {}:", search_server.name);
    for tool in &mcp_tools {
        println!("  - {}", tool.name);
    }
    println!();
    
    // Start a conversation with Claude
    let user_query = "What are the latest developments in Rust async programming? \
                     Use web search to find recent information.";
    
    println!("User: {}\n", user_query);
    
    let mut messages = vec![Message::user(user_query)];
    
    // Create request with tools from MCP
    let mut request = CompletionRequest {
        messages: messages.clone(),
        tools: None,
        options: None,
    };
    
    // Add MCP tools to the request
    for tool in &mcp_tools {
        request.add_tool(tool.clone());
    }
    
    // First call - Claude decides what to do
    println!("Claude is thinking...\n");
    let response = claude.complete(request).await?;
    
    // Check if Claude wants to use a tool
    if let Some(tool_use) = response.tool_use() {
        println!("Claude wants to use tool: {}", tool_use.name);
        println!("Arguments: {}\n", tool_use.input);
        
        // Execute the tool via MCP gateway
        println!("Executing tool via MCP gateway...");
        let tool_result = gateway.call_tool(
            &search_server.name,
            &tool_use.name,
            Some(tool_use.input.clone()),
        ).await?;
        
        let result_text = tool_result_to_string(&tool_result);
        println!("Tool result received ({} bytes)\n", result_text.len());
        
        // Add tool use and result to conversation
        messages.push(Message::assistant_with_tool_use(tool_use.clone()));
        messages.push(Message::tool_result(tool_use.id.clone(), result_text));
        
        // Second call - Claude processes the results
        println!("Claude is processing results...\n");
        let final_request = CompletionRequest {
            messages: messages.clone(),
            tools: None,
            options: None,
        };
        
        let final_response = claude.complete(final_request).await?;
        
        println!("Claude's final response:");
        println!("{}", final_response.text());
        println!("\n(Used {} tokens)", final_response.usage.total_tokens);
    } else {
        // Claude responded without using tools
        println!("Claude's response:");
        println!("{}", response.text());
    }
    
    println!("\n=== Example Complete ===");
    
    Ok(())
}
