use rmcp::model::Tool;
use serde::{Deserialize, Serialize};

use crate::{
    error::AiError,
    mcp::{Auth, server::{ConnectionType, McpConnection, McpServer}}
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
}

impl From<&Tool> for ToolInfo {
    fn from(tool: &Tool) -> Self {
        Self {
            name: tool.name.to_string(),
            description: tool.description.clone().unwrap_or_default().to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpServerInfo {
    pub name: String,
    pub url: String,
    pub description: String,
    pub status: String,
    pub tools: Vec<ToolInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GatewayInfo {
    pub name: String,
    pub url: String,
    pub servers: Vec<McpServerInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GatewayResponse {
    pub servers: Vec<McpServerInfo>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct McpGateway {
    pub url: String,
    pub auth: Auth,
    pub servers: Vec<McpServer>
}

impl From<&McpGateway> for GatewayInfo {
    fn from(gateway: &McpGateway) -> Self {
        let url = url::Url::parse(&gateway.url).ok();
        let name = url
            .and_then(|u| u.host_str().map(String::from))
            .unwrap_or_else(|| gateway.url.clone());
        
        let servers = gateway.servers.iter().map(|server| {
            let server_url = url::Url::parse(&server.url).ok();
            let status = if server_url.is_some() { "unknown" } else { "invalid_url" };
            
            McpServerInfo {
                name: server.name.clone(),
                url: server.url.clone(),
                description: server.description.clone().unwrap_or_default(),
                status: status.to_string(),
                tools: Vec::new(),
            }
        }).collect();
        
        Self {
            name,
            url: gateway.url.clone(),
            servers,
        }
    }
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
        let servers: GatewayResponse = reqwest::get(&self.url)
            .await?
            .json()
            .await?;

        let gateway_info = GatewayInfo {
            name: url::Url::parse(&self.url)
                .ok()
                .and_then(|u| u.host_str().map(String::from))
                .unwrap_or_else(|| self.url.clone()),
            url: self.url.clone(),
            servers: servers.servers,
        };
            
        Ok(gateway_info) 
    }

    pub async fn connect_all_servers(&mut self) -> Result<(GatewayInfo, Vec<McpConnection>), AiError> {
        let gateway_info = self.get_gateway_info().await?;
        println!("Gateway info: {:?}", gateway_info);
        let mut connections = Vec::new();
        for server_info in gateway_info.clone().servers {
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

        Ok((gateway_info, connections))
    }
}
