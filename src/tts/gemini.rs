use crate::client::config::GeminiConfig;
use crate::error::{Error, Result};
use super::{SpeechRequest, SpeechResponse, AudioFormat};
use base64::Engine;

/// Default model for Gemini TTS
const DEFAULT_MODEL: &str = "gemini-2.5-flash-preview-tts";

/// Generate speech using Gemini's TTS API
pub(crate) async fn generate(
    config: &GeminiConfig,
    request: &SpeechRequest,
) -> Result<SpeechResponse> {
    // Determine model to use
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // Build URL
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model,
        config.api_key
    );

    // Build the prompt - include any style instructions in the text
    let prompt_text = if let Some(instructions) = &request.instructions {
        format!("{}: {}", instructions, request.text)
    } else {
        request.text.clone()
    };

    // Build request body
    let body = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": prompt_text
            }]
        }],
        "generationConfig": {
            "responseModalities": ["AUDIO"],
            "speechConfig": {
                "voiceConfig": {
                    "prebuiltVoiceConfig": {
                        "voiceName": request.voice.to_gemini_string()
                    }
                }
            }
        }
    });

    // Make the request
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::GeminiError(format!("HTTP request failed: {}", e)))?;

    // Check for errors
    if !response.status().is_success() {
        let status = response.status();
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(Error::from_gemini_error(format!(
            "HTTP {}: {}",
            status, error_text
        )));
    }

    // Parse response
    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to parse response: {}", e)))?;

    // Extract audio data from response
    let audio_data = response_json["candidates"][0]["content"]["parts"][0]["inlineData"]["data"]
        .as_str()
        .ok_or_else(|| Error::GeminiError("No audio data in response".into()))?;

    // Decode base64 audio (raw PCM)
    let pcm_data = base64::engine::general_purpose::STANDARD
        .decode(audio_data)
        .map_err(|e| Error::GeminiError(format!("Failed to decode audio: {}", e)))?;

    // Wrap raw PCM with WAV headers so it plays correctly
    let audio = wrap_pcm_with_wav_header(&pcm_data);

    Ok(SpeechResponse {
        audio,
        format: AudioFormat::Wav,
    })
}

/// Wrap raw PCM audio data with a WAV header.
/// Gemini TTS returns raw PCM: 24000 Hz, 16-bit, mono
fn wrap_pcm_with_wav_header(pcm_data: &[u8]) -> Vec<u8> {
    let sample_rate: u32 = 24000;
    let bits_per_sample: u16 = 16;
    let num_channels: u16 = 1;
    let byte_rate = sample_rate * (bits_per_sample as u32 / 8) * num_channels as u32;
    let block_align = num_channels * (bits_per_sample / 8);
    let data_size = pcm_data.len() as u32;
    let file_size = 36 + data_size;

    let mut wav = Vec::with_capacity(44 + pcm_data.len());

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&file_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt subchunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // subchunk size
    wav.extend_from_slice(&1u16.to_le_bytes());  // audio format (PCM)
    wav.extend_from_slice(&num_channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits_per_sample.to_le_bytes());

    // data subchunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    wav.extend_from_slice(pcm_data);

    wav
}
