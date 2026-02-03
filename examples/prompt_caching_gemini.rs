//! Example: Implicit Caching with Gemini
//!
//! This example demonstrates Gemini's automatic implicit caching behavior.
//! Unlike Anthropic where you must opt-in with `.with_cache()`, Gemini automatically
//! caches content when it meets the minimum token threshold.
//!
//! Run with: cargo run --example prompt_caching_gemini
//!
//! **Key differences from Anthropic:**
//! - Gemini uses "implicit caching" - automatic since May 2025
//! - No explicit API changes needed - caching happens automatically
//! - The `.with_cache()` call is a no-op for Gemini (included for API compatibility)
//! - Gemini reports cached tokens via `cachedContentTokenCount` in responses
//! - Minimum ~2048 tokens for explicit caching (implicit may be lower)
//! - 90% discount on Gemini 2.5, 75% on 2.0 Flash

use std::io::{self, Write};
use anyhow::Result;
use cnctd_ai::{GeminiConfig, Client, CompletionRequest, Message};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    let api_key = std::env::var("GEMINI_API_KEY")?;

    let client = Client::gemini(
        GeminiConfig {
            api_key,
            model: "gemini-2.0-flash".into(),
        },
        None,
    )?;

    // Create a system prompt
    // Gemini automatically caches content that meets its threshold
    // We'll use a substantial prompt to demonstrate the implicit caching
    let system_prompt = r#"
You are an expert AI assistant specializing in Rust programming. You have comprehensive knowledge of the Rust programming language, its ecosystem, and best practices for writing production-quality code.

## Core Expertise

### Memory Safety and Ownership
- Deep understanding of Rust's ownership model and borrowing rules
- Knowledge of lifetimes, smart pointers (Box, Rc, Arc), and interior mutability
- Familiarity with unsafe Rust and when it's appropriate

### Async Programming
- Expertise with async/await syntax and the tokio runtime
- Understanding of futures, streams, and channels
- Knowledge of common async pitfalls

### Error Handling
- Mastery of Result and Option types
- Experience with thiserror and anyhow crates
- Best practices for error propagation

### Popular Crates
- serde for serialization
- reqwest for HTTP
- sqlx for databases
- tracing for logging

## Response Guidelines

1. Provide clear explanations with code examples
2. Highlight common pitfalls
3. Suggest idiomatic patterns
4. Be concise but complete
"#;

    println!("=== Gemini Implicit Caching Example ===\n");
    println!("Note: Gemini caches automatically - with_cache() is a no-op for API compatibility\n");
    println!("Making first request...\n");

    // First request - Gemini may cache automatically
    let request1 = CompletionRequest {
        messages: vec![
            Message::system(system_prompt).with_cache(), // No-op for Gemini, but API-compatible
            Message::user("What is the difference between String and &str in Rust?"),
        ],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: None,
    };

    print!("Q: What is the difference between String and &str in Rust?\n\nA: ");
    io::stdout().flush().unwrap();

    let mut stream = client.complete_stream(request1).await?;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        if let Some(text) = chunk.text() {
            print!("{}", text);
            io::stdout().flush().unwrap();
        }
    }
    println!("\n");

    if let Some(response) = stream.final_response() {
        println!("--- First Request Stats ---");
        println!("Prompt tokens: {}", response.usage.prompt_tokens);
        println!("Completion tokens: {}", response.usage.completion_tokens);
        if let Some(created) = response.usage.cache_creation_tokens {
            println!("Cache creation tokens: {} (wrote to cache)", created);
        }
        if let Some(read) = response.usage.cache_read_tokens {
            println!("Cache read tokens: {} (read from cache)", read);
        }
        println!("Used cache: {}", response.usage.used_cache());
    }

    println!("\n---\n");
    println!("Making second request (may show cached tokens if implicit caching activated)...\n");

    // Second request - may read from implicit cache
    let request2 = CompletionRequest {
        messages: vec![
            Message::system(system_prompt).with_cache(), // No-op for Gemini, included for API compatibility
            Message::user("How do I handle errors idiomatically in Rust?"),
        ],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: None,
    };

    print!("Q: How do I handle errors idiomatically in Rust?\n\nA: ");
    io::stdout().flush().unwrap();

    let mut stream2 = client.complete_stream(request2).await?;
    while let Some(chunk) = stream2.next().await {
        let chunk = chunk?;
        if let Some(text) = chunk.text() {
            print!("{}", text);
            io::stdout().flush().unwrap();
        }
    }
    println!("\n");

    if let Some(response) = stream2.final_response() {
        println!("--- Second Request Stats ---");
        println!("Prompt tokens: {}", response.usage.prompt_tokens);
        println!("Completion tokens: {}", response.usage.completion_tokens);
        if let Some(created) = response.usage.cache_creation_tokens {
            println!("Cache creation tokens: {} (wrote to cache)", created);
        }
        if let Some(read) = response.usage.cache_read_tokens {
            println!("Cache read tokens: {} (read from cache)", read);
        }
        println!("Used cache: {}", response.usage.used_cache());
        println!("Effective prompt tokens (non-cached): {}", response.usage.effective_prompt_tokens());
    }

    println!("\n=== Done ===");
    println!("\nNote: Gemini uses implicit caching (automatic since May 2025).");
    println!("- 90% discount on Gemini 2.5 models, 75% on Gemini 2.0");
    println!("- Caching happens automatically when content meets the token threshold.");
    println!("- The with_cache() call is a no-op for Gemini (included for API compatibility).");
    println!("- If cache_read_tokens shows 0, the content may be below the caching threshold.");

    Ok(())
}
