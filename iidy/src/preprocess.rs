use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_yaml::Value;
use std::path::PathBuf;

use crate::yaml::{TagContext, YamlPreprocessor};
use crate::yaml::imports::loaders::ProductionImportLoader;

/// YAML preprocessing system that processes iidy custom tags and handlebars templates.
///
/// This function converts a serde_yaml::Value to a YAML string, processes it through
/// the full two-phase preprocessing pipeline, and then deserializes the result to the requested type.
pub async fn preprocess<T: DeserializeOwned>(value: Value) -> Result<T> {
    preprocess_with_base_location(value, "input.yaml").await
}

/// YAML preprocessing with a specific base location for resolving relative imports
pub async fn preprocess_with_base_location<T: DeserializeOwned>(value: Value, base_location: &str) -> Result<T> {
    // Convert Value back to YAML string for parsing with custom tags
    let yaml_string = serde_yaml::to_string(&value)?;
    
    // Use the full preprocessing pipeline
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader);
    
    // Process through the full two-phase pipeline
    let resolved = preprocessor.process(&yaml_string, base_location).await?;
    
    // Deserialize to the requested type
    Ok(serde_yaml::from_value(resolved)?)
}

/// Synchronous version for backward compatibility (uses blocking runtime)
pub fn preprocess_sync<T: DeserializeOwned>(value: Value) -> Result<T> {
    // Create a tokio runtime for the async preprocessing
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(preprocess_with_base_location(value, "input.yaml"))
}

/// Create a preprocessing context with environment variables and default settings  
/// This is now mainly used for adding environment variables that can be accessed during preprocessing
fn _create_preprocessing_context(base_location: &str) -> Result<TagContext> {
    let mut context = TagContext::new()
        .with_base_path(PathBuf::from(base_location));
    
    // Add common environment variables that might be used in stack-args
    if let Ok(env) = std::env::var("ENVIRONMENT") {
        context = context.with_variable("environment", Value::String(env));
    }
    if let Ok(app_name) = std::env::var("APP_NAME") {
        context = context.with_variable("app_name", Value::String(app_name));
    }
    if let Ok(region) = std::env::var("AWS_REGION") {
        context = context.with_variable("region", Value::String(region));
    }
    
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    
    #[derive(Debug, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
    }
    
    #[tokio::test]
    async fn test_preprocess_simple_yaml() -> Result<()> {
        let yaml_value = serde_yaml::from_str::<Value>(r#"
name: "test"
value: 42
"#)?;
        
        let result: TestConfig = preprocess_with_base_location(yaml_value, "test.yaml").await?;
        assert_eq!(result.name, "test");
        assert_eq!(result.value, 42);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_preprocess_with_custom_tags() -> Result<()> {
        // Test full preprocessing with custom tags
        let yaml_value = serde_yaml::from_str::<Value>(r#"
$defs:
  app_name: "my-app"
  environment: "production"

name: "test"
stack_name: !$join ["-", ["{{app_name}}", "{{environment}}"]]
"#)?;
        
        let result = preprocess_with_base_location::<std::collections::HashMap<String, Value>>(yaml_value, "test.yaml").await?;
        
        // Verify the custom tag was processed
        if let Some(Value::String(stack_name)) = result.get("stack_name") {
            assert_eq!(stack_name, "my-app-production");
        } else {
            panic!("Expected stack_name to be processed");
        }
        
        Ok(())
    }
    
    #[test]
    fn test_preprocess_sync() -> Result<()> {
        let yaml_value = serde_yaml::from_str::<Value>(r#"
name: "test"
value: 42
"#)?;
        
        let result: TestConfig = preprocess_sync(yaml_value)?;
        assert_eq!(result.name, "test");
        assert_eq!(result.value, 42);
        
        Ok(())
    }
    
    #[tokio::test]
    async fn test_complex_yaml_structure_with_preprocessing() -> Result<()> {
        let yaml_value = serde_yaml::from_str::<Value>(r#"
$defs:
  db_host: "db.example.com"
  cache_host: "cache.example.com"

database:
  host: !$ db_host
  port: 5432
  settings:
    - "ssl=true"
    - "timeout=30"
features:
  enabled: true
  count: 10
  cache_endpoint: !$ cache_host
"#)?;
        
        let result = preprocess_with_base_location::<std::collections::HashMap<String, Value>>(yaml_value, "test.yaml").await?;
        assert!(result.contains_key("database"));
        assert!(result.contains_key("features"));
        
        // Verify preprocessing worked
        if let Some(Value::Mapping(database)) = result.get("database") {
            if let Some(Value::String(host)) = database.get(&Value::String("host".to_string())) {
                assert_eq!(host, "db.example.com");
            }
        }
        
        Ok(())
    }
}
