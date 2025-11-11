use crate::mcp::Auth;

pub struct McpGateway {
    pub url: String,
    pub auth: Auth,
}