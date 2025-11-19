//! MCP (Model Context Protocol) gateway integration
//!
//! This module provides a client for interacting with MCP gateways that expose
//! multiple MCP servers via HTTP. The gateway acts as a proxy, allowing discovery
//! of available servers and execution of their tools.

mod gateway;

pub use gateway::{McpGateway, ServerInfo, ListServersResponse, tool_result_to_string};
