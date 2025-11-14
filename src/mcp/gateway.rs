use rmcp::model::Tool;
use serde::{Deserialize, Serialize};

use crate::{error::AiError, mcp::{Auth, server::{ConnectionType, McpConnection, McpServer}}};


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub url: String,
    pub description: String,
    pub status: String,
    // pub tools: Vec<ToolInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GatewayInfo {
    pub servers: Vec<McpServerInfo>,
}

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

    pub async fn get_gateway_info(&self) -> Result<GatewayInfo, AiError> {
        let servers: GatewayInfo = reqwest::get(&self.url)
            .await?
            .json()
            .await?;
            
        Ok(servers) 
        
    }

    pub async fn connect_all_servers(&mut self) -> Result<Vec<McpConnection>, AiError> {
        let gateway_info = self.get_gateway_info().await?;
        println!("Gateway info: {:?}", gateway_info);
        let mut connections = Vec::new();
        for server_info in gateway_info.servers {
            let server = McpServer::new(
                server_info.name,
                server_info.url,
                self.auth.clone(),
                Some(server_info.description),
                ConnectionType::Http,
            );
            
            let connection = server.connect().await?;
            connections.push(connection);
        };

        Ok(connections)
    }
}