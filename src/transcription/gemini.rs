use crate::client::config::GeminiConfig;
use crate::error::{Error, Result};
use super::{AudioInput, TranscriptionRequest, TranscriptionResponse, TranscriptSegment, Speaker};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Deserialize;

/// Default model for Gemini transcription
const DEFAULT_MODEL: &str = "gemini-2.0-flash";

/// Maximum size for inline audio (20MB)
const MAX_INLINE_SIZE: usize = 20 * 1024 * 1024;

/// Transcribe audio using Gemini's multimodal generateContent API
pub(crate) async fn transcribe(
    config: &GeminiConfig,
    request: &TranscriptionRequest,
) -> Result<TranscriptionResponse> {
    // Determine model to use
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // Get audio data and mime type
    let (audio_bytes, mime_type) = get_audio_data(&request.audio).await?;

    // Build the prompt for transcription
    let prompt = build_transcription_prompt(request);

    // Build the request body
    let body = if audio_bytes.len() > MAX_INLINE_SIZE {
        // For large files, use the Files API
        let file_uri = upload_file(config, &audio_bytes, &mime_type).await?;
        build_request_with_file(&file_uri, &mime_type, &prompt)
    } else {
        // For smaller files, use inline base64
        build_request_with_inline(&audio_bytes, &mime_type, &prompt)
    };

    // Build headers
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    // Build URL
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model,
        config.api_key
    );

    // Make the request
    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .headers(headers)
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

    // Extract the transcript text
    let candidate = response_json["candidates"]
        .get(0)
        .ok_or_else(|| Error::GeminiError("No candidates in response".into()))?;

    let parts = candidate["content"]["parts"]
        .as_array()
        .ok_or_else(|| Error::GeminiError("No parts in response".into()))?;

    let mut transcript_text = String::new();
    for part in parts {
        if let Some(text) = part["text"].as_str() {
            transcript_text.push_str(text);
        }
    }

    // Parse the transcript - Gemini returns natural language, so we need to
    // extract structured data if timestamps/diarization were requested
    let parsed = parse_gemini_transcript(&transcript_text, request)?;

    Ok(parsed)
}

/// Get audio bytes and MIME type from the input
async fn get_audio_data(input: &AudioInput) -> Result<(Vec<u8>, String)> {
    match input {
        AudioInput::FilePath(path) => {
            let data = tokio::fs::read(path).await.map_err(|e| {
                Error::Other(format!("Failed to read audio file: {}", e))
            })?;
            let mime_type = mime_type_from_path(path);
            Ok((data, mime_type))
        }
        AudioInput::Url(url) => {
            let response = reqwest::get(url).await.map_err(|e| {
                Error::Other(format!("Failed to fetch audio from URL: {}", e))
            })?;

            if !response.status().is_success() {
                return Err(Error::Other(format!(
                    "Failed to fetch audio: HTTP {}",
                    response.status()
                )));
            }

            // Try to get content type from response
            let content_type = response
                .headers()
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.split(';').next().unwrap_or(s).to_string())
                .unwrap_or_else(|| "audio/mpeg".to_string());

            let bytes = response.bytes().await.map_err(|e| {
                Error::Other(format!("Failed to read audio bytes: {}", e))
            })?;

            Ok((bytes.to_vec(), content_type))
        }
        AudioInput::Bytes { data, mime_type } => {
            Ok((data.clone(), mime_type.clone()))
        }
    }
}

/// Determine MIME type from file path extension
fn mime_type_from_path(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "mp3" => "audio/mpeg",
        "mp4" | "m4a" => "audio/mp4",
        "wav" => "audio/wav",
        "webm" => "audio/webm",
        "ogg" => "audio/ogg",
        "flac" => "audio/flac",
        "aac" => "audio/aac",
        _ => "audio/mpeg",
    }
    .to_string()
}

/// Build the transcription prompt based on request options
fn build_transcription_prompt(request: &TranscriptionRequest) -> String {
    let mut prompt = String::from("Transcribe this audio accurately.");

    if let Some(lang) = &request.language {
        prompt.push_str(&format!(" The audio is in {} (ISO-639-1: {}).",
            language_name(lang), lang));
    }

    if request.timestamps {
        prompt.push_str(" Include timestamps for each segment in the format [MM:SS-MM:SS].");
    }

    if request.diarization {
        prompt.push_str(" Identify different speakers and label them (Speaker 1, Speaker 2, etc.).");
    }

    if let Some(context) = &request.prompt {
        prompt.push_str(&format!(" Context: {}", context));
    }

    if request.timestamps || request.diarization {
        prompt.push_str("\n\nFormat the output as:\n");
        if request.timestamps && request.diarization {
            prompt.push_str("[MM:SS-MM:SS] Speaker N: <text>\n");
        } else if request.timestamps {
            prompt.push_str("[MM:SS-MM:SS] <text>\n");
        } else if request.diarization {
            prompt.push_str("Speaker N: <text>\n");
        }
    }

    prompt
}

/// Convert ISO-639-1 code to language name
fn language_name(code: &str) -> &str {
    match code {
        "en" => "English",
        "es" => "Spanish",
        "fr" => "French",
        "de" => "German",
        "it" => "Italian",
        "pt" => "Portuguese",
        "ru" => "Russian",
        "zh" => "Chinese",
        "ja" => "Japanese",
        "ko" => "Korean",
        "ar" => "Arabic",
        "hi" => "Hindi",
        _ => "the specified language",
    }
}

/// Build request body with inline audio data
fn build_request_with_inline(
    audio_bytes: &[u8],
    mime_type: &str,
    prompt: &str,
) -> serde_json::Value {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(audio_bytes);

    serde_json::json!({
        "contents": [{
            "parts": [
                {
                    "inlineData": {
                        "mimeType": mime_type,
                        "data": encoded
                    }
                },
                {
                    "text": prompt
                }
            ]
        }]
    })
}

/// Build request body with uploaded file URI
fn build_request_with_file(
    file_uri: &str,
    mime_type: &str,
    prompt: &str,
) -> serde_json::Value {
    serde_json::json!({
        "contents": [{
            "parts": [
                {
                    "fileData": {
                        "mimeType": mime_type,
                        "fileUri": file_uri
                    }
                },
                {
                    "text": prompt
                }
            ]
        }]
    })
}

/// Upload a file to the Gemini Files API
async fn upload_file(
    config: &GeminiConfig,
    data: &[u8],
    mime_type: &str,
) -> Result<String> {
    #[derive(Deserialize)]
    struct FileUploadResponse {
        file: FileInfo,
    }

    #[derive(Deserialize)]
    struct FileInfo {
        uri: String,
    }

    // Step 1: Start resumable upload
    let init_url = format!(
        "https://generativelanguage.googleapis.com/upload/v1beta/files?key={}",
        config.api_key
    );

    let client = reqwest::Client::new();

    // Upload metadata
    let metadata = serde_json::json!({
        "file": {
            "displayName": "audio_upload"
        }
    });

    let init_response = client
        .post(&init_url)
        .header("X-Goog-Upload-Protocol", "resumable")
        .header("X-Goog-Upload-Command", "start")
        .header("X-Goog-Upload-Header-Content-Type", mime_type)
        .header("X-Goog-Upload-Header-Content-Length", data.len().to_string())
        .header("Content-Type", "application/json")
        .json(&metadata)
        .send()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to initiate upload: {}", e)))?;

    if !init_response.status().is_success() {
        let error_text = init_response.text().await.unwrap_or_default();
        return Err(Error::GeminiError(format!("Upload init failed: {}", error_text)));
    }

    // Get upload URL from header
    let upload_url = init_response
        .headers()
        .get("x-goog-upload-url")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::GeminiError("No upload URL in response".into()))?
        .to_string();

    // Step 2: Upload the data
    let upload_response = client
        .post(&upload_url)
        .header("Content-Length", data.len().to_string())
        .header("X-Goog-Upload-Offset", "0")
        .header("X-Goog-Upload-Command", "upload, finalize")
        .body(data.to_vec())
        .send()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to upload file: {}", e)))?;

    if !upload_response.status().is_success() {
        let error_text = upload_response.text().await.unwrap_or_default();
        return Err(Error::GeminiError(format!("Upload failed: {}", error_text)));
    }

    // Parse response to get file URI
    let file_response: FileUploadResponse = upload_response
        .json()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to parse upload response: {}", e)))?;

    Ok(file_response.file.uri)
}

/// Parse Gemini's natural language transcript into structured format
fn parse_gemini_transcript(
    text: &str,
    request: &TranscriptionRequest,
) -> Result<TranscriptionResponse> {
    let mut segments = Vec::new();
    let mut speakers = std::collections::HashSet::new();
    let mut plain_text = String::new();

    // If no special formatting requested, just return the text
    if !request.timestamps && !request.diarization {
        return Ok(TranscriptionResponse {
            text: text.trim().to_string(),
            segments: None,
            speakers: None,
            language: request.language.clone(),
            duration: None,
        });
    }

    // Try to parse structured output
    // Expected formats:
    // [MM:SS-MM:SS] Speaker N: text
    // [MM:SS-MM:SS] text
    // Speaker N: text

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let mut start_time: Option<f64> = None;
        let mut end_time: Option<f64> = None;
        let mut speaker: Option<String> = None;
        let mut segment_text = line.to_string();

        // Parse timestamp [MM:SS-MM:SS]
        if let Some(captures) = parse_timestamp_range(line) {
            start_time = Some(captures.0);
            end_time = Some(captures.1);
            segment_text = captures.2.to_string();
        }

        // Parse speaker
        if let Some(captures) = parse_speaker(&segment_text) {
            speaker = Some(captures.0.clone());
            speakers.insert(captures.0);
            segment_text = captures.1.to_string();
        }

        // Build plain text
        if !plain_text.is_empty() {
            plain_text.push(' ');
        }
        plain_text.push_str(&segment_text);

        // Only add segments if we have timestamps
        if start_time.is_some() || speaker.is_some() {
            segments.push(TranscriptSegment {
                start: start_time.unwrap_or(0.0),
                end: end_time.unwrap_or(0.0),
                text: segment_text,
                speaker,
            });
        }
    }

    // Build speakers list
    let speakers_list: Option<Vec<Speaker>> = if !speakers.is_empty() {
        Some(
            speakers
                .into_iter()
                .enumerate()
                .map(|(i, id)| Speaker {
                    id: id.clone(),
                    label: Some(format!("Speaker {}", i + 1)),
                })
                .collect(),
        )
    } else {
        None
    };

    Ok(TranscriptionResponse {
        text: if plain_text.is_empty() {
            text.trim().to_string()
        } else {
            plain_text
        },
        segments: if segments.is_empty() {
            None
        } else {
            Some(segments)
        },
        speakers: speakers_list,
        language: request.language.clone(),
        duration: None, // Gemini doesn't provide duration
    })
}

/// Parse timestamp range like [00:15-00:30] or [0:15-0:30]
fn parse_timestamp_range(text: &str) -> Option<(f64, f64, &str)> {
    // Look for pattern [MM:SS-MM:SS] or [M:SS-M:SS]
    if !text.starts_with('[') {
        return None;
    }

    let end_bracket = text.find(']')?;
    let timestamp_part = &text[1..end_bracket];
    let rest = text[end_bracket + 1..].trim();

    // Split by hyphen
    let parts: Vec<&str> = timestamp_part.split('-').collect();
    if parts.len() != 2 {
        return None;
    }

    let start = parse_timestamp(parts[0])?;
    let end = parse_timestamp(parts[1])?;

    Some((start, end, rest))
}

/// Parse single timestamp like 00:15 or 1:30:45
fn parse_timestamp(ts: &str) -> Option<f64> {
    let parts: Vec<&str> = ts.trim().split(':').collect();
    match parts.len() {
        2 => {
            // MM:SS
            let mins: f64 = parts[0].parse().ok()?;
            let secs: f64 = parts[1].parse().ok()?;
            Some(mins * 60.0 + secs)
        }
        3 => {
            // HH:MM:SS
            let hours: f64 = parts[0].parse().ok()?;
            let mins: f64 = parts[1].parse().ok()?;
            let secs: f64 = parts[2].parse().ok()?;
            Some(hours * 3600.0 + mins * 60.0 + secs)
        }
        _ => None,
    }
}

/// Parse speaker label like "Speaker 1:" or "Speaker A:"
fn parse_speaker(text: &str) -> Option<(String, &str)> {
    let lower = text.to_lowercase();

    // Try "Speaker N:" pattern
    if lower.starts_with("speaker ") {
        if let Some(colon_pos) = text.find(':') {
            let speaker = text[..colon_pos].trim().to_string();
            let rest = text[colon_pos + 1..].trim();
            return Some((speaker, rest));
        }
    }

    None
}
