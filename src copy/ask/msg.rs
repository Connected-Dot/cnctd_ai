use serde::{Deserialize, Serialize};

/// A simple chat role set. Expand later if you add multi-part content.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// One message in your canonical transcript.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Msg {
    pub role: Role,
    pub content: String,
    #[serde(default)]
    pub name: Option<String>, // optional sender label
}