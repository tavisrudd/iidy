//! Environment variable import loader
//! 
//! Provides functionality for loading environment variables with optional defaults

use anyhow::{Result, anyhow};
use serde_json::Value;

use crate::yaml::imports::{ImportData, ImportType};

/// Load an environment variable import
pub async fn load_env_import(location: &str, base_location: &str) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(3, ':').collect();
    if parts.len() < 2 || parts[0] != "env" {
        return Err(anyhow!("Invalid env import format: {}", location));
    }

    let var_name = parts[1];
    let default_value = parts.get(2);

    let data = match std::env::var(var_name) {
        Ok(value) => value,
        Err(_) => {
            if let Some(default) = default_value {
                default.to_string()
            } else {
                return Err(anyhow!("Env-var {} not found from {}", var_name, base_location));
            }
        }
    };

    Ok(ImportData {
        import_type: ImportType::Env,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    })
}