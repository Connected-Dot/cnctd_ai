use crate::client::config::GeminiConfig;
use crate::error::{Error, Result};
use super::{ImageGenerationRequest, ImageGenerationResponse, GeneratedImage};
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};

/// Default model for Gemini image generation (Nano Banana)
/// Options:
/// - "gemini-2.0-flash-exp" - Experimental (older)
/// - "gemini-2.5-flash-image" - Nano Banana (stable, 1024px)
/// - "gemini-3-pro-image-preview" - Nano Banana Pro (up to 4K)
const DEFAULT_MODEL: &str = "gemini-3-pro-image-preview";

/// Generate images using Gemini's native image generation (Nano Banana)
pub(crate) async fn generate(
    config: &GeminiConfig,
    request: &ImageGenerationRequest,
) -> Result<ImageGenerationResponse> {
    // Determine model to use
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // Build headers
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    // Build URL
    let url = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
        model,
        config.api_key
    );

    // Build request body
    // Gemini uses generateContent with responseModalities including IMAGE
    let body = serde_json::json!({
        "contents": [{
            "parts": [{
                "text": request.prompt
            }]
        }],
        "generationConfig": {
            "responseModalities": ["IMAGE"],
            "imageConfig": {
                "aspectRatio": request.aspect_ratio.to_gemini_string()
            }
        }
    });

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

    // Extract images from the response
    let images = extract_images_from_response(&response_json, &request.format)?;

    Ok(ImageGenerationResponse { images })
}

/// Extract generated images from Gemini response
fn extract_images_from_response(
    response: &serde_json::Value,
    format: &super::ImageFormat,
) -> Result<Vec<GeneratedImage>> {
    let candidates = response["candidates"]
        .as_array()
        .ok_or_else(|| Error::GeminiError("No candidates in response".into()))?;

    let mut images = Vec::new();

    for candidate in candidates {
        let parts = candidate["content"]["parts"]
            .as_array()
            .ok_or_else(|| Error::GeminiError("No parts in response".into()))?;

        for part in parts {
            // Check for inline image data
            if let Some(inline_data) = part.get("inlineData") {
                let data = inline_data["data"]
                    .as_str()
                    .ok_or_else(|| Error::GeminiError("No data in inlineData".into()))?;

                let mime_type = inline_data["mimeType"]
                    .as_str()
                    .unwrap_or(format.mime_type())
                    .to_string();

                images.push(GeneratedImage {
                    data: data.to_string(),
                    mime_type,
                    revised_prompt: None,
                });
            }
        }
    }

    if images.is_empty() {
        return Err(Error::GeminiError("No images in response".into()));
    }

    Ok(images)
}
