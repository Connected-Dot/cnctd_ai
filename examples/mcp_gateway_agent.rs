//! Example: Complete Agent with MCP Gateway Integration
//!
//! This example demonstrates a complete agentic workflow with proper token management.
//!
//! Set these environment variables:
//! - ANTHROPIC_API_KEY: Your Anthropic API key
//! - GATEWAY_URL: URL of your MCP gateway (e.g., https://mcp.cnctd.world)
//! - GATEWAY_TOKEN: Bearer token for authentication (optional)

use anyhow::Result;
use cnctd_ai::{
    Client, AnthropicConfig, Message, CompletionRequest, RequestOptions,
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
    
    // Find brave-search server (if available)
    let search_server = servers.iter()
        .find(|s| s.name == "brave-search")
        .ok_or_else(|| anyhow::anyhow!("brave-search server not found"))?;
    
    // Get tools from the search server
    let mcp_tools = gateway.list_tools(&search_server.name).await?;
    
    // Start a conversation with Claude - be explicit about ONE search
    let user_query = "What are the latest developments in Rust async programming? \
                     Search the web ONCE and then provide a comprehensive answer based on those results.";
    
    println!("User: {}\n", user_query);
    
    let mut messages = vec![Message::user(user_query)];
    
    // Create request with tools from MCP and reduced max_tokens
    let mut request = CompletionRequest {
        messages: messages.clone(),
        tools: None,
        options: Some(RequestOptions {
            max_tokens: Some(1024),  // Reduced from default 4096
            ..Default::default()
        }),
    };
    
    // Add MCP tools to the request
    for tool in &mcp_tools {
        request.add_tool(tool.clone());
    }
    
    // Single iteration approach - simpler and more token-efficient
    println!("Claude is thinking...\n");
    let response = claude.complete(request.clone()).await?;
    
    // Check if Claude wants to use a tool
    if let Some(tool_use) = response.tool_use() {
        println!("Claude is using tool: {}", tool_use.name);
        
        // Execute the tool
        let tool_result = gateway.call_tool(
            &search_server.name,
            &tool_use.name,
            Some(tool_use.input.clone()),
        ).await?;
        
        let mut result_text = tool_result_to_string(&tool_result);
        
        // Aggressively truncate to keep tokens manageable
        const MAX_RESULT_SIZE: usize = 1500;
        if result_text.len() > MAX_RESULT_SIZE {
            result_text.truncate(MAX_RESULT_SIZE);
            result_text.push_str("\n\n[Results truncated]");
        }
        
        println!("Tool executed ({} bytes of results)\n", result_text.len());
        
        // Add messages and get final response
        messages.push(response.message.clone());
        messages.push(Message::tool_result(tool_use.id.clone(), result_text));
        
        // Final response
        request.messages = messages;
        let final_response = claude.complete(request).await?;
        
        println!("Claude's response:");
        println!("{}\n", final_response.text());
        
        println!("Token usage:");
        println!("  First call:  {} prompt, {} completion", 
            response.usage.prompt_tokens, 
            response.usage.completion_tokens
        );
        println!("  Second call: {} prompt, {} completion", 
            final_response.usage.prompt_tokens,
            final_response.usage.completion_tokens
        );
        println!("  Total: {} tokens", 
            response.usage.total_tokens + final_response.usage.total_tokens
        );
    } else {
        // Claude responded directly without tools
        println!("Claude's response:");
        println!("{}", response.text());
    }
    
    println!("\n=== Example Complete ===");
    
    Ok(())
}