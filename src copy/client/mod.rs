//! Universal client facade: `ask` and `ask_json<T>` with a simple provider switch.

// use std::time::Instant;

// use async_openai::Client as OpenAIClient;
// use anthropic_sdk::Client as AnthropicClient;
// use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
// use serde_json::{json, Value};

// use crate::client::openai::get_embedding;
// use crate::error::AiError;
// use crate::util::parse_json;

pub mod openai;
pub mod anthropic; 

/// Provider selector (keep ids stable for client/server).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ProviderAPI {
    OpenAI,       // OpenAI (or Azure OpenAI if your base_url points there)
    Anthropic,    // Claude (adapter to add later)
    // Add others later (e.g., Gemini)
}

impl ProviderAPI {
    pub fn to_string(&self) -> String {
        match self {
            ProviderAPI::OpenAI => "OpenAI".to_string(),
            ProviderAPI::Anthropic => "Anthropic".to_string(),
        }
    }
}


// pub struct UniversalClient {
//     cfg: ClientConfig,
//     http: reqwest::Client,
// }

// impl UniversalClient {
//     pub fn new(cfg: ClientConfig) -> Result<Self, AiError> {
//         let http = reqwest::Client::builder()
//             .user_agent("cnctd-ai/0.1")
//             .timeout(cfg.request_timeout)
//             .build()
//             .map_err(|e| AiError::Provider(e.to_string()))?;
//         Ok(Self { cfg, http })
//     }

//     /// Free-form chat; normalized response.
//     pub async fn ask(&self, mut req: AskRequest) -> Result<AskResponse, AiError> {
//         self.hydrate_defaults(&mut req);

//         let started = Instant::now();
//         let mut resp = match req.provider {
//             ProviderId::OpenAI | ProviderId::OpenRouter | ProviderId::CnctdAI => {
//                 openai::ask(&self.http, &self.cfg, &req).await
//             }
//             ProviderId::Anthropic => anthropic::ask(&self.http, &self.cfg, &req).await,
//         }?;
//         resp.latency_ms = started.elapsed().as_millis();
//         Ok(resp)
//     }

//     /// Strict JSON → deserialize into T. One auto-repair attempt.
//     pub async fn ask_json<T: DeserializeOwned>(&self, mut req: AskRequest) -> Result<T, AiError> {
//         if req.options.json_mode.is_none() { req.options.json_mode = Some(true); }
//         if req.options.temperature.is_none() { req.options.temperature = Some(0.0); }
//         if req.options.max_output_tokens.is_none() { req.options.max_output_tokens = Some(512); }

//         let first = self.ask(req.clone()).await?;
//         if let Ok(v) = parse_json::<T>(&first.text) {
//             return Ok(v);
//         }

//         // one repair attempt
//         let mut msgs = req.messages.clone();
//         msgs.push(crate::types::Msg {
//             role: Role::User,
//             content: "Your previous output was not valid JSON. Re-emit only valid minified JSON. No prose."
//                 .to_string(),
//             name: None,
//         });
//         req.messages = msgs;

//         let second = self.ask(req).await?;
//         parse_json::<T>(&second.text).map_err(AiError::Json)
//     }

//     fn hydrate_defaults(&self, req: &mut AskRequest) {
//         if req.model.is_none() {
//             req.model = Some(self.cfg.default_model.clone());
//         }
//         if req.options.temperature.is_none() {
//             req.options.temperature = Some(0.2);
//         }
//         if req.options.max_output_tokens.is_none() {
//             req.options.max_output_tokens = Some(512);
//         }
//         if req.options.stream.is_none() {
//             req.options.stream = Some(false);
//         }
//     }

//     pub async fn get_models(provider: ProviderId) -> Result<Value, AiError> {
//         let models = match provider {
//             ProviderId::OpenAI | ProviderId::OpenRouter | ProviderId::CnctdAI => {
//                 let client = OpenAIClient::new();
//                 let models = client.models().list().await.map_err(|e| AiError::Provider(e.to_string()))?.data;
//                 models.into_iter().map(|m| m.id).collect::<Vec<String>>()
//             }
//             ProviderId::Anthropic => {
//                 vec!["claude-2".to_string(), "claude-instant-100k".to_string()]
//             }
//         };

//         Ok(json!(models))
//     }

//     pub async fn get_embedding(&self, text: &str) -> Result<Vec<f32>, AiError> {
//         let embedding = get_embedding(&self.http, &self.cfg, text).await?;

//         println!("Embedding: {:?}", embedding);

//         Ok(embedding)
//     }
// }
