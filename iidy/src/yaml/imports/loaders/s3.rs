//! AWS S3 import loader
//!
//! Provides functionality for loading content from S3 objects

use anyhow::{Result, anyhow};
use async_trait::async_trait;

use super::utils::resolve_doc_from_import_data;
use crate::yaml::imports::{ImportData, ImportType};

/// Trait for S3 operations (allows mocking in tests)
#[async_trait]
pub trait S3Client: Send + Sync {
    async fn get_object(&self, bucket: &str, key: &str) -> Result<String>;
}

/// Production S3 client implementation
pub struct AwsS3Client {
    client: aws_sdk_s3::Client,
}

impl AwsS3Client {
    pub fn new(aws_config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_s3::Client::new(aws_config),
        }
    }
}

#[async_trait]
impl S3Client for AwsS3Client {
    async fn get_object(&self, bucket: &str, key: &str) -> Result<String> {
        let response = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .map_err(|e| anyhow!("Failed to fetch S3 object s3://{}/{}: {}", bucket, key, e))?;

        let body = response.body.collect().await.map_err(|e| {
            anyhow!(
                "Failed to read S3 object body s3://{}/{}: {}",
                bucket,
                key,
                e
            )
        })?;

        String::from_utf8(body.into_bytes().to_vec()).map_err(|e| {
            anyhow!(
                "S3 object s3://{}/{} contains invalid UTF-8: {}",
                bucket,
                key,
                e
            )
        })
    }
}

/// Load an S3 import
pub async fn load_s3_import(
    location: &str,
    aws_config: &aws_config::SdkConfig,
) -> Result<ImportData> {
    let client = AwsS3Client::new(aws_config);
    load_s3_import_with_client(location, &client).await
}

/// Load an S3 import with custom client (for testing)
pub async fn load_s3_import_with_client(
    location: &str,
    client: &dyn S3Client,
) -> Result<ImportData> {
    // Parse s3://bucket/key format
    let (bucket, key) = parse_s3_location(location)?;

    // Download object content
    let data = client.get_object(&bucket, &key).await?;

    // Parse document based on object key extension
    let doc = resolve_doc_from_import_data(&data, &key)?;

    Ok(ImportData {
        import_type: ImportType::S3,
        resolved_location: location.to_string(),
        data,
        doc,
    })
}

/// Parse S3 location string into bucket and key
fn parse_s3_location(location: &str) -> Result<(String, String)> {
    if !location.starts_with("s3://") {
        return Err(anyhow!("Invalid S3 location format: {}", location));
    }

    let path = location.strip_prefix("s3://").unwrap();
    let parts: Vec<&str> = path.splitn(2, '/').collect();

    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(anyhow!("Invalid S3 location format: {}", location));
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Mock S3 client for testing
    struct MockS3Client {
        responses: HashMap<String, Result<String>>,
    }

    impl MockS3Client {
        fn new() -> Self {
            Self {
                responses: HashMap::new(),
            }
        }

        fn expect_get_object(mut self, bucket: &str, key: &str, response: Result<String>) -> Self {
            let lookup_key = format!("{}/{}", bucket, key);
            self.responses.insert(lookup_key, response);
            self
        }
    }

    #[async_trait]
    impl S3Client for MockS3Client {
        async fn get_object(&self, bucket: &str, key: &str) -> Result<String> {
            let lookup_key = format!("{}/{}", bucket, key);
            match self.responses.get(&lookup_key) {
                Some(Ok(content)) => Ok(content.clone()),
                Some(Err(e)) => Err(anyhow!("{}", e)),
                None => Err(anyhow!("Unexpected S3 request: s3://{}/{}", bucket, key)),
            }
        }
    }

    #[test]
    fn test_parse_s3_location_valid() -> Result<()> {
        let (bucket, key) = parse_s3_location("s3://my-bucket/path/to/file.yaml")?;
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "path/to/file.yaml");

        Ok(())
    }

    #[test]
    fn test_parse_s3_location_root_key() -> Result<()> {
        let (bucket, key) = parse_s3_location("s3://my-bucket/file.json")?;
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "file.json");

        Ok(())
    }

    #[test]
    fn test_parse_s3_location_invalid_format() {
        assert!(parse_s3_location("http://example.com").is_err());
        assert!(parse_s3_location("s3://").is_err());
        assert!(parse_s3_location("s3://bucket").is_err());
        assert!(parse_s3_location("s3://bucket/").is_err());
        assert!(parse_s3_location("s3:///key").is_err());
    }

    #[tokio::test]
    async fn test_load_s3_import_yaml() -> Result<()> {
        let yaml_content = "test: value\nother: data";
        let client = MockS3Client::new().expect_get_object(
            "my-bucket",
            "config.yaml",
            Ok(yaml_content.to_string()),
        );

        let result = load_s3_import_with_client("s3://my-bucket/config.yaml", &client).await?;

        assert_eq!(result.import_type, ImportType::S3);
        assert_eq!(result.resolved_location, "s3://my-bucket/config.yaml");
        assert_eq!(result.data, yaml_content);

        // Should parse as YAML
        if let serde_yaml::Value::Mapping(map) = result.doc {
            assert!(map.contains_key(&serde_yaml::Value::String("test".to_string())));
            assert!(map.contains_key(&serde_yaml::Value::String("other".to_string())));
        } else {
            panic!("Expected parsed YAML object");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_load_s3_import_json() -> Result<()> {
        let json_content = r#"{"key": "value", "number": 42}"#;
        let client = MockS3Client::new().expect_get_object(
            "my-bucket",
            "config.json",
            Ok(json_content.to_string()),
        );

        let result = load_s3_import_with_client("s3://my-bucket/config.json", &client).await?;

        assert_eq!(result.import_type, ImportType::S3);
        assert_eq!(result.data, json_content);

        // Should parse as JSON
        if let serde_yaml::Value::Mapping(map) = result.doc {
            assert_eq!(
                map.get(&serde_yaml::Value::String("key".to_string())),
                Some(&serde_yaml::Value::String("value".to_string()))
            );
            assert_eq!(
                map.get(&serde_yaml::Value::String("number".to_string())),
                Some(&serde_yaml::Value::Number(serde_yaml::Number::from(42)))
            );
        } else {
            panic!("Expected parsed JSON object");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_load_s3_import_text() -> Result<()> {
        let text_content = "Just plain text content";
        let client = MockS3Client::new().expect_get_object(
            "my-bucket",
            "file.txt",
            Ok(text_content.to_string()),
        );

        let result = load_s3_import_with_client("s3://my-bucket/file.txt", &client).await?;

        assert_eq!(result.import_type, ImportType::S3);
        assert_eq!(result.data, text_content);
        assert_eq!(
            result.doc,
            serde_yaml::Value::String(text_content.to_string())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_load_s3_import_error() {
        let client = MockS3Client::new().expect_get_object(
            "my-bucket",
            "nonexistent.yaml",
            Err(anyhow!("NoSuchKey")),
        );

        let result = load_s3_import_with_client("s3://my-bucket/nonexistent.yaml", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NoSuchKey"));
    }
}
