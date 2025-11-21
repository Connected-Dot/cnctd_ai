use std::time::Duration;

/// Configuration for agent behavior
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Maximum number of reasoning iterations before stopping
    pub max_iterations: usize,
    
    /// Maximum time for the entire agent run
    pub max_duration: Option<Duration>,
    
    /// Whether to stop on first error or continue with other tools
    pub stop_on_error: bool,
    
    /// Maximum length for tool results (truncates if longer)
    pub max_tool_result_length: Option<usize>,
    
    /// Whether to include thinking/reasoning in traces
    pub include_reasoning: bool,
    
    /// Custom system prompt to prepend to conversations
    pub system_prompt: Option<String>,
    
    /// Whether to automatically retry failed tool calls
    pub retry_failed_tools: bool,
    
    /// Maximum number of retries for failed tool calls
    pub max_tool_retries: usize,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_iterations: 3,  // Reduced from 10 to be more conservative
            max_duration: Some(Duration::from_secs(60)), // 1 minute
            stop_on_error: false,
            max_tool_result_length: Some(1500),  // More aggressive truncation
            include_reasoning: true,
            system_prompt: None,
            retry_failed_tools: true,
            max_tool_retries: 2,
        }
    }
}

/// Builder for AgentConfig with fluent API
pub struct AgentConfigBuilder {
    config: AgentConfig,
}

impl AgentConfigBuilder {
    pub fn new() -> Self {
        Self {
            config: AgentConfig::default(),
        }
    }
    
    pub fn max_iterations(mut self, max: usize) -> Self {
        self.config.max_iterations = max;
        self
    }
    
    pub fn max_duration(mut self, duration: Duration) -> Self {
        self.config.max_duration = Some(duration);
        self
    }
    
    pub fn no_time_limit(mut self) -> Self {
        self.config.max_duration = None;
        self
    }
    
    pub fn stop_on_error(mut self, stop: bool) -> Self {
        self.config.stop_on_error = stop;
        self
    }
    
    pub fn max_tool_result_length(mut self, length: usize) -> Self {
        self.config.max_tool_result_length = Some(length);
        self
    }
    
    pub fn unlimited_tool_results(mut self) -> Self {
        self.config.max_tool_result_length = None;
        self
    }
    
    pub fn include_reasoning(mut self, include: bool) -> Self {
        self.config.include_reasoning = include;
        self
    }
    
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.config.system_prompt = Some(prompt.into());
        self
    }
    
    pub fn retry_failed_tools(mut self, retry: bool) -> Self {
        self.config.retry_failed_tools = retry;
        self
    }
    
    pub fn max_tool_retries(mut self, max: usize) -> Self {
        self.config.max_tool_retries = max;
        self
    }
    
    pub fn build(self) -> AgentConfig {
        self.config
    }
}

impl Default for AgentConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}
