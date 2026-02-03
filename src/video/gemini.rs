use crate::client::config::GeminiConfig;
use crate::error::{Error, Result};
use super::{VideoInput, VideoAnalysisRequest, VideoAnalysisResponse};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Deserialize;

/// Default model for Gemini video analysis
const DEFAULT_MODEL: &str = "gemini-2.5-flash";

/// Maximum size for inline video (20MB)
const MAX_INLINE_SIZE: usize = 20 * 1024 * 1024;

/// Analyze video using Gemini's multimodal generateContent API
pub(crate) async fn analyze(
    config: &GeminiConfig,
    request: &VideoAnalysisRequest,
) -> Result<VideoAnalysisResponse> {
    // Determine model to use
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // Build the request body based on input type
    let body = match &request.video {
        VideoInput::Url(url) if is_youtube_url(url) => {
            // YouTube URLs can be passed directly
            build_request_with_youtube(url, &request.prompt)
        }
        VideoInput::Url(url) => {
            // For non-YouTube URLs, fetch the video first
            let (video_bytes, mime_type) = fetch_video_from_url(url).await?;
            if video_bytes.len() > MAX_INLINE_SIZE {
                let file_uri = upload_file(config, &video_bytes, &mime_type).await?;
                build_request_with_file(&file_uri, &mime_type, request)
            } else {
                build_request_with_inline(&video_bytes, &mime_type, request)
            }
        }
        VideoInput::FilePath(path) => {
            let video_bytes = tokio::fs::read(path).await.map_err(|e| {
                Error::Other(format!("Failed to read video file: {}", e))
            })?;
            let mime_type = mime_type_from_path(path);

            if video_bytes.len() > MAX_INLINE_SIZE {
                let file_uri = upload_file(config, &video_bytes, &mime_type).await?;
                build_request_with_file(&file_uri, &mime_type, request)
            } else {
                build_request_with_inline(&video_bytes, &mime_type, request)
            }
        }
        VideoInput::Bytes { data, mime_type } => {
            if data.len() > MAX_INLINE_SIZE {
                let file_uri = upload_file(config, data, mime_type).await?;
                build_request_with_file(&file_uri, mime_type, request)
            } else {
                build_request_with_inline(data, mime_type, request)
            }
        }
        VideoInput::Frames(_) => {
            return Err(Error::UnsupportedOperation(
                "Gemini supports native video - use VideoInput::FilePath, Url, or Bytes instead of pre-extracted frames".to_string()
            ));
        }
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

    // Extract the text from the response
    let candidate = response_json["candidates"]
        .get(0)
        .ok_or_else(|| Error::GeminiError("No candidates in response".into()))?;

    let parts = candidate["content"]["parts"]
        .as_array()
        .ok_or_else(|| Error::GeminiError("No parts in response".into()))?;

    let mut text = String::new();
    for part in parts {
        if let Some(t) = part["text"].as_str() {
            text.push_str(t);
        }
    }

    Ok(VideoAnalysisResponse {
        text: text.trim().to_string(),
        duration: None, // Gemini doesn't return video duration
    })
}

/// Check if URL is a YouTube URL
fn is_youtube_url(url: &str) -> bool {
    url.contains("youtube.com") || url.contains("youtu.be")
}

/// Fetch video bytes from a URL
async fn fetch_video_from_url(url: &str) -> Result<(Vec<u8>, String)> {
    let response = reqwest::get(url).await.map_err(|e| {
        Error::Other(format!("Failed to fetch video from URL: {}", e))
    })?;

    if !response.status().is_success() {
        return Err(Error::Other(format!(
            "Failed to fetch video: HTTP {}",
            response.status()
        )));
    }

    // Try to get content type from response
    let content_type = response
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.split(';').next().unwrap_or(s).to_string())
        .unwrap_or_else(|| "video/mp4".to_string());

    let bytes = response.bytes().await.map_err(|e| {
        Error::Other(format!("Failed to read video bytes: {}", e))
    })?;

    Ok((bytes.to_vec(), content_type))
}

/// Determine MIME type from file path extension
fn mime_type_from_path(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    match ext.as_str() {
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "mov" => "video/quicktime",
        "avi" => "video/avi",
        "mkv" => "video/x-matroska",
        "flv" => "video/x-flv",
        "wmv" => "video/x-ms-wmv",
        "mpeg" | "mpg" => "video/mpeg",
        "3gp" | "3gpp" => "video/3gpp",
        _ => "video/mp4",
    }
    .to_string()
}

/// Build request body for YouTube URLs
fn build_request_with_youtube(
    youtube_url: &str,
    prompt: &str,
) -> serde_json::Value {
    serde_json::json!({
        "contents": [{
            "parts": [
                {
                    "fileData": {
                        "fileUri": youtube_url
                    }
                },
                {
                    "text": prompt
                }
            ]
        }]
    })
}

/// Build request body with inline video data
fn build_request_with_inline(
    video_bytes: &[u8],
    mime_type: &str,
    request: &VideoAnalysisRequest,
) -> serde_json::Value {
    use base64::Engine;
    let encoded = base64::engine::general_purpose::STANDARD.encode(video_bytes);

    let mut video_part = serde_json::json!({
        "inlineData": {
            "mimeType": mime_type,
            "data": encoded
        }
    });

    // Add videoMetadata if fps or offsets are specified
    if request.fps.is_some() || request.start_offset.is_some() || request.end_offset.is_some() {
        let mut metadata = serde_json::Map::new();

        if let Some(fps) = request.fps {
            metadata.insert("fps".into(), serde_json::json!(fps));
        }
        if let Some(start) = request.start_offset {
            metadata.insert("startOffset".into(), serde_json::json!(format!("{}s", start)));
        }
        if let Some(end) = request.end_offset {
            metadata.insert("endOffset".into(), serde_json::json!(format!("{}s", end)));
        }

        video_part["videoMetadata"] = serde_json::Value::Object(metadata);
    }

    serde_json::json!({
        "contents": [{
            "parts": [
                video_part,
                {
                    "text": request.prompt
                }
            ]
        }]
    })
}

/// Build request body with uploaded file URI
fn build_request_with_file(
    file_uri: &str,
    mime_type: &str,
    request: &VideoAnalysisRequest,
) -> serde_json::Value {
    let mut video_part = serde_json::json!({
        "fileData": {
            "mimeType": mime_type,
            "fileUri": file_uri
        }
    });

    // Add videoMetadata if fps or offsets are specified
    if request.fps.is_some() || request.start_offset.is_some() || request.end_offset.is_some() {
        let mut metadata = serde_json::Map::new();

        if let Some(fps) = request.fps {
            metadata.insert("fps".into(), serde_json::json!(fps));
        }
        if let Some(start) = request.start_offset {
            metadata.insert("startOffset".into(), serde_json::json!(format!("{}s", start)));
        }
        if let Some(end) = request.end_offset {
            metadata.insert("endOffset".into(), serde_json::json!(format!("{}s", end)));
        }

        video_part["videoMetadata"] = serde_json::Value::Object(metadata);
    }

    serde_json::json!({
        "contents": [{
            "parts": [
                video_part,
                {
                    "text": request.prompt
                }
            ]
        }]
    })
}

/// Upload a file to the Gemini Files API and wait for it to become ACTIVE
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
        name: Option<String>,
        state: Option<String>,
    }

    #[derive(Deserialize)]
    struct FileStatusResponse {
        state: Option<String>,
        uri: Option<String>,
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
            "displayName": "video_upload"
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

    // Parse response to get file URI and name
    let file_response: FileUploadResponse = upload_response
        .json()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to parse upload response: {}", e)))?;

    let file_uri = file_response.file.uri.clone();
    let file_name = file_response.file.name.clone();
    let initial_state = file_response.file.state.as_deref().unwrap_or("PROCESSING");

    // If already ACTIVE, return immediately
    if initial_state == "ACTIVE" {
        return Ok(file_uri);
    }

    // Step 3: Poll until file is ACTIVE
    // Large video files need processing time before they can be used
    let file_name = file_name.ok_or_else(|| {
        Error::GeminiError("No file name in upload response".into())
    })?;

    let status_url = format!(
        "https://generativelanguage.googleapis.com/v1beta/{}?key={}",
        file_name,
        config.api_key
    );

    // Poll for up to 5 minutes (video processing can take a while for large files)
    let max_attempts = 60;
    let poll_interval = std::time::Duration::from_secs(5);

    for attempt in 1..=max_attempts {
        tokio::time::sleep(poll_interval).await;

        let status_response = client
            .get(&status_url)
            .send()
            .await
            .map_err(|e| Error::GeminiError(format!("Failed to check file status: {}", e)))?;

        if !status_response.status().is_success() {
            let error_text = status_response.text().await.unwrap_or_default();
            return Err(Error::GeminiError(format!("Status check failed: {}", error_text)));
        }

        let status: FileStatusResponse = status_response
            .json()
            .await
            .map_err(|e| Error::GeminiError(format!("Failed to parse status response: {}", e)))?;

        match status.state.as_deref() {
            Some("ACTIVE") => {
                // File is ready - use the URI from status response if available
                return Ok(status.uri.unwrap_or(file_uri));
            }
            Some("FAILED") => {
                return Err(Error::GeminiError("File processing failed".into()));
            }
            Some("PROCESSING") => {
                // Still processing, continue polling
                if attempt % 6 == 0 {
                    // Log progress every 30 seconds
                    eprintln!("Still processing video file... ({}s elapsed)", attempt * 5);
                }
            }
            Some(other) => {
                return Err(Error::GeminiError(format!("Unexpected file state: {}", other)));
            }
            None => {
                // No state field, assume processing
            }
        }
    }

    Err(Error::GeminiError(
        "Timeout waiting for file to become active (5 minutes)".into()
    ))
}
