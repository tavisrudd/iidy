//! Integration tests for the complete imports system
//! 
//! These tests use real YAML fixtures to test end-to-end functionality
//! including complex import scenarios, nested imports, and handlebars
//! interpolation with realistic use cases.

use std::collections::HashMap;
use anyhow::Result;
use serde_json::{json, Value};
use async_trait::async_trait;
use tempfile::TempDir;
use tokio::fs;

use crate::yaml::imports::{ImportLoader, ImportData, ImportType, load_imports};

/// Test fixture loader that simulates a realistic import environment
/// with files, config data, and mock external services
pub struct FixtureImportLoader {
    temp_dir: TempDir,
    mock_responses: HashMap<String, ImportData>,
}

impl FixtureImportLoader {
    pub async fn new() -> Result<Self> {
        let temp_dir = TempDir::new()?;
        let mut loader = Self {
            temp_dir,
            mock_responses: HashMap::new(),
        };
        
        // Set up test fixtures
        loader.setup_fixtures().await?;
        loader.setup_mock_responses();
        
        Ok(loader)
    }
    
    async fn setup_fixtures(&self) -> Result<()> {
        let base_path = self.temp_dir.path();
        
        // Create base configuration files
        fs::write(
            base_path.join("database.yaml"),
            r#"
host: localhost
port: 5432
name: myapp
ssl_enabled: true
connection_pool:
  min_size: 5
  max_size: 20
"#
        ).await?;
        
        fs::write(
            base_path.join("features.json"), 
            r#"{
  "authentication": {
    "enabled": true,
    "providers": ["oauth", "ldap"]
  },
  "logging": {
    "level": "info",
    "format": "json"
  },
  "monitoring": {
    "enabled": true,
    "metrics_endpoint": "/metrics"
  }
}"#
        ).await?;
        
        // Create environment-specific configs
        let envs_dir = base_path.join("envs");
        fs::create_dir(&envs_dir).await?;
        
        fs::write(
            envs_dir.join("production.yaml"),
            r#"
database:
  host: prod-db.internal
  port: 5432
cache:
  redis_url: redis://prod-cache:6379
monitoring:
  enabled: true
  alerting: true
"#
        ).await?;
        
        fs::write(
            envs_dir.join("staging.yaml"),
            r#"
database:
  host: staging-db.internal  
  port: 5432
cache:
  redis_url: redis://staging-cache:6379
monitoring:
  enabled: true
  alerting: false
"#
        ).await?;
        
        // Create a complex nested import scenario
        fs::write(
            base_path.join("app-config.yaml"),
            r#"
$defs:
  environment: production
  app_name: my-awesome-app
  
$imports:
  base_db: database.yaml
  features: features.json
  env_config: envs/{{environment}}.yaml
  version_info: git:describe
  
application:
  name: "{{app_name}}"
  environment: "{{environment}}"
  database: "!$ base_db"
  features: "!$ features"
  env_settings: "!$ env_config"
  version: "!$ version_info"
"#
        ).await?;
        
        // Create a file that imports other files recursively
        fs::write(
            base_path.join("stack-template.yaml"),
            r#"
$defs:
  stack_name: my-stack
  region: us-west-2

$imports:
  app_config: app-config.yaml
  secrets: vault://api-keys

Parameters:
  StackName:
    Type: String
    Default: "{{stack_name}}"
    
  Environment:
    Type: String
    Default: "!$ app_config.environment"
    
Resources:
  Application:
    Type: AWS::ECS::Service
    Properties:
      ServiceName: "{{stack_name}}-app"
      TaskDefinition: "!$ app_config.application"
      
  Database:
    Type: AWS::RDS::DBInstance  
    Properties:
      DBName: "!$ app_config.application.database.name"
      Engine: postgres
      
Outputs:
  AppVersion:
    Value: "!$ app_config.application.version"
    Export:
      Name: "{{stack_name}}-version"
"#
        ).await?;
        
        Ok(())
    }
    
    fn setup_mock_responses(&mut self) {
        // Mock external service responses
        self.mock_responses.insert(
            "vault://api-keys".to_string(),
            ImportData {
                import_type: ImportType::Http,
                resolved_location: "vault://api-keys".to_string(),
                data: r#"{"api_key": "secret-key-12345", "db_password": "super-secure-password"}"#.to_string(),
                doc: json!({
                    "api_key": "secret-key-12345",
                    "db_password": "super-secure-password"
                }),
            }
        );
        
        self.mock_responses.insert(
            "git:describe".to_string(),
            ImportData {
                import_type: ImportType::Git,
                resolved_location: "git:describe".to_string(),
                data: "v2.1.3-15-g8a2b3c4".to_string(),
                doc: Value::String("v2.1.3-15-g8a2b3c4".to_string()),
            }
        );
    }
    
    pub fn base_path(&self) -> &std::path::Path {
        self.temp_dir.path()
    }
}

#[async_trait]
impl ImportLoader for FixtureImportLoader {
    async fn load(&self, location: &str, base_location: &str) -> Result<ImportData> {
        // Check for mock responses first
        if let Some(mock_data) = self.mock_responses.get(location) {
            return Ok(mock_data.clone());
        }
        
        // Fall back to file loading for local files
        if !location.contains(':') || location.starts_with("file:") {
            let clean_location = location.strip_prefix("file:").unwrap_or(location);
            let base_path = std::path::PathBuf::from(base_location);
            let resolved_path = if std::path::Path::new(clean_location).is_absolute() {
                std::path::PathBuf::from(clean_location)
            } else {
                let base_dir = base_path.parent().unwrap_or(std::path::Path::new("."));
                base_dir.join(clean_location)
            };
            
            let data = fs::read_to_string(&resolved_path).await?;
            let doc = if resolved_path.extension().and_then(|s| s.to_str()) == Some("json") {
                serde_json::from_str(&data)?
            } else {
                serde_yaml::from_str(&data)?
            };
            
            return Ok(ImportData {
                import_type: ImportType::File,
                resolved_location: resolved_path.to_string_lossy().to_string(),
                data,
                doc,
            });
        }
        
        Err(anyhow::anyhow!("Mock response not found for: {}", location))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_simple_file_imports() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("database.yaml");
        
        let mut doc = json!({
            "$imports": {
                "db": "database.yaml"
            },
            "application": {
                "database": "!$ db"
            }
        });
        
        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await.unwrap();
        
        // Check that import was loaded
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["db"]["host"], "localhost");
        assert_eq!(env_values["db"]["port"], 5432);
        assert_eq!(env_values["db"]["ssl_enabled"], true);
        
        // Check import record
        assert_eq!(imports_accum.len(), 1);
        assert_eq!(imports_accum[0].key, Some("db".to_string()));
    }
    
    #[tokio::test]
    async fn test_complex_nested_imports() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("app-config.yaml");
        
        let mut doc = serde_yaml::from_str(&fs::read_to_string(&main_file).await.unwrap()).unwrap();
        let mut imports_accum = Vec::new();
        
        load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await.unwrap();
        
        // Check $defs were processed
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["environment"], "production");
        assert_eq!(env_values["app_name"], "my-awesome-app");
        
        // Check file imports
        assert_eq!(env_values["base_db"]["host"], "localhost");
        assert_eq!(env_values["features"]["authentication"]["enabled"], true);
        
        // Check handlebars interpolation worked (env_config should load production.yaml)
        assert_eq!(env_values["env_config"]["database"]["host"], "prod-db.internal");
        assert_eq!(env_values["env_config"]["monitoring"]["alerting"], true);
        
        // Check git mock import
        assert_eq!(env_values["version_info"], "v2.1.3-15-g8a2b3c4");
        
        // Should have multiple import records
        assert!(imports_accum.len() >= 4);
        let import_keys: Vec<String> = imports_accum.iter()
            .filter_map(|r| r.key.clone())
            .collect();
        assert!(import_keys.contains(&"base_db".to_string()));
        assert!(import_keys.contains(&"features".to_string()));
        assert!(import_keys.contains(&"env_config".to_string()));
        assert!(import_keys.contains(&"version_info".to_string()));
    }
    
    #[tokio::test] 
    async fn test_recursive_imports_with_stack_template() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("stack-template.yaml");
        
        let mut doc = serde_yaml::from_str(&fs::read_to_string(&main_file).await.unwrap()).unwrap();
        let mut imports_accum = Vec::new();
        
        load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await.unwrap();
        
        // Check top-level $defs
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["stack_name"], "my-stack");
        assert_eq!(env_values["region"], "us-west-2");
        
        // Check that app_config was imported and processed recursively
        assert!(env_values.contains_key("app_config"));
        let app_config = &env_values["app_config"];
        
        // The app_config import should have been processed recursively
        // so it should have $envValues with its nested imports
        assert!(app_config.get("$envValues").is_some());
        let nested_env_values = app_config["$envValues"].as_object().unwrap();
        
        assert_eq!(nested_env_values["environment"], "production");
        assert_eq!(nested_env_values["app_name"], "my-awesome-app");
        assert!(nested_env_values.contains_key("base_db"));
        assert!(nested_env_values.contains_key("features"));
        assert!(nested_env_values.contains_key("env_config"));
        assert!(nested_env_values.contains_key("version_info"));
        
        // Check that secrets were imported
        assert!(env_values.contains_key("secrets"));
        assert_eq!(env_values["secrets"]["api_key"], "secret-key-12345");
        
        // Should have many import records due to recursive processing
        assert!(imports_accum.len() >= 6); // stack imports + app-config nested imports
    }
    
    #[tokio::test]
    async fn test_import_with_location_metadata() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("database.yaml");
        
        let mut doc = json!({
            "$imports": {
                "config": "features.json"
            }
        });
        
        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await.unwrap();
        
        // Check that $location metadata was added
        let env_values = doc["$envValues"].as_object().unwrap();
        let config = &env_values["config"];
        assert!(config.get("$location").is_some());
        assert!(config["$location"].as_str().unwrap().ends_with("features.json"));
        
        // Original content should still be there
        assert_eq!(config["authentication"]["enabled"], true);
    }
    
    #[tokio::test]
    async fn test_handlebars_interpolation_in_import_paths() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("main.yaml");
        
        let mut doc = json!({
            "$defs": {
                "env": "staging",
                "config_type": "yaml"
            },
            "$imports": {
                "env_config": "envs/{{env}}.{{config_type}}"
            }
        });
        
        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await.unwrap();
        
        // Should have loaded envs/staging.yaml
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["env_config"]["database"]["host"], "staging-db.internal");
        assert_eq!(env_values["env_config"]["monitoring"]["alerting"], false);
    }
    
    #[tokio::test]
    async fn test_import_name_collision_detection() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("database.yaml");
        
        let mut doc = json!({
            "$defs": {
                "config": "local-definition"
            },
            "$imports": {
                "config": "features.json"  // This should collide
            }
        });
        
        let mut imports_accum = Vec::new();
        let result = load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides"));
    }
    
    #[tokio::test]
    async fn test_mixed_file_types_json_and_yaml() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("main.yaml");
        
        let mut doc = json!({
            "$imports": {
                "db_config": "database.yaml",
                "features": "features.json"
            }
        });
        
        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await.unwrap();
        
        let env_values = doc["$envValues"].as_object().unwrap();
        
        // YAML import should work
        assert_eq!(env_values["db_config"]["host"], "localhost");
        assert_eq!(env_values["db_config"]["ssl_enabled"], true);
        
        // JSON import should work  
        assert_eq!(env_values["features"]["authentication"]["enabled"], true);
        assert_eq!(env_values["features"]["logging"]["level"], "info");
    }
    
    #[tokio::test]
    async fn test_sha256_digest_tracking() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("database.yaml");
        
        let mut doc = json!({
            "$imports": {
                "config": "features.json"
            }
        });
        
        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await.unwrap();
        
        // Check that SHA256 digest was calculated
        assert_eq!(imports_accum.len(), 1);
        let import_record = &imports_accum[0];
        assert!(!import_record.sha256_digest.is_empty());
        assert_eq!(import_record.sha256_digest.len(), 64); // SHA256 hex length
    }
    
    #[tokio::test]
    async fn test_error_propagation_for_missing_files() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("main.yaml");
        
        let mut doc = json!({
            "$imports": {
                "missing": "nonexistent-file.yaml"
            }
        });
        
        let mut imports_accum = Vec::new();
        let result = load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await;
        
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        println!("Error message: {}", error_msg);
        assert!(error_msg.contains("No such file") || error_msg.contains("bad import"));
    }
    
    #[tokio::test]
    async fn test_complex_handlebars_helpers() {
        let loader = FixtureImportLoader::new().await.unwrap();
        let main_file = loader.base_path().join("main.yaml");
        
        // Create a file that uses complex handlebars
        let complex_config_path = loader.base_path().join("complex.yaml");
        fs::write(&complex_config_path, r#"
service_name: "{{app_name}}"
config:
  database_url: "postgresql://{{toJson env_config.database}}"
  features_yaml: "{{toYaml features}}"
"#).await.unwrap();
        
        let mut doc = json!({
            "$defs": {
                "app_name": "test-service"
            },
            "$imports": {
                "env_config": "envs/production.yaml",
                "features": "features.json",
                "complex": "complex.yaml"
            }
        });
        
        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc,
            &main_file.to_string_lossy(),
            &mut imports_accum,
            &loader
        ).await.unwrap();
        
        // This test verifies that handlebars with helpers would work
        // (The actual helper implementation would be tested in handlebars module tests)
        let env_values = doc["$envValues"].as_object().unwrap();
        assert!(env_values.contains_key("complex"));
        assert!(env_values.contains_key("env_config"));
        assert!(env_values.contains_key("features"));
    }
}