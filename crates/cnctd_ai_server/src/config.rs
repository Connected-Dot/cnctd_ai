use std::env;

#[derive(Clone, Debug)]
pub struct Config {
    pub port: u16,
    pub mcp_gateway_url: Option<String>,
    pub anthropic_api_key: Option<String>,
    pub openai_api_key: Option<String>,
    pub google_api_key: Option<String>,
    pub ollama_base_url: Option<String>,
    pub transmit_mcp_url: Option<String>,
    pub safe_proxy_key: Option<String>,
    pub pg_connection_string: Option<String>,
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
            transmit_mcp_url: env::var("TRANSMIT_MCP_URL").ok(),
            safe_proxy_key: env::var("SAFE_PROXY_KEY").ok(),
            pg_connection_string: env::var("PG_CONNECTION_STRING").ok(),
        }
    }
}
