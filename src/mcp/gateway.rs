use serde::{Deserialize, Serialize};

use crate::{error::AiError, mcp::{Auth, server::McpServer}};


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpGateway {
    pub url: String,
    pub auth: Auth,
    pub servers: Vec<McpServer>
}

impl McpGateway {
    pub fn new<U>(url: U, auth: Auth) -> Self
    where
        U: Into<String>,
    {
        Self {
            url: url.into(),
            auth: auth.into(),
            servers: Vec::new(),
        }
    }

    pub async fn list_servers(&self) -> Result<Vec<McpServer>, AiError> {
        let servers: Vec<McpServer> = reqwest::get(&self.url)
            .await?
            .json()
            .await?;
            
        Ok(servers) 
        
    }
}