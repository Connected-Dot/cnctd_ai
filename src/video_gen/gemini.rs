use crate::client::config::GeminiConfig;
use crate::error::{Error, Result};
use super::{VideoGenerationRequest, VideoGenerationJob, VideoGenerationStatus, VideoGenerationResponse};

/// Default model for Gemini video generation (Veo)
const DEFAULT_MODEL: &str = "veo-3.1-generate-preview";

/// Start video generation using Gemini's Veo API
pub(crate) async fn generate(
    config: &GeminiConfig,
    request: &VideoGenerationRequest,
) -> Result<VideoGenerationJob> {
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:predictLongRunning?key={}",
        model,
        config.api_key
    );

    // Build instance (prompt + optional first frame)
    let mut instance = serde_json::json!({
        "prompt": request.prompt
    });

    if let Some(ref first_frame) = request.first_frame {
        instance["image"] = serde_json::json!({
            "inlineData": {
                "mimeType": first_frame.mime_type,
                "data": first_frame.data
            }
        });
    }

    // Build parameters
    let mut parameters = serde_json::json!({
        "aspectRatio": request.aspect_ratio.to_gemini_string(),
        "resolution": request.resolution.to_gemini_string()
    });

    if let Some(ref negative_prompt) = request.negative_prompt {
        parameters["negativePrompt"] = serde_json::json!(negative_prompt);
    }

    if let Some(ref last_frame) = request.last_frame {
        parameters["lastFrame"] = serde_json::json!({
            "inlineData": {
                "mimeType": last_frame.mime_type,
                "data": last_frame.data
            }
        });
    }

    let body = serde_json::json!({
        "instances": [instance],
        "parameters": parameters
    });

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::GeminiError(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(Error::GeminiError(format!("HTTP {}: {}", status, error_text)));
    }

    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to parse response: {}", e)))?;

    // Extract operation name
    let operation_name = response_json["name"]
        .as_str()
        .ok_or_else(|| Error::GeminiError("No operation name in response".into()))?
        .to_string();

    Ok(VideoGenerationJob {
        id: operation_name,
        status: VideoGenerationStatus::Queued,
        provider_data: serde_json::json!({ "api_key": config.api_key }),
    })
}

/// Poll for video generation status
pub(crate) async fn poll_status(
    job: &VideoGenerationJob,
) -> Result<VideoGenerationJob> {
    let api_key = job.provider_data["api_key"]
        .as_str()
        .ok_or_else(|| Error::GeminiError("No API key in job data".into()))?;

    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/{}?key={}",
        job.id,
        api_key
    );

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| Error::GeminiError(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(Error::GeminiError(format!("HTTP {}: {}", status, error_text)));
    }

    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to parse response: {}", e)))?;

    let done = response_json["done"].as_bool().unwrap_or(false);

    let status = if done {
        if response_json["error"].is_object() {
            let error_msg = response_json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();
            VideoGenerationStatus::Failed { error: error_msg }
        } else {
            VideoGenerationStatus::Completed
        }
    } else {
        let progress = response_json["metadata"]["progress"].as_f64().map(|p| p as f32);
        VideoGenerationStatus::InProgress { progress }
    };

    Ok(VideoGenerationJob {
        id: job.id.clone(),
        status,
        provider_data: serde_json::json!({
            "api_key": api_key,
            "response": response_json
        }),
    })
}

/// Download the generated video
pub(crate) async fn download(
    job: &VideoGenerationJob,
) -> Result<VideoGenerationResponse> {
    if !matches!(job.status, VideoGenerationStatus::Completed) {
        return Err(Error::GeminiError("Video generation not complete".into()));
    }

    let api_key = job.provider_data["api_key"]
        .as_str()
        .ok_or_else(|| Error::GeminiError("No API key in job data".into()))?;

    let response_data = &job.provider_data["response"];

    // Check if content was filtered by safety
    if let Some(filtered_reasons) = response_data["response"]["generateVideoResponse"]["raiMediaFilteredReasons"].as_array() {
        if !filtered_reasons.is_empty() {
            let reason = filtered_reasons[0].as_str().unwrap_or("Content filtered by safety system");
            return Err(Error::GeminiError(format!("Video generation blocked: {}", reason)));
        }
    }

    // Try multiple possible response paths for the video URI
    let video_uri = response_data["response"]["generateVideoResponse"]["generatedSamples"][0]["video"]["uri"]
        .as_str()
        .or_else(|| response_data["result"]["generateVideoResponse"]["generatedSamples"][0]["video"]["uri"].as_str())
        .or_else(|| response_data["generateVideoResponse"]["generatedSamples"][0]["video"]["uri"].as_str())
        .or_else(|| response_data["generatedSamples"][0]["video"]["uri"].as_str())
        .or_else(|| response_data["videos"][0]["uri"].as_str())
        .ok_or_else(|| {
            Error::GeminiError(format!(
                "No video URI in response. Response structure: {}",
                serde_json::to_string_pretty(response_data).unwrap_or_default()
            ))
        })?;

    // Download the video
    let client = reqwest::Client::new();
    let response = client
        .get(video_uri)
        .header("x-goog-api-key", api_key)
        .send()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to download video: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(Error::GeminiError(format!("Download failed HTTP {}: {}", status, error_text)));
    }

    let video = response
        .bytes()
        .await
        .map_err(|e| Error::GeminiError(format!("Failed to read video data: {}", e)))?
        .to_vec();

    // Try to extract duration from metadata
    let duration = job.provider_data["response"]["response"]["generateVideoResponse"]["generatedSamples"][0]["video"]["duration"]
        .as_str()
        .and_then(|d| d.trim_end_matches('s').parse::<f32>().ok());

    Ok(VideoGenerationResponse {
        video,
        duration_seconds: duration,
    })
}
