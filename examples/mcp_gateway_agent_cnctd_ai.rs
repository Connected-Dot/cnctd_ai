//! Example: Complete Agent with MCP Gateway Integration (Ollama via cnctd.world)
//!
//! This example demonstrates a complete agentic workflow with Ollama models via cnctd.world
//! gateway and proper token management.
//!
//! Set these environment variables:
//! - CNCTD_AI_API_KEY: Your cnctd.world API key
//! - GATEWAY_URL: URL of your MCP gateway (e.g., https://mcp.cnctd.world)
//! - GATEWAY_TOKEN: Bearer token for authentication (optional)
//!
//! Note: Not all Ollama models support tool calling. Models known to work:
//! - qwen3-coder:latest ✓
//! - qwen3:30b ✓
//! - gpt-oss:latest ⚠ (may have parameter validation issues)
//!
//! Models that DON'T support tools:
//! - gemma3:4b ✗
//! - olmo2:7b ✗
//! - olmo2:13b ✗

use anyhow::Result;
use cnctd_ai::{
    Client, ClientOptions, CompletionRequest, McpGateway, Message, OpenAiConfig, RequestOptions, tool_result_to_string
};
use std::env;

// Models known to work well with tools
// const MODEL: &str = "qwen3-coder:latest";  // Fast, good with code
const MODEL: &str = "qwen3:30b";  // Better quality, but may include <think> tags

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    println!("=== MCP Gateway Agent Example (CnctdAI) ===\n");
    
    // Initialize OpenAI-compatible client for cnctd.world
    let api_key = env::var("CNCTD_AI_API_KEY")
        .expect("CNCTD_AI_API_KEY must be set");
    
    let mut options = ClientOptions::default();
    options.base_url = Some("https://api.cnctd.world/ai/v1".to_string());

    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: MODEL.into(),
            organization: None,
        },
        Some(options),
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
    println!("Using model: {}\n", MODEL);
    
    // Get available servers and tools
    let servers = gateway.list_servers().await?;
    
    // Find brave-search server (if available)
    let search_server = servers.iter()
        .find(|s| s.name == "brave-search")
        .ok_or_else(|| anyhow::anyhow!("brave-search server not found"))?;
    
    // Get tools from the search server
    let mcp_tools = gateway.list_tools(&search_server.name).await?;
    
    println!("Loaded {} tools from brave-search server\n", mcp_tools.len());
    
    // Start a conversation - be explicit about ONE search
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
    
    // First call - model decides to use tools
    println!("{} is thinking...\n", MODEL);
    let response = match client.complete(request.clone()).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error during first completion: {}", e);
            if e.to_string().contains("does not support tools") {
                eprintln!("\nThis model doesn't support tool calling. Try one of these instead:");
                eprintln!("  - qwen3-coder:latest");
                eprintln!("  - qwen3:30b");
                eprintln!("  - mistral-nemo:12b (if available)");
            }
            return Err(e);
        }
    };
    
    // Check if model wants to use a tool
    if let Some(tool_use) = response.tool_use() {
        println!("{} is using tool: {}", MODEL, tool_use.name);
        println!("Tool arguments: {}\n", serde_json::to_string_pretty(&tool_use.input)?);
        
        // Execute the tool
        let tool_result = match gateway.call_tool(
            &search_server.name,
            &tool_use.name,
            Some(tool_use.input.clone()),
        ).await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Tool execution error: {}", e);
                if e.to_string().contains("Invalid enum value") {
                    eprintln!("\nThe model provided invalid parameter values.");
                    eprintln!("This is a known issue with some models not following enum constraints.");
                }
                return Err(e);
            }
        };
        
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
        let final_response = client.complete(request).await?;
        
        let response_text = final_response.text();
        
        println!("{}'s response:", MODEL);
        if response_text.is_empty() {
            println!("(Model returned empty response - this can happen with some Ollama models)");
        } else {
            // Strip out <think> tags if present (qwen3 models sometimes do this)
            let cleaned_text = if response_text.contains("<think>") {
                let parts: Vec<&str> = response_text.split("</think>").collect();
                if parts.len() > 1 {
                    println!("(Note: Removed internal reasoning tags)\n");
                    parts[1].trim()
                } else {
                    response_text.as_str()
                }
            } else {
                response_text.as_str()
            };
            
            println!("{}\n", cleaned_text);
        }
        
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
        // Model responded directly without tools
        println!("{}'s response:", MODEL);
        println!("{}", response.text());
        println!("\n(Model chose not to use any tools)");
    }
    
    println!("\n=== Example Complete ===");
    
    Ok(())
}
