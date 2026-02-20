use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn list_models(State(state): State<AppState>) -> Json<Value> {
    let mut providers = Vec::new();

    if state.config.anthropic_api_key.is_some() {
        providers.push("anthropic");
    }
    if state.config.openai_api_key.is_some() {
        providers.push("openai");
    }
    if state.config.google_api_key.is_some() {
        providers.push("google");
    }
    if state.config.ollama_base_url.is_some() {
        providers.push("ollama");
    }

    Json(json!({
        "providers": providers,
    }))
}
