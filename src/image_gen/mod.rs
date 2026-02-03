mod gemini;
mod openai;

use serde::{Deserialize, Serialize};

/// Aspect ratio for generated images
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum AspectRatio {
    #[default]
    Square,      // 1:1
    Portrait,    // 2:3 or 9:16
    Landscape,   // 3:2 or 16:9
    Wide,        // 21:9
    Custom(u32, u32), // Custom ratio
}

impl AspectRatio {
    /// Convert to Gemini aspect ratio string
    pub fn to_gemini_string(&self) -> &'static str {
        match self {
            AspectRatio::Square => "1:1",
            AspectRatio::Portrait => "9:16",
            AspectRatio::Landscape => "16:9",
            AspectRatio::Wide => "21:9",
            AspectRatio::Custom(_, _) => "1:1", // Default for custom
        }
    }

    /// Convert to OpenAI size string
    pub fn to_openai_size(&self) -> &'static str {
        match self {
            AspectRatio::Square => "1024x1024",
            AspectRatio::Portrait => "1024x1536",
            AspectRatio::Landscape => "1536x1024",
            AspectRatio::Wide => "1536x1024", // OpenAI doesn't have 21:9
            AspectRatio::Custom(_, _) => "1024x1024",
        }
    }
}

/// Image quality level
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum ImageQuality {
    Low,
    #[default]
    Medium,
    High,
}

impl ImageQuality {
    pub fn to_openai_string(&self) -> &'static str {
        match self {
            ImageQuality::Low => "low",
            ImageQuality::Medium => "medium",
            ImageQuality::High => "high",
        }
    }
}

/// Image output format
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
    Webp,
}

impl ImageFormat {
    pub fn to_string(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::Webp => "webp",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            ImageFormat::Png => "image/png",
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Webp => "image/webp",
        }
    }
}

/// Request for image generation
#[derive(Debug, Clone)]
pub struct ImageGenerationRequest {
    /// The text prompt describing the image to generate
    pub prompt: String,
    /// Number of images to generate (1-4 for most providers)
    pub n: u8,
    /// Aspect ratio of the generated image
    pub aspect_ratio: AspectRatio,
    /// Quality level
    pub quality: ImageQuality,
    /// Output format
    pub format: ImageFormat,
    /// Override the default model
    pub model: Option<String>,
}

impl ImageGenerationRequest {
    /// Create a new image generation request with default settings
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            n: 1,
            aspect_ratio: AspectRatio::default(),
            quality: ImageQuality::default(),
            format: ImageFormat::default(),
            model: None,
        }
    }

    /// Set the number of images to generate
    pub fn with_count(mut self, n: u8) -> Self {
        self.n = n.clamp(1, 10);
        self
    }

    /// Set the aspect ratio
    pub fn with_aspect_ratio(mut self, ratio: AspectRatio) -> Self {
        self.aspect_ratio = ratio;
        self
    }

    /// Set square aspect ratio (1:1)
    pub fn square(mut self) -> Self {
        self.aspect_ratio = AspectRatio::Square;
        self
    }

    /// Set portrait aspect ratio (9:16)
    pub fn portrait(mut self) -> Self {
        self.aspect_ratio = AspectRatio::Portrait;
        self
    }

    /// Set landscape aspect ratio (16:9)
    pub fn landscape(mut self) -> Self {
        self.aspect_ratio = AspectRatio::Landscape;
        self
    }

    /// Set the quality level
    pub fn with_quality(mut self, quality: ImageQuality) -> Self {
        self.quality = quality;
        self
    }

    /// Set high quality
    pub fn high_quality(mut self) -> Self {
        self.quality = ImageQuality::High;
        self
    }

    /// Set the output format
    pub fn with_format(mut self, format: ImageFormat) -> Self {
        self.format = format;
        self
    }

    /// Override the default model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

/// A single generated image
#[derive(Debug, Clone)]
pub struct GeneratedImage {
    /// Base64-encoded image data
    pub data: String,
    /// MIME type (image/png, image/jpeg, image/webp)
    pub mime_type: String,
    /// Revised prompt (if the model modified it)
    pub revised_prompt: Option<String>,
}

impl GeneratedImage {
    /// Decode the base64 image data to bytes
    pub fn decode(&self) -> crate::Result<Vec<u8>> {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(&self.data)
            .map_err(|e| crate::Error::Parse(format!("Failed to decode image: {}", e)))
    }

    /// Save the image to a file
    pub async fn save(&self, path: impl AsRef<std::path::Path>) -> crate::Result<()> {
        let bytes = self.decode()?;
        tokio::fs::write(path, bytes).await.map_err(|e| {
            crate::Error::Other(format!("Failed to save image: {}", e))
        })
    }
}

/// Response from image generation
#[derive(Debug, Clone)]
pub struct ImageGenerationResponse {
    /// Generated images
    pub images: Vec<GeneratedImage>,
}

impl ImageGenerationResponse {
    /// Get the first generated image
    pub fn first(&self) -> Option<&GeneratedImage> {
        self.images.first()
    }

    /// Save all images to a directory with auto-generated names
    pub async fn save_all(&self, dir: impl AsRef<std::path::Path>, prefix: &str) -> crate::Result<Vec<std::path::PathBuf>> {
        let dir = dir.as_ref();
        tokio::fs::create_dir_all(dir).await.map_err(|e| {
            crate::Error::Other(format!("Failed to create directory: {}", e))
        })?;

        let mut paths = Vec::new();
        for (i, image) in self.images.iter().enumerate() {
            let ext = match image.mime_type.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                _ => "png",
            };
            let path = dir.join(format!("{}_{}.{}", prefix, i + 1, ext));
            image.save(&path).await?;
            paths.push(path);
        }
        Ok(paths)
    }
}

// Re-export implementations for use by Client
pub(crate) use gemini::generate as gemini_generate;
pub(crate) use openai::generate as openai_generate;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder() {
        let request = ImageGenerationRequest::new("A sunset over mountains")
            .with_count(2)
            .landscape()
            .high_quality();

        assert_eq!(request.prompt, "A sunset over mountains");
        assert_eq!(request.n, 2);
        assert!(matches!(request.aspect_ratio, AspectRatio::Landscape));
        assert!(matches!(request.quality, ImageQuality::High));
    }

    #[test]
    fn test_aspect_ratio_conversions() {
        assert_eq!(AspectRatio::Square.to_gemini_string(), "1:1");
        assert_eq!(AspectRatio::Portrait.to_gemini_string(), "9:16");
        assert_eq!(AspectRatio::Landscape.to_openai_size(), "1536x1024");
    }
}
