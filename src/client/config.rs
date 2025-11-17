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
}