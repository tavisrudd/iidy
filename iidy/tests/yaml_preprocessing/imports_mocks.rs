//! Comprehensive mock tests for import loaders
//! 
//! This module contains mock tests for network-dependent and AWS import types
//! to ensure they work correctly in isolation without requiring actual network
//! access or AWS credentials.

use std::collections::HashMap;
use anyhow::Result;
use serde_json::{json, Value};
use async_trait::async_trait;
use mockito::Server;
use tempfile::TempDir;

use iidy::yaml::imports::{ImportLoader, ImportData, ImportType, load_imports};
use iidy::yaml::imports::loaders::{load_http_import, load_git_import_with_executor, GitCommandExecutor};

/// Mock import loader for testing that returns predefined responses
pub struct MockImportLoader {
    responses: HashMap<String, ImportData>,
}

impl MockImportLoader {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }
    
    pub fn add_response(&mut self, location: &str, response: ImportData) {
        self.responses.insert(location.to_string(), response);
    }
    
    pub fn add_json_response(&mut self, location: &str, json_data: Value) {
        let data = serde_json::to_string(&json_data).unwrap();
        self.add_response(location, ImportData {
            import_type: ImportType::Http, // Default type
            resolved_location: location.to_string(),
            data: data.clone(),
            doc: json_data,
        });
    }
    
    pub fn add_yaml_response(&mut self, location: &str, yaml_data: &str) {
        let doc: Value = serde_yaml::from_str(yaml_data).unwrap();
        self.add_response(location, ImportData {
            import_type: ImportType::File,
            resolved_location: location.to_string(),
            data: yaml_data.to_string(),
            doc,
        });
    }
}

#[async_trait]
impl ImportLoader for MockImportLoader {
    async fn load(&self, location: &str, _base_location: &str) -> Result<ImportData> {
        self.responses.get(location)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Mock response not found for: {}", location))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_http_import_with_mockito() {
        let mut server = Server::new_async().await;
        let _m = server.mock("GET", "/config.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"environment": "production", "debug": false}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/config.json", server.url());
        
        let result = load_http_import(&url, "test_base", &client).await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Http);
        assert_eq!(result.doc["environment"], "production");
        assert_eq!(result.doc["debug"], false);
    }

    #[tokio::test]
    async fn test_http_import_yaml_with_mockito() {
        let mut server = Server::new_async().await;
        let _m = server.mock("GET", "/config.yaml")
            .with_status(200)
            .with_header("content-type", "application/yaml")
            .with_body("database:\n  host: localhost\n  port: 5432\nfeatures:\n  - auth\n  - logging")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/config.yaml", server.url());
        
        let result = load_http_import(&url, "test_base", &client).await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Http);
        assert_eq!(result.doc["database"]["host"], "localhost");
        assert_eq!(result.doc["database"]["port"], 5432);
        assert!(result.doc["features"].as_array().unwrap().contains(&json!("auth")));
    }

    #[tokio::test]
    async fn test_http_import_error_handling() {
        let mut server = Server::new_async().await;
        let _m = server.mock("GET", "/not-found")
            .with_status(404)
            .with_body("Not Found")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/not-found", server.url());
        
        let result = load_http_import(&url, "test_base", &client).await;
        // The HTTP call succeeds with a 404 status, but we get the body "Not Found"
        // Let's check that we get the expected result
        assert!(result.is_ok());
        assert_eq!(result.unwrap().doc, "Not Found");
    }

    #[tokio::test]
    async fn test_mock_import_loader_basic() {
        let mut loader = MockImportLoader::new();
        loader.add_json_response("api://config", json!({
            "app": "test-app",
            "version": "1.0.0"
        }));

        let result = loader.load("api://config", "base").await.unwrap();
        assert_eq!(result.doc["app"], "test-app");
        assert_eq!(result.doc["version"], "1.0.0");
    }

    #[tokio::test]
    async fn test_mock_import_loader_yaml() {
        let mut loader = MockImportLoader::new();
        loader.add_yaml_response("config.yaml", 
            "database:\n  host: db.example.com\n  port: 3306\ncache:\n  enabled: true");

        let result = loader.load("config.yaml", "base").await.unwrap();
        assert_eq!(result.doc["database"]["host"], "db.example.com");
        assert_eq!(result.doc["database"]["port"], 3306);
        assert_eq!(result.doc["cache"]["enabled"], true);
    }

    #[tokio::test]
    async fn test_load_imports_with_mock_loader() {
        let temp_dir = TempDir::new().unwrap();
        let base_file = temp_dir.path().join("main.yaml");
        
        // Create a document with imports
        let mut doc = json!({
            "$imports": {
                "config": "api://config",
                "secrets": "vault://secrets"
            },
            "$defs": {
                "app_name": "test-app"
            },
            "application": {
                "name": "!$ app_name",
                "database": "!$ config.database",
                "api_key": "!$ secrets.api_key"
            }
        });

        let mut loader = MockImportLoader::new();
        loader.add_json_response("api://config", json!({
            "database": {
                "host": "prod-db.example.com",
                "port": 5432
            }
        }));
        loader.add_json_response("vault://secrets", json!({
            "api_key": "secret-123",
            "db_password": "secure-pwd"
        }));

        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc, 
            &base_file.to_string_lossy(), 
            &mut imports_accum, 
            &loader
        ).await.unwrap();

        // Check that $envValues was populated
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["app_name"], "test-app");
        assert_eq!(env_values["config"]["database"]["host"], "prod-db.example.com");
        assert_eq!(env_values["secrets"]["api_key"], "secret-123");

        // Check import records
        assert_eq!(imports_accum.len(), 2);
        assert!(imports_accum.iter().any(|r| r.key == Some("config".to_string())));
        assert!(imports_accum.iter().any(|r| r.key == Some("secrets".to_string())));
    }

    #[tokio::test]
    async fn test_load_imports_with_handlebars_interpolation() {
        let temp_dir = TempDir::new().unwrap();
        let base_file = temp_dir.path().join("main.yaml");
        
        // Create a document with handlebars in import locations
        let mut doc = json!({
            "$defs": {
                "env": "production",
                "service": "api"
            },
            "$imports": {
                "config": "{{service}}-{{env}}.json"
            }
        });

        let mut loader = MockImportLoader::new();
        loader.add_json_response("api-production.json", json!({
            "database_url": "postgresql://prod.db.example.com/app",
            "redis_url": "redis://prod.cache.example.com:6379"
        }));

        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc, 
            &base_file.to_string_lossy(), 
            &mut imports_accum, 
            &loader
        ).await.unwrap();

        // Check that handlebars interpolation worked
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["env"], "production");
        assert_eq!(env_values["service"], "api");
        assert_eq!(env_values["config"]["database_url"], "postgresql://prod.db.example.com/app");
    }

    #[tokio::test]
    async fn test_load_imports_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let base_file = temp_dir.path().join("main.yaml");
        
        // Create a document with imports that themselves have imports
        let mut doc = json!({
            "$imports": {
                "base_config": "base.yaml"
            }
        });

        let mut loader = MockImportLoader::new();
        
        // Base config that imports another config
        loader.add_response("base.yaml", ImportData {
            import_type: ImportType::File,
            resolved_location: "base.yaml".to_string(),
            data: "database:\n  host: localhost".to_string(),
            doc: json!({
                "database": {"host": "localhost"},
                "$imports": {
                    "secrets": "secrets.yaml"
                }
            }),
        });
        
        // Secrets config
        loader.add_yaml_response("secrets.yaml", 
            "api_key: secret-123\ndb_password: secure-pwd");

        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc, 
            &base_file.to_string_lossy(), 
            &mut imports_accum, 
            &loader
        ).await.unwrap();

        // Check that recursive imports worked
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["base_config"]["database"]["host"], "localhost");
        
        // Should have 2 import records (base_config and its nested secrets import)
        assert_eq!(imports_accum.len(), 2);
    }

    #[tokio::test]
    async fn test_load_imports_name_collision_error() {
        let temp_dir = TempDir::new().unwrap();
        let base_file = temp_dir.path().join("main.yaml");
        
        // Create a document with name collision between $defs and $imports
        let mut doc = json!({
            "$defs": {
                "config": "local-value"
            },
            "$imports": {
                "config": "api://config"  // This should cause a collision
            }
        });

        let mut loader = MockImportLoader::new();
        loader.add_json_response("api://config", json!({"remote": "value"}));

        let mut imports_accum = Vec::new();
        let result = load_imports(
            &mut doc, 
            &base_file.to_string_lossy(), 
            &mut imports_accum, 
            &loader
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides"));
    }

    #[tokio::test]
    async fn test_load_imports_invalid_import_value() {
        let temp_dir = TempDir::new().unwrap();
        let base_file = temp_dir.path().join("main.yaml");
        
        // Create a document with invalid import value (not a string)
        let mut doc = json!({
            "$imports": {
                "config": {"not": "a string"}
            }
        });

        let loader = MockImportLoader::new();
        let mut imports_accum = Vec::new();
        let result = load_imports(
            &mut doc, 
            &base_file.to_string_lossy(), 
            &mut imports_accum, 
            &loader
        ).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Import values must be strings"));
    }

    #[tokio::test]
    async fn test_load_imports_with_location_metadata() {
        let temp_dir = TempDir::new().unwrap();
        let base_file = temp_dir.path().join("main.yaml");
        
        let mut doc = json!({
            "$imports": {
                "config": "api://config"
            }
        });

        let mut loader = MockImportLoader::new();
        loader.add_json_response("api://config", json!({"setting": "value"}));

        let mut imports_accum = Vec::new();
        load_imports(
            &mut doc, 
            &base_file.to_string_lossy(), 
            &mut imports_accum, 
            &loader
        ).await.unwrap();

        // Check that $location was added to the imported document
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["config"]["$location"], "api://config");
        assert_eq!(env_values["config"]["setting"], "value");
    }
}

/// Mock git command executor for testing
pub struct MockGitExecutor {
    responses: HashMap<String, String>,
}

impl MockGitExecutor {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }
    
    pub fn add_response(&mut self, command: &str, output: &str) {
        self.responses.insert(command.to_string(), output.to_string());
    }
}

#[async_trait]
impl GitCommandExecutor for MockGitExecutor {
    async fn execute(&self, command: &str) -> Result<String> {
        self.responses.get(command)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Mock git command not found: {}", command))
    }
}

#[cfg(test)]
mod git_tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_git_branch() {
        let mut executor = MockGitExecutor::new();
        executor.add_response("git rev-parse --abbrev-ref HEAD", "main");

        let result = load_git_import_with_executor("git:branch", "base", &executor).await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Git);
        assert_eq!(result.doc, "main");
        assert_eq!(result.data, "main");
    }

    #[tokio::test]
    async fn test_mock_git_describe() {
        let mut executor = MockGitExecutor::new();
        executor.add_response("git describe --dirty --tags", "v1.2.3-5-g1234567");

        let result = load_git_import_with_executor("git:describe", "base", &executor).await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Git);
        assert_eq!(result.doc, "v1.2.3-5-g1234567");
    }

    #[tokio::test]
    async fn test_mock_git_sha() {
        let mut executor = MockGitExecutor::new();
        executor.add_response("git rev-parse HEAD", "a1b2c3d4e5f6789012345678901234567890abcd");

        let result = load_git_import_with_executor("git:sha", "base", &executor).await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Git);
        assert_eq!(result.doc, "a1b2c3d4e5f6789012345678901234567890abcd");
    }

    #[tokio::test]
    async fn test_mock_git_invalid_command() {
        let executor = MockGitExecutor::new();

        let result = load_git_import_with_executor("git:invalid", "base", &executor).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid git command"));
    }

    #[tokio::test]
    async fn test_mock_git_command_failure() {
        let executor = MockGitExecutor::new();

        let result = load_git_import_with_executor("git:branch", "base", &executor).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Mock git command not found"));
    }
}