use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct InvalidateRequest {
    pub session_salt: String,
}

/// POST /obfuscation/invalidate
///
/// Removes a cached obfuscation session so the next request for that salt
/// re-fetches from the source URL. Authenticated with the same bearer token
/// used by the source URL (`OBFUSCATION_SOURCE_TOKEN`).
pub async fn invalidate_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<InvalidateRequest>,
) -> StatusCode {
    let cache = match &state.session_cache {
        Some(c) => c,
        None => return StatusCode::NOT_FOUND,
    };

    // Validate bearer token
    let expected = cache.source_token();
    let provided = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    match provided {
        Some(token) if token == expected => {}
        _ => return StatusCode::UNAUTHORIZED,
    }

    if cache.invalidate(&body.session_salt).await {
        tracing::info!(
            "Invalidated obfuscation session [salt={}...]",
            &body.session_salt[..body.session_salt.len().min(8)]
        );
    }

    StatusCode::OK
}
