mod openai;
mod gemini;

use serde::{Deserialize, Serialize};

/// Voice options for TTS
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Voice {
    // OpenAI voices
    Alloy,
    Ash,
    Ballad,
    Coral,
    Echo,
    Fable,
    Onyx,
    Nova,
    Sage,
    Shimmer,
    Verse,
    Marin,
    Cedar,
    // Gemini voices (presets)
    Puck,
    Charon,
    Kore,
    Fenrir,
    Aoede,
    // Custom voice by name
    Custom(String),
}

impl Default for Voice {
    fn default() -> Self {
        Voice::Nova // Good default for both providers
    }
}

impl Voice {
    /// Get the voice name as a string (for OpenAI and realtime APIs)
    pub fn as_str(&self) -> &str {
        self.to_openai_string()
    }

    /// Convert to OpenAI voice name
    pub fn to_openai_string(&self) -> &str {
        match self {
            Voice::Alloy => "alloy",
            Voice::Ash => "ash",
            Voice::Ballad => "ballad",
            Voice::Coral => "coral",
            Voice::Echo => "echo",
            Voice::Fable => "fable",
            Voice::Onyx => "onyx",
            Voice::Nova => "nova",
            Voice::Sage => "sage",
            Voice::Shimmer => "shimmer",
            Voice::Verse => "verse",
            Voice::Marin => "marin",
            Voice::Cedar => "cedar",
            // Map Gemini voices to closest OpenAI equivalent
            Voice::Puck => "nova",
            Voice::Charon => "onyx",
            Voice::Kore => "shimmer",
            Voice::Fenrir => "echo",
            Voice::Aoede => "alloy",
            Voice::Custom(name) => name.as_str(),
        }
    }

    /// Convert to Gemini voice name
    pub fn to_gemini_string(&self) -> &str {
        match self {
            Voice::Puck => "Puck",
            Voice::Charon => "Charon",
            Voice::Kore => "Kore",
            Voice::Fenrir => "Fenrir",
            Voice::Aoede => "Aoede",
            // Map OpenAI voices to closest Gemini equivalent
            Voice::Nova | Voice::Alloy => "Puck",
            Voice::Onyx | Voice::Echo => "Charon",
            Voice::Shimmer | Voice::Coral => "Kore",
            Voice::Fable | Voice::Sage => "Fenrir",
            Voice::Ash | Voice::Ballad | Voice::Verse | Voice::Marin | Voice::Cedar => "Aoede",
            Voice::Custom(name) => name.as_str(),
        }
    }
}

/// Audio output format
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum AudioFormat {
    #[default]
    Mp3,
    Opus,
    Aac,
    Flac,
    Wav,
    Pcm,
}

impl AudioFormat {
    pub fn to_string(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Opus => "opus",
            AudioFormat::Aac => "aac",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::Pcm => "pcm",
        }
    }

    pub fn mime_type(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "audio/mpeg",
            AudioFormat::Opus => "audio/opus",
            AudioFormat::Aac => "audio/aac",
            AudioFormat::Flac => "audio/flac",
            AudioFormat::Wav => "audio/wav",
            AudioFormat::Pcm => "audio/pcm",
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            AudioFormat::Mp3 => "mp3",
            AudioFormat::Opus => "opus",
            AudioFormat::Aac => "aac",
            AudioFormat::Flac => "flac",
            AudioFormat::Wav => "wav",
            AudioFormat::Pcm => "pcm",
        }
    }
}

/// Request for text-to-speech generation
#[derive(Debug, Clone)]
pub struct SpeechRequest {
    /// The text to convert to speech
    pub text: String,
    /// Voice to use
    pub voice: Voice,
    /// Audio output format
    pub format: AudioFormat,
    /// Speed of speech (0.25 to 4.0, default 1.0)
    pub speed: f32,
    /// Instructions for how the voice should speak (OpenAI gpt-4o-mini-tts only)
    pub instructions: Option<String>,
    /// Override the default model
    pub model: Option<String>,
}

impl SpeechRequest {
    /// Create a new speech request with default settings
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice: Voice::default(),
            format: AudioFormat::default(),
            speed: 1.0,
            instructions: None,
            model: None,
        }
    }

    /// Set the voice
    pub fn with_voice(mut self, voice: Voice) -> Self {
        self.voice = voice;
        self
    }

    /// Set the audio format
    pub fn with_format(mut self, format: AudioFormat) -> Self {
        self.format = format;
        self
    }

    /// Set the speed (0.25 to 4.0)
    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed.clamp(0.25, 4.0);
        self
    }

    /// Set instructions for voice style (OpenAI gpt-4o-mini-tts only)
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Override the default model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    // Convenience voice setters
    pub fn voice_alloy(self) -> Self { self.with_voice(Voice::Alloy) }
    pub fn voice_nova(self) -> Self { self.with_voice(Voice::Nova) }
    pub fn voice_onyx(self) -> Self { self.with_voice(Voice::Onyx) }
    pub fn voice_shimmer(self) -> Self { self.with_voice(Voice::Shimmer) }
    pub fn voice_echo(self) -> Self { self.with_voice(Voice::Echo) }
    pub fn voice_fable(self) -> Self { self.with_voice(Voice::Fable) }
}

/// Response from text-to-speech generation
#[derive(Debug, Clone)]
pub struct SpeechResponse {
    /// Raw audio data
    pub audio: Vec<u8>,
    /// Audio format
    pub format: AudioFormat,
}

impl SpeechResponse {
    /// Save the audio to a file
    pub async fn save(&self, path: impl AsRef<std::path::Path>) -> crate::Result<()> {
        tokio::fs::write(path, &self.audio).await.map_err(|e| {
            crate::Error::Other(format!("Failed to save audio: {}", e))
        })
    }

    /// Save with auto-generated extension based on format
    pub async fn save_with_extension(&self, path_without_ext: impl AsRef<std::path::Path>) -> crate::Result<std::path::PathBuf> {
        let path = path_without_ext.as_ref().with_extension(self.format.extension());
        self.save(&path).await?;
        Ok(path)
    }

    /// Get the audio data as bytes
    pub fn as_bytes(&self) -> &[u8] {
        &self.audio
    }

    /// Get the MIME type
    pub fn mime_type(&self) -> &'static str {
        self.format.mime_type()
    }
}

// Re-export implementations for use by Client
pub(crate) use openai::generate as openai_generate;
pub(crate) use gemini::generate as gemini_generate;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_builder() {
        let request = SpeechRequest::new("Hello, world!")
            .with_voice(Voice::Nova)
            .with_speed(1.5)
            .with_format(AudioFormat::Mp3);

        assert_eq!(request.text, "Hello, world!");
        assert_eq!(request.speed, 1.5);
    }

    #[test]
    fn test_speed_clamping() {
        let request = SpeechRequest::new("Test").with_speed(10.0);
        assert_eq!(request.speed, 4.0);

        let request = SpeechRequest::new("Test").with_speed(0.1);
        assert_eq!(request.speed, 0.25);
    }

    #[test]
    fn test_voice_conversions() {
        assert_eq!(Voice::Nova.to_openai_string(), "nova");
        assert_eq!(Voice::Puck.to_gemini_string(), "Puck");
    }
}
