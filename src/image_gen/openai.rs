use crate::client::config::OpenAiConfig;
use crate::error::{Error, Result};
use super::{ImageGenerationRequest, ImageGenerationResponse, GeneratedImage};
use serde::Deserialize;

/// Default model for OpenAI image generation
const DEFAULT_MODEL: &str = "gpt-image-1";

/// Generate images using OpenAI's GPT Image API
pub(crate) async fn generate(
    _sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
    config: &OpenAiConfig,
    request: &ImageGenerationRequest,
) -> Result<ImageGenerationResponse> {
    // Determine model to use
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // Build URL
    let url = "https://api.openai.com/v1/images/generations";

    // Build request body
    let body = serde_json::json!({
        "model": model,
        "prompt": request.prompt,
        "n": request.n,
        "size": request.aspect_ratio.to_openai_size(),
        "quality": request.quality.to_openai_string(),
        "output_format": request.format.to_string()
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
    let response_json: OpenAiImageResponse = response
        .json()
        .await
        .map_err(|e| Error::Parse(format!("Failed to parse response: {}", e)))?;

    // Convert to our format
    let images = response_json
        .data
        .into_iter()
        .map(|img| GeneratedImage {
            data: img.b64_json,
            mime_type: request.format.mime_type().to_string(),
            revised_prompt: img.revised_prompt,
        })
        .collect();

    Ok(ImageGenerationResponse { images })
}

#[derive(Deserialize)]
struct OpenAiImageResponse {
    data: Vec<OpenAiImageData>,
}

#[derive(Deserialize)]
struct OpenAiImageData {
    b64_json: String,
    revised_prompt: Option<String>,
}
