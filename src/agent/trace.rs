use std::time::Duration;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{CompletionResponse, ToolUse};
use super::StopReason;

/// Complete execution trace for an agent run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTrace {
    /// All events that occurred during execution
    pub events: Vec<TraceEvent>,
    
    /// Final result text (if any)
    pub result: Option<String>,
    
    /// Reason the agent stopped
    pub stop_reason: StopReason,
    
    /// Total execution time
    pub duration: Duration,
    
    /// Total tokens used
    pub total_tokens: u32,
    
    /// Number of iterations
    pub iterations: usize,
    
    /// Number of errors
    pub errors: usize,
    
    /// Number of successful tool calls
    pub successful_tool_calls: usize,
}

impl AgentTrace {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            result: None,
            stop_reason: StopReason::Success,
            duration: Duration::from_secs(0),
            total_tokens: 0,
            iterations: 0,
            errors: 0,
            successful_tool_calls: 0,
        }
    }
    
    /// Add an event to the trace
    pub fn add_event(&mut self, event: TraceEvent) {
        self.events.push(event);
    }
    
    /// Get all tool executions from the trace
    pub fn tool_executions(&self) -> Vec<&ToolExecution> {
        self.events
            .iter()
            .filter_map(|e| match e {
                TraceEvent::ToolExecution(exec) => Some(exec),
                _ => None,
            })
            .collect()
    }
    
    /// Get all errors from the trace
    pub fn errors_trace(&self) -> Vec<&str> {
        self.events
            .iter()
            .filter_map(|e| match e {
                TraceEvent::Error(msg) => Some(msg.as_str()),
                _ => None,
            })
            .collect()
    }
    
    /// Get all model responses
    pub fn model_responses(&self) -> Vec<&String> {
        self.events
            .iter()
            .filter_map(|e| match e {
                TraceEvent::ModelResponse { text, .. } => Some(text),
                _ => None,
            })
            .collect()
    }
    
    /// Print a human-readable summary
    pub fn print_summary(&self) {
        println!("\n=== Agent Execution Summary ===");
        println!("Status: {}", self.stop_reason);
        println!("Duration: {:.2}s", self.duration.as_secs_f64());
        println!("Iterations: {}", self.iterations);
        println!("Tool Calls: {} successful, {} errors", 
            self.successful_tool_calls, self.errors);
        println!("Total Tokens: {}", self.total_tokens);
        
        if let Some(result) = &self.result {
            println!("\n--- Final Result ---");
            println!("{}", result);
        }
        
        if !self.errors_trace().is_empty() {
            println!("\n--- Errors ---");
            for error in self.errors_trace() {
                println!("  • {}", error);
            }
        }
    }
    
    /// Print detailed trace of all events
    pub fn print_detailed(&self) {
        println!("\n=== Detailed Agent Trace ===");
        
        for (i, event) in self.events.iter().enumerate() {
            println!("\n[Event {}] {}", i + 1, event.event_type());
            
            match event {
                TraceEvent::Start { task } => {
                    println!("  Task: {}", task);
                }
                TraceEvent::Iteration { iteration } => {
                    println!("  Iteration: {}", iteration);
                }
                TraceEvent::ModelThinking { text } => {
                    println!("  Thinking: {}", text);
                }
                TraceEvent::ModelResponse { text, tokens, .. } => {
                    println!("  Tokens: {}", tokens);
                    println!("  Response: {}", text);
                }
                TraceEvent::ToolExecution(exec) => {
                    println!("  Tool: {}", exec.tool_name);
                    if let Some(input) = &exec.input {
                        println!("  Input: {}", serde_json::to_string_pretty(input).unwrap_or_default());
                    }
                    if let Some(output) = &exec.output {
                        let truncated = if output.len() > 200 {
                            format!("{}... ({} bytes)", &output[..200], output.len())
                        } else {
                            output.clone()
                        };
                        println!("  Output: {}", truncated);
                    }
                    if let Some(err) = &exec.error {
                        println!("  Error: {}", err);
                    }
                    println!("  Duration: {:.2}s", exec.duration.as_secs_f64());
                }
                TraceEvent::Error(msg) => {
                    println!("  Message: {}", msg);
                }
                TraceEvent::Complete { reason } => {
                    println!("  Reason: {}", reason);
                }
            }
        }
        
        self.print_summary();
    }
}

impl Default for AgentTrace {
    fn default() -> Self {
        Self::new()
    }
}

/// Individual event in the agent execution
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TraceEvent {
    /// Agent started with a task
    Start {
        task: String,
    },
    
    /// New iteration started
    Iteration {
        iteration: usize,
    },
    
    /// Model is thinking/reasoning (if captured)
    ModelThinking {
        text: String,
    },
    
    /// Model produced a response
    ModelResponse {
        text: String,
        tokens: u32,
        finish_reason: String,
    },
    
    /// Tool was executed
    ToolExecution(ToolExecution),
    
    /// Error occurred
    Error(String),
    
    /// Agent completed
    Complete {
        reason: StopReason,
    },
}

impl TraceEvent {
    /// Get a human-readable event type name
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Start { .. } => "Start",
            Self::Iteration { .. } => "Iteration",
            Self::ModelThinking { .. } => "Model Thinking",
            Self::ModelResponse { .. } => "Model Response",
            Self::ToolExecution(_) => "Tool Execution",
            Self::Error(_) => "Error",
            Self::Complete { .. } => "Complete",
        }
    }
}

/// Details of a single tool execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    /// Name of the tool
    pub tool_name: String,
    
    /// Tool call ID
    pub tool_call_id: String,
    
    /// Input arguments
    pub input: Option<Value>,
    
    /// Output result (if successful)
    pub output: Option<String>,
    
    /// Error message (if failed)
    pub error: Option<String>,
    
    /// Time taken to execute
    pub duration: Duration,
    
    /// Server name (for MCP tools)
    pub server_name: Option<String>,
}

impl ToolExecution {
    pub fn new(tool_use: &ToolUse) -> Self {
        Self {
            tool_name: tool_use.name.clone(),
            tool_call_id: tool_use.id.clone(),
            input: Some(tool_use.input.clone()),
            output: None,
            error: None,
            duration: Duration::from_secs(0),
            server_name: None,
        }
    }
    
    /// Mark as successful with output
    pub fn with_output(mut self, output: String, duration: Duration) -> Self {
        self.output = Some(output);
        self.duration = duration;
        self
    }
    
    /// Mark as failed with error
    pub fn with_error(mut self, error: String, duration: Duration) -> Self {
        self.error = Some(error);
        self.duration = duration;
        self
    }
    
    /// Set the MCP server name
    pub fn with_server(mut self, server: String) -> Self {
        self.server_name = Some(server);
        self
    }
    
    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        self.error.is_none()
    }
}
