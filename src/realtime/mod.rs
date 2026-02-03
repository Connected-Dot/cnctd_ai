//! Real-time audio streaming support for AI providers
//!
//! This module provides bidirectional audio streaming capabilities using WebSocket connections.
//! Currently supports OpenAI's Realtime API.
//!
//! # Example
//!
//! ```rust,no_run
//! use cnctd_ai::{Client, OpenAiConfig};
//! use cnctd_ai::realtime::{RealtimeConfig, Modality};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let client = Client::openai(OpenAiConfig {
//!         api_key: std::env::var("OPENAI_API_KEY")?,
//!         model: "gpt-4o".to_string(),
//!         ..Default::default()
//!     })?;
//!
//!     let config = RealtimeConfig::default()
//!         .with_instructions("You are a helpful assistant.");
//!
//!     let mut session = client.connect_realtime(config).await?;
//!
//!     // Send text and trigger a response
//!     session.send_text("Hello!").await?;
//!
//!     // Listen for events
//!     while let Some(event) = session.next_event().await {
//!         match event {
//!             cnctd_ai::realtime::RealtimeEvent::AudioDelta { delta } => {
//!                 // Play audio...
//!             }
//!             cnctd_ai::realtime::RealtimeEvent::TranscriptDone { text } => {
//!                 println!("Assistant: {}", text);
//!             }
//!             _ => {}
//!         }
//!     }
//!
//!     Ok(())
//! }
//! ```

pub mod config;
pub mod openai;

pub use config::{RealtimeConfig, RealtimeAudioFormat, Modality, VadConfig};

use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message as WsMessage;

/// Events received from a realtime session
#[derive(Debug, Clone)]
pub enum RealtimeEvent {
    /// Connection established, session created
    SessionCreated {
        session_id: Option<String>,
    },
    /// Session configuration was updated
    SessionUpdated,
    /// Speech detected in input audio
    SpeechStarted,
    /// Speech ended in input audio
    SpeechStopped,
    /// Audio delta from the model (raw audio bytes)
    AudioDelta {
        delta: Vec<u8>,
    },
    /// Transcript delta (incremental text from speech)
    TranscriptDelta {
        delta: String,
    },
    /// Transcript complete
    TranscriptDone {
        text: String,
    },
    /// Text delta (for text-only responses)
    TextDelta {
        delta: String,
    },
    /// Text complete
    TextDone {
        text: String,
    },
    /// Response generation complete
    ResponseDone,
    /// Error occurred
    Error {
        message: String,
    },
    /// Connection closed
    Disconnected,
}

/// A realtime audio session handle
pub struct RealtimeSession {
    event_rx: mpsc::Receiver<RealtimeEvent>,
    command_tx: mpsc::Sender<WsMessage>,
    session_id: Option<String>,
}

impl RealtimeSession {
    /// Get the next event from the session
    pub async fn next_event(&mut self) -> Option<RealtimeEvent> {
        self.event_rx.recv().await
    }

    /// Send raw audio data to the session
    /// The audio should be in the format specified in the config (default: PCM 16-bit)
    pub async fn send_audio(&self, audio: &[u8]) -> crate::Result<()> {
        let msg = openai::build_audio_append(audio);
        self.command_tx
            .send(WsMessage::Text(msg.to_string().into()))
            .await
            .map_err(|e| crate::error::Error::WebSocketError(format!("Send failed: {}", e)))
    }

    /// Commit the audio buffer and trigger a response
    /// Call this after sending audio chunks to signal the end of speech
    pub async fn commit_audio(&self) -> crate::Result<()> {
        let msg = openai::build_audio_commit();
        self.command_tx
            .send(WsMessage::Text(msg.to_string().into()))
            .await
            .map_err(|e| crate::error::Error::WebSocketError(format!("Send failed: {}", e)))
    }

    /// Send a text message and trigger a response
    pub async fn send_text(&self, text: &str) -> crate::Result<()> {
        // First, add the text as a conversation item
        let item_msg = openai::build_conversation_item(text);
        self.command_tx
            .send(WsMessage::Text(item_msg.to_string().into()))
            .await
            .map_err(|e| crate::error::Error::WebSocketError(format!("Send failed: {}", e)))?;

        // Then trigger a response
        let response_msg = openai::build_response_create(None);
        self.command_tx
            .send(WsMessage::Text(response_msg.to_string().into()))
            .await
            .map_err(|e| crate::error::Error::WebSocketError(format!("Send failed: {}", e)))
    }

    /// Interrupt the current response
    /// Use this when the user starts speaking while the model is responding
    pub async fn interrupt(&self) -> crate::Result<()> {
        let msg = openai::build_response_cancel();
        self.command_tx
            .send(WsMessage::Text(msg.to_string().into()))
            .await
            .map_err(|e| crate::error::Error::WebSocketError(format!("Send failed: {}", e)))
    }

    /// Close the session
    pub async fn close(self) -> crate::Result<()> {
        // Dropping command_tx will cause the write task to exit
        // which will close the WebSocket connection
        drop(self.command_tx);
        Ok(())
    }

    /// Get the session ID if available
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
}
