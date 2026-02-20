use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub mcp_gateway_url: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub google_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
    pub mcp_server_url: Option<String>,
    pub obfuscation_key: Option<String>,
    pub obfuscation_source_url: Option<String>,
    pub obfuscation_source_token: Option<String>,
}

impl Config {
    pub fn from_env() -> Self {
        Self {
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3002),
            mcp_gateway_url: env::var("MCP_GATEWAY_URL").ok(),
            anthropic_api_key: env::var("ANTHROPIC_API_KEY").ok(),
            openai_api_key: env::var("OPENAI_API_KEY").ok(),
            google_api_key: env::var("GOOGLE_API_KEY").ok(),
            ollama_base_url: env::var("OLLAMA_BASE_URL").ok(),
            mcp_server_url: env::var("MCP_SERVER_URL").ok(),
            obfuscation_key: env::var("OBFUSCATION_KEY").ok(),
            obfuscation_source_url: env::var("OBFUSCATION_SOURCE_URL").ok(),
            obfuscation_source_token: env::var("OBFUSCATION_SOURCE_TOKEN").ok(),
        }
    }
}
