use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    BadRequest(String),
    Internal(String),
    ProviderNotConfigured(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadRequest(msg) => write!(f, "Bad request: {msg}"),
            Self::Internal(msg) => write!(f, "Internal error: {msg}"),
            Self::ProviderNotConfigured(p) => write!(f, "Provider not configured: {p}"),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            Self::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg.clone()),
            Self::Internal(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg.clone()),
            Self::ProviderNotConfigured(p) => (
                StatusCode::BAD_REQUEST,
                format!("Provider not configured: {p}"),
            ),
        };
        let body = axum::Json(json!({ "error": message }));
        (status, body).into_response()
    }
}

impl From<cnctd_ai::Error> for AppError {
    fn from(err: cnctd_ai::Error) -> Self {
        Self::Internal(err.to_string())
    }
}
