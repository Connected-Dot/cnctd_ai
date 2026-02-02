#[derive(Clone, Debug)]
pub struct AnthropicConfig {
    pub api_key: String,
    pub model: String,
    pub version: Option<String>,
}

#[derive(Clone, Debug)]
pub struct OpenAiConfig {
    pub api_key: String,
    pub model: String,
    pub organization: Option<String>,
    /// Model to use for transcription (default: "whisper-1")
    /// Options: "whisper-1", "gpt-4o-transcribe", "gpt-4o-mini-transcribe"
    pub transcription_model: Option<String>,
}

#[derive(Clone, Debug)]
pub struct GeminiConfig {
    pub api_key: String,
    pub model: String,
}
