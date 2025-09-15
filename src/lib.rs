use async_openai::Client;
use async_openai::types::{
    CreateChatCompletionRequestArgs,
    ChatCompletionRequestUserMessageArgs,   // <- use role-specific builders
    // ChatCompletionRequestSystemMessageArgs, // add if you want a system prompt
};
use serde_json::{json, Value};

pub struct CnctdAi;

impl CnctdAi {
    pub async fn ask(msg: &str) -> Result<Value, async_openai::error::OpenAIError> {
        let client = Client::new();

        // pick a real model; keep it in env so you can swap providers
        let model = std::env::var("AI_MODEL").unwrap_or_else(|_| "gpt-5".to_string());

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

        let response = client.chat().create(request).await?;

        let answer = response.clone().choices
            .into_iter()
            .find_map(|c| c.message.content)
            .unwrap_or_default();

        Ok(json!({ "response": answer, "raw": response }) )
    }
}
