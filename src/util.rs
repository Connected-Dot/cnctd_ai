use core::error;

use serde::de::DeserializeOwned;

use crate::{ask::config::AskConfig, error::AiError};

pub fn parse_json<T: DeserializeOwned>(s: &str) -> Result<T, String> {
    if let Ok(v) = serde_json::from_str::<T>(s) {
        return Ok(v);
    }
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if let Ok(v) = serde_json::from_str::<T>(&s[start..=end]) {
            return Ok(v);
        }
    }
    let cleaned = s
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    serde_json::from_str::<T>(cleaned).map_err(|e| e.to_string())
}

pub fn get_http_client(ask_config: &AskConfig) -> Result<reqwest::Client, AiError> {
    Ok(
        reqwest::Client::builder()
            .user_agent(format!("cnctd-ai-{}-api", ask_config.api.to_string().to_lowercase()))
            .timeout(ask_config.request_timeout)
            .build()
            .map_err(|e| AiError::Http(e.to_string()))?
    )
}