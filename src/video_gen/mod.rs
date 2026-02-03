mod gemini;
mod openai;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Video aspect ratio
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum VideoAspectRatio {
    #[default]
    Landscape,  // 16:9
    Portrait,   // 9:16
    Square,     // 1:1
}

impl VideoAspectRatio {
    pub fn to_gemini_string(&self) -> &'static str {
        match self {
            VideoAspectRatio::Landscape => "16:9",
            VideoAspectRatio::Portrait => "9:16",
            VideoAspectRatio::Square => "1:1",
        }
    }

    pub fn to_openai_size(&self, is_pro: bool) -> &'static str {
        match (self, is_pro) {
            (VideoAspectRatio::Landscape, true) => "1792x1024",
            (VideoAspectRatio::Landscape, false) => "1280x720",
            (VideoAspectRatio::Portrait, true) => "1024x1792",
            (VideoAspectRatio::Portrait, false) => "720x1280",
            (VideoAspectRatio::Square, _) => "1280x720", // No square support, default to landscape
        }
    }
}

/// Video resolution
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum VideoResolution {
    #[default]
    HD720p,
    HD1080p,
    UHD4K,
}

impl VideoResolution {
    pub fn to_gemini_string(&self) -> &'static str {
        match self {
            VideoResolution::HD720p => "720p",
            VideoResolution::HD1080p => "1080p",
            VideoResolution::UHD4K => "4k",
        }
    }
}

/// Video duration in seconds
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum VideoDuration {
    #[default]
    Short4s,
    Medium8s,
    Long12s,
}

impl VideoDuration {
    pub fn to_seconds(&self) -> u8 {
        match self {
            VideoDuration::Short4s => 4,
            VideoDuration::Medium8s => 8,
            VideoDuration::Long12s => 12,
        }
    }

    pub fn to_openai_string(&self) -> &'static str {
        match self {
            VideoDuration::Short4s => "4",
            VideoDuration::Medium8s => "8",
            VideoDuration::Long12s => "12",
        }
    }
}

/// Input image for image-to-video generation
#[derive(Debug, Clone)]
pub struct VideoInputImage {
    /// Base64-encoded image data
    pub data: String,
    /// MIME type (image/png, image/jpeg)
    pub mime_type: String,
}

impl VideoInputImage {
    /// Create from base64 data
    pub fn from_base64(data: String, mime_type: &str) -> Self {
        Self {
            data,
            mime_type: mime_type.to_string(),
        }
    }

    /// Create from file path
    pub async fn from_file(path: impl AsRef<std::path::Path>) -> crate::Result<Self> {
        use base64::Engine;

        let path = path.as_ref();
        let data = tokio::fs::read(path).await.map_err(|e| {
            crate::Error::Other(format!("Failed to read image file: {}", e))
        })?;

        let mime_type = match path.extension().and_then(|e| e.to_str()) {
            Some("png") => "image/png",
            Some("jpg") | Some("jpeg") => "image/jpeg",
            Some("webp") => "image/webp",
            _ => "image/png",
        };

        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);

        Ok(Self {
            data: encoded,
            mime_type: mime_type.to_string(),
        })
    }
}

/// Request for video generation
#[derive(Debug, Clone)]
pub struct VideoGenerationRequest {
    /// Text prompt describing the video
    pub prompt: String,
    /// Duration of the video
    pub duration: VideoDuration,
    /// Aspect ratio
    pub aspect_ratio: VideoAspectRatio,
    /// Resolution (Gemini only)
    pub resolution: VideoResolution,
    /// Optional first frame image (image-to-video)
    pub first_frame: Option<VideoInputImage>,
    /// Optional last frame image (Gemini only)
    pub last_frame: Option<VideoInputImage>,
    /// Negative prompt (Gemini only)
    pub negative_prompt: Option<String>,
    /// Override default model
    pub model: Option<String>,
}

impl VideoGenerationRequest {
    /// Create a new video generation request
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            duration: VideoDuration::default(),
            aspect_ratio: VideoAspectRatio::default(),
            resolution: VideoResolution::default(),
            first_frame: None,
            last_frame: None,
            negative_prompt: None,
            model: None,
        }
    }

    /// Set the duration
    pub fn with_duration(mut self, duration: VideoDuration) -> Self {
        self.duration = duration;
        self
    }

    /// Set 4-second duration
    pub fn short(mut self) -> Self {
        self.duration = VideoDuration::Short4s;
        self
    }

    /// Set 8-second duration
    pub fn medium(mut self) -> Self {
        self.duration = VideoDuration::Medium8s;
        self
    }

    /// Set 12-second duration (OpenAI only)
    pub fn long(mut self) -> Self {
        self.duration = VideoDuration::Long12s;
        self
    }

    /// Set aspect ratio
    pub fn with_aspect_ratio(mut self, ratio: VideoAspectRatio) -> Self {
        self.aspect_ratio = ratio;
        self
    }

    /// Set landscape (16:9)
    pub fn landscape(mut self) -> Self {
        self.aspect_ratio = VideoAspectRatio::Landscape;
        self
    }

    /// Set portrait (9:16)
    pub fn portrait(mut self) -> Self {
        self.aspect_ratio = VideoAspectRatio::Portrait;
        self
    }

    /// Set resolution (Gemini only)
    pub fn with_resolution(mut self, resolution: VideoResolution) -> Self {
        self.resolution = resolution;
        self
    }

    /// Set 1080p resolution
    pub fn hd(mut self) -> Self {
        self.resolution = VideoResolution::HD1080p;
        self
    }

    /// Set 4K resolution
    pub fn uhd(mut self) -> Self {
        self.resolution = VideoResolution::UHD4K;
        self
    }

    /// Set first frame for image-to-video
    pub fn with_first_frame(mut self, image: VideoInputImage) -> Self {
        self.first_frame = Some(image);
        self
    }

    /// Set last frame (Gemini only)
    pub fn with_last_frame(mut self, image: VideoInputImage) -> Self {
        self.last_frame = Some(image);
        self
    }

    /// Set negative prompt (Gemini only)
    pub fn with_negative_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.negative_prompt = Some(prompt.into());
        self
    }

    /// Override the default model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

/// Status of a video generation job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VideoGenerationStatus {
    Queued,
    InProgress { progress: Option<f32> },
    Completed,
    Failed { error: String },
}

/// Handle to a pending video generation job
#[derive(Debug, Clone)]
pub struct VideoGenerationJob {
    /// Job/operation ID
    pub id: String,
    /// Current status
    pub status: VideoGenerationStatus,
    /// Provider-specific data
    pub(crate) provider_data: serde_json::Value,
}

impl VideoGenerationJob {
    /// Check if the job is complete
    pub fn is_complete(&self) -> bool {
        matches!(self.status, VideoGenerationStatus::Completed | VideoGenerationStatus::Failed { .. })
    }

    /// Check if the job failed
    pub fn is_failed(&self) -> bool {
        matches!(self.status, VideoGenerationStatus::Failed { .. })
    }
}

/// Response from video generation
#[derive(Debug, Clone)]
pub struct VideoGenerationResponse {
    /// Video data (MP4)
    pub video: Vec<u8>,
    /// Duration in seconds
    pub duration_seconds: Option<f32>,
}

impl VideoGenerationResponse {
    /// Save the video to a file
    pub async fn save(&self, path: impl AsRef<std::path::Path>) -> crate::Result<()> {
        tokio::fs::write(path, &self.video).await.map_err(|e| {
            crate::Error::Other(format!("Failed to save video: {}", e))
        })
    }

    /// Save with .mp4 extension
    pub async fn save_mp4(&self, path_without_ext: impl AsRef<std::path::Path>) -> crate::Result<PathBuf> {
        let path = path_without_ext.as_ref().with_extension("mp4");
        self.save(&path).await?;
        Ok(path)
    }
}

// Re-export implementations for use by Client
pub(crate) use gemini::generate as gemini_generate;
pub(crate) use gemini::poll_status as gemini_poll;
pub(crate) use gemini::download as gemini_download;
pub(crate) use openai::generate as openai_generate;
pub(crate) use openai::poll_status as openai_poll;
pub(crate) use openai::download as openai_download;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder() {
        let request = VideoGenerationRequest::new("A sunset over the ocean")
            .medium()
            .landscape()
            .hd();

        assert_eq!(request.prompt, "A sunset over the ocean");
        assert!(matches!(request.duration, VideoDuration::Medium8s));
        assert!(matches!(request.aspect_ratio, VideoAspectRatio::Landscape));
    }

    #[test]
    fn test_aspect_ratio_conversions() {
        assert_eq!(VideoAspectRatio::Landscape.to_gemini_string(), "16:9");
        assert_eq!(VideoAspectRatio::Portrait.to_openai_size(false), "720x1280");
        assert_eq!(VideoAspectRatio::Landscape.to_openai_size(true), "1792x1024");
    }

    #[test]
    fn test_duration_conversions() {
        assert_eq!(VideoDuration::Medium8s.to_seconds(), 8);
        assert_eq!(VideoDuration::Long12s.to_openai_string(), "12");
    }
}
