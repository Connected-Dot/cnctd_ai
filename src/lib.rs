use async_openai::config::OpenAIConfig;
use async_openai::Client;
use async_openai::types::{
    CreateChatCompletionRequestArgs,
    ChatCompletionRequestUserMessageArgs,   // <- use role-specific builders
    // ChatCompletionRequestSystemMessageArgs, // add if you want a system prompt
};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use serde_json::{json, Value};

pub mod config;
pub mod error;
pub mod client;
pub mod types;
pub mod util;

pub struct CnctdAi;

impl CnctdAi {
    pub async fn ask(msg: &str) -> Result<Value, async_openai::error::OpenAIError> {
        // let client = Client::new();
        let client = openrouter_client().map_err(|e| async_openai::error::OpenAIError::StreamError(e.to_string()))?;
        println!("client: {:?}", client);

        let models = client.models().list().await;

        println!("models: {:?}", models);

        // pick a real model; keep it in env so you can swap providers
        // let model = std::env::var("AI_MODEL").unwrap_or_else(|_| "gpt-5".to_string());
        let model = "openrouter/auto".to_string();

        let messages = vec![
            // If you want a system message, add it first:
            // ChatCompletionRequestSystemMessageArgs::default()
            //     .content("You are terse.")
            //     .build()?.into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(msg)
                .build()?
                .into(),
        ];

        let request = CreateChatCompletionRequestArgs::default()
            .model(model)
            .messages(messages)
            .build()?;

        println!("request: {:?}", request);

        let response = client.chat().create(request).await?;
        println!("response: {:?}", response);

        let answer = response.clone().choices
            .into_iter()
            .find_map(|c| c.message.content)
            .unwrap_or_default();

        Ok(json!({ "response": answer, "raw": response }) )
    }

    pub async fn get_models() -> Result<Value, async_openai::error::OpenAIError> {
        let client = Client::new();
        // println!("client: {:?}", client);

        let config = client.config();

        println!("config: {:?}", config);
        let models = client.models().list().await?;
        models.data.iter().for_each(|m| println!("model: {:?}", m));

        Ok(json!({ "models": models.data }) )
    }
}


fn openrouter_client() -> Result<Client<OpenAIConfig>, Box<dyn std::error::Error>> {
    let key = std::env::var("OPENROUTER_API_KEY")?;

    // Optional but recommended attribution headers (helps with rate limits/analytics on their end)
    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", key))?);
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));

    let http = reqwest::Client::builder()
        .user_agent("cnctd-ai/0.1")
        .default_headers(headers)
        .build()?;

    let cfg = OpenAIConfig::new().with_api_base("https://openrouter.ai/api/v1");
    Ok(Client::with_config(cfg).with_http_client(http))
}