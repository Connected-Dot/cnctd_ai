#[derive(thiserror::Error, Debug)]
pub enum AiError {
    #[error("auth failed")]
    Auth,
    #[error("rate limited")]
    RateLimited,
    #[error("timeout")]
    Timeout,
    #[error("provider error: {0}")]
    Provider(String),
    #[error("json error: {0}")]
    Json(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("unknown model: {0}")]
    UnknownModel(String),
    #[error("mcp error: {0}")]
    McpError(String),
    #[error("unsupported provider")]
    Unsupported,
}

impl AiError {
    pub fn from_reqwest_error(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            AiError::Timeout
        } else if err.status() == Some(reqwest::StatusCode::UNAUTHORIZED) {
            AiError::Auth
        } else if err.status() == Some(reqwest::StatusCode::TOO_MANY_REQUESTS) {
            AiError::RateLimited
        } else {
            AiError::Http(err.to_string())
        }
    }

    pub fn from_serde_error(err: serde_json::Error) -> Self {
        AiError::Json(err.to_string())
    }

    pub fn from_mcp_error(err: String) -> Self {
        AiError::McpError(err)
    }

    
}

impl From<std::io::Error> for AiError {
    fn from(err: std::io::Error) -> Self {
        AiError::Provider(err.to_string())
    }
}