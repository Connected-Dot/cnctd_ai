use std::time::Duration;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Provider error ({provider}): {message}")]
    ProviderError {
        provider: String,
        message: String,
        status_code: Option<u16>,
    },
    
    #[error("Invalid request: {0}")]
    InvalidRequest(String),
    
    #[error("Rate limited: retry after {retry_after:?}")]
    RateLimited { 
        retry_after: Option<Duration> 
    },
    
    #[error("Authentication failed")]
    AuthenticationFailed,
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("Anthropic SDK error: {0}")]
    AnthropicError(String),
    
    #[error("OpenAI SDK error: {0}")]
    OpenAiError(#[from] async_openai::error::OpenAIError),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;