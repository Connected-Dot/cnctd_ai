mod openai;
mod gemini;

use std::path::PathBuf;

/// Request for audio transcription
#[derive(Debug, Clone)]
pub struct TranscriptionRequest {
    /// The audio input - file path, URL, or raw bytes
    pub audio: AudioInput,
    /// ISO-639-1 language hint (e.g., "en", "es") to improve accuracy
    pub language: Option<String>,
    /// Request word/segment timestamps
    pub timestamps: bool,
    /// Enable speaker identification (diarization)
    pub diarization: bool,
    /// Context hint to improve accuracy (e.g., proper nouns, domain terms)
    pub prompt: Option<String>,
    /// Override the default transcription model
    pub model: Option<String>,
}

/// Audio input source
#[derive(Debug, Clone)]
pub enum AudioInput {
    /// Path to a local audio file
    FilePath(PathBuf),
    /// URL to fetch audio from
    Url(String),
    /// Raw audio bytes with MIME type
    Bytes { data: Vec<u8>, mime_type: String },
}

impl From<PathBuf> for AudioInput {
    fn from(path: PathBuf) -> Self {
        AudioInput::FilePath(path)
    }
}

impl From<&str> for AudioInput {
    fn from(s: &str) -> Self {
        if s.starts_with("http://") || s.starts_with("https://") {
            AudioInput::Url(s.to_string())
        } else {
            AudioInput::FilePath(PathBuf::from(s))
        }
    }
}

impl From<String> for AudioInput {
    fn from(s: String) -> Self {
        if s.starts_with("http://") || s.starts_with("https://") {
            AudioInput::Url(s)
        } else {
            AudioInput::FilePath(PathBuf::from(s))
        }
    }
}

/// Response from transcription
#[derive(Debug, Clone)]
pub struct TranscriptionResponse {
    /// Full transcript text
    pub text: String,
    /// Timestamped segments (if requested)
    pub segments: Option<Vec<TranscriptSegment>>,
    /// Identified speakers (if diarization was requested)
    pub speakers: Option<Vec<Speaker>>,
    /// Detected language (ISO-639-1)
    pub language: Option<String>,
    /// Audio duration in seconds
    pub duration: Option<f64>,
}

/// A segment of transcribed text with timing
#[derive(Debug, Clone)]
pub struct TranscriptSegment {
    /// Start time in seconds
    pub start: f64,
    /// End time in seconds
    pub end: f64,
    /// Transcribed text for this segment
    pub text: String,
    /// Speaker identifier (if diarization enabled)
    pub speaker: Option<String>,
}

/// Speaker information from diarization
#[derive(Debug, Clone)]
pub struct Speaker {
    /// Unique speaker identifier
    pub id: String,
    /// Optional human-readable label (e.g., "Speaker 1")
    pub label: Option<String>,
}

impl Default for TranscriptionRequest {
    fn default() -> Self {
        Self {
            audio: AudioInput::FilePath(PathBuf::new()),
            language: None,
            timestamps: false,
            diarization: false,
            prompt: None,
            model: None,
        }
    }
}

impl TranscriptionRequest {
    /// Create a new transcription request from an audio source
    pub fn new(audio: impl Into<AudioInput>) -> Self {
        Self {
            audio: audio.into(),
            ..Default::default()
        }
    }

    /// Set the language hint
    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = Some(language.into());
        self
    }

    /// Enable timestamps in the response
    pub fn with_timestamps(mut self) -> Self {
        self.timestamps = true;
        self
    }

    /// Enable speaker diarization
    pub fn with_diarization(mut self) -> Self {
        self.diarization = true;
        self
    }

    /// Add a context prompt
    pub fn with_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.prompt = Some(prompt.into());
        self
    }

    /// Override the default model
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }
}

// Re-export implementations for use by Client
pub(crate) use openai::transcribe as openai_transcribe;
pub(crate) use gemini::transcribe as gemini_transcribe;
