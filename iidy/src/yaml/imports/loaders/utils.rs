//! Shared utility functions for import loaders
//! 
//! Common functions used across different import loader modules

use std::path::Path;
use anyhow::{Result, anyhow};
use serde_yaml::Value;

/// Parse document data based on file extension
pub fn resolve_doc_from_import_data(data: &str, location: &str) -> Result<Value> {
    let path = Path::new(location);
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    match extension {
        "yaml" | "yml" => {
            serde_yaml::from_str(data)
                .map_err(|e| anyhow!("Failed to parse YAML from {}: {}", location, e))
        },
        "json" => {
            serde_json::from_str(data)
                .map_err(|e| anyhow!("Failed to parse JSON from {}: {}", location, e))
        },
        _ => Ok(Value::String(data.to_string())),
    }
}

/// Parse data from SSM parameter store with optional format specification
pub fn parse_data_from_param_store(payload: &str, format: Option<&str>) -> Result<Value> {
    match format {
        Some("json") => serde_json::from_str(payload)
            .map_err(|e| anyhow!("Invalid JSON in SSM parameter: {}", e)),
        Some("yaml") => serde_yaml::from_str(payload)
            .map_err(|e| anyhow!("Invalid YAML in SSM parameter: {}", e)),
        _ => Ok(Value::String(payload.to_string())),
    }
}