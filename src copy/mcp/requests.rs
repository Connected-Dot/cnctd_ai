use serde_json::{json, Value};

/// JSON-RPC request builder
pub struct JsonRpcRequest {
    method: String,
    params: Value,
    id: u64,
}

impl JsonRpcRequest {
    pub fn new(method: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            params: json!({}),
            id: 1,
        }
    }

    pub fn with_params(mut self, params: Value) -> Self {
        self.params = params;
        self
    }

    pub fn with_id(mut self, id: u64) -> Self {
        self.id = id;
        self
    }

    pub fn build(self) -> Value {
        json!({
            "jsonrpc": "2.0",
            "id": self.id,
            "method": self.method,
            "params": self.params
        })
    }
}

/// Convenience constructors for common MCP methods
impl JsonRpcRequest {
    pub fn tools_list() -> Self {
        Self::new("tools/list")
    }

    pub fn tools_call(name: impl Into<String>, arguments: Value) -> Self {
        Self::new("tools/call")
            .with_params(json!({
                "name": name.into(),
                "arguments": arguments
            }))
    }

    pub fn resources_list() -> Self {
        Self::new("resources/list")
    }

    pub fn resources_read(uri: impl Into<String>) -> Self {
        Self::new("resources/read")
            .with_params(json!({
                "uri": uri.into()
            }))
    }
}

pub enum McpRequest {
    ToolsList,
    ToolsCall { name: String, arguments: Value },
    ResourcesList,
    ResourcesRead { uri: String },
}

impl McpRequest {
    fn to_jsonrpc(&self, id: u64) -> Value {
        match self {
            Self::ToolsList => JsonRpcRequest::tools_list().with_id(id).build(),
            Self::ToolsCall { name, arguments } => {
                JsonRpcRequest::tools_call(name, arguments.clone()).with_id(id).build()
            }
            Self::ResourcesList => JsonRpcRequest::resources_list().with_id(id).build(),
            Self::ResourcesRead { uri } => {
                JsonRpcRequest::resources_read(uri).with_id(id).build()
            }
        }
    }
}