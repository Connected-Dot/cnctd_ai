//! Example: Real-time Audio with OpenAI
//!
//! This example demonstrates how to use OpenAI's Realtime API for
//! bidirectional audio streaming conversations.
//!
//! Note: This requires access to OpenAI's Realtime API (gpt-4o-realtime-preview)
//!
//! Run with: cargo run --example realtime_audio

use anyhow::Result;
use cnctd_ai::{Client, OpenAiConfig};
use cnctd_ai::realtime::{RealtimeConfig, RealtimeEvent, Modality};
use cnctd_ai::tts::Voice;

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();

    println!("=== Real-time Audio Example ===\n");

    let api_key = std::env::var("OPENAI_API_KEY")?;

    let client = Client::openai(
        OpenAiConfig {
            api_key,
            model: "gpt-4o".into(), // Base model, realtime uses gpt-4o-realtime-preview
            ..Default::default()
        },
        None,
    )?;

    // Configure the realtime session
    let config = RealtimeConfig::new("gpt-4o-realtime-preview")
        .with_voice(Voice::Alloy)  // Note: Nova not supported by Realtime API
        .with_modalities(vec![Modality::Text, Modality::Audio])
        .with_instructions("You are a helpful and friendly assistant. Keep responses brief and conversational.");

    println!("Connecting to OpenAI Realtime API...");
    println!("Model: gpt-4o-realtime-preview");
    println!("Voice: Alloy");
    println!();

    // Connect to the realtime session
    let mut session = match client.connect_realtime(config).await {
        Ok(s) => s,
        Err(e) => {
            println!("Failed to connect: {}", e);
            println!("\nNote: This example requires access to OpenAI's Realtime API.");
            println!("Make sure you have the correct API key and model access.");
            return Ok(());
        }
    };

    println!("Connected! Sending a text message...\n");

    // Send a text message (simulating what you might say)
    session.send_text("Hello! What's the weather like today?").await?;

    // Listen for events
    let mut audio_bytes_received = 0usize;
    let mut transcript = String::new();

    println!("Waiting for response...\n");

    // Set a timeout for the demo
    let timeout = tokio::time::Duration::from_secs(30);
    let start = std::time::Instant::now();

    loop {
        if start.elapsed() > timeout {
            println!("\nTimeout reached.");
            break;
        }

        // Use a short timeout for each event to allow checking the overall timeout
        let event = tokio::time::timeout(
            tokio::time::Duration::from_millis(100),
            session.next_event()
        ).await;

        match event {
            Ok(Some(event)) => {
                match event {
                    RealtimeEvent::SessionCreated { session_id } => {
                        println!("Session created: {:?}", session_id);
                    }
                    RealtimeEvent::SessionUpdated => {
                        println!("Session configuration updated");
                    }
                    RealtimeEvent::SpeechStarted => {
                        println!("[Speech detected in input]");
                    }
                    RealtimeEvent::SpeechStopped => {
                        println!("[Speech ended in input]");
                    }
                    RealtimeEvent::AudioDelta { delta } => {
                        audio_bytes_received += delta.len();
                        // In a real app, you'd play this audio
                        print!(".");
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                    RealtimeEvent::TranscriptDelta { delta } => {
                        transcript.push_str(&delta);
                    }
                    RealtimeEvent::TranscriptDone { text } => {
                        println!("\n\nAssistant transcript: {}", text);
                    }
                    RealtimeEvent::TextDelta { delta } => {
                        print!("{}", delta);
                        std::io::Write::flush(&mut std::io::stdout()).unwrap();
                    }
                    RealtimeEvent::TextDone { text } => {
                        println!("\n\nAssistant (text): {}", text);
                    }
                    RealtimeEvent::ResponseDone => {
                        println!("\n--- Response complete ---");
                        println!("Audio received: {} bytes", audio_bytes_received);
                        break;
                    }
                    RealtimeEvent::Error { message } => {
                        println!("\nError: {}", message);
                        break;
                    }
                    RealtimeEvent::Disconnected => {
                        println!("\nDisconnected from server");
                        break;
                    }
                }
            }
            Ok(None) => {
                println!("Connection closed");
                break;
            }
            Err(_) => {
                // Timeout on this event, continue waiting
                continue;
            }
        }
    }

    // Clean up
    println!("\nClosing session...");
    session.close().await?;

    println!("\n=== Example Complete ===");
    println!("\nIn a real application, you would:");
    println!("1. Capture microphone audio and send via session.send_audio()");
    println!("2. Play received audio bytes through speakers");
    println!("3. Handle interruptions when user starts speaking");

    Ok(())
}

/// Example of how to send audio (not runnable without audio capture)
#[allow(dead_code)]
async fn send_audio_example(session: &cnctd_ai::realtime::RealtimeSession) -> Result<()> {
    // In a real app, you'd capture audio from a microphone
    // The audio should be PCM 16-bit by default

    // Simulated audio chunk (would come from microphone)
    let audio_chunk: Vec<u8> = vec![0u8; 1024]; // Placeholder

    // Send audio to the session
    session.send_audio(&audio_chunk).await?;

    // When the user stops speaking (detected via VAD or manually):
    session.commit_audio().await?;

    Ok(())
}

/// Example of interruption handling
#[allow(dead_code)]
async fn handle_interruption(session: &cnctd_ai::realtime::RealtimeSession) -> Result<()> {
    // If the model is speaking and the user starts talking,
    // you can interrupt the current response:
    session.interrupt().await?;

    Ok(())
}
