use std::pin::Pin;

use futures_core::Stream;
use serde_json::{json, Value};

use crate::ask::config::AskConfig;
use crate::ask::request::{AskChunk, AskRequest};
use crate::client::anthropic::AnthropicApi;
use crate::client::openai::OpenAiApi;
use crate::client::ProviderAPI;
use crate::error::AiError;

pub mod error;
pub mod client;
pub mod ask;
// pub mod types;
pub mod util;

pub struct CnctdAi;

impl CnctdAi {
    pub async fn ask(ask_request: AskRequest, ask_config: AskConfig) -> Result<Value, AiError> {
        let ask_response = match ask_config.api {
            ProviderAPI::OpenAI => OpenAiApi::ask(ask_config, &ask_request).await?,
            ProviderAPI::Anthropic => AnthropicApi::ask(ask_config, &ask_request).await?,
        };

        Ok(json!(ask_response) )
    }

    pub async fn ask_stream<'a>(ask_request: &'a AskRequest, ask_config: AskConfig) -> Result<Pin<Box<dyn Stream<Item = Result<AskChunk, AiError>> + Send + 'a>>, AiError> {
        match ask_config.api {
            ProviderAPI::OpenAI => {
                let s = OpenAiApi::ask_stream(ask_config, ask_request).await?;
                let s: Pin<Box<dyn Stream<Item = Result<AskChunk, AiError>> + Send + 'a>> = Box::pin(s);
                Ok(s)
            }
            ProviderAPI::Anthropic => {
                let s = AnthropicApi::ask_stream(ask_config, ask_request).await?;
                let s: Pin<Box<dyn Stream<Item = Result<AskChunk, AiError>> + Send + 'a>> = Box::pin(s);
                Ok(s)
            }
        }
    }

    pub async fn get_models(ask_config: &AskConfig) -> Result<Value, AiError> {
        let models = match ask_config.api {
            ProviderAPI::OpenAI => OpenAiApi::get_models(ask_config).await?,
            ProviderAPI::Anthropic => AnthropicApi::get_models(ask_config).await?,
        };

        Ok(models)
    }
}


// fn openrouter_client() -> Result<Client<OpenAIConfig>, Box<dyn std::error::Error>> {
//     let key = std::env::var("OPENROUTER_API_KEY")?;

//     // Optional but recommended attribution headers (helps with rate limits/analytics on their end)
//     let mut headers = HeaderMap::new();
//     headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", key))?);
//     headers.insert("Content-Type", HeaderValue::from_static("application/json"));

//     let http = reqwest::Client::builder()
//         .user_agent("cnctd-ai/0.1")
//         .default_headers(headers)
//         .build()?;

//     let cfg = OpenAIConfig::new().with_api_base("https://openrouter.ai/api/v1");
//     Ok(Client::with_config(cfg).with_http_client(http))
// }