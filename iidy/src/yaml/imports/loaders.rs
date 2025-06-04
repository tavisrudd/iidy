//! Import loader implementations for different import types
//! 
//! This module contains the concrete implementations of import loaders
//! for file, environment, git, random, and other import types.

use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use serde_json::Value;
use async_trait::async_trait;
use tokio::fs;
use url::Url;

use super::{ImportType, ImportData, ImportLoader, parse_import_type, sha256_digest};

/// Production import loader that implements all import types
pub struct ProductionImportLoader {
    pub http_client: reqwest::Client,
    pub aws_config: Option<aws_config::SdkConfig>,
}

impl ProductionImportLoader {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
            aws_config: None,
        }
    }

    pub fn with_aws_config(mut self, config: aws_config::SdkConfig) -> Self {
        self.aws_config = Some(config);
        self
    }
}

#[async_trait]
impl ImportLoader for ProductionImportLoader {
    async fn load(&self, location: &str, base_location: &str) -> Result<ImportData> {
        let import_type = parse_import_type(location, base_location)?;
        
        match import_type {
            ImportType::File => load_file_import(location, base_location).await,
            ImportType::Env => load_env_import(location, base_location).await,
            ImportType::Git => load_git_import(location, base_location).await,
            ImportType::Random => load_random_import(location, base_location).await,
            ImportType::Filehash => load_filehash_import(location, base_location, false).await,
            ImportType::FilehashBase64 => load_filehash_import(location, base_location, true).await,
            ImportType::Http => load_http_import(location, base_location, &self.http_client).await,
            ImportType::S3 => {
                let config = self.aws_config.as_ref()
                    .ok_or_else(|| anyhow!("AWS config not available for S3 import"))?;
                load_s3_import(location, base_location, config).await
            },
            ImportType::Cfn => {
                let config = self.aws_config.as_ref()
                    .ok_or_else(|| anyhow!("AWS config not available for CloudFormation import"))?;
                load_cfn_import(location, base_location, config).await
            },
            ImportType::Ssm => {
                let config = self.aws_config.as_ref()
                    .ok_or_else(|| anyhow!("AWS config not available for SSM import"))?;
                load_ssm_import(location, base_location, config).await
            },
            ImportType::SsmPath => {
                let config = self.aws_config.as_ref()
                    .ok_or_else(|| anyhow!("AWS config not available for SSM path import"))?;
                load_ssm_path_import(location, base_location, config).await
            },
        }
    }
}

/// Load a file import from the local filesystem
pub async fn load_file_import(location: &str, base_location: &str) -> Result<ImportData> {
    // Remove file: prefix if present
    let clean_location = location.strip_prefix("file:").unwrap_or(location);
    
    // Resolve path relative to base location
    let base_path = if base_location.starts_with("file:") {
        PathBuf::from(base_location.strip_prefix("file:").unwrap_or(base_location))
    } else {
        PathBuf::from(base_location)
    };
    
    let resolved_path = if Path::new(clean_location).is_absolute() {
        PathBuf::from(clean_location)
    } else {
        let base_dir = base_path.parent().unwrap_or(Path::new("."));
        base_dir.join(clean_location)
    };

    let resolved_location = resolved_path.to_string_lossy().to_string();
    
    // Read the file content
    let data = fs::read_to_string(&resolved_path).await
        .map_err(|e| anyhow!(
            "\"{}\" has a bad import \"$imports: ... {}\". {}",
            base_location, location, e
        ))?;

    // Parse the document based on file extension
    let doc = resolve_doc_from_import_data(&data, &resolved_location)?;

    Ok(ImportData {
        import_type: ImportType::File,
        resolved_location,
        data,
        doc,
    })
}

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

/// Load a git import (branch, describe, sha)
pub async fn load_git_import(location: &str, _base_location: &str) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0] != "git" {
        return Err(anyhow!("Invalid git import format: {}", location));
    }

    let git_command = parts[1];
    let data = match git_command {
        "branch" => execute_git_command("git rev-parse --abbrev-ref HEAD").await?,
        "describe" => execute_git_command("git describe --dirty --tags").await?,
        "sha" => execute_git_command("git rev-parse HEAD").await?,
        _ => return Err(anyhow!("Invalid git command: {}", location)),
    };

    Ok(ImportData {
        import_type: ImportType::Git,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    })
}

/// Load a random import (dashed-name, name, int)
pub async fn load_random_import(location: &str, base_location: &str) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(2, ':').collect();
    if parts.len() != 2 || parts[0] != "random" {
        return Err(anyhow!("Invalid random import format: {}", location));
    }

    let random_type = parts[1];
    let data = match random_type {
        "dashed-name" => generate_dashed_name(),
        "name" => generate_name(),
        "int" => generate_random_int(),
        _ => return Err(anyhow!("Invalid random type in {} at {}", location, base_location)),
    };

    Ok(ImportData {
        import_type: ImportType::Random,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    })
}

/// Load a filehash import (SHA256 of file content)
pub async fn load_filehash_import(location: &str, base_location: &str, base64: bool) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid filehash import format: {}", location));
    }

    let mut file_path = parts[1];
    let allow_missing = file_path.starts_with('?');
    if allow_missing {
        file_path = file_path.strip_prefix('?').unwrap().trim();
    }

    // Resolve path relative to base location
    let base_path = PathBuf::from(base_location);
    let resolved_path = if Path::new(file_path).is_absolute() {
        PathBuf::from(file_path)
    } else {
        let base_dir = base_path.parent().unwrap_or(Path::new("."));
        base_dir.join(file_path)
    };

    let resolved_location = resolved_path.to_string_lossy().to_string();

    if !resolved_path.exists() {
        if allow_missing {
            let data = "FILE_MISSING".to_string();
            return Ok(ImportData {
                import_type: if base64 { ImportType::FilehashBase64 } else { ImportType::Filehash },
                resolved_location,
                data: data.clone(),
                doc: Value::String(data),
            });
        } else {
            return Err(anyhow!("Invalid location {} for filehash in {}", resolved_location, base_location));
        }
    }

    let file_content = fs::read(&resolved_path).await
        .map_err(|e| anyhow!("Failed to read file {}: {}", resolved_location, e))?;

    let hash = sha256_digest(&String::from_utf8_lossy(&file_content));
    let data = if base64 {
        use base64::{Engine as _, engine::general_purpose};
        general_purpose::STANDARD.encode(hex::decode(&hash).unwrap())
    } else {
        hash
    };

    Ok(ImportData {
        import_type: if base64 { ImportType::FilehashBase64 } else { ImportType::Filehash },
        resolved_location,
        data: data.clone(),
        doc: Value::String(data),
    })
}

/// Load an HTTP import
pub async fn load_http_import(location: &str, _base_location: &str, client: &reqwest::Client) -> Result<ImportData> {
    let data = client.get(location).send().await?
        .text().await?;
    
    let doc = resolve_doc_from_import_data(&data, location)?;

    Ok(ImportData {
        import_type: ImportType::Http,
        resolved_location: location.to_string(),
        data,
        doc,
    })
}

/// Load an S3 import
pub async fn load_s3_import(location: &str, base_location: &str, aws_config: &aws_config::SdkConfig) -> Result<ImportData> {
    let resolved_location = if location.starts_with("s3:") {
        location.to_string()
    } else {
        // Resolve relative to base location
        let base_path = base_location.strip_prefix("s3:/").unwrap_or(base_location);
        let base_path_buf = PathBuf::from(base_path);
        let base_dir = base_path_buf.parent().unwrap_or(Path::new(""));
        format!("s3:/{}", base_dir.join(location).to_string_lossy())
    };

    let uri = Url::parse(&resolved_location.replace("s3:/", "s3://"))?;
    let bucket = uri.host_str().ok_or_else(|| anyhow!("Invalid S3 URI: {}", resolved_location))?;
    let key = uri.path().strip_prefix('/').unwrap_or(uri.path());

    let s3_client = aws_sdk_s3::Client::new(aws_config);
    let response = s3_client.get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let data = response.body.collect().await?.into_bytes();
    let data_str = String::from_utf8(data.to_vec())?;
    let doc = resolve_doc_from_import_data(&data_str, &resolved_location)?;

    Ok(ImportData {
        import_type: ImportType::S3,
        resolved_location,
        data: data_str,
        doc,
    })
}

/// Load a CloudFormation import (stack outputs, exports, etc.)
pub async fn load_cfn_import(location: &str, _base_location: &str, aws_config: &aws_config::SdkConfig) -> Result<ImportData> {
    // Parse cfn:field:stack/key format
    let parts: Vec<&str> = location.splitn(3, ':').collect();
    if parts.len() < 2 || parts[0] != "cfn" {
        return Err(anyhow!("Invalid CloudFormation import format: {}", location));
    }

    let field = parts[1];
    let remaining = parts.get(2).map_or("", |s| *s);
    
    let cfn_client = aws_sdk_cloudformation::Client::new(aws_config);

    match field {
        "export" => {
            let export_name = remaining;
            // TODO: Implement export listing and lookup
            Ok(ImportData {
                import_type: ImportType::Cfn,
                resolved_location: location.to_string(),
                data: "TODO: CFN export".to_string(),
                doc: Value::String("TODO: CFN export".to_string()),
            })
        },
        "output" | "parameter" | "tag" | "resource" | "stack" => {
            let stack_parts: Vec<&str> = remaining.splitn(2, '/').collect();
            let stack_name = stack_parts[0];
            let field_key = stack_parts.get(1);
            
            // TODO: Implement stack describe and field extraction
            Ok(ImportData {
                import_type: ImportType::Cfn,
                resolved_location: location.to_string(),
                data: format!("TODO: CFN {} from {}", field, stack_name),
                doc: Value::String(format!("TODO: CFN {} from {}", field, stack_name)),
            })
        },
        _ => Err(anyhow!("Invalid CloudFormation field: {}", field)),
    }
}

/// Load an SSM import (single parameter)
pub async fn load_ssm_import(location: &str, base_location: &str, aws_config: &aws_config::SdkConfig) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(3, ':').collect();
    if parts.len() < 2 || parts[0] != "ssm" {
        return Err(anyhow!("Invalid SSM import format: {}", location));
    }

    let parameter_name = parts[1];
    let format = parts.get(2);

    let ssm_client = aws_sdk_ssm::Client::new(aws_config);
    let response = ssm_client.get_parameter()
        .name(parameter_name)
        .with_decryption(true)
        .send()
        .await?;

    let parameter_value = response.parameter()
        .and_then(|p| p.value())
        .ok_or_else(|| anyhow!("Invalid SSM parameter {} import at {}", parameter_name, base_location))?;

    let doc = parse_data_from_param_store(parameter_value, format.copied())?;

    Ok(ImportData {
        import_type: ImportType::Ssm,
        resolved_location: location.to_string(),
        data: parameter_value.to_string(),
        doc,
    })
}

/// Load an SSM path import (parameter hierarchy)
pub async fn load_ssm_path_import(location: &str, _base_location: &str, aws_config: &aws_config::SdkConfig) -> Result<ImportData> {
    let parts: Vec<&str> = location.splitn(3, ':').collect();
    if parts.len() < 2 || parts[0] != "ssm-path" {
        return Err(anyhow!("Invalid SSM path import format: {}", location));
    }

    let mut path = parts[1].to_string();
    if !path.ends_with('/') {
        path.push('/');
    }
    let _format = parts.get(2);

    // TODO: Implement parameter path listing and parsing
    let _ssm_client = aws_sdk_ssm::Client::new(aws_config);
    
    Ok(ImportData {
        import_type: ImportType::SsmPath,
        resolved_location: location.to_string(),
        data: "TODO: SSM path".to_string(),
        doc: Value::String("TODO: SSM path".to_string()),
    })
}

// Helper functions

/// Resolve document from import data based on file extension or content type
fn resolve_doc_from_import_data(data: &str, location: &str) -> Result<Value> {
    let path = Path::new(location);
    let extension = path.extension().and_then(|s| s.to_str()).unwrap_or("");

    match extension {
        "yaml" | "yml" => {
            serde_yaml::from_str(data)
                .map_err(|e| anyhow!("Failed to parse YAML from {}: {}", location, e))
        },
        "json" => {
            serde_json::from_str(data)
                .map_err(|e| anyhow!("Failed to parse JSON from {}: {}", location, e))
        },
        _ => Ok(Value::String(data.to_string())),
    }
}

/// Parse data from Parameter Store with optional format specification
fn parse_data_from_param_store(payload: &str, format: Option<&str>) -> Result<Value> {
    match format {
        Some("json") => serde_json::from_str(payload)
            .map_err(|e| anyhow!("Invalid JSON in SSM parameter: {}", e)),
        Some("yaml") => serde_yaml::from_str(payload)
            .map_err(|e| anyhow!("Invalid YAML in SSM parameter: {}", e)),
        _ => Ok(Value::String(payload.to_string())),
    }
}

/// Execute a git command and return the output
async fn execute_git_command(command: &str) -> Result<String> {
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(command)
        .output()
        .await?;

    if !output.status.success() {
        return Err(anyhow!("Git command failed: {}", command));
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Generate a dashed name for random imports
fn generate_dashed_name() -> String {
    use rand::Rng;
    let adjectives = ["red", "blue", "green", "happy", "clever", "brave", "swift", "mighty"];
    let nouns = ["cat", "dog", "bird", "fish", "lion", "eagle", "shark", "tiger"];
    
    let mut rng = rand::thread_rng();
    let adj = adjectives[rng.gen_range(0..adjectives.len())];
    let noun = nouns[rng.gen_range(0..nouns.len())];
    
    format!("{}-{}", adj, noun)
}

/// Generate a name (no dashes) for random imports
fn generate_name() -> String {
    generate_dashed_name().replace('-', "")
}

/// Generate a random integer for random imports
fn generate_random_int() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    rng.gen_range(1..1000).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::env;

    #[tokio::test]
    async fn test_file_import_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.yaml");
        fs::write(&file_path, "key: value\nnum: 42").await.unwrap();

        let result = load_file_import(
            &file_path.to_string_lossy(),
            temp_dir.path().to_string_lossy().as_ref()
        ).await.unwrap();

        assert_eq!(result.import_type, ImportType::File);
        assert_eq!(result.doc["key"], "value");
        assert_eq!(result.doc["num"], 42);
    }

    #[tokio::test]
    async fn test_file_import_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");
        fs::write(&file_path, r#"{"key": "value", "num": 42}"#).await.unwrap();

        let result = load_file_import(
            &file_path.to_string_lossy(),
            temp_dir.path().to_string_lossy().as_ref()
        ).await.unwrap();

        assert_eq!(result.import_type, ImportType::File);
        assert_eq!(result.doc["key"], "value");
        assert_eq!(result.doc["num"], 42);
    }

    #[tokio::test]
    async fn test_file_import_relative_path() {
        let temp_dir = TempDir::new().unwrap();
        let subdir = temp_dir.path().join("subdir");
        fs::create_dir(&subdir).await.unwrap();
        
        let base_file = subdir.join("base.yaml");
        let data_file = temp_dir.path().join("data.yaml");
        
        fs::write(&data_file, "imported: true").await.unwrap();

        let result = load_file_import(
            "../data.yaml",
            &base_file.to_string_lossy()
        ).await.unwrap();

        assert_eq!(result.import_type, ImportType::File);
        assert_eq!(result.doc["imported"], true);
    }

    #[tokio::test]
    async fn test_env_import_with_value() {
        unsafe {
            env::set_var("TEST_VAR", "test_value");
        }
        
        let result = load_env_import("env:TEST_VAR", "test_base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Env);
        assert_eq!(result.doc, Value::String("test_value".to_string()));
        
        unsafe {
            env::remove_var("TEST_VAR");
        }
    }

    #[tokio::test]
    async fn test_env_import_with_default() {
        let result = load_env_import("env:NONEXISTENT_VAR:default_value", "test_base").await.unwrap();
        
        assert_eq!(result.import_type, ImportType::Env);
        assert_eq!(result.doc, Value::String("default_value".to_string()));
    }

    #[tokio::test]
    async fn test_env_import_missing_no_default() {
        let result = load_env_import("env:NONEXISTENT_VAR", "test_base").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_random_imports() {
        let dashed = load_random_import("random:dashed-name", "test_base").await.unwrap();
        assert!(dashed.data.contains('-'));
        
        let name = load_random_import("random:name", "test_base").await.unwrap();
        assert!(!name.data.contains('-'));
        
        let int = load_random_import("random:int", "test_base").await.unwrap();
        assert!(int.data.parse::<i32>().is_ok());
    }

    #[tokio::test]
    async fn test_filehash_import() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.txt");
        fs::write(&file_path, "hello world").await.unwrap();

        let result = load_filehash_import(
            &format!("filehash:{}", file_path.to_string_lossy()),
            temp_dir.path().to_string_lossy().as_ref(),
            false
        ).await.unwrap();

        assert_eq!(result.import_type, ImportType::Filehash);
        // Should be SHA256 of "hello world"
        assert_eq!(result.data, "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9");
    }

    #[tokio::test]
    async fn test_filehash_missing_file_allowed() {
        let result = load_filehash_import(
            "filehash:?/nonexistent/file.txt",
            "/base/path",
            false
        ).await.unwrap();

        assert_eq!(result.data, "FILE_MISSING");
    }
}