//! Comprehensive test cases based on iidy-js reference implementation
//!
//! These tests replicate specific scenarios from the original TypeScript
//! implementation to ensure feature parity and robust edge case handling.

use std::collections::HashMap;
use anyhow::Result;
use serde_json::{json, Value};
use async_trait::async_trait;

use crate::yaml::imports::{ImportLoader, ImportData, ImportType, load_imports, parse_import_type};

/// Enhanced mock loader that supports more sophisticated testing patterns
pub struct EnhancedMockLoader {
    responses: HashMap<String, ImportData>,
}

impl EnhancedMockLoader {
    pub fn new() -> Self {
        Self {
            responses: HashMap::new(),
        }
    }
    
    pub fn add_response(&mut self, location: &str, response: ImportData) {
        self.responses.insert(location.to_string(), response);
    }
    
    /// Add a simple string response
    pub fn add_string_response(&mut self, location: &str, data: &str) {
        self.add_response(location, ImportData {
            import_type: ImportType::File,
            resolved_location: location.to_string(),
            data: data.to_string(),
            doc: Value::String(data.to_string()),
        });
    }
    
    /// Add a response that simulates a nested document with its own imports
    pub fn add_nested_import_response(&mut self, location: &str, nested_doc: Value) {
        let data = serde_json::to_string(&nested_doc).unwrap();
        self.add_response(location, ImportData {
            import_type: ImportType::S3,
            resolved_location: location.to_string(),
            data,
            doc: nested_doc,
        });
    }
}

#[async_trait]
impl ImportLoader for EnhancedMockLoader {
    async fn load(&self, location: &str, _base_location: &str) -> Result<ImportData> {
        self.responses.get(location)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("Mock response not found for: {}", location))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test cases based on lines 155-192 from TypeScript implementation
    #[test]
    fn test_parse_import_type_comprehensive_edge_cases() {
        // Test with various base locations like the TypeScript implementation
        for base_location in ["/", "/home/test", "."] {
            // File imports
            assert_eq!(parse_import_type("test.yaml", base_location).unwrap(), ImportType::File);
            assert_eq!(parse_import_type("/root/test.yaml", base_location).unwrap(), ImportType::File);
            assert_eq!(parse_import_type("sub/test.yaml", base_location).unwrap(), ImportType::File);
            assert_eq!(parse_import_type("sub/test.json", base_location).unwrap(), ImportType::File);

            // S3 imports
            assert_eq!(parse_import_type("s3://bucket/test.yaml", base_location).unwrap(), ImportType::S3);

            // HTTP/HTTPS imports (both should map to Http)
            assert_eq!(parse_import_type("http://host.com/test.yaml", base_location).unwrap(), ImportType::Http);
            assert_eq!(parse_import_type("https://host.com/test.yaml", base_location).unwrap(), ImportType::Http);

            // SSM imports with various path formats
            assert_eq!(parse_import_type("ssm:/foo", base_location).unwrap(), ImportType::Ssm);
            assert_eq!(parse_import_type("ssm:foo", base_location).unwrap(), ImportType::Ssm);
            assert_eq!(parse_import_type("ssm:/foo/bar", base_location).unwrap(), ImportType::Ssm);

            // SSM-PATH imports
            assert_eq!(parse_import_type("ssm-path:/foo", base_location).unwrap(), ImportType::SsmPath);
            assert_eq!(parse_import_type("ssm-path:/", base_location).unwrap(), ImportType::SsmPath);

            // Random generators
            assert_eq!(parse_import_type("random:dashed-name", base_location).unwrap(), ImportType::Random);
            assert_eq!(parse_import_type("random:name", base_location).unwrap(), ImportType::Random);
            assert_eq!(parse_import_type("random:int", base_location).unwrap(), ImportType::Random);

            // File hash
            assert_eq!(parse_import_type("filehash:foo.yaml", base_location).unwrap(), ImportType::Filehash);
        }
    }

    #[test]
    fn test_import_type_security_restrictions_comprehensive() {
        // Test that local-only imports are blocked from remote templates
        
        // Should fail: file import from S3 template
        let result = parse_import_type("./local.yaml", "s3://bucket/template.yaml");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed from remote template"));
        
        // Should fail: env import from HTTP template  
        let result = parse_import_type("env:SECRET", "https://example.com/template.yaml");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not allowed from remote template"));
        
        // Should succeed: explicit S3 import from S3 template
        assert_eq!(
            parse_import_type("s3://other/config.yaml", "s3://bucket/template.yaml").unwrap(), 
            ImportType::S3
        );
        
        // Should inherit base type when no explicit type
        assert_eq!(
            parse_import_type("other/config.yaml", "s3://bucket/template.yaml").unwrap(),
            ImportType::S3
        );
    }

    // Test case based on lines 126-153 from TypeScript implementation
    #[tokio::test]
    async fn test_mock_import_loader_like_typescript() {
        let mut loader = EnhancedMockLoader::new();
        
        // Set up mock responses like the TypeScript test
        loader.add_string_response("s3://mock/mock1", "mock");
        loader.add_nested_import_response("s3://mock/mock2", json!({
            "$imports": {"a": "s3://mock/mock1"},
            "literal": 1234,
            "aref": "!$ a"
        }));

        // Test basic import and reference resolution
        let mut test_doc = json!({
            "$imports": {"a": "s3://mock/mock1"},
            "literal": 1234,
            "aref": "!$ a"
        });

        let mut imports_accum = Vec::new();
        load_imports(&mut test_doc, "root", &mut imports_accum, &loader).await.unwrap();

        // After import processing, should have loaded 'a' into $envValues
        let env_values = test_doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["a"], "mock");
        assert_eq!(test_doc["literal"], 1234);
        
        // Test nested document imports
        let mut nested_test = json!({
            "$imports": {"nested": "s3://mock/mock2"},
            "literal": "!$ nested.literal"
        });

        let mut imports_accum = Vec::new();
        load_imports(&mut nested_test, "root", &mut imports_accum, &loader).await.unwrap();

        // Should have recursively processed the nested document
        let env_values = nested_test["$envValues"].as_object().unwrap();
        assert!(env_values.contains_key("nested"));
        let nested = &env_values["nested"];
        assert_eq!(nested["literal"], 1234);
        
        // Check that nested imports were processed
        assert!(nested["$envValues"].as_object().unwrap().contains_key("a"));
        assert_eq!(nested["$envValues"]["a"], "mock");
    }

    #[tokio::test]
    async fn test_import_location_metadata_injection() {
        let mut loader = EnhancedMockLoader::new();
        loader.add_response("api://config", ImportData {
            import_type: ImportType::Http,
            resolved_location: "api://config".to_string(),
            data: r#"{"setting": "value"}"#.to_string(),
            doc: json!({"setting": "value"}),
        });

        let mut doc = json!({
            "$imports": {"config": "api://config"}
        });

        let mut imports_accum = Vec::new();
        load_imports(&mut doc, "base", &mut imports_accum, &loader).await.unwrap();

        // Should have $location metadata injected
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["config"]["$location"], "api://config");
        assert_eq!(env_values["config"]["setting"], "value");
    }

    #[tokio::test]
    async fn test_import_records_comprehensive_tracking() {
        let mut loader = EnhancedMockLoader::new();
        loader.add_string_response("config1.yaml", "content1");
        loader.add_string_response("config2.yaml", "content2");

        let mut doc = json!({
            "$imports": {
                "config1": "config1.yaml",
                "config2": "config2.yaml"
            }
        });

        let mut imports_accum = Vec::new();
        load_imports(&mut doc, "base.yaml", &mut imports_accum, &loader).await.unwrap();

        // Should track detailed import metadata
        assert_eq!(imports_accum.len(), 2);
        
        let config1_record = imports_accum.iter()
            .find(|r| r.key == Some("config1".to_string())).unwrap();
        assert_eq!(config1_record.from, "base.yaml");
        assert_eq!(config1_record.imported, "config1.yaml");
        assert!(!config1_record.sha256_digest.is_empty());
        assert_eq!(config1_record.sha256_digest.len(), 64); // SHA256 hex length

        let config2_record = imports_accum.iter()
            .find(|r| r.key == Some("config2".to_string())).unwrap();
        assert_eq!(config2_record.from, "base.yaml");
        assert_eq!(config2_record.imported, "config2.yaml");
        assert!(!config2_record.sha256_digest.is_empty());
    }

    #[tokio::test]
    async fn test_invalid_import_mapping_error() {
        let loader = EnhancedMockLoader::new();
        
        // Test error when $imports is not a mapping
        let mut doc = json!({
            "$imports": ["not", "a", "mapping"]
        });

        let mut imports_accum = Vec::new();
        let result = load_imports(&mut doc, "base", &mut imports_accum, &loader).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Should be mapping"));
    }

    #[tokio::test]
    async fn test_invalid_import_value_type_error() {
        let loader = EnhancedMockLoader::new();
        
        // Test error when import value is not a string
        let mut doc = json!({
            "$imports": {
                "config": {"not": "a string"}
            }
        });

        let mut imports_accum = Vec::new();
        let result = load_imports(&mut doc, "base", &mut imports_accum, &loader).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Import values must be strings"));
    }

    #[tokio::test]
    async fn test_import_params_collision_detection() {
        let mut loader = EnhancedMockLoader::new();
        loader.add_string_response("ssm:/db/host", "localhost");

        // Test collision detection with $params
        let mut doc = json!({
            "$imports": {"dbHost": "ssm:/db/host"},
            "$params": [{"Name": "dbHost", "Type": "String"}]
        });

        let mut imports_accum = Vec::new();
        let result = load_imports(&mut doc, "base", &mut imports_accum, &loader).await;
        
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("collides"));
    }

    #[tokio::test]
    async fn test_handlebars_in_import_locations() {
        let mut loader = EnhancedMockLoader::new();
        loader.add_string_response("api-production.json", "config-data");

        // Test that handlebars in import locations are resolved
        let mut doc = json!({
            "$defs": {"env": "production", "service": "api"},
            "$imports": {"config": "{{service}}-{{env}}.json"}
        });

        let mut imports_accum = Vec::new();
        load_imports(&mut doc, "base", &mut imports_accum, &loader).await.unwrap();

        // Should have resolved handlebars interpolation in import location
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["config"], "config-data");
        
        // Check that the import record shows the resolved location
        let import_record = imports_accum.iter()
            .find(|r| r.key == Some("config".to_string())).unwrap();
        assert_eq!(import_record.imported, "api-production.json");
    }

    #[tokio::test]
    async fn test_nested_imports_with_collision_prevention() {
        let mut loader = EnhancedMockLoader::new();
        
        // Create a nested document with its own $defs and $imports
        loader.add_nested_import_response("nested.yaml", json!({
            "$defs": {"local_var": "nested_value"},
            "$imports": {"external": "external.yaml"},
            "config": {
                "value": "!$ local_var",
                "external_ref": "!$ external"
            }
        }));
        
        loader.add_string_response("external.yaml", "external_data");

        let mut doc = json!({
            "$defs": {"main_var": "main_value"},
            "$imports": {"nested": "nested.yaml"},
            "main_config": {
                "main": "!$ main_var",
                "nested_config": "!$ nested.config"
            }
        });

        let mut imports_accum = Vec::new();
        load_imports(&mut doc, "base", &mut imports_accum, &loader).await.unwrap();

        // Should have processed nested imports without collision
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["main_var"], "main_value");
        assert!(env_values.contains_key("nested"));
        
        let nested = &env_values["nested"];
        assert_eq!(nested["config"]["value"], "!$ local_var");
        assert_eq!(nested["config"]["external_ref"], "!$ external");
        
        // Check that nested $envValues contains both local_var and external
        let nested_env = nested["$envValues"].as_object().unwrap();
        assert_eq!(nested_env["local_var"], "nested_value");
        assert_eq!(nested_env["external"], "external_data");
        
        // Should have multiple import records
        assert!(imports_accum.len() >= 2);
    }

    #[tokio::test]
    async fn test_yaml_vs_json_vs_text_import_parsing() {
        let mut loader = EnhancedMockLoader::new();
        
        // YAML content should be parsed into structured data
        loader.add_response("config.yaml", ImportData {
            import_type: ImportType::File,
            resolved_location: "config.yaml".to_string(),
            data: "database:\n  host: localhost\n  port: 5432\n".to_string(),
            doc: json!({"database": {"host": "localhost", "port": 5432}}),
        });
        
        // JSON content should be parsed into structured data
        loader.add_response("settings.json", ImportData {
            import_type: ImportType::File,
            resolved_location: "settings.json".to_string(),
            data: r#"{"api_key": "secret", "timeout": 30}"#.to_string(),
            doc: json!({"api_key": "secret", "timeout": 30}),
        });
        
        // Plain text should remain as string
        loader.add_response("version.txt", ImportData {
            import_type: ImportType::File,
            resolved_location: "version.txt".to_string(),
            data: "v1.2.3".to_string(),
            doc: json!("v1.2.3"),
        });

        let mut doc = json!({
            "$imports": {
                "db_config": "config.yaml",
                "app_settings": "settings.json",
                "version": "version.txt"
            }
        });

        let mut imports_accum = Vec::new();
        load_imports(&mut doc, "base", &mut imports_accum, &loader).await.unwrap();

        let env_values = doc["$envValues"].as_object().unwrap();
        
        // YAML should be structured
        assert_eq!(env_values["db_config"]["database"]["host"], "localhost");
        assert_eq!(env_values["db_config"]["database"]["port"], 5432);
        
        // JSON should be structured
        assert_eq!(env_values["app_settings"]["api_key"], "secret");
        assert_eq!(env_values["app_settings"]["timeout"], 30);
        
        // Text should be string
        assert_eq!(env_values["version"], "v1.2.3");
    }

    #[tokio::test]
    async fn test_complex_nested_handlebars_with_imports() {
        let mut loader = EnhancedMockLoader::new();
        
        loader.add_response("env/production.json", ImportData {
            import_type: ImportType::File,
            resolved_location: "env/production.json".to_string(),
            data: r#"{"database": {"host": "prod-db.example.com", "port": 5432}, "redis": {"host": "prod-redis.example.com"}}"#.to_string(),
            doc: json!({
                "database": {"host": "prod-db.example.com", "port": 5432},
                "redis": {"host": "prod-redis.example.com"}
            }),
        });

        let mut doc = json!({
            "$defs": {
                "environment": "production",
                "app_name": "my-app"
            },
            "$imports": {
                "env_config": "env/{{environment}}.json"
            },
            "connection_strings": {
                "database": "postgresql://{{env_config.database.host}}:{{env_config.database.port}}/{{app_name}}",
                "redis": "redis://{{env_config.redis.host}}:6379"
            }
        });

        let mut imports_accum = Vec::new();
        load_imports(&mut doc, "base", &mut imports_accum, &loader).await.unwrap();

        // Should have resolved handlebars in import location and loaded the config
        let env_values = doc["$envValues"].as_object().unwrap();
        assert_eq!(env_values["environment"], "production");
        assert_eq!(env_values["app_name"], "my-app");
        assert_eq!(env_values["env_config"]["database"]["host"], "prod-db.example.com");
        
        // Import record should show the interpolated location
        let import_record = imports_accum.iter()
            .find(|r| r.key == Some("env_config".to_string())).unwrap();
        assert_eq!(import_record.imported, "env/production.json");
    }
}