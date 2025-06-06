//! HTTP/HTTPS import loader
//! 
//! Provides functionality for fetching content from HTTP/HTTPS URLs

use anyhow::Result;

use crate::yaml::imports::{ImportData, ImportType};
use super::utils::resolve_doc_from_import_data;

/// Load an HTTP import
pub async fn load_http_import(location: &str, _base_location: &str, client: &reqwest::Client) -> Result<ImportData> {
    let response = client.get(location).send().await?;
    let data = response.error_for_status()?.text().await?;
    
    let doc = resolve_doc_from_import_data(&data, location)?;

    Ok(ImportData {
        import_type: ImportType::Http,
        resolved_location: location.to_string(),
        data,
        doc,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito;
    use serde_yaml::Value;

    #[tokio::test]
    async fn test_load_http_import_success() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/test.yaml")
            .with_status(200)
            .with_header("content-type", "application/yaml")
            .with_body("test: value\nother: data")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/test.yaml", &server.url());
        let result = load_http_import(&url, "/base", &client).await?;

        mock.assert_async().await;

        assert_eq!(result.import_type, ImportType::Http);
        assert_eq!(result.resolved_location, url);
        assert!(result.data.contains("test: value"));
        assert!(result.data.contains("other: data"));
        
        // Should parse as YAML
        if let Value::Mapping(map) = result.doc {
            assert!(map.contains_key(&Value::String("test".to_string())));
            assert!(map.contains_key(&Value::String("other".to_string())));
        } else {
            panic!("Expected parsed YAML object");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_load_http_import_json() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/test.json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"key": "value", "number": 42}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/test.json", &server.url());
        let result = load_http_import(&url, "/base", &client).await?;

        mock.assert_async().await;

        assert_eq!(result.import_type, ImportType::Http);
        assert!(result.data.contains("\"key\": \"value\""));
        
        // Should parse as JSON (converted to YAML Value)
        if let Value::Mapping(map) = result.doc {
            assert_eq!(map.get(&Value::String("key".to_string())), Some(&Value::String("value".to_string())));
            assert_eq!(map.get(&Value::String("number".to_string())), Some(&Value::Number(serde_yaml::Number::from(42))));
        } else {
            panic!("Expected parsed JSON object");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_load_http_import_plain_text() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/test.txt")
            .with_status(200)
            .with_header("content-type", "text/plain")
            .with_body("Just plain text content")
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/test.txt", &server.url());
        let result = load_http_import(&url, "/base", &client).await?;

        mock.assert_async().await;

        assert_eq!(result.import_type, ImportType::Http);
        assert_eq!(result.data, "Just plain text content");
        
        // Should parse as string
        assert_eq!(result.doc, Value::String("Just plain text content".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_http_import_404() {
        let mut server = mockito::Server::new_async().await;
        let mock = server.mock("GET", "/nonexistent")
            .with_status(404)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/nonexistent", &server.url());
        let result = load_http_import(&url, "/base", &client).await;

        mock.assert_async().await;

        assert!(result.is_err());
        // reqwest should return an error for 404 status
    }

    #[tokio::test]
    async fn test_load_http_import_invalid_url() {
        let client = reqwest::Client::new();
        let result = load_http_import("not-a-valid-url", "/base", &client).await;

        assert!(result.is_err());
        // Should fail due to invalid URL
    }

    #[tokio::test]
    async fn test_load_http_import_connection_error() {
        let client = reqwest::Client::new();
        // Use a URL that should fail to connect
        let result = load_http_import("http://localhost:9999/nonexistent", "/base", &client).await;

        assert!(result.is_err());
        // Should fail due to connection error
    }

    #[tokio::test]
    async fn test_load_http_import_large_response() -> Result<()> {
        let mut server = mockito::Server::new_async().await;
        let large_content = "x".repeat(10000); // 10KB of 'x'
        
        let mock = server.mock("GET", "/large")
            .with_status(200)
            .with_body(&large_content)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let url = format!("{}/large", &server.url());
        let result = load_http_import(&url, "/base", &client).await?;

        mock.assert_async().await;

        assert_eq!(result.import_type, ImportType::Http);
        assert_eq!(result.data.len(), 10000);
        assert_eq!(result.data, large_content);

        Ok(())
    }
}