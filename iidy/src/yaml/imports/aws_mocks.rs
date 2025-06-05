//! AWS service mock tests for import loaders
//! 
//! This module contains mock tests for AWS-dependent import types using
//! mock AWS SDK responses to test the import functionality without requiring
//! actual AWS credentials or services.

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use anyhow::Result;
    use serde_json::{json, Value};
    use async_trait::async_trait;
    
    use crate::yaml::imports::{ImportLoader, ImportData, ImportType};

    /// Mock AWS import loader that simulates AWS service responses
    pub struct MockAwsImportLoader {
        s3_responses: HashMap<String, String>,
        cfn_responses: HashMap<String, Value>,
        ssm_responses: HashMap<String, String>,
    }

    impl MockAwsImportLoader {
        pub fn new() -> Self {
            Self {
                s3_responses: HashMap::new(),
                cfn_responses: HashMap::new(),
                ssm_responses: HashMap::new(),
            }
        }
        
        pub fn add_s3_response(&mut self, bucket_key: &str, content: &str) {
            self.s3_responses.insert(bucket_key.to_string(), content.to_string());
        }
        
        pub fn add_cfn_response(&mut self, stack_field: &str, response: Value) {
            self.cfn_responses.insert(stack_field.to_string(), response);
        }
        
        pub fn add_ssm_response(&mut self, parameter: &str, value: &str) {
            self.ssm_responses.insert(parameter.to_string(), value.to_string());
        }
    }

    #[async_trait]
    impl ImportLoader for MockAwsImportLoader {
        async fn load(&self, location: &str, _base_location: &str) -> Result<ImportData> {
            if location.starts_with("s3://") {
                // Mock S3 import
                let key = location.strip_prefix("s3://").unwrap();
                let content = self.s3_responses.get(key)
                    .ok_or_else(|| anyhow::anyhow!("Mock S3 object not found: {}", key))?;
                
                let doc = if location.ends_with(".json") {
                    serde_json::from_str(content)?
                } else if location.ends_with(".yaml") || location.ends_with(".yml") {
                    serde_yaml::from_str(content)?
                } else {
                    Value::String(content.clone())
                };
                
                Ok(ImportData {
                    import_type: ImportType::S3,
                    resolved_location: location.to_string(),
                    data: content.clone(),
                    doc,
                })
            } else if location.starts_with("cfn:") {
                // Mock CloudFormation import
                let key = location.strip_prefix("cfn:").unwrap();
                let response = self.cfn_responses.get(key)
                    .ok_or_else(|| anyhow::anyhow!("Mock CFN response not found: {}", key))?;
                
                Ok(ImportData {
                    import_type: ImportType::Cfn,
                    resolved_location: location.to_string(),
                    data: serde_json::to_string(response)?,
                    doc: response.clone(),
                })
            } else if location.starts_with("ssm:") {
                // Mock SSM import
                let parts: Vec<&str> = location.splitn(3, ':').collect();
                let param_name = parts[1];
                let format = parts.get(2);
                
                let value = self.ssm_responses.get(param_name)
                    .ok_or_else(|| anyhow::anyhow!("Mock SSM parameter not found: {}", param_name))?;
                
                let doc = match format.copied() {
                    Some("json") => serde_json::from_str(value)?,
                    Some("yaml") => serde_yaml::from_str(value)?,
                    _ => Value::String(value.clone()),
                };
                
                Ok(ImportData {
                    import_type: ImportType::Ssm,
                    resolved_location: location.to_string(),
                    data: value.clone(),
                    doc,
                })
            } else {
                Err(anyhow::anyhow!("Unsupported mock import type: {}", location))
            }
        }
    }

    #[tokio::test]
    async fn test_mock_s3_json_import() {
        let mut loader = MockAwsImportLoader::new();
        loader.add_s3_response("my-bucket/config.json", r#"{
            "environment": "production",
            "database": {
                "host": "prod-db.amazonaws.com",
                "port": 5432
            }
        }"#);

        let result = loader.load("s3://my-bucket/config.json", "base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::S3);
        assert_eq!(result.doc["environment"], "production");
        assert_eq!(result.doc["database"]["host"], "prod-db.amazonaws.com");
        assert_eq!(result.doc["database"]["port"], 5432);
    }

    #[tokio::test]
    async fn test_mock_s3_yaml_import() {
        let mut loader = MockAwsImportLoader::new();
        loader.add_s3_response("my-bucket/app.yaml", 
            "application:\n  name: my-app\n  version: 2.1.0\nfeatures:\n  - auth\n  - logging\n  - monitoring");

        let result = loader.load("s3://my-bucket/app.yaml", "base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::S3);
        assert_eq!(result.doc["application"]["name"], "my-app");
        assert_eq!(result.doc["application"]["version"], "2.1.0");
        assert!(result.doc["features"].as_array().unwrap().contains(&json!("auth")));
    }

    #[tokio::test]
    async fn test_mock_s3_text_import() {
        let mut loader = MockAwsImportLoader::new();
        loader.add_s3_response("my-bucket/secret.txt", "super-secret-api-key-12345");

        let result = loader.load("s3://my-bucket/secret.txt", "base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::S3);
        assert_eq!(result.doc, "super-secret-api-key-12345");
    }

    #[tokio::test]
    async fn test_mock_cfn_stack_output() {
        let mut loader = MockAwsImportLoader::new();
        loader.add_cfn_response("output/MyStack/DatabaseUrl", 
            json!("postgresql://prod.db.internal:5432/myapp"));

        let result = loader.load("cfn:output/MyStack/DatabaseUrl", "base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.doc, "postgresql://prod.db.internal:5432/myapp");
    }

    #[tokio::test]
    async fn test_mock_cfn_export() {
        let mut loader = MockAwsImportLoader::new();
        loader.add_cfn_response("export/VpcId", json!("vpc-12345678"));

        let result = loader.load("cfn:export/VpcId", "base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.doc, "vpc-12345678");
    }

    #[tokio::test]
    async fn test_mock_ssm_string_parameter() {
        let mut loader = MockAwsImportLoader::new();
        loader.add_ssm_response("/app/database/host", "prod-db.internal");

        let result = loader.load("ssm:/app/database/host", "base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Ssm);
        assert_eq!(result.doc, "prod-db.internal");
    }

    #[tokio::test]
    async fn test_mock_ssm_json_parameter() {
        let mut loader = MockAwsImportLoader::new();
        loader.add_ssm_response("/app/database/config", r#"{
            "host": "prod-db.internal",
            "port": 5432,
            "database": "myapp",
            "ssl": true
        }"#);

        let result = loader.load("ssm:/app/database/config:json", "base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Ssm);
        assert_eq!(result.doc["host"], "prod-db.internal");
        assert_eq!(result.doc["port"], 5432);
        assert_eq!(result.doc["ssl"], true);
    }

    #[tokio::test]
    async fn test_mock_ssm_yaml_parameter() {
        let mut loader = MockAwsImportLoader::new();
        loader.add_ssm_response("/app/features", 
            "enabled:\n  - auth\n  - logging\ndisabled:\n  - beta_features");

        let result = loader.load("ssm:/app/features:yaml", "base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Ssm);
        assert!(result.doc["enabled"].as_array().unwrap().contains(&json!("auth")));
        assert!(result.doc["disabled"].as_array().unwrap().contains(&json!("beta_features")));
    }

    #[tokio::test]
    async fn test_mock_aws_import_not_found() {
        let loader = MockAwsImportLoader::new();
        
        let result = loader.load("s3://missing-bucket/missing-key", "base").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Mock S3 object not found"));
    }

    #[tokio::test]
    async fn test_mock_multiple_aws_services() {
        let mut loader = MockAwsImportLoader::new();
        
        // Set up multiple AWS service responses
        loader.add_s3_response("config-bucket/app.json", r#"{"app_name": "my-app"}"#);
        loader.add_cfn_response("output/InfraStack/VpcId", json!("vpc-abcdef"));
        loader.add_ssm_response("/app/database/password", "super-secret-password");

        // Test S3 import
        let s3_result = loader.load("s3://config-bucket/app.json", "base").await.unwrap();
        assert_eq!(s3_result.doc["app_name"], "my-app");

        // Test CloudFormation import
        let cfn_result = loader.load("cfn:output/InfraStack/VpcId", "base").await.unwrap();
        assert_eq!(cfn_result.doc, "vpc-abcdef");

        // Test SSM import
        let ssm_result = loader.load("ssm:/app/database/password", "base").await.unwrap();
        assert_eq!(ssm_result.doc, "super-secret-password");
    }
}