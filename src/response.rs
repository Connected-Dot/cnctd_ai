use serde::{Deserialize, Serialize};
use crate::message::Message;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionResponse {
    pub message: Message,
    pub usage: Usage,
    pub finish_reason: FinishReason,
    pub model: String,
}

impl CompletionResponse {
    /// Convenience method to get the text content
    pub fn text(&self) -> &str {
        &self.message.content
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    Length,
    ContentFilter,
    #[serde(other)]
    Other,
}
