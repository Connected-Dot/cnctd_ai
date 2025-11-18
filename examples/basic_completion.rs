use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    println!("=== Testing Anthropic ===");
    test_anthropic().await?;
    
    println!("\n=== Testing OpenAI ===");
    test_openai().await?;
    
    Ok(())
}

async fn test_anthropic() -> Result<()> {
    use cnctd_ai::{Client, AnthropicConfig, Message, CompletionRequest};
    
    let api_key = std::env::var("ANTHROPIC_API_KEY")?;
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None, // use default ClientOptions
    )?;
    
    let request = CompletionRequest {
        messages: vec![Message::user("What is Rust?")],
        tools: None,
        options: None,
    };
    
    let response = client.complete(request).await?;
    
    println!("Model: {}", response.model);
    println!("Response: {}", response.text());
    println!("Usage: {} prompt + {} completion = {} total tokens",
        response.usage.prompt_tokens,
        response.usage.completion_tokens,
        response.usage.total_tokens
    );
    println!("Finish reason: {:?}", response.finish_reason);
    
    Ok(())
}

async fn test_openai() -> Result<()> {
    use cnctd_ai::{Client, OpenAiConfig, Message, CompletionRequest};
    
    let api_key = std::env::var("OPENAI_API_KEY")?;
    
    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(),
            organization: None,
        },
        None, // use default ClientOptions
    )?;
    
    let request = CompletionRequest {
        messages: vec![Message::user("What is Rust?")],
        tools: None,
        options: None,
    };
    
    let response = client.complete(request).await?;
    
    println!("Model: {}", response.model);
    println!("Response: {}", response.text());
    println!("Usage: {} prompt + {} completion = {} total tokens",
        response.usage.prompt_tokens,
        response.usage.completion_tokens,
        response.usage.total_tokens
    );
    println!("Finish reason: {:?}", response.finish_reason);
    
    Ok(())
}