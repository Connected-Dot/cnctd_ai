use std::time::Instant;
use serde_json::Value;

use crate::{Client, CompletionRequest, CompletionResponse, Error, Message, ToolUse};
use crate::mcp::{McpGateway, tool_result_to_string};

use super::{AgentConfig, AgentState, AgentTrace, StopReason, TraceEvent, ToolExecution};

/// Executes the agent loop with tool calling
pub struct AgentExecutor<'a> {
    client: &'a Client,
    config: &'a AgentConfig,
    gateway: Option<&'a McpGateway>,
}

impl<'a> AgentExecutor<'a> {
    pub fn new(
        client: &'a Client,
        config: &'a AgentConfig,
        gateway: Option<&'a McpGateway>,
    ) -> Self {
        Self {
            client,
            config,
            gateway,
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
            let result = if let Some(gateway) = self.gateway {
                // MCP tool execution
                self.execute_mcp_tool(gateway, tool_use).await
            } else {
                // For now, only MCP tools are supported
                // In the future, this could support custom tool executors
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
    
    /// Execute a tool via MCP gateway
    async fn execute_mcp_tool(
        &self,
        gateway: &McpGateway,
        tool_use: &ToolUse,
    ) -> Result<(String, String), String> {
        // Parse tool name to extract server and tool
        // Format: "server_name:tool_name" or just "tool_name"
        let (server_name, tool_name) = if tool_use.name.contains(':') {
            let parts: Vec<&str> = tool_use.name.splitn(2, ':').collect();
            (parts[0].to_string(), parts[1].to_string())
        } else {
            // Try to infer server from available tools
            match self.find_server_for_tool(gateway, &tool_use.name).await {
                Some(server) => (server, tool_use.name.clone()),
                None => return Err(format!("Could not find server for tool: {}", tool_use.name)),
            }
        };
        
        // Execute the tool
        let result = gateway
            .call_tool(&server_name, &tool_name, Some(tool_use.input.clone()))
            .await
            .map_err(|e| format!("MCP tool error: {}", e))?;
        
        let output = tool_result_to_string(&result);
        Ok((output, server_name))
    }
    
    /// Find which server provides a given tool
    async fn find_server_for_tool(
        &self,
        gateway: &McpGateway,
        tool_name: &str,
    ) -> Option<String> {
        // Get all servers
        let servers = gateway.list_servers().await.ok()?;
        
        // Check each server for the tool
        for server in servers {
            let tools = gateway.list_tools(&server.name).await.ok()?;
            if tools.iter().any(|t| t.name == tool_name) {
                return Some(server.name);
            }
        }
        
        None
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
