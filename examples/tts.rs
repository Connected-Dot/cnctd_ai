//! Text-to-speech example using OpenAI or Gemini
//!
//! Run with OpenAI:
//! OPENAI_API_KEY=your_key cargo run --example tts -- "Hello, world!"
//!
//! Run with Gemini:
//! GEMINI_API_KEY=your_key cargo run --example tts -- --gemini "Hello, world!"

use cnctd_ai::{Client, GeminiConfig, OpenAiConfig, SpeechRequest, Voice};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Check for --gemini flag
    let use_gemini = args.iter().any(|a| a == "--gemini");

    // Get text from args (skip program name and --gemini flag)
    let text: String = args.iter()
        .skip(1)
        .filter(|a| *a != "--gemini")
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    let text = if text.is_empty() {
        "Hello! This is a test of the text to speech system. Pretty cool, right?".to_string()
    } else {
        text
    };

    println!("=== Text-to-Speech Example ===\n");
    println!("Text: {}\n", text);

    if use_gemini {
        // Use Gemini
        let api_key = std::env::var("GEMINI_API_KEY")
            .expect("GEMINI_API_KEY environment variable must be set");

        let client = Client::gemini(
            GeminiConfig {
                api_key,
                model: "gemini-2.5-flash".to_string(),
            },
            None,
        )?;

        println!("Using Gemini TTS (Puck voice)...\n");

        let request = SpeechRequest::new(&text)
            .with_voice(Voice::Puck);

        match client.generate_speech(request).await {
            Ok(response) => {
                let output_path = "speech_gemini.wav";
                response.save(output_path).await?;
                println!("Audio saved to: {}", output_path);
                println!("Format: {:?}", response.format);
                println!("Size: {} bytes", response.audio.len());
            }
            Err(e) => {
                eprintln!("Error generating speech: {}\n", e);
            }
        }
    } else {
        // Use OpenAI
        let api_key = std::env::var("OPENAI_API_KEY")
            .expect("OPENAI_API_KEY environment variable must be set");

        let client = Client::openai(OpenAiConfig {
            api_key,
            model: "gpt-4o".to_string(),
            organization: None,
            transcription_model: None,
        }, None)?;

        println!("Using OpenAI TTS (Nova voice)...\n");

        let request = SpeechRequest::new(&text)
            .with_voice(Voice::Nova)
            .with_speed(1.0);

        match client.generate_speech(request).await {
            Ok(response) => {
                let output_path = "speech_openai.mp3";
                response.save(output_path).await?;
                println!("Audio saved to: {}", output_path);
                println!("Format: {:?}", response.format);
                println!("Size: {} bytes", response.audio.len());
            }
            Err(e) => {
                eprintln!("Error generating speech: {}\n", e);
            }
        }
    }

    println!("\nDone!");
    Ok(())
}
