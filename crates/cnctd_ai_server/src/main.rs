mod config;
mod error;
mod obfuscation;
mod routes;
mod state;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tracing_subscriber::EnvFilter;

use config::Config;
use state::AppState;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("cnctd_ai_server=info".parse().unwrap()))
        .init();

    let config = Config::from_env();
    let port = config.port;
    let state = AppState::new(config).await;

    let app = Router::new()
        .route("/health", get(routes::health::health))
        .route("/models", get(routes::models::list_models))
        .route("/chat", post(routes::chat::chat_stream))
        .route("/tools", get(routes::chat::list_tools))
        .route("/agents/run", post(routes::agents::run_agent))
        .route("/agents/runs/{run_id}", get(routes::agents::get_run))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("cnctd_ai_server listening on {addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
