//! AWS SSM Parameter Store import loader
//! 
//! Provides functionality for loading parameters from AWS Systems Manager Parameter Store

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use crate::yaml::imports::{ImportData, ImportType};
use super::utils::parse_data_from_param_store;

/// SSM Parameter representation
#[derive(Debug, Clone)]
pub struct SsmParameter {
    pub name: String,
    pub value: String,
}

/// Trait for SSM operations (allows mocking in tests)
#[async_trait]
pub trait SsmClient: Send + Sync {
    async fn get_parameter(&self, name: &str) -> Result<SsmParameter>;
    async fn get_parameters_by_path(&self, path: &str) -> Result<Vec<SsmParameter>>;
}

/// Production SSM client implementation
pub struct AwsSsmClient {
    client: aws_sdk_ssm::Client,
}

impl AwsSsmClient {
    pub fn new(aws_config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_ssm::Client::new(aws_config),
        }
    }
}

#[async_trait]
impl SsmClient for AwsSsmClient {
    async fn get_parameter(&self, name: &str) -> Result<SsmParameter> {
        let response = self.client
            .get_parameter()
            .name(name)
            .with_decryption(true)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch SSM parameter {}: {}", name, e))?;
        
        let parameter = response.parameter
            .ok_or_else(|| anyhow!("SSM parameter {} not found", name))?;
        
        let value = parameter.value
            .ok_or_else(|| anyhow!("SSM parameter {} has no value", name))?;
        
        Ok(SsmParameter {
            name: name.to_string(),
            value,
        })
    }

    async fn get_parameters_by_path(&self, path: &str) -> Result<Vec<SsmParameter>> {
        let response = self.client
            .get_parameters_by_path()
            .path(path)
            .recursive(true)
            .with_decryption(true)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch SSM parameters by path {}: {}", path, e))?;
        
        let mut parameters = Vec::new();
        for param in response.parameters.unwrap_or_default() {
            if let (Some(name), Some(value)) = (param.name, param.value) {
                parameters.push(SsmParameter { name, value });
            }
        }
        
        Ok(parameters)
    }
}

/// Load an SSM parameter import
pub async fn load_ssm_import(location: &str, aws_config: &aws_config::SdkConfig) -> Result<ImportData> {
    let client = AwsSsmClient::new(aws_config);
    load_ssm_import_with_client(location, &client).await
}

/// Load an SSM parameter import with custom client (for testing)
pub async fn load_ssm_import_with_client(location: &str, client: &dyn SsmClient) -> Result<ImportData> {
    // Parse ssm:/parameter/path or ssm:/parameter/path:format
    let (parameter_name, format) = parse_ssm_location(location)?;
    
    // Get parameter from SSM
    let parameter = client.get_parameter(&parameter_name).await?;
    
    // Parse data based on format specification
    let doc = parse_data_from_param_store(&parameter.value, format.as_deref())?;
    
    Ok(ImportData {
        import_type: ImportType::Ssm,
        resolved_location: location.to_string(),
        data: parameter.value,
        doc,
    })
}

/// Load SSM parameter path import (multiple parameters)
pub async fn load_ssm_path_import(location: &str, aws_config: &aws_config::SdkConfig) -> Result<ImportData> {
    let client = AwsSsmClient::new(aws_config);
    load_ssm_path_import_with_client(location, &client).await
}

/// Load SSM parameter path import with custom client (for testing)
pub async fn load_ssm_path_import_with_client(location: &str, client: &dyn SsmClient) -> Result<ImportData> {
    // Parse ssm-path:/parameter/path or ssm-path:/parameter/path:format
    let (parameter_path, format) = parse_ssm_path_location(location)?;
    
    // Get parameters by path from SSM
    let parameters = client.get_parameters_by_path(&parameter_path).await?;
    
    // Build a JSON object with parameter names as keys
    let mut result = serde_json::Map::new();
    
    for parameter in parameters {
        // Remove the base path prefix to get relative key
        let key = parameter.name.strip_prefix(&parameter_path)
            .unwrap_or(&parameter.name)
            .strip_prefix('/')
            .unwrap_or(&parameter.name);
        
        // Parse value based on format or treat as string
        let parsed_value = match format.as_deref() {
            Some("json") => serde_json::from_str(&parameter.value).unwrap_or(serde_json::Value::String(parameter.value)),
            Some("yaml") => {
                match serde_yaml::from_str::<serde_json::Value>(&parameter.value) {
                    Ok(value) => value,
                    Err(_) => serde_json::Value::String(parameter.value),
                }
            },
            _ => serde_json::Value::String(parameter.value),
        };
        
        result.insert(key.to_string(), parsed_value);
    }
    
    let data = serde_json::to_string(&result)?;
    let doc = serde_yaml::to_value(result)?;
    
    Ok(ImportData {
        import_type: ImportType::SsmPath,
        resolved_location: location.to_string(),
        data,
        doc,
    })
}

/// Parse SSM parameter location
fn parse_ssm_location(location: &str) -> Result<(String, Option<String>)> {
    if !location.starts_with("ssm:") {
        return Err(anyhow!("Invalid SSM location format: {}", location));
    }
    
    let path = location.strip_prefix("ssm:").unwrap();
    let parts: Vec<&str> = path.splitn(2, ':').collect();
    
    let parameter_name = parts[0];
    let format = parts.get(1).map(|s| s.to_string());
    
    if parameter_name.is_empty() {
        return Err(anyhow!("Invalid SSM parameter name in: {}", location));
    }
    
    Ok((parameter_name.to_string(), format))
}

/// Parse SSM parameter path location  
fn parse_ssm_path_location(location: &str) -> Result<(String, Option<String>)> {
    if !location.starts_with("ssm-path:") {
        return Err(anyhow!("Invalid SSM path location format: {}", location));
    }
    
    let path = location.strip_prefix("ssm-path:").unwrap();
    let parts: Vec<&str> = path.splitn(2, ':').collect();
    
    let parameter_path = parts[0];
    let format = parts.get(1).map(|s| s.to_string());
    
    if parameter_path.is_empty() {
        return Err(anyhow!("Invalid SSM parameter path in: {}", location));
    }
    
    Ok((parameter_path.to_string(), format))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Mock SSM client for testing
    struct MockSsmClient {
        parameters: HashMap<String, String>,
        path_parameters: HashMap<String, Vec<SsmParameter>>,
    }

    impl MockSsmClient {
        fn new() -> Self {
            Self {
                parameters: HashMap::new(),
                path_parameters: HashMap::new(),
            }
        }

        fn with_parameter(mut self, name: &str, value: &str) -> Self {
            self.parameters.insert(name.to_string(), value.to_string());
            self
        }

        fn with_path_parameters(mut self, path: &str, parameters: Vec<(&str, &str)>) -> Self {
            let params = parameters.into_iter()
                .map(|(name, value)| SsmParameter {
                    name: name.to_string(),
                    value: value.to_string(),
                })
                .collect();
            self.path_parameters.insert(path.to_string(), params);
            self
        }
    }

    #[async_trait]
    impl SsmClient for MockSsmClient {
        async fn get_parameter(&self, name: &str) -> Result<SsmParameter> {
            match self.parameters.get(name) {
                Some(value) => Ok(SsmParameter {
                    name: name.to_string(),
                    value: value.clone(),
                }),
                None => Err(anyhow!("Parameter {} not found", name)),
            }
        }

        async fn get_parameters_by_path(&self, path: &str) -> Result<Vec<SsmParameter>> {
            match self.path_parameters.get(path) {
                Some(params) => Ok(params.clone()),
                None => Ok(Vec::new()),
            }
        }
    }

    #[test]
    fn test_parse_ssm_location() -> Result<()> {
        let (name, format) = parse_ssm_location("ssm:/app/config/database")?;
        assert_eq!(name, "/app/config/database");
        assert_eq!(format, None);
        
        let (name, format) = parse_ssm_location("ssm:/app/config/api:json")?;
        assert_eq!(name, "/app/config/api");
        assert_eq!(format, Some("json".to_string()));
        
        Ok(())
    }

    #[test]
    fn test_parse_ssm_path_location() -> Result<()> {
        let (path, format) = parse_ssm_path_location("ssm-path:/app/config")?;
        assert_eq!(path, "/app/config");
        assert_eq!(format, None);
        
        let (path, format) = parse_ssm_path_location("ssm-path:/app/config:yaml")?;
        assert_eq!(path, "/app/config");
        assert_eq!(format, Some("yaml".to_string()));
        
        Ok(())
    }

    #[test]
    fn test_parse_ssm_location_invalid() {
        assert!(parse_ssm_location("invalid:format").is_err());
        assert!(parse_ssm_location("ssm:").is_err());
        assert!(parse_ssm_path_location("ssm-path:").is_err());
    }

    #[tokio::test]
    async fn test_load_ssm_import_string() -> Result<()> {
        let client = MockSsmClient::new()
            .with_parameter("/app/config/database", "localhost:5432");

        let result = load_ssm_import_with_client("ssm:/app/config/database", &client).await?;

        assert_eq!(result.import_type, ImportType::Ssm);
        assert_eq!(result.resolved_location, "ssm:/app/config/database");
        assert_eq!(result.data, "localhost:5432");
        assert_eq!(result.doc, serde_yaml::Value::String("localhost:5432".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_ssm_import_json() -> Result<()> {
        let json_value = r#"{"host": "localhost", "port": 5432}"#;
        let client = MockSsmClient::new()
            .with_parameter("/app/config/database", json_value);

        let result = load_ssm_import_with_client("ssm:/app/config/database:json", &client).await?;

        assert_eq!(result.import_type, ImportType::Ssm);
        assert_eq!(result.data, json_value);
        
        // Should parse as JSON
        if let serde_yaml::Value::Mapping(map) = result.doc {
            assert_eq!(map.get(&serde_yaml::Value::String("host".to_string())), Some(&serde_yaml::Value::String("localhost".to_string())));
            assert_eq!(map.get(&serde_yaml::Value::String("port".to_string())), Some(&serde_yaml::Value::Number(serde_yaml::Number::from(5432))));
        } else {
            panic!("Expected parsed JSON object");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_load_ssm_import_yaml() -> Result<()> {
        let yaml_value = "host: localhost\nport: 5432";
        let client = MockSsmClient::new()
            .with_parameter("/app/config/database", yaml_value);

        let result = load_ssm_import_with_client("ssm:/app/config/database:yaml", &client).await?;

        assert_eq!(result.import_type, ImportType::Ssm);
        assert_eq!(result.data, yaml_value);
        
        // Should parse as YAML
        if let serde_yaml::Value::Mapping(map) = result.doc {
            assert_eq!(map.get(&serde_yaml::Value::String("host".to_string())), Some(&serde_yaml::Value::String("localhost".to_string())));
            assert_eq!(map.get(&serde_yaml::Value::String("port".to_string())), Some(&serde_yaml::Value::Number(serde_yaml::Number::from(5432))));
        } else {
            panic!("Expected parsed YAML object");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_load_ssm_path_import() -> Result<()> {
        let parameters = vec![
            ("/app/config/database/host", "localhost"),
            ("/app/config/database/port", "5432"),
            ("/app/config/cache/host", "redis"),
        ];
        
        let client = MockSsmClient::new()
            .with_path_parameters("/app/config", parameters);

        let result = load_ssm_path_import_with_client("ssm-path:/app/config", &client).await?;

        assert_eq!(result.import_type, ImportType::SsmPath);
        assert_eq!(result.resolved_location, "ssm-path:/app/config");
        
        // Should parse as JSON object with nested structure
        if let serde_yaml::Value::Mapping(map) = result.doc {
            assert!(map.contains_key(&serde_yaml::Value::String("database/host".to_string())));
            assert!(map.contains_key(&serde_yaml::Value::String("database/port".to_string())));
            assert!(map.contains_key(&serde_yaml::Value::String("cache/host".to_string())));
        } else {
            panic!("Expected parsed object");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_load_ssm_import_not_found() {
        let client = MockSsmClient::new();
        let result = load_ssm_import_with_client("ssm:/nonexistent/parameter", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}