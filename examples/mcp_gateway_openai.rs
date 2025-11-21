//! Example: MCP Gateway Integration with OpenAI
//!
//! This example demonstrates how to:
//! 1. Connect to an MCP gateway
//! 2. Discover available servers
//! 3. List tools from specific servers
//! 4. Execute a tool
//!
//! This is the OpenAI version of the mcp_gateway.rs example.
//!
//! Set these environment variables:
//! - GATEWAY_URL: URL of your MCP gateway (e.g., https://mcp.cnctd.world)
//! - GATEWAY_TOKEN: Bearer token for authentication (optional)

use cnctd_ai::{McpGateway, tool_result_to_string};
use std::env;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables
    dotenvy::dotenv().ok();
    
    let gateway_url = env::var("GATEWAY_URL")
        .unwrap_or_else(|_| "https://mcp.cnctd.world".to_string());
    
    println!("=== MCP Gateway Example (OpenAI) ===\n");
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
        println!("  • {}", server.name);
        if let Some(url) = &server.url {
            println!("    URL: {}", url);
        }
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
    }
    
    // Step 3: Execute a tool (find an appropriate one to demonstrate)
    println!("\nStep 3: Example tool execution...");
    
    // Define some simple tools we can demonstrate with safe, read-only operations
    let demo_tools = [
        ("time", "get_current_time", serde_json::json!({
            "timezone": "America/New_York"
        })),
        ("brave-search", "brave_web_search", serde_json::json!({
            "query": "Rust programming language"
        })),
        ("github", "get_me", serde_json::json!({})),
        ("template", "template_status", serde_json::json!({})),
    ];
    
    let mut executed = false;
    for (server_name, tool_name, arguments) in &demo_tools {
        // Check if this server exists
        if servers.iter().any(|s| s.name == *server_name) {
            println!("Executing {}:{}...", server_name, tool_name);
            
            match gateway.call_tool(server_name, tool_name, Some(arguments.clone())).await {
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
                    executed = true;
                    break;
                },
                Err(e) => {
                    println!("✗ Failed to execute {}:{}: {}", server_name, tool_name, e);
                    println!("Trying next tool...\n");
                }
            }
        }
    }
    
    if !executed {
        println!("No suitable tools found for demonstration");
        println!("Available servers: {}", servers.iter().map(|s| s.name.as_str()).collect::<Vec<_>>().join(", "));
    }
    
    println!("\n=== Example Complete ===");
    
    Ok(())
}
