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
    
    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),
    
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    
    #[error("Network error: {0}")]
    Network(String),
    
    #[error("Parse error: {0}")]
    Parse(String),
    
    #[error("Tool execution error: {0}")]
    ToolExecution(String),
    
    #[error("Anthropic SDK error: {0}")]
    AnthropicError(String),
    
    #[error("OpenAI SDK error: {0}")]
    OpenAiError(#[from] async_openai::error::OpenAIError),
    
    #[error("Gemini API error: {0}")]
    GeminiError(String),
    
    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),
    
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),

    #[error("Audio file too large: {size_bytes} bytes (max: {max_bytes} bytes)")]
    AudioTooLarge {
        size_bytes: u64,
        max_bytes: u64,
    },

    #[error("Unsupported audio format: {0}")]
    UnsupportedAudioFormat(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("WebSocket error: {0}")]
    WebSocketError(String),

    #[error("Realtime session closed")]
    RealtimeSessionClosed,

    #[error("{0}")]
    Other(String),
}

impl Error {
    /// Parse an Anthropic error string to extract status code and classify error type
    pub fn from_anthropic_error(error_msg: String) -> Self {
        // Parse error patterns from Anthropic SDK
        // Format examples:
        // "Authentication failed: invalid x-api-key"
        // "Resource not found: model: invalid-model-name"
        // "Bad request: messages: at least one message is required"
        // "HTTP 401: {\"error\": {...}}"
        
        // Check for authentication errors
        if error_msg.to_lowercase().contains("authentication failed") 
            || error_msg.to_lowercase().contains("invalid x-api-key")
            || error_msg.contains("401") {
            return Error::AuthenticationFailed(error_msg);
        }
        
        // Check for rate limiting
        if error_msg.to_lowercase().contains("rate limit") 
            || error_msg.contains("429") {
            return Error::RateLimited { retry_after: None };
        }
        
        // Check for validation errors
        if error_msg.to_lowercase().contains("bad request") 
            || error_msg.to_lowercase().contains("invalid") 
            || error_msg.contains("400") {
            return Error::InvalidRequest(error_msg);
        }
        
        // Extract HTTP status code if present
        let status_code = if let Some(pos) = error_msg.find("HTTP ") {
            let status_str = &error_msg[pos + 5..];
            if let Some(end) = status_str.find(|c: char| !c.is_numeric()) {
                status_str[..end].parse::<u16>().ok()
            } else {
                None
            }
        } else {
            None
        };
        
        // Return as ProviderError with extracted status code
        if let Some(code) = status_code {
            Error::ProviderError {
                provider: "Anthropic".to_string(),
                message: error_msg,
                status_code: Some(code),
            }
        } else {
            // Fallback to generic Anthropic error
            Error::AnthropicError(error_msg)
        }
    }
    
    /// Parse a Gemini error string to classify error type
    pub fn from_gemini_error(error_msg: String) -> Self {
        // Check for authentication errors
        if error_msg.to_lowercase().contains("api key")
            || error_msg.contains("401")
            || error_msg.contains("403") {
            return Error::AuthenticationFailed(error_msg);
        }
        
        // Check for rate limiting
        if error_msg.to_lowercase().contains("rate limit")
            || error_msg.to_lowercase().contains("quota")
            || error_msg.contains("429") {
            return Error::RateLimited { retry_after: None };
        }
        
        // Check for validation errors
        if error_msg.to_lowercase().contains("invalid")
            || error_msg.contains("400") {
            return Error::InvalidRequest(error_msg);
        }
        
        // Fallback to generic Gemini error
        Error::GeminiError(error_msg)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
