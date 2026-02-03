use crate::client::config::OpenAiConfig;
use crate::error::{Error, Result};
use super::{VideoGenerationRequest, VideoGenerationJob, VideoGenerationStatus, VideoGenerationResponse};

/// Default model for OpenAI video generation (Sora)
const DEFAULT_MODEL: &str = "sora-2";

/// Start video generation using OpenAI's Sora API
pub(crate) async fn generate(
    _sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &VideoGenerationRequest,
) -> Result<VideoGenerationJob> {
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    let is_pro = model.contains("pro");
    let size = request.aspect_ratio.to_openai_size(is_pro);

    let url = "https://api.openai.com/v1/videos";

    // Build request body
    let mut body = serde_json::json!({
        "model": model,
        "prompt": request.prompt,
        "size": size,
        "seconds": request.duration.to_openai_string()
    });

    // If we have a first frame, we need to use multipart form
    // For now, we'll just use JSON (text-to-video)
    // Image-to-video would require multipart handling

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Network(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(Error::ProviderError {
            provider: "OpenAI".to_string(),
            message: format!("HTTP {}: {}", status, error_text),
            status_code: Some(status.as_u16()),
        });
    }

    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::Parse(format!("Failed to parse response: {}", e)))?;

    let video_id = response_json["id"]
        .as_str()
        .ok_or_else(|| Error::Parse("No video ID in response".into()))?
        .to_string();

    let status_str = response_json["status"].as_str().unwrap_or("queued");
    let status = match status_str {
        "queued" => VideoGenerationStatus::Queued,
        "in_progress" => {
            let progress = response_json["progress"].as_f64().map(|p| p as f32);
            VideoGenerationStatus::InProgress { progress }
        }
        "completed" => VideoGenerationStatus::Completed,
        "failed" => VideoGenerationStatus::Failed {
            error: response_json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string(),
        },
        _ => VideoGenerationStatus::Queued,
    };

    Ok(VideoGenerationJob {
        id: video_id,
        status,
        provider_data: serde_json::json!({ "api_key": config.api_key }),
    })
}

/// Poll for video generation status
pub(crate) async fn poll_status(
    job: &VideoGenerationJob,
) -> Result<VideoGenerationJob> {
    let api_key = job.provider_data["api_key"]
        .as_str()
        .ok_or_else(|| Error::Parse("No API key in job data".into()))?;

    let url = format!("https://api.openai.com/v1/videos/{}", job.id);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| Error::Network(format!("HTTP request failed: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(Error::ProviderError {
            provider: "OpenAI".to_string(),
            message: format!("HTTP {}: {}", status, error_text),
            status_code: Some(status.as_u16()),
        });
    }

    let response_json: serde_json::Value = response
        .json()
        .await
        .map_err(|e| Error::Parse(format!("Failed to parse response: {}", e)))?;

    let status_str = response_json["status"].as_str().unwrap_or("queued");
    let status = match status_str {
        "queued" => VideoGenerationStatus::Queued,
        "in_progress" => {
            let progress = response_json["progress"].as_f64().map(|p| p as f32);
            VideoGenerationStatus::InProgress { progress }
        }
        "completed" => VideoGenerationStatus::Completed,
        "failed" => VideoGenerationStatus::Failed {
            error: response_json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string(),
        },
        _ => VideoGenerationStatus::Queued,
    };

    Ok(VideoGenerationJob {
        id: job.id.clone(),
        status,
        provider_data: serde_json::json!({ "api_key": api_key }),
    })
}

/// Download the generated video
pub(crate) async fn download(
    job: &VideoGenerationJob,
) -> Result<VideoGenerationResponse> {
    if !matches!(job.status, VideoGenerationStatus::Completed) {
        return Err(Error::Parse("Video generation not complete".into()));
    }

    let api_key = job.provider_data["api_key"]
        .as_str()
        .ok_or_else(|| Error::Parse("No API key in job data".into()))?;

    let url = format!("https://api.openai.com/v1/videos/{}/content", job.id);

    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| Error::Network(format!("Failed to download video: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(Error::ProviderError {
            provider: "OpenAI".to_string(),
            message: format!("Download failed HTTP {}: {}", status, error_text),
            status_code: Some(status.as_u16()),
        });
    }

    let video = response
        .bytes()
        .await
        .map_err(|e| Error::Parse(format!("Failed to read video data: {}", e)))?
        .to_vec();

    Ok(VideoGenerationResponse {
        video,
        duration_seconds: None, // OpenAI doesn't include this in the download
    })
}
