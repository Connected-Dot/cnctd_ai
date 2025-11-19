//! Example: MCP Gateway Integration
//!
//! This example demonstrates how to:
//! 1. Connect to an MCP gateway
//! 2. Discover available servers
//! 3. List tools from specific servers
//! 4. Execute a tool
//!
//! Set these environment variables:
//! - GATEWAY_URL: URL of your MCP gateway (e.g., https://api.cnctd.world)
//! - GATEWAY_TOKEN: Bearer token for authentication (optional)

use cnctd_ai::{McpGateway, Tool, tool_result_to_string};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();
    
    let gateway_url = env::var("GATEWAY_URL")
        .unwrap_or_else(|_| "https://api.cnctd.world".to_string());
    
    println!("=== MCP Gateway Example ===\n");
    println!("Gateway URL: {}\n", gateway_url);
    
    // Create gateway client with optional authentication
    let gateway = if let Ok(token) = env::var("GATEWAY_TOKEN") {
        println!("Using authenticated connection\n");
        McpGateway::with_auth(&gateway_url, token)
    } else {
        println!("Using unauthenticated connection\n");
        McpGateway::new(&gateway_url)
    };
    
    // Step 1: Discover available servers
    println!("Step 1: Discovering available servers...");
    let servers = gateway.list_servers().await?;
    println!("Found {} servers:\n", servers.len());
    
    for server in &servers {
        println!("  • {} ({})", server.name, server.url);
        if let Some(desc) = &server.description {
            println!("    {}", desc);
        }
        println!("    Tools: {}", server.available_tools.len());
        println!();
    }
    
    // Step 2: List tools from a specific server
    if let Some(first_server) = servers.first() {
        println!("\nStep 2: Listing tools from '{}'...", first_server.name);
        let tools = gateway.list_tools(&first_server.name).await?;
        println!("Found {} tools:\n", tools.len());
        
        for (idx, tool) in tools.iter().take(5).enumerate() {
            println!("  {}. {} - {}", 
                idx + 1, 
                tool.name,
                tool.description.as_deref().unwrap_or("No description")
            );
        }
        
        if tools.len() > 5 {
            println!("  ... and {} more tools", tools.len() - 5);
        }
        
        // Step 3: Convert rmcp tools to cnctd_ai tools
        println!("\nStep 3: Converting to cnctd_ai Tool format...");
        let cnctd_tools: Vec<Tool> = tools.iter().map(Tool::from).collect();
        println!("Converted {} tools", cnctd_tools.len());
        
        // Validate the first tool
        if let Some(first_tool) = cnctd_tools.first() {
            println!("\nValidating first tool: {}", first_tool.name);
            match first_tool.validate() {
                Ok(_) => println!("  ✓ Tool schema is valid"),
                Err(e) => println!("  ✗ Tool schema invalid: {}", e),
            }
        }
        
        // Step 4: Execute a tool (if available)
        println!("\nStep 4: Example tool execution...");
        
        // Look for a simple tool to demonstrate execution
        // This is just an example - adjust based on your available tools
        if first_server.name == "brave-search" {
            println!("Executing brave_web_search...");
            
            let arguments = serde_json::json!({
                "query": "Rust programming language"
            });
            
            match gateway.call_tool(&first_server.name, "brave_web_search", Some(arguments)).await {
                Ok(result) => {
                    println!("\n✓ Tool executed successfully!");
                    println!("Is error: {}", result.is_error.unwrap_or(false));
                    
                    let result_text = tool_result_to_string(&result);
                    let preview = if result_text.len() > 200 {
                        format!("{}...", &result_text[..200])
                    } else {
                        result_text
                    };
                    println!("Result preview:\n{}", preview);
                },
                Err(e) => {
                    println!("\n✗ Tool execution failed: {}", e);
                }
            }
        } else {
            println!("Skipping execution - adjust example for your available tools");
        }
    } else {
        println!("\nNo servers available to demonstrate tool usage");
    }
    
    println!("\n=== Example Complete ===");
    
    Ok(())
}
