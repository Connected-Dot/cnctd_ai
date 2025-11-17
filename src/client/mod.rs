pub mod config;
pub mod options;

pub use config::{AnthropicConfig, OpenAiConfig};
pub use options::ClientOptions;

#[derive(Clone)]
pub struct Client {
    provider: ProviderType,
    options: ClientOptions,
}

impl Client {
    pub fn anthropic(
    config: AnthropicConfig,
    options: Option<ClientOptions>,
) -> Result<Self, crate::error::Error> {
    let options = options.unwrap_or_default();
    
    Ok(Self {
        provider: ProviderType::Anthropic { config },
        options,
    })
}
    
    pub fn openai(
        config: OpenAiConfig,
        options: Option<ClientOptions>,
    ) -> Result<Self, crate::error::Error> {
        let options = options.unwrap_or_default();
        
        // Create the OpenAI config
        let mut openai_config = async_openai::config::OpenAIConfig::new()
            .with_api_key(&config.api_key);
        
        if let Some(org) = &config.organization {
            openai_config = openai_config.with_org_id(org);
        }
        
        if let Some(base_url) = &options.base_url {
            openai_config = openai_config.with_api_base(base_url);
        }
        
        let sdk_client = async_openai::Client::with_config(openai_config);
        
        Ok(Self {
            provider: ProviderType::OpenAi {
                sdk_client,
                config,
            },
            options,
        })
    }

     pub async fn complete(
        &self,
        request: crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::response::CompletionResponse> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                self.complete_anthropic(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                self.complete_openai(sdk_client, config, &request).await
            }
        }
    }
    
    async fn complete_anthropic(
        &self,
        config: &AnthropicConfig,
        request: &crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::response::CompletionResponse> {
        use anthropic_sdk::Client as AnthropicClient;
        use serde_json::json;
        
        // Separate system messages from user/assistant messages
        let system_message = request.messages
            .iter()
            .find(|m| matches!(m.role, crate::message::Role::System))
            .map(|m| m.content.clone());
        
        // Convert user/assistant messages to Anthropic format
        let messages: Vec<serde_json::Value> = request.messages
            .iter()
            .filter(|m| !matches!(m.role, crate::message::Role::System))
            .map(|msg| {
                let role = match msg.role {
                    crate::message::Role::User => "user",
                    crate::message::Role::Assistant => "assistant",
                    crate::message::Role::System => "user", // shouldn't hit this due to filter
                };
                json!({
                    "role": role,
                    "content": msg.content
                })
            })
            .collect();
        
        // Build the request using Anthropic's builder
        let mut client_builder = AnthropicClient::new()
            .auth(&config.api_key)
            .model(&config.model)
            .messages(&json!(messages));
        
        // Add system message if present
        if let Some(system) = system_message {
            client_builder = client_builder.system(&system);
        }
        
        // Apply options - must set max_tokens (required by Anthropic)
        let max_tokens = request.options
            .as_ref()
            .and_then(|o| o.max_tokens)
            .unwrap_or(4096);
        client_builder = client_builder.max_tokens(max_tokens as i32);
        
        if let Some(opts) = &request.options {
            if let Some(temp) = opts.temperature {
                client_builder = client_builder.temperature(temp);
            }
            if let Some(top_p) = opts.top_p {
                client_builder = client_builder.top_p(top_p.into());
            }
        }
        
        // Set version if provided
        if let Some(version) = &config.version {
            client_builder = client_builder.version(version);
        } else {
            client_builder = client_builder.version("2023-06-01");
        }
        
        let anthropic_request = client_builder
            .build()
            .map_err(|e| crate::error::Error::AnthropicError(e.to_string()))?;
        
        // Collect response chunks
        let response_text = std::sync::Arc::new(tokio::sync::Mutex::new(String::new()));
        let response_text_clone = response_text.clone();
        
        anthropic_request
            .execute(move |chunk| {
                let text = response_text_clone.clone();
                async move {
                    text.lock().await.push_str(&chunk);
                }
            })
            .await
            .map_err(|e| crate::error::Error::AnthropicError(e.to_string()))?;
        
        let final_text = response_text.lock().await.clone();
        
        // Create response
        let message = crate::message::Message {
            role: crate::message::Role::Assistant,
            content: final_text,
        };
        
        Ok(crate::response::CompletionResponse {
            message,
            usage: crate::response::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            },
            finish_reason: crate::response::FinishReason::Stop,
            model: config.model.clone(),
        })
    }
    
    async fn complete_openai(
        &self,
        sdk_client: &async_openai::Client<async_openai::config::OpenAIConfig>,
        config: &OpenAiConfig,
        request: &crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::response::CompletionResponse> {
        use async_openai::types::{
            ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
            ChatCompletionRequestUserMessageArgs, ChatCompletionRequestAssistantMessageArgs,
            CreateChatCompletionRequestArgs,
        };
        
        // Convert our messages to OpenAI format
        let mut messages = Vec::new();
        for msg in &request.messages {
            let openai_msg = match msg.role {
                crate::message::Role::System => {
                    ChatCompletionRequestMessage::System(
                        ChatCompletionRequestSystemMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    )
                }
                crate::message::Role::User => {
                    ChatCompletionRequestMessage::User(
                        ChatCompletionRequestUserMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    )
                }
                crate::message::Role::Assistant => {
                    ChatCompletionRequestMessage::Assistant(
                        ChatCompletionRequestAssistantMessageArgs::default()
                            .content(msg.content.clone())
                            .build()?
                    )
                }
            };
            messages.push(openai_msg);
        }
        
        // Build the request
        let mut request_builder = CreateChatCompletionRequestArgs::default();
        request_builder.model(&config.model).messages(messages);
        
        // Apply options if provided
        if let Some(opts) = &request.options {
            if let Some(temp) = opts.temperature {
                request_builder.temperature(temp);
            }
            if let Some(max_tokens) = opts.max_tokens {
                request_builder.max_tokens(max_tokens);
            }
            if let Some(top_p) = opts.top_p {
                request_builder.top_p(top_p);
            }
            if let Some(stop) = &opts.stop_sequences {
                request_builder.stop(stop.clone());
            }
        }
        
        let openai_request = request_builder
            .build()?;
        
        // Make the API call
        let response = sdk_client
            .chat()
            .create(openai_request)
            .await?;
        
        // Convert response back to our format
        let choice = response
            .choices
            .first()
            .ok_or_else(|| crate::error::Error::Other("No response from OpenAI".into()))?;
        
        let message = crate::message::Message {
            role: crate::message::Role::Assistant,
            content: choice.message.content.clone().unwrap_or_default(),
        };
        
        // Handle optional usage
        let usage = if let Some(usage) = &response.usage {
            crate::response::Usage {
                prompt_tokens: usage.prompt_tokens,
                completion_tokens: usage.completion_tokens,
                total_tokens: usage.total_tokens,
            }
        } else {
            crate::response::Usage {
                prompt_tokens: 0,
                completion_tokens: 0,
                total_tokens: 0,
            }
        };
        
        let finish_reason = match &choice.finish_reason {
            Some(async_openai::types::FinishReason::Stop) => crate::response::FinishReason::Stop,
            Some(async_openai::types::FinishReason::Length) => crate::response::FinishReason::Length,
            Some(async_openai::types::FinishReason::ContentFilter) => crate::response::FinishReason::ContentFilter,
            _ => crate::response::FinishReason::Other,
        };
        
        Ok(crate::response::CompletionResponse {
            message,
            usage,
            finish_reason,
            model: response.model,
        })
    }
}

#[derive(Clone)]
enum ProviderType {
    Anthropic {
        config: AnthropicConfig,
    },
    OpenAi {
        sdk_client: async_openai::Client<async_openai::config::OpenAIConfig>,
        config: OpenAiConfig,
    },
}
