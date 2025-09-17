use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    Openai,     // OpenAI / Azure-compatible
    Anthropic,  // Claude
    Openrouter, // OpenRouter gateway
}


pub struct AskOptions {
    pub temperature: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub stream: bool,
    pub profile: Option<String>, // e.g., "fast"
}

pub struct AskRequest {
    pub provider: Provider,
    pub model: Option<String>,   // overrides config default
    pub system: Option<String>,
    pub messages: Vec<Msg>,
    pub options: AskOptions,
    pub context_refs: Vec<String>, // conversation/project/doc ids
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Msg {
    pub role: Role,
    pub content: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserClaims {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub exp: Option<i64>,
}

pub struct UniversalResponse {
    pub text: String,
    pub finish_reason: String,    // normalized
    pub usage: Option<Usage>,
    pub latency_ms: u128,
    pub provider_meta: serde_json::Value,
}

pub struct Usage { pub prompt_tokens: Option<u32>, pub completion_tokens: Option<u32>, pub total_tokens: Option<u32> }
