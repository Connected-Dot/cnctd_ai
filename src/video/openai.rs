use async_openai::Client as OpenAiSdkClient;
use crate::client::config::OpenAiConfig;
use crate::error::{Error, Result};
use crate::ImageContent;
use super::{VideoInput, VideoAnalysisRequest, VideoAnalysisResponse};

/// Default model for OpenAI video analysis (via vision)
const DEFAULT_MODEL: &str = "gpt-4o";

/// Analyze video using OpenAI's vision API with pre-extracted frames
///
/// OpenAI does not support native video input. Instead, users must:
/// 1. Extract frames from the video (e.g., using ffmpeg at 2-4 fps)
/// 2. Pass the frames as `VideoInput::Frames(Vec<ImageContent>)`
///
/// Example ffmpeg command to extract frames:
/// ```bash
/// ffmpeg -i video.mp4 -vf "fps=2" -q:v 2 frame_%04d.jpg
/// ```
pub(crate) async fn analyze(
    _sdk_client: &OpenAiSdkClient<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &VideoAnalysisRequest,
) -> Result<VideoAnalysisResponse> {
    // OpenAI only supports frame-based video analysis
    let frames = match &request.video {
        VideoInput::Frames(frames) => frames.clone(),
        VideoInput::FilePath(_) | VideoInput::Url(_) | VideoInput::Bytes { .. } => {
            return Err(Error::UnsupportedOperation(
                "OpenAI does not support native video input. \
                 Please extract frames from the video and use VideoAnalysisRequest::from_frames(). \
                 Example: ffmpeg -i video.mp4 -vf \"fps=2\" -q:v 2 frame_%04d.jpg".to_string()
            ));
        }
    };

    if frames.is_empty() {
        return Err(Error::InvalidRequest("No frames provided for video analysis".into()));
    }

    // Determine model to use
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // Build the request using the Responses API
    let url = "https://api.openai.com/v1/responses";

    // Build content array with frames and prompt
    let mut content = Vec::new();

    // Add prompt first
    content.push(serde_json::json!({
        "type": "input_text",
        "text": format!(
            "Analyze this video (shown as {} sequential frames). {}",
            frames.len(),
            request.prompt
        )
    }));

    // Add each frame as an image
    for (i, frame) in frames.iter().enumerate() {
        // Add frame number annotation
        content.push(serde_json::json!({
            "type": "input_text",
            "text": format!("[Frame {}]", i + 1)
        }));

        // Add the image
        content.push(serde_json::json!({
            "type": "input_image",
            "image_url": format!("data:{};base64,{}", frame.media_type, frame.data)
        }));
    }

    // Build the request body
    let body = serde_json::json!({
        "model": model,
        "input": content
    });

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

    // Parse response
    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::Parse(format!("Failed to parse response: {}", e)))?;

    // Extract text from the response
    // Responses API format: { "output": [{ "type": "message", "content": [{ "type": "output_text", "text": "..." }] }] }
    let text = extract_text_from_response(&response_json)?;

    Ok(VideoAnalysisResponse {
        text,
        duration: None,
    })
}

/// Extract text from OpenAI Responses API response
fn extract_text_from_response(response: &serde_json::Value) -> Result<String> {
    let output = response["output"]
        .as_array()
        .ok_or_else(|| Error::Parse("No output in response".into()))?;

    let mut text = String::new();

    for item in output {
        if item["type"].as_str() == Some("message") {
            if let Some(content) = item["content"].as_array() {
                for part in content {
                    if part["type"].as_str() == Some("output_text") {
                        if let Some(t) = part["text"].as_str() {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(t);
                        }
                    }
                }
            }
        }
    }

    if text.is_empty() {
        // Fallback: try to find text anywhere in the response
        if let Some(t) = response["output"][0]["content"][0]["text"].as_str() {
            return Ok(t.to_string());
        }
        return Err(Error::Parse("No text content in response".into()));
    }

    Ok(text)
}

// Suppress unused import warning - ImageContent is used in documentation
#[allow(unused_imports)]
use crate::ImageContent as _;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_text_from_response() {
        let response = serde_json::json!({
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "This video shows a cat playing with a ball."
                }]
            }]
        });

        let text = extract_text_from_response(&response).unwrap();
        assert_eq!(text, "This video shows a cat playing with a ball.");
    }
}
