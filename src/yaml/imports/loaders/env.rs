//! Environment variable import loader
//!
//! Provides functionality for loading environment variables with optional defaults

use anyhow::{Result, anyhow};
use serde_yaml::Value;

use crate::yaml::imports::{ImportData, ImportType};

/// Parse and validate env import location format
fn parse_env_location(location: &str) -> Result<(&str, Option<&str>)> {
    let parts: Vec<&str> = location.splitn(3, ':').collect();
    if parts.len() < 2 || parts[0] != "env" {
        return Err(anyhow!("Invalid env import format: {}", location));
    }

    let var_name = parts[1];
    let default_value = parts.get(2).copied();

    Ok((var_name, default_value))
}

/// Create ImportData from environment variable value
fn create_env_import_data(location: &str, data: String) -> ImportData {
    ImportData {
        import_type: ImportType::Env,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    }
}

/// Get environment variable value with optional default
fn get_env_value(
    var_name: &str,
    default_value: Option<&str>,
    base_location: &str,
) -> Result<String> {
    match std::env::var(var_name) {
        Ok(value) => Ok(value),
        Err(_) => {
            if let Some(default) = default_value {
                Ok(default.to_string())
            } else {
                Err(anyhow!(
                    "Env-var {} not found from {}",
                    var_name,
                    base_location
                ))
            }
        }
    }
}

/// Load an environment variable import
pub async fn load_env_import(location: &str, base_location: &str) -> Result<ImportData> {
    let (var_name, default_value) = parse_env_location(location)?;
    let data = get_env_value(var_name, default_value, base_location)?;
    Ok(create_env_import_data(location, data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_load_env_import_existing_var() -> Result<()> {
        // Set a test environment variable
        unsafe {
            std::env::set_var("TEST_ENV_VAR", "test_value");
        }

        let result = load_env_import("env:TEST_ENV_VAR", "/base").await?;

        assert_eq!(result.import_type, ImportType::Env);
        assert_eq!(result.resolved_location, "env:TEST_ENV_VAR");
        assert_eq!(result.data, "test_value");
        assert_eq!(result.doc, Value::String("test_value".to_string()));

        // Clean up
        unsafe {
            std::env::remove_var("TEST_ENV_VAR");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_load_env_import_with_default() -> Result<()> {
        // Use a variable that doesn't exist
        let result = load_env_import("env:NONEXISTENT_VAR:default_value", "/base").await?;

        assert_eq!(result.import_type, ImportType::Env);
        assert_eq!(
            result.resolved_location,
            "env:NONEXISTENT_VAR:default_value"
        );
        assert_eq!(result.data, "default_value");
        assert_eq!(result.doc, Value::String("default_value".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_env_import_missing_no_default() {
        let result = load_env_import("env:DEFINITELY_NONEXISTENT_VAR", "/base").await;

        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Env-var DEFINITELY_NONEXISTENT_VAR not found")
        );
    }

    #[tokio::test]
    async fn test_load_env_import_invalid_format() {
        let result = load_env_import("invalid:format", "/base").await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid env import format")
        );
    }

    #[tokio::test]
    async fn test_load_env_import_empty_var_name() {
        let result = load_env_import("env:", "/base").await;

        // This should still work but look for an empty variable name
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Env-var  not found")
        );
    }

    #[tokio::test]
    async fn test_load_env_import_default_with_colons() -> Result<()> {
        // Test that defaults can contain colons
        let result =
            load_env_import("env:NONEXISTENT_VAR:http://example.com:8080", "/base").await?;

        assert_eq!(result.data, "http://example.com:8080");

        Ok(())
    }

    #[tokio::test]
    async fn test_load_env_import_empty_value() -> Result<()> {
        // Set an empty environment variable
        unsafe {
            std::env::set_var("EMPTY_TEST_VAR", "");
        }

        let result = load_env_import("env:EMPTY_TEST_VAR", "/base").await?;

        assert_eq!(result.data, "");
        assert_eq!(result.doc, Value::String("".to_string()));

        // Clean up
        unsafe {
            std::env::remove_var("EMPTY_TEST_VAR");
        }

        Ok(())
    }
}
