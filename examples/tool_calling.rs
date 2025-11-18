use anyhow::Result;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    println!("=== Testing Anthropic Tool Calling ===");
    test_anthropic_tools().await?;
    
    println!("\n=== Testing OpenAI Tool Calling ===");
    test_openai_tools().await?;
    
    Ok(())
}

async fn test_anthropic_tools() -> Result<()> {
    use cnctd_ai::{Client, AnthropicConfig, Message, CompletionRequest, Tool, ToolDefinition};
    
    let api_key = std::env::var("ANTHROPIC_API_KEY")?;
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    )?;
    
    // Define a simple weather tool
    let weather_tool = Tool::new(
        "get_weather",
        "Get the current weather for a location",
        json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and state, e.g. San Francisco, CA"
                },
                "unit": {
                    "type": "string",
                    "enum": ["celsius", "fahrenheit"],
                    "description": "The temperature unit"
                }
            },
            "required": ["location"]
        })
    );
    
    let mut messages = vec![
        Message::user("What's the weather like in San Francisco?")
    ];
    
    let mut request = CompletionRequest {
        messages: messages.clone(),
        options: None,
    };
    request.add_tool(weather_tool.clone());
    
    let response = client.complete(request).await?;
    
    println!("Response: {}", response.text());
    
    // Check if the model wants to use a tool
    if let Some(tool_use) = response.tool_use() {
        println!("\nTool requested: {}", tool_use.name);
        println!("Tool arguments: {}", tool_use.input);
        
        // Simulate tool execution
        let weather_result = json!({
            "temperature": 72,
            "unit": "fahrenheit",
            "conditions": "sunny"
        });
        
        // Add assistant's tool request to history
        messages.push(Message::assistant_with_tool_use(tool_use.clone()));
        
        // Add tool result
        messages.push(Message::tool_result(
            tool_use.id.clone(),
            weather_result.to_string()
        ));
        
        // Get final response
        let request = CompletionRequest {
            messages: messages.clone(),
            options: None,
        };
        
        let final_response = client.complete(request).await?;
        println!("\nFinal response: {}", final_response.text());
    }
    
    Ok(())
}

async fn test_openai_tools() -> Result<()> {
    use cnctd_ai::{Client, OpenAiConfig, Message, CompletionRequest, Tool};
    
    let api_key = std::env::var("OPENAI_API_KEY")?;
    
    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(),
            organization: None,
        },
        None,
    )?;
    
    // Same tool definition works for both providers
    let weather_tool = Tool::new(
        "get_weather",
        "Get the current weather for a location",
        json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city and state, e.g. San Francisco, CA"
                },
                "unit": {
                    "type": "string",
                    "enum": ["celsius", "fahrenheit"],
                    "description": "The temperature unit"
                }
            },
            "required": ["location"]
        })
    );
    
    let mut messages = vec![
        Message::user("What's the weather like in New York?")
    ];
    
    let mut request = CompletionRequest {
        messages: messages.clone(),
        options: None,
    };
    request.add_tool(weather_tool.clone());
    
    let response = client.complete(request).await?;
    
    println!("Response: {}", response.text());
    
    if let Some(tool_use) = response.tool_use() {
        println!("\nTool requested: {}", tool_use.name);
        println!("Tool arguments: {}", tool_use.input);
        
        // Simulate tool execution
        let weather_result = json!({
            "temperature": 45,
            "unit": "fahrenheit",
            "conditions": "cloudy"
        });
        
        // Add assistant's tool request to history
        messages.push(Message::assistant_with_tool_use(tool_use.clone()));
        
        // Add tool result
        messages.push(Message::tool_result(
            tool_use.id.clone(),
            weather_result.to_string()
        ));
        
        // Get final response
        let request = CompletionRequest {
            messages: messages.clone(),
            options: None,
        };
        
        let final_response = client.complete(request).await?;
        println!("\nFinal response: {}", final_response.text());
    }
    
    Ok(())
}