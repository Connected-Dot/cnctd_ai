use serde::{Deserialize, Serialize};

pub mod gateway;
pub mod server;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Auth {
    Bearer(String),
    None,
}
