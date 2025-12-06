use crate::{Client, CompletionRequest, Error, RequestOptions};
use crate::mcp::McpGateway;

use super::{AgentConfig, AgentConfigBuilder, AgentExecutor, AgentTrace};

/// High-level agent that orchestrates autonomous task execution
pub struct Agent<'a> {
    client: &'a Client,
    config: AgentConfig,
    gateway: Option<&'a McpGateway>,
    servers: Option<Vec<String>>,
}

impl<'a> Agent<'a> {
    /// Create a new agent with default configuration
    pub fn new(client: &'a Client) -> Self {
        Self {
            client,
            config: AgentConfig::default(),
            gateway: None,
            servers: None,
        }
    }
    
    /// Create a new agent with custom configuration
    pub fn with_config(client: &'a Client, config: AgentConfig) -> Self {
        Self {
            client,
            config,
            gateway: None,
            servers: None,
        }
    }
    
    /// Create a new agent with a configuration builder
    pub fn builder(client: &'a Client) -> AgentBuilder<'a> {
        AgentBuilder::new(client)
    }
    
    /// Set the MCP gateway for tool execution
    pub fn with_gateway(mut self, gateway: &'a McpGateway) -> Self {
        self.gateway = Some(gateway);
        self
    }
    
    /// Set specific servers to load tools from (if not set, loads all servers)
    pub fn with_servers(mut self, servers: Vec<String>) -> Self {
        self.servers = Some(servers);
        self
    }
    
    /// Run the agent with a task
    pub async fn run(
        &self,
        task: impl Into<String>,
        request: CompletionRequest,
    ) -> Result<AgentTrace, Error> {
        let executor = AgentExecutor::new(self.client, &self.config, self.gateway);
        executor.execute(task, request).await
    }
    
    /// Run the agent with a simple task (no pre-configured request)
    /// If a gateway is configured, automatically loads tools from specified servers
    pub async fn run_simple(&self, task: impl Into<String>) -> Result<AgentTrace, Error> {
        let mut request = CompletionRequest {
            messages: Vec::new(),
            tools: None,
            built_in_tools: None,
            options: Some(RequestOptions {
                max_tokens: Some(1024),  // Reasonable default for simple tasks
                ..Default::default()
            }),
        };
        
        // Auto-discover and load tools from gateway if available
        if let Some(gateway) = self.gateway {
            let servers_to_load = if let Some(servers) = &self.servers {
                // Load only specified servers
                servers.clone()
            } else {
                // Load all servers
                gateway.list_servers()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|s| s.name)
                    .collect()
            };
            
            for server_name in servers_to_load {
                if let Ok(tools) = gateway.list_tools(&server_name).await {
                    for tool in tools {
                        request.add_tool(tool);
                    }
                }
            }
        }
        
        self.run(task, request).await
    }
}

/// Builder for creating agents with fluent API
pub struct AgentBuilder<'a> {
    client: &'a Client,
    config_builder: AgentConfigBuilder,
    gateway: Option<&'a McpGateway>,
    servers: Option<Vec<String>>,
}

impl<'a> AgentBuilder<'a> {
    pub fn new(client: &'a Client) -> Self {
        Self {
            client,
            config_builder: AgentConfigBuilder::new(),
            gateway: None,
            servers: None,
        }
    }
    
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.config_builder = self.config_builder.max_iterations(max);
        self
    }
    
    pub fn max_duration(mut self, duration: std::time::Duration) -> Self {
        self.config_builder = self.config_builder.max_duration(duration);
        self
    }
    
    pub fn no_time_limit(mut self) -> Self {
        self.config_builder = self.config_builder.no_time_limit();
        self
    }
    
    pub fn stop_on_error(mut self, stop: bool) -> Self {
        self.config_builder = self.config_builder.stop_on_error(stop);
        self
    }
    
    pub fn max_tool_result_length(mut self, length: usize) -> Self {
        self.config_builder = self.config_builder.max_tool_result_length(length);
        self
    }
    
    pub fn unlimited_tool_results(mut self) -> Self {
        self.config_builder = self.config_builder.unlimited_tool_results();
        self
    }
    
    pub fn include_reasoning(mut self, include: bool) -> Self {
        self.config_builder = self.config_builder.include_reasoning(include);
        self
    }
    
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config_builder = self.config_builder.system_prompt(prompt);
        self
    }
    
    pub fn retry_failed_tools(mut self, retry: bool) -> Self {
        self.config_builder = self.config_builder.retry_failed_tools(retry);
        self
    }
    
    pub fn max_tool_retries(mut self, max: usize) -> Self {
        self.config_builder = self.config_builder.max_tool_retries(max);
        self
    }
    
    pub fn gateway(mut self, gateway: &'a McpGateway) -> Self {
        self.gateway = Some(gateway);
        self
    }
    
    pub fn servers(mut self, servers: Vec<String>) -> Self {
        self.servers = Some(servers);
        self
    }
    
    pub fn build(self) -> Agent<'a> {
        Agent {
            client: self.client,
            config: self.config_builder.build(),
            gateway: self.gateway,
            servers: self.servers,
        }
    }
}
