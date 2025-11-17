use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::client::ProviderAPI;


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AskConfig {
    pub model: String,
    pub api: ProviderAPI,
    pub url: String,
    pub api_key: String,
    pub request_timeout: Duration,
}

impl AskConfig {
    pub fn new(model: String, api: ProviderAPI, api_key: String, url: Option<String>, request_timeout: Option<Duration>) -> Self {
        let url = match api {
            ProviderAPI::OpenAI => url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            ProviderAPI::Anthropic => url.unwrap_or_else(|| "https://api.anthropic.com/v1".to_string()),
        };
        let request_timeout = request_timeout.unwrap_or_else(|| Duration::from_secs(30));
        Self {
            model,
            api,
            url,
            api_key,
            request_timeout,
        }
    }

    pub fn default_openai(api_key: String) -> Self {
        Self::new("gpt-5-nano".to_string(), ProviderAPI::OpenAI, api_key, None, None)
    }

    pub fn default_anthropic(api_key: String) -> Self {
        Self::new("sonnet".to_string(), ProviderAPI::Anthropic, api_key, None, None)
    }
}
