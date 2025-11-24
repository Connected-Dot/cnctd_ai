//! Basic Gemini example demonstrating completion and streaming
//! 
//! Set your GEMINI_API_KEY environment variable before running:
//! ```
//! export GEMINI_API_KEY=your_api_key_here
//! cargo run --example gemini_basic
//! ```

use std::io::{self, Write};
use anyhow::Result;
use cnctd_ai::{Client, GeminiConfig, Message, CompletionRequest};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    
    println!("=== Testing Gemini Completion ===");
    test_gemini_completion().await?;
    
    println!("\n=== Testing Gemini Streaming ===");
    test_gemini_streaming().await?;
    
    Ok(())
}

async fn test_gemini_completion() -> Result<()> {
    let api_key = std::env::var("GEMINI_API_KEY")?;
    
    let client = Client::gemini(
        GeminiConfig {
            api_key,
            model: "gemini-2.0-flash".into(),
        },
        None, // use default ClientOptions
    )?;
    
    let request = CompletionRequest {
        messages: vec![
            Message::system("You are a helpful assistant that gives concise answers."),
            Message::user("What is Rust programming language?"),
        ],
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

async fn test_gemini_streaming() -> Result<()> {
    let api_key = std::env::var("GEMINI_API_KEY")?;
    
    let client = Client::gemini(
        GeminiConfig {
            api_key,
            model: "gemini-2.0-flash".into(),
        },
        None,
    )?;
    
    let request = CompletionRequest {
        messages: vec![
            Message::user("Explain AI streaming in one sentence."),
        ],
        tools: None,
        options: None,
    };
    
    print!("Assistant: ");
    io::stdout().flush().unwrap();
    
    let mut stream = client.complete_stream(request).await?;
    
    // Stream the response chunks
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(text) = chunk.text() {
            print!("{}", text);
            io::stdout().flush().unwrap();
        }
    }
    
    println!("\n");
    
    // Display final metadata
    if let Some(final_response) = stream.final_response() {
        println!("---");
        println!("Model: {}", final_response.model);
        println!("Finish Reason: {:?}", final_response.finish_reason);
        println!("Usage:");
        println!("  Prompt tokens: {}", final_response.usage.prompt_tokens);
        println!("  Completion tokens: {}", final_response.usage.completion_tokens);
        println!("  Total tokens: {}", final_response.usage.total_tokens);
    }
    
    Ok(())
}
