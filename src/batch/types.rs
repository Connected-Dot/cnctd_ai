//! Batch API type definitions
//! 
//! These types are provider-agnostic and used for both OpenAI and Anthropic batch APIs.

use serde::{Deserialize, Serialize};
use crate::request::CompletionRequest;
use crate::response::CompletionResponse;

/// A single item in a batch request
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchItem {
    /// Your custom identifier for matching results back to requests
    pub custom_id: String,
    /// The model to use for this request
    pub model: String,
    /// The completion request parameters
    pub request: CompletionRequest,
}

impl BatchItem {
    pub fn new(custom_id: impl Into<String>, model: impl Into<String>, request: CompletionRequest) -> Self {
        Self {
            custom_id: custom_id.into(),
            model: model.into(),
            request,
        }
    }
}

/// Information about a batch job
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchInfo {
    /// Provider's batch ID
    pub id: String,
    /// Current status of the batch
    pub status: BatchStatus,
    /// Request counts (if available)
    pub counts: Option<BatchCounts>,
    /// Unix timestamp when batch was created
    pub created_at: Option<i64>,
    /// Unix timestamp when batch expires
    pub expires_at: Option<i64>,
    /// Unix timestamp when batch completed
    pub completed_at: Option<i64>,
    /// Error message if batch failed
    pub error_message: Option<String>,
}

/// Counts of requests in various states
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BatchCounts {
    pub total: u32,
    pub completed: u32,
    pub failed: u32,
}

/// Status of a batch job
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchStatus {
    /// Batch is being validated
    Validating,
    /// Batch is queued for processing
    InProgress,
    /// Batch is being finalized
    Finalizing,
    /// Batch completed successfully (all items processed, some may have failed)
    Completed,
    /// Batch failed entirely
    Failed,
    /// Batch is being cancelled
    Cancelling,
    /// Batch was cancelled
    Cancelled,
    /// Batch expired before completion
    Expired,
}

impl BatchStatus {
    /// Returns true if the batch is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            BatchStatus::Completed
                | BatchStatus::Failed
                | BatchStatus::Cancelled
                | BatchStatus::Expired
        )
    }
}

/// Result of a single batch item
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchResult {
    /// The custom_id from the original request
    pub custom_id: String,
    /// The result - either success or error
    pub result: BatchResultType,
}

/// The outcome of processing a batch item
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum BatchResultType {
    Success(CompletionResponse),
    Error(BatchItemError),
}

impl BatchResult {
    /// Returns the response if successful
    pub fn success(&self) -> Option<&CompletionResponse> {
        match &self.result {
            BatchResultType::Success(resp) => Some(resp),
            BatchResultType::Error(_) => None,
        }
    }

    /// Returns the error if failed
    pub fn error(&self) -> Option<&BatchItemError> {
        match &self.result {
            BatchResultType::Success(_) => None,
            BatchResultType::Error(err) => Some(err),
        }
    }

    /// Returns true if this item succeeded
    pub fn is_success(&self) -> bool {
        matches!(self.result, BatchResultType::Success(_))
    }
}

/// Error for a single batch item
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BatchItemError {
    pub error_type: String,
    pub message: String,
}

/// Options for awaiting batch completion
#[derive(Clone, Debug)]
pub struct BatchAwaitOptions {
    /// How often to poll for status (default: 10 seconds)
    pub poll_interval: std::time::Duration,
    /// Maximum time to wait (default: 24 hours)
    pub timeout: std::time::Duration,
}

impl Default for BatchAwaitOptions {
    fn default() -> Self {
        Self {
            poll_interval: std::time::Duration::from_secs(10),
            timeout: std::time::Duration::from_secs(24 * 60 * 60),
        }
    }
}
