use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct InvalidateRequest {
    #[serde(default)]
    pub session_salt: Option<String>,
}

/// POST /obfuscation/invalidate
///
/// If `session_salt` is provided, removes that specific cached session.
/// If omitted, clears all cached sessions (bulk invalidation).
/// Authenticated with the same bearer token used by the source URL
/// (`OBFUSCATION_SOURCE_TOKEN`).
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

    match body.session_salt {
        Some(salt) => {
            if cache.invalidate(&salt).await {
                tracing::info!(
                    "Invalidated obfuscation session [salt={}...]",
                    &salt[..salt.len().min(8)]
                );
            }
        }
        None => {
            let count = cache.invalidate_all().await;
            tracing::info!("Invalidated all obfuscation sessions ({})", count);
        }
    }

    StatusCode::OK
}
