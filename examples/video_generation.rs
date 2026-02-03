//! Video generation example using Gemini Veo or OpenAI Sora
//!
//! Run with Gemini (Veo):
//! GEMINI_API_KEY=your_key cargo run --example video_generation -- "A drone shot over mountains"
//!
//! Run with OpenAI (Sora):
//! OPENAI_API_KEY=your_key cargo run --example video_generation -- --openai "A drone shot over mountains"

use cnctd_ai::{Client, GeminiConfig, OpenAiConfig, VideoGenerationRequest};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    // Check for --openai flag
    let use_openai = args.iter().any(|a| a == "--openai");

    // Get prompt from args
    let prompt: String = args.iter()
        .skip(1)
        .filter(|a| *a != "--openai")
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    let prompt = if prompt.is_empty() {
        "A cinematic drone shot flying over a tropical beach at golden hour, waves gently crashing, palm trees swaying in the breeze".to_string()
    } else {
        prompt
    };

    println!("=== Video Generation Example ===\n");
    println!("Prompt: {}\n", prompt);

    if use_openai {
        // Use OpenAI Sora
        let api_key = std::env::var("OPENAI_API_KEY")
            .expect("OPENAI_API_KEY environment variable must be set");

        let client = Client::openai(OpenAiConfig {
            api_key,
            model: "gpt-4o".to_string(),
            organization: None,
            transcription_model: None,
        }, None)?;

        println!("Using OpenAI Sora (sora-2)...\n");

        let request = VideoGenerationRequest::new(&prompt)
            .medium()  // 8 seconds
            .landscape();

        println!("Starting video generation...");
        let mut job = client.generate_video(request).await?;
        println!("Job ID: {}", job.id);

        // Poll for completion
        loop {
            job = client.poll_video_status(&job).await?;

            match &job.status {
                cnctd_ai::VideoGenerationStatus::Queued => {
                    println!("Status: Queued...");
                }
                cnctd_ai::VideoGenerationStatus::InProgress { progress } => {
                    if let Some(p) = progress {
                        println!("Status: In progress ({:.0}%)...", p * 100.0);
                    } else {
                        println!("Status: In progress...");
                    }
                }
                cnctd_ai::VideoGenerationStatus::Completed => {
                    println!("Status: Completed!");
                    break;
                }
                cnctd_ai::VideoGenerationStatus::Failed { error } => {
                    eprintln!("Generation failed: {}", error);
                    return Ok(());
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        // Download the video
        println!("\nDownloading video...");
        let response = client.download_video(&job).await?;

        let output_path = "generated_video_openai.mp4";
        response.save(output_path).await?;
        println!("Video saved to: {}", output_path);
        println!("Size: {} bytes", response.video.len());
    } else {
        // Use Gemini Veo
        let api_key = std::env::var("GEMINI_API_KEY")
            .expect("GEMINI_API_KEY environment variable must be set");

        let client = Client::gemini(
            GeminiConfig {
                api_key,
                model: "gemini-2.5-flash".to_string(),
            },
            None,
        )?;

        println!("Using Gemini Veo (veo-3.1-generate-preview)...\n");

        let request = VideoGenerationRequest::new(&prompt)
            .medium()    // 8 seconds
            .landscape()
            .hd();       // 1080p

        println!("Starting video generation...");
        let mut job = client.generate_video(request).await?;
        println!("Job ID: {}", job.id);

        // Poll for completion
        loop {
            job = client.poll_video_status(&job).await?;

            match &job.status {
                cnctd_ai::VideoGenerationStatus::Queued => {
                    println!("Status: Queued...");
                }
                cnctd_ai::VideoGenerationStatus::InProgress { progress } => {
                    if let Some(p) = progress {
                        println!("Status: In progress ({:.0}%)...", p * 100.0);
                    } else {
                        println!("Status: In progress...");
                    }
                }
                cnctd_ai::VideoGenerationStatus::Completed => {
                    println!("Status: Completed!");
                    break;
                }
                cnctd_ai::VideoGenerationStatus::Failed { error } => {
                    eprintln!("Generation failed: {}", error);
                    return Ok(());
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }

        // Download the video
        println!("\nDownloading video...");
        let response = client.download_video(&job).await?;

        let output_path = "generated_video_gemini.mp4";
        response.save(output_path).await?;
        println!("Video saved to: {}", output_path);
        println!("Size: {} bytes", response.video.len());
        if let Some(duration) = response.duration_seconds {
            println!("Duration: {:.1}s", duration);
        }
    }

    println!("\nDone!");
    Ok(())
}
