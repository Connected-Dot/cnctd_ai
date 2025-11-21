use std::time::{Duration, Instant};
use serde::{Deserialize, Serialize};

/// Current state of the agent during execution
#[derive(Debug, Clone)]
pub struct AgentState {
    /// Number of iterations completed
    pub iteration: usize,
    
    /// When the agent started
    pub start_time: Instant,
    
    /// Total tokens used across all calls
    pub total_tokens: u32,
    
    /// Whether the agent should continue
    pub should_continue: bool,
    
    /// Reason for stopping (if stopped)
    pub stop_reason: Option<StopReason>,
    
    /// Number of errors encountered
    pub error_count: usize,
    
    /// Number of successful tool calls
    pub successful_tool_calls: usize,
}

impl AgentState {
    pub fn new() -> Self {
        Self {
            iteration: 0,
            start_time: Instant::now(),
            total_tokens: 0,
            should_continue: true,
            stop_reason: None,
            error_count: 0,
            successful_tool_calls: 0,
        }
    }
    
    /// Get elapsed time since agent started
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
    
    /// Check if we've exceeded time limit
    pub fn exceeded_time_limit(&self, max_duration: Option<Duration>) -> bool {
        if let Some(max) = max_duration {
            self.elapsed() > max
        } else {
            false
        }
    }
    
    /// Mark the agent as complete
    pub fn complete(&mut self, reason: StopReason) {
        self.should_continue = false;
        self.stop_reason = Some(reason);
    }
    
    /// Increment iteration counter
    pub fn next_iteration(&mut self) {
        self.iteration += 1;
    }
    
    /// Add tokens to the total
    pub fn add_tokens(&mut self, tokens: u32) {
        self.total_tokens += tokens;
    }
    
    /// Record an error
    pub fn record_error(&mut self) {
        self.error_count += 1;
    }
    
    /// Record a successful tool call
    pub fn record_tool_success(&mut self) {
        self.successful_tool_calls += 1;
    }
}

impl Default for AgentState {
    fn default() -> Self {
        Self::new()
    }
}

/// Reason why the agent stopped
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    /// Task completed successfully
    Success,
    
    /// Hit maximum iteration limit
    MaxIterations,
    
    /// Hit time limit
    Timeout,
    
    /// Model decided it was done (stopped naturally)
    ModelStopped,
    
    /// Encountered an error and stop_on_error is true
    Error,
    
    /// No tools were requested (unusual state)
    NoToolsRequested,
    
    /// User or external cancellation
    Cancelled,
}

impl std::fmt::Display for StopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "Task completed successfully"),
            Self::MaxIterations => write!(f, "Reached maximum iterations"),
            Self::Timeout => write!(f, "Time limit exceeded"),
            Self::ModelStopped => write!(f, "Model indicated completion"),
            Self::Error => write!(f, "Stopped due to error"),
            Self::NoToolsRequested => write!(f, "No tools requested"),
            Self::Cancelled => write!(f, "Cancelled by user"),
        }
    }
}
