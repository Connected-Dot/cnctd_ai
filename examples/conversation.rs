use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    println!("=== Testing Anthropic Conversation ===");
    test_anthropic_conversation().await?;
    
    println!("\n=== Testing OpenAI Conversation ===");
    test_openai_conversation().await?;
    
    Ok(())
}

async fn test_anthropic_conversation() -> Result<()> {
    use cnctd_ai::{Client, AnthropicConfig, Message, CompletionRequest};
    
    let api_key = std::env::var("ANTHROPIC_API_KEY")?;
    
    let client = Client::anthropic(
        AnthropicConfig {
            api_key,
            model: "claude-sonnet-4-20250514".into(),
            version: None,
        },
        None,
    )?;
    
    // Start conversation history with a system message
    let mut messages = vec![
        Message::system("You are a helpful assistant that specializes in programming languages."),
    ];
    
    // First turn: Ask about Rust
    messages.push(Message::user("What is Rust in one sentence?"));
    
    let request = CompletionRequest {
        messages: messages.clone(),
        options: None,
    };
    
    let response = client.complete(request).await?;
    println!("User: What is Rust in one sentence?");
    println!("Assistant: {}", response.text());
    println!("(Used {} tokens)\n", response.usage.total_tokens);
    
    // Add assistant's response to history
    messages.push(Message::assistant(&*response.text()));
    
    // Second turn: Follow-up question
    messages.push(Message::user("What makes it different from C++?"));
    
    let request = CompletionRequest {
        messages: messages.clone(),
        options: None,
    };
    
    let response = client.complete(request).await?;
    println!("User: What makes it different from C++?");
    println!("Assistant: {}", response.text());
    println!("(Used {} tokens)\n", response.usage.total_tokens);
    
    // Add assistant's response to history
    messages.push(Message::assistant(&*response.text()));
    
    // Third turn: Another follow-up
    messages.push(Message::user("Give me a simple code example"));
    
    let request = CompletionRequest {
        messages: messages.clone(),
        options: None,
    };
    
    let response = client.complete(request).await?;
    println!("User: Give me a simple code example");
    println!("Assistant: {}", response.text());
    println!("(Used {} tokens)", response.usage.total_tokens);
    
    Ok(())
}

async fn test_openai_conversation() -> Result<()> {
    use cnctd_ai::{Client, OpenAiConfig, Message, CompletionRequest};
    
    let api_key = std::env::var("OPENAI_API_KEY")?;
    
    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(),
            organization: None,
        },
        None,
    )?;
    
    // Start conversation history with a system message
    let mut messages = vec![
        Message::system("You are a helpful assistant that specializes in programming languages."),
    ];
    
    // First turn: Ask about Python
    messages.push(Message::user("What is Python in one sentence?"));
    
    let request = CompletionRequest {
        messages: messages.clone(),
        options: None,
    };
    
    let response = client.complete(request).await?;
    println!("User: What is Python in one sentence?");
    println!("Assistant: {}", response.text());
    println!("(Used {} tokens)\n", response.usage.total_tokens);
    
    // Add assistant's response to history
    messages.push(Message::assistant(&*response.text()));
    
    // Second turn: Follow-up question
    messages.push(Message::user("What makes it popular for beginners?"));
    
    let request = CompletionRequest {
        messages: messages.clone(),
        options: None,
    };
    
    let response = client.complete(request).await?;
    println!("User: What makes it popular for beginners?");
    println!("Assistant: {}", response.text());
    println!("(Used {} tokens)\n", response.usage.total_tokens);
    
    // Add assistant's response to history
    messages.push(Message::assistant(&*response.text()));
    
    // Third turn: Another follow-up
    messages.push(Message::user("Show me Hello World in Python"));
    
    let request = CompletionRequest {
        messages: messages.clone(),
        options: None,
    };
    
    let response = client.complete(request).await?;
    println!("User: Show me Hello World in Python");
    println!("Assistant: {}", response.text());
    println!("(Used {} tokens)", response.usage.total_tokens);
    
    Ok(())
}