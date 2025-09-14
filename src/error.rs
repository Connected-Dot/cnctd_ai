use thiserror::Error;

#[derive(Debug, Error)]
pub enum AiError {
    #[error("http: {0}")] Http(#[from] reqwest::Error),
    #[error("provider: {0}")] Provider(String),
    #[error("empty response")]
    Empty,
}
