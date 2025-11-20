use crate::Tool;
use std::borrow::Cow;
use std::sync::Arc;
use serde_json::{Map, Value};

pub fn create_tool(
    name: &str,
    description: &str,
    schema: Value,
) -> Result<Tool, serde_json::Error> {
    let schema_map = serde_json::from_value::<Map<String, Value>>(schema)?;
    
    Ok(Tool {
        name: Cow::Owned(name.to_string()),
        description: Some(Cow::Owned(description.to_string())),
        input_schema: Arc::new(schema_map),
    })
}

pub fn create_tool_borrowed(
    name: &'static str,
    description: &'static str,
    schema: Value,
) -> Result<Tool, serde_json::Error> {
    let schema_map = serde_json::from_value::<Map<String, Value>>(schema)?;
    
    Ok(Tool {
        name: Cow::Borrowed(name),
        description: Some(Cow::Borrowed(description)),
        input_schema: Arc::new(schema_map),
    })
}
