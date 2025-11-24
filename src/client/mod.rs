pub mod config;
pub mod options;
mod anthropic;
mod openai;
mod gemini;

pub use config::{AnthropicConfig, OpenAiConfig, GeminiConfig};
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

    pub fn gemini(
        config: GeminiConfig,
        options: Option<ClientOptions>,
    ) -> Result<Self, crate::error::Error> {
        let options = options.unwrap_or_default();

        Ok(Self {
            provider: ProviderType::Gemini { config },
            options,
        })
    }

    pub async fn complete(
        &self,
        request: crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::response::CompletionResponse> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                anthropic::complete(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                openai::complete(sdk_client, config, &request).await
            }
            ProviderType::Gemini { config } => {
                gemini::complete(config, &request).await
            }
        }
    }

    pub async fn complete_stream(
        &self,
        request: crate::request::CompletionRequest,
    ) -> crate::error::Result<crate::stream::CompletionStream> {
        match &self.provider {
            ProviderType::Anthropic { config } => {
                anthropic::stream(config, &request).await
            }
            ProviderType::OpenAi { sdk_client, config } => {
                openai::stream(sdk_client, config, &request).await
            }
            ProviderType::Gemini { config } => {
                gemini::stream(config, &request).await
            }
        }
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
    Gemini {
        config: GeminiConfig,
    },
}
