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
        use anthropic_sdk::{Anthropic, MessageCreateBuilder};
        
        // Create the Anthropic client
        let client = Anthropic::new(&config.api_key)
            .map_err(|e| crate::error::Error::AnthropicError(e.to_string()))?;
        
        // Build the message request
        let mut builder = MessageCreateBuilder::new(&config.model, 
            request.options.as_ref().and_then(|o| o.max_tokens).unwrap_or(4096));
        
        // Add system message if present
        if let Some(system_msg) = request.messages.iter().find(|m| matches!(m.role, crate::message::Role::System)) {
            builder = builder.system(&system_msg.content);
        }
        
        // Add user/assistant messages
        for msg in request.messages.iter().filter(|m| !matches!(m.role, crate::message::Role::System)) {
            match msg.role {
                crate::message::Role::User => {
                    builder = builder.user(&*msg.content);
                }
                crate::message::Role::Assistant => {
                    builder = builder.assistant(&*msg.content);
                }
                crate::message::Role::System => {} // Already handled
            }
        }
        
        // Apply options
        if let Some(opts) = &request.options {
            if let Some(temp) = opts.temperature {
                builder = builder.temperature(temp);
            }
            if let Some(top_p) = opts.top_p {
                builder = builder.top_p(top_p);
            }
        }
        
        // Make the API call
        let anthropic_response = client.messages()
            .create(builder.build())
            .await
            .map_err(|e| crate::error::Error::AnthropicError(e.to_string()))?;
        
        // Extract text content from response
        let content = anthropic_response.content
            .iter()
            .filter_map(|block| {
                if let anthropic_sdk::ContentBlock::Text { text } = block {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("");
        
        // Create our response
        let message = crate::message::Message {
            role: crate::message::Role::Assistant,
            content,
        };
        
        let usage = crate::response::Usage {
            prompt_tokens: anthropic_response.usage.input_tokens,
            completion_tokens: anthropic_response.usage.output_tokens,
            total_tokens: anthropic_response.usage.input_tokens + anthropic_response.usage.output_tokens,
        };
        
        let finish_reason = match anthropic_response.stop_reason {
            Some(anthropic_sdk::StopReason::EndTurn) => crate::response::FinishReason::Stop,
            Some(anthropic_sdk::StopReason::MaxTokens) => crate::response::FinishReason::Length,
            Some(anthropic_sdk::StopReason::StopSequence) => crate::response::FinishReason::Stop,
            _ => crate::response::FinishReason::Other,
        };
        
        Ok(crate::response::CompletionResponse {
            message,
            usage,
            finish_reason,
            model: anthropic_response.model,
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
