use std::io::{self, Write};
use anyhow::Result;
use cnctd_ai::{AnthropicConfig, Client, CompletionRequest, Message, OpenAiConfig, GeminiConfig};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    println!("=== Testing Anthropic Streaming ===");
    test_anthropic_streaming().await?;

    println!("\n=== Testing OpenAI Streaming ===");
    test_openai_streaming().await?;

    println!("\n=== Testing Gemini Streaming ===");
    test_gemini_streaming().await?;

    Ok(())
}

async fn test_anthropic_streaming() -> Result<()> {
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
        messages: vec![
            Message::user("Explain AI streaming in one sentence.")
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

async fn test_openai_streaming() -> Result<()> {
    let api_key = std::env::var("OPENAI_API_KEY")?;

    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(),
            organization: None,
        },
        None,
    )?;

    let request = CompletionRequest {
        messages: vec![
            Message::user("Explain AI streaming in one sentence.")
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
            Message::user("Explain AI streaming in one sentence.")
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
