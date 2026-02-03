use serde::{Deserialize, Serialize};
use crate::tts::Voice;

/// Audio format for realtime audio streams
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RealtimeAudioFormat {
    /// PCM 16-bit audio (default)
    #[default]
    Pcm16,
    /// G.711 u-law
    G711Ulaw,
    /// G.711 a-law
    G711Alaw,
}

impl RealtimeAudioFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            RealtimeAudioFormat::Pcm16 => "pcm16",
            RealtimeAudioFormat::G711Ulaw => "g711_ulaw",
            RealtimeAudioFormat::G711Alaw => "g711_alaw",
        }
    }
}

/// Modality for realtime sessions
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Modality {
    /// Text input/output
    Text,
    /// Audio input/output
    Audio,
}

/// Voice activity detection (VAD) configuration
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VadConfig {
    /// Type of VAD: "server_vad" or "none"
    #[serde(rename = "type")]
    pub vad_type: String,
    /// Activation threshold (0.0-1.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<f32>,
    /// Silence duration in ms before speech end
    #[serde(skip_serializing_if = "Option::is_none")]
    pub silence_duration_ms: Option<u32>,
    /// Prefix padding in ms
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefix_padding_ms: Option<u32>,
}

impl Default for VadConfig {
    fn default() -> Self {
        Self {
            vad_type: "server_vad".to_string(),
            threshold: Some(0.5),
            silence_duration_ms: Some(500),
            prefix_padding_ms: Some(300),
        }
    }
}

/// Configuration for a realtime audio session
#[derive(Clone, Debug)]
pub struct RealtimeConfig {
    /// Model to use (e.g., "gpt-4o-realtime-preview")
    pub model: String,
    /// Voice for audio output
    pub voice: Voice,
    /// Modalities to enable
    pub modalities: Vec<Modality>,
    /// System instructions for the session
    pub instructions: Option<String>,
    /// Input audio format
    pub input_audio_format: RealtimeAudioFormat,
    /// Output audio format
    pub output_audio_format: RealtimeAudioFormat,
    /// Voice activity detection config
    pub vad_config: Option<VadConfig>,
    /// Temperature for generation
    pub temperature: Option<f32>,
    /// Maximum output tokens
    pub max_output_tokens: Option<u32>,
}

impl Default for RealtimeConfig {
    fn default() -> Self {
        Self {
            model: "gpt-4o-realtime-preview".to_string(),
            voice: Voice::Alloy,
            modalities: vec![Modality::Text, Modality::Audio],
            instructions: None,
            input_audio_format: RealtimeAudioFormat::Pcm16,
            output_audio_format: RealtimeAudioFormat::Pcm16,
            vad_config: Some(VadConfig::default()),
            temperature: None,
            max_output_tokens: None,
        }
    }
}

impl RealtimeConfig {
    /// Create a new config with the specified model
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            ..Default::default()
        }
    }

    /// Set the voice for audio output
    pub fn with_voice(mut self, voice: Voice) -> Self {
        self.voice = voice;
        self
    }

    /// Set the modalities (text, audio, or both)
    pub fn with_modalities(mut self, modalities: Vec<Modality>) -> Self {
        self.modalities = modalities;
        self
    }

    /// Set system instructions
    pub fn with_instructions(mut self, instructions: impl Into<String>) -> Self {
        self.instructions = Some(instructions.into());
        self
    }

    /// Set input audio format
    pub fn with_input_format(mut self, format: RealtimeAudioFormat) -> Self {
        self.input_audio_format = format;
        self
    }

    /// Set output audio format
    pub fn with_output_format(mut self, format: RealtimeAudioFormat) -> Self {
        self.output_audio_format = format;
        self
    }

    /// Disable voice activity detection (manual turn management)
    pub fn without_vad(mut self) -> Self {
        self.vad_config = None;
        self
    }

    /// Set temperature for generation
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set maximum output tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_output_tokens = Some(max_tokens);
        self
    }
}
