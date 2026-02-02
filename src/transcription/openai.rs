use async_openai::types::{
    AudioInput as OpenAiAudioInput, AudioResponseFormat, CreateTranscriptionRequest,
    InputSource, TimestampGranularity,
};
use crate::client::config::OpenAiConfig;
use crate::error::{Error, Result};
use super::{AudioInput, TranscriptionRequest, TranscriptionResponse, TranscriptSegment};

/// Default model for transcription
const DEFAULT_MODEL: &str = "whisper-1";

/// Transcribe audio using OpenAI's Whisper API
pub(crate) async fn transcribe(
    sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &TranscriptionRequest,
) -> Result<TranscriptionResponse> {
    // Determine model to use
    let model = request
        .model
        .clone()
        .or_else(|| config.transcription_model.clone())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // Convert our AudioInput to OpenAI's InputSource
    let input_source = match &request.audio {
        AudioInput::FilePath(path) => InputSource::Path { path: path.clone() },
        AudioInput::Url(url) => {
            // Fetch the URL and use the bytes
            let response = reqwest::get(url).await.map_err(|e| {
                Error::Other(format!("Failed to fetch audio from URL: {}", e))
            })?;

            if !response.status().is_success() {
                return Err(Error::Other(format!(
                    "Failed to fetch audio: HTTP {}",
                    response.status()
                )));
            }

            let bytes = response.bytes().await.map_err(|e| {
                Error::Other(format!("Failed to read audio bytes: {}", e))
            })?;

            // Extract filename from URL or use default
            let filename = url
                .split('/')
                .last()
                .and_then(|s| s.split('?').next())
                .unwrap_or("audio.mp3")
                .to_string();

            InputSource::Bytes { filename, bytes }
        }
        AudioInput::Bytes { data, mime_type } => {
            // Determine filename extension from mime type
            let ext = mime_type_to_extension(mime_type);
            InputSource::VecU8 {
                filename: format!("audio.{}", ext),
                vec: data.clone(),
            }
        }
    };

    // Build the OpenAI request
    let openai_audio_input = OpenAiAudioInput {
        source: input_source,
    };

    let mut openai_request = CreateTranscriptionRequest {
        file: openai_audio_input,
        model: model.clone(),
        prompt: request.prompt.clone(),
        response_format: Some(AudioResponseFormat::VerboseJson),
        temperature: None,
        language: request.language.clone(),
        timestamp_granularities: None,
    };

    // Request timestamps if needed
    if request.timestamps {
        openai_request.timestamp_granularities = Some(vec![
            TimestampGranularity::Segment,
            TimestampGranularity::Word,
        ]);
    }

    // Use verbose_json to get segments and timing
    let response = sdk_client
        .audio()
        .transcribe_verbose_json(openai_request)
        .await?;

    // Convert segments if present
    let segments = response.segments.map(|segs| {
        segs.into_iter()
            .map(|seg| TranscriptSegment {
                start: seg.start as f64,
                end: seg.end as f64,
                text: seg.text,
                speaker: None, // Whisper doesn't do diarization natively
            })
            .collect()
    });

    // Note: Standard Whisper models don't support diarization
    // gpt-4o-transcribe models may have different capabilities
    if request.diarization && !model.contains("gpt-4o") {
        // For now, we don't error but the user should know diarization isn't available
        // for standard whisper-1 model
    }

    Ok(TranscriptionResponse {
        text: response.text,
        segments: if request.timestamps { segments } else { None },
        speakers: None, // Not supported by Whisper
        language: Some(response.language),
        duration: Some(response.duration as f64),
    })
}

/// Convert MIME type to file extension
fn mime_type_to_extension(mime_type: &str) -> &str {
    match mime_type {
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/mp4" => "mp4",
        "audio/m4a" | "audio/x-m4a" => "m4a",
        "audio/wav" | "audio/x-wav" => "wav",
        "audio/webm" => "webm",
        "audio/ogg" => "ogg",
        "audio/flac" => "flac",
        _ => "mp3", // Default
    }
}
