use anyhow::Result;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    println!("=== Testing Anthropic Streaming Tool Calling ===");
    test_anthropic_streaming_tools().await?;
    
    println!("\n=== Testing OpenAI Streaming Tool Calling ===");
    test_openai_streaming_tools().await?;
    
    Ok(())
}

async fn test_anthropic_streaming_tools() -> Result<()> {
    use cnctd_ai::{Client, AnthropicConfig, Message, CompletionRequest, create_tool};
    
    let api_key = std::env::var("ANTHROPIC_API_KEY")?;
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    )?;
    
    // Define a simple weather tool using the helper function
    let weather_tool = create_tool(
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
    )?;
    
    let mut messages = vec![
        Message::user("What's the weather like in San Francisco?")
    ];
    
    let mut request = CompletionRequest {
        messages: messages.clone(),
        tools: None,
        options: None,
    };
    request.add_tool(weather_tool.clone());
    
    // First streaming request - expect tool use
    let mut stream = client.complete_stream(request).await?;
    
    print!("Streaming response: ");
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(text) = chunk.text() {
            print!("{}", text);
        }
    }
    println!();
    
    // Check if we got a tool use request
    if let Some(tool_use) = stream.tool_use() {
        println!("\nTool requested: {}", tool_use.name);
        println!("Tool arguments: {}", tool_use.input);
        
        // Simulate tool execution
        let weather_result = json!({
            "temperature": 68,
            "unit": "fahrenheit",
            "conditions": "partly cloudy"
        });
        
        // Add assistant's tool request to history
        messages.push(Message::assistant_with_tool_use(tool_use.clone()));
        
        // Add tool result
        messages.push(Message::tool_result(
            tool_use.id.clone(),
            weather_result.to_string()
        ));
        
        // Second streaming request - get final answer
        let request = CompletionRequest {
            messages: messages.clone(),
            tools: None,
            options: None,
        };
        
        let mut final_stream = client.complete_stream(request).await?;
        
        print!("\nFinal streaming response: ");
        while let Some(chunk) = final_stream.next().await {
            let chunk = chunk?;
            if let Some(text) = chunk.text() {
                print!("{}", text);
            }
        }
        println!();
        
        // Show usage stats
        if let Some(response) = final_stream.final_response() {
            println!("(Used {} tokens)", response.usage.total_tokens);
        }
    }
    
    Ok(())
}

async fn test_openai_streaming_tools() -> Result<()> {
    use cnctd_ai::{Client, OpenAiConfig, Message, CompletionRequest, create_tool};
    
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
    let weather_tool = create_tool(
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
    )?;
    
    let mut messages = vec![
        Message::user("What's the weather like in New York?")
    ];
    
    let mut request = CompletionRequest {
        messages: messages.clone(),
        tools: None,
        options: None,
    };
    request.add_tool(weather_tool.clone());
    
    // First streaming request - expect tool use
    let mut stream = client.complete_stream(request).await?;
    
    print!("Streaming response: ");
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(text) = chunk.text() {
            print!("{}", text);
        }
    }
    println!();
    
    // Check if we got a tool use request
    if let Some(tool_use) = stream.tool_use() {
        println!("\nTool requested: {}", tool_use.name);
        println!("Tool arguments: {}", tool_use.input);
        
        // Simulate tool execution
        let weather_result = json!({
            "temperature": 42,
            "unit": "fahrenheit",
            "conditions": "rainy"
        });
        
        // Add assistant's tool request to history
        messages.push(Message::assistant_with_tool_use(tool_use.clone()));
        
        // Add tool result
        messages.push(Message::tool_result(
            tool_use.id.clone(),
            weather_result.to_string()
        ));
        
        // Second streaming request - get final answer
        let request = CompletionRequest {
            messages: messages.clone(),
            tools: None,
            options: None,
        };
        
        let mut final_stream = client.complete_stream(request).await?;
        
        print!("\nFinal streaming response: ");
        while let Some(chunk) = final_stream.next().await {
            let chunk = chunk?;
            if let Some(text) = chunk.text() {
                print!("{}", text);
            }
        }
        println!();
        
        // Show usage stats
        if let Some(response) = final_stream.final_response() {
            println!("(Used {} tokens)", response.usage.total_tokens);
        }
    }
    
    Ok(())
}
