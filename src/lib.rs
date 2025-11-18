pub mod error;
pub mod client;
pub mod request;
pub mod response;
pub mod message;
pub mod stream;

pub use client::{Client, ClientOptions, AnthropicConfig, OpenAiConfig};
pub use error::{Error, Result};
pub use message::{Message, Role};
pub use request::{CompletionRequest, RequestOptions};
pub use response::{CompletionResponse, Usage, FinishReason};
pub use stream::{CompletionStream, StreamChunk};