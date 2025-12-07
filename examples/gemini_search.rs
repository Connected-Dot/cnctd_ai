//! Example demonstrating Gemini Google Search grounding
//!
//! This example shows how to use the built-in Google Search tool
//! with Gemini models to get grounded, up-to-date responses.
//!
//! Run with: GEMINI_API_KEY=your_key cargo run --example gemini_search

use cnctd_ai::{
    Client, GeminiConfig, CompletionRequest,
    Message, RequestOptions,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create Gemini client
    dotenvy::dotenv().ok();
    let api_key = std::env::var("GEMINI_API_KEY")
        .expect("GEMINI_API_KEY environment variable required");

    let client = Client::gemini(
        GeminiConfig {
            api_key,
            model: "gemini-2.5-flash".to_string(),
        },
        None,
    )?;

    // Create request with Google Search grounding enabled
    let request = CompletionRequest {
        messages: vec![
            Message::user("What are the latest developments in AI that happened this week?"),
        ],
        tools: None,
        built_in_tools: None,
        options: Some(RequestOptions {
            max_tokens: Some(1024),
            ..Default::default()
        }),
    }.with_google_search(); // Enable Google Search grounding

    println!("Sending request with Google Search grounding...\n");

    let response = client.complete(request).await?;

    println!("Response:");
    println!("{}\n", response.text());

    // Check if the response was grounded
    if response.is_grounded() {
        println!("--- Grounding Information ---");

        // Print search queries that were executed
        if let Some(queries) = response.search_queries() {
            println!("\nSearch queries used:");
            for query in queries {
                println!("  - {}", query);
            }
        }

        // Print sources/citations
        if let Some(sources) = response.sources() {
            println!("\nSources:");
            for (i, chunk) in sources.iter().enumerate() {
                if let Some(web) = &chunk.web {
                    println!(
                        "  {}. {} - {}",
                        i + 1,
                        web.title.as_deref().unwrap_or("(no title)"),
                        web.uri.as_deref().unwrap_or("(no url)")
                    );
                }
            }
        }
    } else {
        println!("Response was not grounded (model answered from its knowledge)");
    }

    println!("\n--- Usage ---");
    println!("Prompt tokens: {}", response.usage.prompt_tokens);
    println!("Completion tokens: {}", response.usage.completion_tokens);
    println!("Total tokens: {}", response.usage.total_tokens);

    Ok(())
}
