//! Image generation example using Gemini's Nano Banana model
//!
//! Run with:
//! GEMINI_API_KEY=your_key cargo run --example image_generation -- "your prompt here"

use cnctd_ai::{Client, GeminiConfig, ImageGenerationRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get prompt from command line args
    let args: Vec<String> = std::env::args().collect();
    let prompt = if args.len() > 1 {
        args[1..].join(" ")
    } else {
        "A serene Japanese garden with a koi pond, cherry blossoms falling, and a traditional wooden bridge in soft morning light".to_string()
    };

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

    println!("=== Generating Image with Gemini (Nano Banana) ===\n");

    // Generate an image
    let request = ImageGenerationRequest::new(&prompt)
        .landscape()
        .high_quality();

    println!("Prompt: {}\n", request.prompt);
    println!("Generating image...\n");

    match client.generate_image(request).await {
        Ok(response) => {
            if let Some(image) = response.first() {
                // Save the image
                let output_path = "generated_image.png";
                image.save(output_path).await?;
                println!("Image saved to: {}", output_path);
                println!("MIME type: {}", image.mime_type);
                println!("Data size: {} bytes (base64)", image.data.len());
            } else {
                println!("No images generated");
            }
        }
        Err(e) => {
            eprintln!("Error generating image: {}\n", e);
        }
    }

    println!("\nDone!");
    Ok(())
}
