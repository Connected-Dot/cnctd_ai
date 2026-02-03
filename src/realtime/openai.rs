use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async_with_config, tungstenite::Message as WsMessage};
use tokio_tungstenite::tungstenite::http;

use crate::client::OpenAiConfig;
use crate::error::{Error, Result};
use super::config::{RealtimeConfig, Modality};
use super::{RealtimeEvent, RealtimeSession};

const OPENAI_REALTIME_URL: &str = "wss://api.openai.com/v1/realtime";

/// Connect to OpenAI's Realtime API
pub async fn connect(
    openai_config: &OpenAiConfig,
    realtime_config: RealtimeConfig,
) -> Result<RealtimeSession> {
    // Build WebSocket URL with model parameter
    let url = format!("{}?model={}", OPENAI_REALTIME_URL, realtime_config.model);

    // Build request with authentication headers using tungstenite's http re-export
    let request = http::Request::builder()
        .uri(&url)
        .header("Authorization", format!("Bearer {}", openai_config.api_key))
        .header("OpenAI-Beta", "realtime=v1")
        .header("Sec-WebSocket-Key", generate_websocket_key())
        .header("Sec-WebSocket-Version", "13")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Host", "api.openai.com")
        .body(())
        .map_err(|e| Error::WebSocketError(format!("Failed to build request: {}", e)))?;

    // Connect to WebSocket
    let (ws_stream, _response) = connect_async_with_config(request, None, false).await
        .map_err(|e| Error::WebSocketError(format!("Connection failed: {}", e)))?;

    let (write, read) = ws_stream.split();

    // Create channels for communication
    let (event_tx, event_rx) = mpsc::channel::<RealtimeEvent>(100);
    let (command_tx, command_rx) = mpsc::channel::<WsMessage>(100);

    // Spawn task to handle incoming messages
    let event_tx_clone = event_tx.clone();
    tokio::spawn(async move {
        handle_incoming(read, event_tx_clone).await;
    });

    // Spawn task to handle outgoing messages
    tokio::spawn(async move {
        handle_outgoing(write, command_rx).await;
    });

    // Send session configuration
    let session_config = build_session_config(&realtime_config);
    command_tx.send(WsMessage::Text(session_config.to_string().into()))
        .await
        .map_err(|e| Error::WebSocketError(format!("Failed to send config: {}", e)))?;

    Ok(RealtimeSession {
        event_rx,
        command_tx,
        session_id: None,
    })
}

/// Handle incoming WebSocket messages
async fn handle_incoming(
    mut read: futures::stream::SplitStream<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>>,
    event_tx: mpsc::Sender<RealtimeEvent>,
) {
    while let Some(msg) = read.next().await {
        match msg {
            Ok(WsMessage::Text(text)) => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Some(event) = parse_event(&json) {
                        if event_tx.send(event).await.is_err() {
                            break; // Receiver dropped
                        }
                    }
                }
            }
            Ok(WsMessage::Close(_)) => {
                let _ = event_tx.send(RealtimeEvent::Disconnected).await;
                break;
            }
            Err(e) => {
                let _ = event_tx.send(RealtimeEvent::Error {
                    message: e.to_string(),
                }).await;
                break;
            }
            _ => {} // Ignore ping/pong/binary
        }
    }
}

/// Handle outgoing WebSocket messages
async fn handle_outgoing(
    mut write: futures::stream::SplitSink<tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>, WsMessage>,
    mut command_rx: mpsc::Receiver<WsMessage>,
) {
    while let Some(msg) = command_rx.recv().await {
        if write.send(msg).await.is_err() {
            break;
        }
    }
}

/// Parse a server event into a RealtimeEvent
fn parse_event(json: &serde_json::Value) -> Option<RealtimeEvent> {
    let event_type = json.get("type")?.as_str()?;

    match event_type {
        "session.created" => {
            let session_id = json.get("session")
                .and_then(|s| s.get("id"))
                .and_then(|id| id.as_str())
                .map(|s| s.to_string());
            Some(RealtimeEvent::SessionCreated { session_id })
        }
        "session.updated" => {
            Some(RealtimeEvent::SessionUpdated)
        }
        "response.audio.delta" => {
            let delta = json.get("delta")?.as_str()?;
            // Decode base64 audio data
            let audio_data = base64::Engine::decode(
                &base64::engine::general_purpose::STANDARD,
                delta
            ).ok()?;
            Some(RealtimeEvent::AudioDelta { delta: audio_data })
        }
        "response.audio_transcript.delta" => {
            let delta = json.get("delta")?.as_str()?.to_string();
            Some(RealtimeEvent::TranscriptDelta { delta })
        }
        "response.audio_transcript.done" => {
            let transcript = json.get("transcript")?.as_str()?.to_string();
            Some(RealtimeEvent::TranscriptDone { text: transcript })
        }
        "response.text.delta" => {
            let delta = json.get("delta")?.as_str()?.to_string();
            Some(RealtimeEvent::TextDelta { delta })
        }
        "response.text.done" => {
            let text = json.get("text")?.as_str()?.to_string();
            Some(RealtimeEvent::TextDone { text })
        }
        "input_audio_buffer.speech_started" => {
            Some(RealtimeEvent::SpeechStarted)
        }
        "input_audio_buffer.speech_stopped" => {
            Some(RealtimeEvent::SpeechStopped)
        }
        "response.done" => {
            Some(RealtimeEvent::ResponseDone)
        }
        "error" => {
            let error = json.get("error")
                .and_then(|e| e.get("message"))
                .and_then(|m| m.as_str())
                .unwrap_or("Unknown error")
                .to_string();
            Some(RealtimeEvent::Error { message: error })
        }
        _ => None, // Ignore unknown events
    }
}

/// Build the session configuration message
fn build_session_config(config: &RealtimeConfig) -> serde_json::Value {
    let mut session = serde_json::json!({
        "modalities": config.modalities.iter().map(|m| match m {
            Modality::Text => "text",
            Modality::Audio => "audio",
        }).collect::<Vec<_>>(),
        "voice": config.voice.as_str(),
        "input_audio_format": config.input_audio_format.as_str(),
        "output_audio_format": config.output_audio_format.as_str(),
    });

    if let Some(instructions) = &config.instructions {
        session["instructions"] = serde_json::json!(instructions);
    }

    if let Some(temp) = config.temperature {
        session["temperature"] = serde_json::json!(temp);
    }

    if let Some(max_tokens) = config.max_output_tokens {
        session["max_response_output_tokens"] = serde_json::json!(max_tokens);
    }

    if let Some(vad) = &config.vad_config {
        let mut turn_detection = serde_json::json!({
            "type": vad.vad_type,
        });
        if let Some(threshold) = vad.threshold {
            turn_detection["threshold"] = serde_json::json!(threshold);
        }
        if let Some(silence_ms) = vad.silence_duration_ms {
            turn_detection["silence_duration_ms"] = serde_json::json!(silence_ms);
        }
        if let Some(prefix_ms) = vad.prefix_padding_ms {
            turn_detection["prefix_padding_ms"] = serde_json::json!(prefix_ms);
        }
        session["turn_detection"] = turn_detection;
    } else {
        session["turn_detection"] = serde_json::json!(null);
    }

    serde_json::json!({
        "type": "session.update",
        "session": session
    })
}

/// Build an audio buffer append message
pub fn build_audio_append(audio_data: &[u8]) -> serde_json::Value {
    let base64_audio = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, audio_data);
    serde_json::json!({
        "type": "input_audio_buffer.append",
        "audio": base64_audio
    })
}

/// Build a commit audio buffer message
pub fn build_audio_commit() -> serde_json::Value {
    serde_json::json!({
        "type": "input_audio_buffer.commit"
    })
}

/// Build a response create message for text input
pub fn build_response_create(text: Option<&str>) -> serde_json::Value {
    let mut event = serde_json::json!({
        "type": "response.create"
    });

    if let Some(t) = text {
        event["response"] = serde_json::json!({
            "modalities": ["text", "audio"],
            "instructions": t
        });
    }

    event
}

/// Build a cancel response message
pub fn build_response_cancel() -> serde_json::Value {
    serde_json::json!({
        "type": "response.cancel"
    })
}

/// Build a conversation item create message for text input
pub fn build_conversation_item(text: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "conversation.item.create",
        "item": {
            "type": "message",
            "role": "user",
            "content": [{
                "type": "input_text",
                "text": text
            }]
        }
    })
}

/// Generate a random WebSocket key
fn generate_websocket_key() -> String {
    use base64::Engine;
    let mut key = [0u8; 16];
    // Use a simple random generation (in production, use proper random)
    for (i, byte) in key.iter_mut().enumerate() {
        *byte = ((std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() + i as u128) % 256) as u8;
    }
    base64::engine::general_purpose::STANDARD.encode(key)
}
