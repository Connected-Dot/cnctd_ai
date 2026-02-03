//! Video analysis example using Gemini's native video support
//!
//! Run with:
//! GEMINI_API_KEY=your_key cargo run --example video_analysis

use cnctd_ai::{Client, GeminiConfig, VideoAnalysisRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get API key from environment
    let api_key = std::env::var("GEMINI_API_KEY")
        .expect("GEMINI_API_KEY environment variable must be set");

    // Create Gemini client
    let client = Client::gemini(
        GeminiConfig {
            api_key,
            model: "gemini-2.5-flash".to_string(),
        },
        None,
    )?;

    // Test with local video file (47MB - will use Files API upload)
    println!("=== Analyzing Local Video File (via Files API) ===\n");
    println!("Uploading 47MB video file...\n");

    let request = VideoAnalysisRequest::new(
        "/Users/kyleebner/Library/CloudStorage/GoogleDrive-snugglebmusic@gmail.com/My Drive/colin and kyle/Colin and Kyle- Jobless in Jersey City -- Ep1.mp4",
        "Describe what happens in this video. Who are the people and what are they talking about?"
    );

    match client.analyze_video(request).await {
        Ok(response) => {
            println!("Analysis:\n{}\n", response.text);
        }
        Err(e) => {
            eprintln!("Error analyzing local video: {}\n", e);
        }
    }

    println!("Done!");
    Ok(())
}
