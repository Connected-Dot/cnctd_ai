use serde::{Deserialize, Serialize};
use serde_json::Value;
use crate::error::{Error, Result};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl Tool {
    pub fn new(name: impl Into<String>, description: impl Into<String>, input_schema: Value) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }
    
    /// Validates the tool's input schema is a proper JSON Schema object
    pub fn validate(&self) -> Result<()> {
        // Check that input_schema is an object
        if !self.input_schema.is_object() {
            return Err(Error::InvalidRequest(
                format!("Tool '{}': input_schema must be a JSON object, got {}", 
                    self.name, self.input_schema)
            ));
        }
        
        let schema_obj = self.input_schema.as_object().unwrap();
        
        // Check for required 'type' field
        if let Some(schema_type) = schema_obj.get("type") {
            if let Some(type_str) = schema_type.as_str() {
                // Valid JSON Schema types
                let valid_types = ["object", "array", "string", "number", "integer", "boolean", "null"];
                if !valid_types.contains(&type_str) {
                    return Err(Error::InvalidRequest(
                        format!("Tool '{}': invalid schema type '{}'. Must be one of: {}", 
                            self.name, type_str, valid_types.join(", "))
                    ));
                }
            } else {
                return Err(Error::InvalidRequest(
                    format!("Tool '{}': 'type' field must be a string", self.name)
                ));
            }
        } else {
            return Err(Error::InvalidRequest(
                format!("Tool '{}': input_schema missing required 'type' field", self.name)
            ));
        }
        
        // If type is object, validate properties structure if present
        if let Some(schema_type) = schema_obj.get("type").and_then(|t| t.as_str()) {
            if schema_type == "object" {
                if let Some(properties) = schema_obj.get("properties") {
                    if !properties.is_object() {
                        return Err(Error::InvalidRequest(
                            format!("Tool '{}': 'properties' must be an object", self.name)
                        ));
                    }
                }
                
                if let Some(required) = schema_obj.get("required") {
                    if !required.is_array() {
                        return Err(Error::InvalidRequest(
                            format!("Tool '{}': 'required' must be an array", self.name)
                        ));
                    }
                }
            }
        }
        
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}