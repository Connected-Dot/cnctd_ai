mod gemini;
mod openai;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::ImageContent;

/// Video content for vision-capable models
/// Supports base64-encoded video data or file references
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoContent {
    /// Base64-encoded video data (for inline content)
    pub data: String,
    /// MIME type (video/mp4, video/webm, etc.)
    pub media_type: String,
}

impl VideoContent {
    /// Create new video content from base64 data and MIME type
    pub fn new(data: impl Into<String>, media_type: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            media_type: media_type.into(),
        }
    }

    /// Create from MP4 base64 data
    pub fn mp4(data: impl Into<String>) -> Self {
        Self::new(data, "video/mp4")
    }

    /// Create from WebM base64 data
    pub fn webm(data: impl Into<String>) -> Self {
        Self::new(data, "video/webm")
    }

    /// Create from MOV base64 data
    pub fn mov(data: impl Into<String>) -> Self {
        Self::new(data, "video/quicktime")
    }

    /// Create from AVI base64 data
    pub fn avi(data: impl Into<String>) -> Self {
        Self::new(data, "video/avi")
    }

    /// Load video from file and encode as base64
    pub async fn from_file(path: impl AsRef<std::path::Path>) -> crate::Result<Self> {
        let path = path.as_ref();
        let data = tokio::fs::read(path).await.map_err(|e| {
            crate::Error::Other(format!("Failed to read video file: {}", e))
        })?;

        let mime_type = mime_type_from_extension(path);
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data);

        Ok(Self::new(encoded, mime_type))
    }
}

/// Video input source for analysis requests
#[derive(Debug, Clone)]
pub enum VideoInput {
    /// Path to a local video file
    FilePath(PathBuf),
    /// URL to fetch video from (including YouTube URLs)
    Url(String),
    /// Raw video bytes with MIME type
    Bytes { data: Vec<u8>, mime_type: String },
    /// Pre-extracted frames (for OpenAI which doesn't support native video)
    Frames(Vec<ImageContent>),
}

impl VideoInput {
    /// Check if this is a YouTube URL
    pub fn is_youtube(&self) -> bool {
        match self {
            VideoInput::Url(url) => {
                url.contains("youtube.com") || url.contains("youtu.be")
            }
            _ => false,
        }
    }
}

impl From<PathBuf> for VideoInput {
    fn from(path: PathBuf) -> Self {
        VideoInput::FilePath(path)
    }
}

impl From<&str> for VideoInput {
    fn from(s: &str) -> Self {
        if s.starts_with("http://") || s.starts_with("https://") {
            VideoInput::Url(s.to_string())
        } else {
            VideoInput::FilePath(PathBuf::from(s))
        }
    }
}

impl From<String> for VideoInput {
    fn from(s: String) -> Self {
        if s.starts_with("http://") || s.starts_with("https://") {
            VideoInput::Url(s)
        } else {
            VideoInput::FilePath(PathBuf::from(s))
        }
    }
}

impl From<Vec<ImageContent>> for VideoInput {
    fn from(frames: Vec<ImageContent>) -> Self {
        VideoInput::Frames(frames)
    }
}

/// Request for video analysis
#[derive(Debug, Clone)]
pub struct VideoAnalysisRequest {
    /// The video input - file path, URL, bytes, or pre-extracted frames
    pub video: VideoInput,
    /// The prompt/question for video analysis
    pub prompt: String,
    /// Frame sampling rate in frames per second (default 1.0, Gemini only)
    pub fps: Option<f32>,
    /// Start offset in seconds for video clipping (Gemini only)
    pub start_offset: Option<f64>,
    /// End offset in seconds for video clipping (Gemini only)
    pub end_offset: Option<f64>,
    /// Override the default model
    pub model: Option<String>,
}

impl VideoAnalysisRequest {
    /// Create a new video analysis request
    pub fn new(video: impl Into<VideoInput>, prompt: impl Into<String>) -> Self {
        Self {
            video: video.into(),
            prompt: prompt.into(),
            fps: None,
            start_offset: None,
            end_offset: None,
            model: None,
        }
    }

    /// Create a video analysis request from pre-extracted frames (for OpenAI)
    pub fn from_frames(frames: Vec<ImageContent>, prompt: impl Into<String>) -> Self {
        Self {
            video: VideoInput::Frames(frames),
            prompt: prompt.into(),
            fps: None,
            start_offset: None,
            end_offset: None,
            model: None,
        }
    }

    /// Set the frame sampling rate (Gemini only)
    pub fn with_fps(mut self, fps: f32) -> Self {
        self.fps = Some(fps);
        self
    }

    /// Set the start offset for video clipping (Gemini only)
    pub fn with_start_offset(mut self, seconds: f64) -> Self {
        self.start_offset = Some(seconds);
        self
    }

    /// Set the end offset for video clipping (Gemini only)
    pub fn with_end_offset(mut self, seconds: f64) -> Self {
        self.end_offset = Some(seconds);
        self
    }

    /// Override the default model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

/// Response from video analysis
#[derive(Debug, Clone)]
pub struct VideoAnalysisResponse {
    /// Analysis text from the model
    pub text: String,
    /// Video duration in seconds (if available)
    pub duration: Option<f64>,
}

/// Infer MIME type from file extension
fn mime_type_from_extension(path: &std::path::Path) -> String {
    match path.extension().and_then(|e| e.to_str()) {
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("mov") => "video/quicktime",
        Some("avi") => "video/avi",
        Some("mkv") => "video/x-matroska",
        Some("flv") => "video/x-flv",
        Some("wmv") => "video/x-ms-wmv",
        Some("mpeg") | Some("mpg") => "video/mpeg",
        Some("3gp") | Some("3gpp") => "video/3gpp",
        _ => "video/mp4", // Default
    }
    .to_string()
}

// Re-export implementations for use by Client
pub(crate) use gemini::analyze as gemini_analyze;
pub(crate) use openai::analyze as openai_analyze;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_input_from_str() {
        // File path
        let input: VideoInput = "/path/to/video.mp4".into();
        assert!(matches!(input, VideoInput::FilePath(_)));

        // URL
        let input: VideoInput = "https://example.com/video.mp4".into();
        assert!(matches!(input, VideoInput::Url(_)));

        // YouTube URL
        let input: VideoInput = "https://www.youtube.com/watch?v=abc123".into();
        assert!(matches!(input, VideoInput::Url(_)));
        assert!(input.is_youtube());

        // YouTube short URL
        let input: VideoInput = "https://youtu.be/abc123".into();
        assert!(input.is_youtube());
    }

    #[test]
    fn test_video_content_creation() {
        let content = VideoContent::mp4("base64data");
        assert_eq!(content.media_type, "video/mp4");

        let content = VideoContent::webm("base64data");
        assert_eq!(content.media_type, "video/webm");
    }

    #[test]
    fn test_mime_type_detection() {
        assert_eq!(
            mime_type_from_extension(std::path::Path::new("video.mp4")),
            "video/mp4"
        );
        assert_eq!(
            mime_type_from_extension(std::path::Path::new("video.webm")),
            "video/webm"
        );
        assert_eq!(
            mime_type_from_extension(std::path::Path::new("video.mov")),
            "video/quicktime"
        );
    }
}
