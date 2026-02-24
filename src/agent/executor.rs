use std::sync::Arc;
use std::time::Instant;

use crate::{Client, CompletionRequest, Error, Message, ToolUse};
use crate::mcp::{McpClient, tool_result_to_string};

use super::{AgentConfig, AgentState, AgentTrace, StopReason, TraceEvent, ToolExecution};

/// Executes the agent loop with tool calling
pub struct AgentExecutor<'a> {
    client: &'a Client,
    config: &'a AgentConfig,
    mcp_client: Option<Arc<McpClient>>,
}

impl<'a> AgentExecutor<'a> {
    pub fn new(
        client: &'a Client,
        config: &'a AgentConfig,
        mcp_client: Option<Arc<McpClient>>,
    ) -> Self {
        Self {
            client,
            config,
            mcp_client,
        }
    }
    
    /// Execute the agent with a task
    pub async fn execute(
        &self,
        task: impl Into<String>,
        mut request: CompletionRequest,
    ) -> Result<AgentTrace, Error> {
        let task = task.into();
        let mut trace = AgentTrace::new();
        let mut state = AgentState::new();
        
        // Add start event
        trace.add_event(TraceEvent::Start { task: task.clone() });
        
        // Prepend system prompt if configured
        if let Some(system_prompt) = &self.config.system_prompt {
            request.messages.insert(0, Message::system(system_prompt.clone()));
        }
        
        // Add the user task
        request.messages.push(Message::user(task));
        
        // Main agent loop
        while state.should_continue {
            // Check iteration limit
            if state.iteration >= self.config.max_iterations {
                state.complete(StopReason::MaxIterations);
                break;
            }
            
            // Check time limit
            if state.exceeded_time_limit(self.config.max_duration) {
                state.complete(StopReason::Timeout);
                break;
            }
            
            state.next_iteration();
            trace.add_event(TraceEvent::Iteration {
                iteration: state.iteration,
            });
            
            // Call the model
            let response = match self.client.complete(request.clone()).await {
                Ok(resp) => resp,
                Err(e) => {
                    let error_msg = format!("Model error: {}", e);
                    trace.add_event(TraceEvent::Error(error_msg.clone()));
                    state.record_error();
                    
                    if self.config.stop_on_error {
                        state.complete(StopReason::Error);
                        break;
                    }
                    
                    // Try to continue
                    continue;
                }
            };
            
            // Track tokens
            state.add_tokens(response.usage.total_tokens);
            
            // Add model response to trace
            trace.add_event(TraceEvent::ModelResponse {
                text: response.text().to_string(),
                tokens: response.usage.total_tokens,
                finish_reason: format!("{:?}", response.finish_reason),
            });
            
            // Check if model wants to use tools
            let tool_uses = match &response.tool_uses {
                Some(tools) if !tools.is_empty() => tools.clone(),
                _ => {
                    // No tools requested - model is done
                    trace.result = Some(response.text().to_string());
                    state.complete(StopReason::ModelStopped);
                    break;
                }
            };
            
            // Add assistant message with tool uses to history
            request.messages.push(response.message.clone());
            
            // Execute each tool
            for tool_use in tool_uses {
                let execution = self.execute_tool(&tool_use, &mut state).await;
                
                let result_text = if let Some(output) = &execution.output {
                    self.truncate_result(output.clone())
                } else if let Some(error) = &execution.error {
                    format!("Error: {}", error)
                } else {
                    "No result".to_string()
                };
                
                // Add tool result to message history
                request.messages.push(Message::tool_result(
                    tool_use.id.clone(),
                    result_text,
                ));
                
                trace.add_event(TraceEvent::ToolExecution(execution));
            }
        }
        
        // Finalize trace
        trace.stop_reason = state.stop_reason.clone().unwrap_or(StopReason::Success);
        trace.duration = state.elapsed();
        trace.total_tokens = state.total_tokens;
        trace.iterations = state.iteration;
        trace.errors = state.error_count;
        trace.successful_tool_calls = state.successful_tool_calls;
        
        trace.add_event(TraceEvent::Complete {
            reason: trace.stop_reason.clone(),
        });
        
        Ok(trace)
    }
    
    /// Execute a single tool with retry logic
    async fn execute_tool(
        &self,
        tool_use: &ToolUse,
        state: &mut AgentState,
    ) -> ToolExecution {
        let mut attempts = 0;
        let max_attempts = if self.config.retry_failed_tools {
            self.config.max_tool_retries + 1
        } else {
            1
        };
        
        loop {
            attempts += 1;
            let start = Instant::now();
            
            // Try to execute the tool
            let result = if let Some(mcp_client) = &self.mcp_client {
                self.execute_mcp_tool(mcp_client, tool_use).await
            } else {
                Err("No tool executor available".to_string())
            };
            
            let duration = start.elapsed();
            
            match result {
                Ok((output, server_name)) => {
                    state.record_tool_success();
                    return ToolExecution::new(tool_use)
                        .with_output(output, duration)
                        .with_server(server_name);
                }
                Err(error) => {
                    if attempts >= max_attempts {
                        state.record_error();
                        return ToolExecution::new(tool_use)
                            .with_error(error, duration);
                    }
                    // Retry
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                }
            }
        }
    }
    
    /// Execute a tool via MCP client
    async fn execute_mcp_tool(
        &self,
        mcp_client: &McpClient,
        tool_use: &ToolUse,
    ) -> Result<(String, String), String> {
        let result = mcp_client
            .call_tool(&tool_use.name, Some(tool_use.input.clone()))
            .await
            .map_err(|e| format!("MCP tool error: {}", e))?;

        let output = tool_result_to_string(&result);
        Ok((output, "mcp".to_string()))
    }
    
    /// Truncate result if it exceeds configured length
    fn truncate_result(&self, mut result: String) -> String {
        if let Some(max_len) = self.config.max_tool_result_length {
            if result.len() > max_len {
                result.truncate(max_len);
                result.push_str("\n\n[Result truncated]");
            }
        }
        result
    }
}
