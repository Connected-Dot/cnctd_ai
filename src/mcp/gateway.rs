use serde::{Deserialize, Serialize};

use crate::mcp::{Auth, server::McpServer};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpGateway {
    pub url: String,
    pub auth: Auth,
}

impl McpGateway {
    pub fn new<U>(url: U, auth: Auth) -> Self
    where
        U: Into<String>,
    {
        Self {
            url: url.into(),
            auth,
        }
    }

    // pub async fn list_servers(&self) -> Vec<McpServer> {
    //     let body = reqwest::get(&self.url)
    //         .await?
    //         .json()
    //         .await?;
            
        
    // }
}