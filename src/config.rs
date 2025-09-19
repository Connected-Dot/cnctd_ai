// src/config.rs
//! Minimal config for cnctd_ai.
//! Load once at startup (e.g., from env or a config file).

use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::types::ProviderId;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AiConfig {
    /// Which provider to use by default (can be overridden in AskRequest).
    pub default_provider: ProviderId,
    /// Default model for that provider.
    pub default_model: String,

    /// OpenAI or OpenRouter API key.
    pub openai_api_key: Option<String>,
    /// Base URL: "https://api.openai.com/v1" or "https://openrouter.ai/api/v1"
    pub openai_base_url: String,

    /// Anthropic Claude API key (if configured).
    pub anthropic_api_key: Option<String>,
    pub anthropic_base_url: String,

    /// Global timeout for requests.
    pub request_timeout: Duration,
}

impl AiConfig {
    /// Load from environment variables (good enough for dev/prod).
    pub fn from_env() -> Self {
        Self {
            default_provider: std::env::var("AI_DEFAULT_PROVIDER")
                .ok()
                .and_then(|s| match s.as_str() {
                    "openai" => Some(ProviderId::OpenAI),
                    "openrouter" => Some(ProviderId::OpenRouter),
                    "anthropic" => Some(ProviderId::Anthropic),
                    _ => None,
                })
                .unwrap_or(ProviderId::OpenAI),
            default_model: std::env::var("AI_DEFAULT_MODEL")
                .unwrap_or_else(|_| "gpt-4.1-mini".to_string()),

            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            openai_base_url: std::env::var("OPENAI_BASE_URL")
                .unwrap_or_else(|_| "https://api.openai.com/v1".to_string()),

            anthropic_api_key: std::env::var("ANTHROPIC_API_KEY").ok(),
            anthropic_base_url: std::env::var("ANTHROPIC_BASE_URL")
                .unwrap_or_else(|_| "https://api.anthropic.com/v1".to_string()),

            request_timeout: Duration::from_secs(
                std::env::var("AI_TIMEOUT_SECS")
                    .ok()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(30),
            ),
        }
    }
}
