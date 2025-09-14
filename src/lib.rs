pub mod error;
pub mod client;

pub use client::{AiClient, AskRequest, AskResponse};
use serde_json::{json, Value};

use crate::error::AiError;

pub struct CnctdAi;

impl CnctdAi {
    pub async fn ask(msg: &str) -> Result<Value, AiError> {
        Ok(json!({
            "response": format!("You asked Cnctd AI: {}", msg)
        }))
    }
}