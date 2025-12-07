//! Example demonstrating Gemini's built-in tools:
//! - Code Execution (Python with numpy, pandas, etc.)
//! - URL Context (fetch and analyze webpages)
//! - Google Maps (location-aware queries)
//!
//! Run with: GEMINI_API_KEY=your_key cargo run --example gemini_tools

use cnctd_ai::{Client, CompletionRequest, GeminiConfig, Message};
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = env::var("GEMINI_API_KEY")
        .expect("GEMINI_API_KEY environment variable must be set");

    // Create Gemini client with 2.0 Flash model
    let client = Client::gemini(
        GeminiConfig {
            api_key,
            model: "gemini-2.0-flash".to_string(),
        },
        None,
    )?;

    // =======================================================================
    // Example 1: Code Execution
    // =======================================================================
    println!("=== Code Execution Example ===\n");

    let code_request = CompletionRequest {
        messages: vec![
            Message::user("Calculate the first 20 Fibonacci numbers and show them in a formatted list. Also compute their sum."),
        ],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: None,
    }.with_code_execution();

    let code_response = client.complete(code_request).await?;
    println!("Response:\n{}\n", code_response.text());

    if let Some(code_results) = code_response.code_results() {
        println!("Code Execution Results:");
        for (i, result) in code_results.iter().enumerate() {
            println!("  [{}] Language: {:?}", i + 1, result.language);
            if let Some(code) = &result.code {
                println!("      Code: {}", code.lines().next().unwrap_or(""));
                if code.lines().count() > 1 {
                    println!("      ... ({} more lines)", code.lines().count() - 1);
                }
            }
            println!("      Outcome: {:?}", result.outcome);
            if let Some(output) = &result.output {
                let preview: String = output.chars().take(200).collect();
                println!("      Output: {}{}", preview, if output.len() > 200 { "..." } else { "" });
            }
        }
    }

    println!("\n");

    // =======================================================================
    // Example 2: URL Context
    // =======================================================================
    println!("=== URL Context Example ===\n");

    let url_request = CompletionRequest {
        messages: vec![
            Message::user("Summarize the key features described on https://ai.google.dev/gemini-api/docs/tools"),
        ],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: None,
    }.with_url_context();

    let url_response = client.complete(url_request).await?;
    println!("Response:\n{}\n", url_response.text());

    // =======================================================================
    // Example 3: Google Maps with location
    // =======================================================================
    println!("=== Google Maps Example ===\n");

    // NYC coordinates (Times Square area)
    let maps_request = CompletionRequest {
        messages: vec![
            Message::user("What are some highly-rated coffee shops near me with outdoor seating?"),
        ],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: None,
    }
    .with_google_maps(Some(true))  // Enable widget
    .with_location(40.758896, -73.985130);  // Times Square

    let maps_response = client.complete(maps_request).await?;
    println!("Response:\n{}\n", maps_response.text());

    if maps_response.has_maps_widget() {
        println!("Maps Widget Token: Available (can be used to render interactive widget)");
    }

    // Check for grounding metadata (Maps uses similar structure to Search)
    if maps_response.is_grounded() {
        if let Some(sources) = maps_response.sources() {
            println!("\nSources:");
            for (i, chunk) in sources.iter().enumerate() {
                if let Some(web) = &chunk.web {
                    println!("  [{}] {} - {}", 
                        i + 1,
                        web.title.as_deref().unwrap_or("Untitled"),
                        web.uri.as_deref().unwrap_or("")
                    );
                }
            }
        }
    }

    // =======================================================================
    // Example 4: Combined tools (Search + Code)
    // =======================================================================
    println!("\n=== Combined Tools Example (Search + Code) ===\n");

    let combined_request = CompletionRequest {
        messages: vec![
            Message::user("Search for the current population of the top 5 most populous countries, then create a bar chart showing this data."),
        ],
        tools: None,
        built_in_tools: None,
        tool_config: None,
        options: None,
    }
    .with_google_search()
    .with_code_execution();

    let combined_response = client.complete(combined_request).await?;
    println!("Response:\n{}\n", combined_response.text());

    if combined_response.is_grounded() {
        println!("Search queries used: {:?}", combined_response.search_queries());
    }

    if combined_response.has_code_execution() {
        println!("Code was executed to generate visualization");
    }

    println!("\n=== All examples complete ===");

    Ok(())
}
