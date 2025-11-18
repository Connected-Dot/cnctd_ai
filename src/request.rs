use serde::{Deserialize, Serialize};
use crate::{Tool, message::Message};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub messages: Vec<Message>,
    pub tools: Option<Vec<Tool>>,
    pub options: Option<RequestOptions>,
}

impl CompletionRequest {
    pub fn add_tool(&mut self, tool: Tool) {
        if let Some(ref mut tools) = self.tools {
            tools.push(tool);
        } else {
            self.tools = Some(vec![tool]);
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RequestOptions {
    pub temperature: Option<f32>,
    pub max_tokens: Option<u32>,
    pub top_p: Option<f32>,
    pub stop_sequences: Option<Vec<String>>,
}