use crate::client::config::OpenAiConfig;
use crate::error::{Error, Result};
use super::{SpeechRequest, SpeechResponse, AudioFormat};

/// Default model for OpenAI TTS
const DEFAULT_MODEL: &str = "tts-1";

/// Generate speech using OpenAI's TTS API
pub(crate) async fn generate(
    _sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &SpeechRequest,
) -> Result<SpeechResponse> {
    // Determine model to use
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // Build URL
    let url = "https://api.openai.com/v1/audio/speech";

    // Build request body
    let mut body = serde_json::json!({
        "model": model,
        "input": request.text,
        "voice": request.voice.to_openai_string(),
        "response_format": request.format.to_string(),
        "speed": request.speed
    });

    // Add instructions if provided (only works with gpt-4o-mini-tts)
    if let Some(instructions) = &request.instructions {
        body["instructions"] = serde_json::json!(instructions);
    }

    // Make the request
    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("HTTP request failed: {}", e)))?;

    // Check for errors
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::ProviderError {
            provider: "OpenAI".to_string(),
            message: format!("HTTP {}: {}", status, error_text),
            status_code: Some(status.as_u16()),
        });
    }

    // Get audio bytes directly from response
    let audio = response
        .bytes()
        .await
        .map_err(|e| Error::Parse(format!("Failed to read audio data: {}", e)))?
        .to_vec();

    Ok(SpeechResponse {
        audio,
        format: request.format,
    })
}
