//! Batch processing support for AI providers
//!
//! This module provides a unified API for batch processing across multiple AI providers.
//! Batch processing allows you to submit up to 10,000 requests at once for asynchronous
//! processing, typically at a 50% cost reduction with a 24-hour SLA.
//!
//! # Supported Providers
//!
//! - **Anthropic**: Full support via Message Batches API
//! - **OpenAI**: Full support via Batch API
//! - **Gemini**: Not supported (returns `Error::UnsupportedOperation`)
//!
//! # Example
//!
//! ```rust,no_run
//! use cnctd_ai::{Client, CompletionRequest, Message, Role};
//! use cnctd_ai::batch::{BatchItem, BatchAwaitOptions};
//!
//! async fn example() -> cnctd_ai::Result<()> {
//!     let client = Client::anthropic("your-api-key", "claude-sonnet-4-20250514");
//!
//!     // Create batch items
//!     let items = vec![
//!         BatchItem::new("request-1", CompletionRequest {
//!             messages: vec![Message::user("Hello!")],
//!             ..Default::default()
//!         }),
//!         BatchItem::new("request-2", CompletionRequest {
//!             messages: vec![Message::user("Hi there!")],
//!             ..Default::default()
//!         }),
//!     ];
//!
//!     // Submit batch
//!     let batch = client.create_batch(items).await?;
//!     println!("Created batch: {}", batch.id);
//!
//!     // Wait for completion and get results
//!     let results = client.await_batch(&batch.id, None).await?;
//!     for result in results {
//!         println!("{}: {:?}", result.custom_id, result.is_success());
//!     }
//!
//!     Ok(())
//! }
//! ```

mod types;
pub(crate) mod anthropic;
pub(crate) mod openai;

pub use types::*;
