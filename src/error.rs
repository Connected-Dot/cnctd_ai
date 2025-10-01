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
    #[error("unsupported provider")]
    Unsupported,
}
