//! File system import loaders
//!
//! Provides functionality for loading files and computing file hashes

use anyhow::{Result, anyhow};
use serde_yaml::Value;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::fs;

use super::utils::resolve_doc_from_import_data;
use crate::yaml::imports::{ImportData, ImportType};

/// Resolve file path relative to base location
fn resolve_file_path(location: &str, base_location: &str) -> PathBuf {
    // Remove file: prefix if present
    let clean_location = location.strip_prefix("file:").unwrap_or(location);

    // Resolve path relative to base location
    let base_path = if base_location.starts_with("file:") {
        PathBuf::from(base_location.strip_prefix("file:").unwrap_or(base_location))
    } else {
        PathBuf::from(base_location)
    };

    if Path::new(clean_location).is_absolute() {
        PathBuf::from(clean_location)
    } else {
        let base_dir = base_path.parent().unwrap_or(Path::new("."));
        base_dir.join(clean_location)
    }
}

/// Create ImportData from file content
fn create_file_import_data(resolved_location: String, data: String, doc: Value) -> ImportData {
    ImportData {
        import_type: ImportType::File,
        resolved_location,
        data,
        doc,
    }
}

/// Load a file import from the local filesystem
pub async fn load_file_import(location: &str, base_location: &str) -> Result<ImportData> {
    let resolved_path = resolve_file_path(location, base_location);
    let resolved_location = resolved_path.to_string_lossy().to_string();

    // Read the file content
    let data = fs::read_to_string(&resolved_path).await.map_err(|e| {
        anyhow!(
            "\"{}\" has a bad import \"$imports: ... {}\". {}",
            base_location,
            location,
            e
        )
    })?;

    // Parse the document based on file extension
    let doc = resolve_doc_from_import_data(&data, &resolved_location)?;

    Ok(create_file_import_data(resolved_location, data, doc))
}

/// Parse filehash location and extract file path and options
fn parse_filehash_location(location: &str) -> Result<(&str, bool)> {
    let parts: Vec<&str> = location.splitn(2, ':').collect();
    if parts.len() != 2 {
        return Err(anyhow!("Invalid filehash import format: {}", location));
    }

    let mut file_path = parts[1];
    let allow_missing = file_path.starts_with('?');
    if allow_missing {
        file_path = file_path.strip_prefix('?').unwrap().trim();
    }

    Ok((file_path, allow_missing))
}

/// Create ImportData for filehash
fn create_filehash_import_data(
    _location: &str,
    resolved_location: String,
    data: String,
    base64: bool,
) -> ImportData {
    ImportData {
        import_type: if base64 {
            ImportType::FilehashBase64
        } else {
            ImportType::Filehash
        },
        resolved_location,
        data: data.clone(),
        doc: Value::String(data),
    }
}

/// Load a filehash import (SHA256 of file content)
pub async fn load_filehash_import(
    location: &str,
    base_location: &str,
    base64: bool,
) -> Result<ImportData> {
    let (file_path, allow_missing) = parse_filehash_location(location)?;

    // Use base_location directly for path resolution since filehash uses different logic
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
            return Ok(create_filehash_import_data(
                location,
                resolved_location,
                data,
                base64,
            ));
        } else {
            return Err(anyhow!(
                "Invalid location {} for filehash in {}",
                resolved_location,
                base_location
            ));
        }
    }

    let file_content = fs::read(&resolved_path)
        .await
        .map_err(|e| anyhow!("Failed to read file {}: {}", resolved_location, e))?;

    let hash = sha256_digest(&String::from_utf8_lossy(&file_content));
    let data = if base64 {
        use base64::{Engine as _, engine::general_purpose};
        general_purpose::STANDARD.encode(hex::decode(&hash).unwrap())
    } else {
        hash
    };

    Ok(create_filehash_import_data(
        location,
        resolved_location,
        data,
        base64,
    ))
}

/// Compute SHA256 hash of a string
fn sha256_digest(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn test_load_file_import_absolute_path() -> Result<()> {
        // Create a temporary file
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "test: value")?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let result = load_file_import(&temp_path, "/some/base").await?;

        assert_eq!(result.import_type, ImportType::File);
        assert_eq!(result.resolved_location, temp_path);
        assert!(result.data.contains("test: value"));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_file_import_relative_path() -> Result<()> {
        // Create a temporary directory and file
        let temp_dir = tempfile::tempdir()?;
        let base_file = temp_dir.path().join("base.yaml");
        let target_file = temp_dir.path().join("target.yaml");

        std::fs::write(&target_file, "imported: data")?;

        let base_location = base_file.to_string_lossy().to_string();
        let result = load_file_import("target.yaml", &base_location).await?;

        assert_eq!(result.import_type, ImportType::File);
        assert!(result.data.contains("imported: data"));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_file_import_with_file_prefix() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "prefixed: value")?;
        let temp_path = temp_file.path().to_string_lossy().to_string();
        let file_url = format!("file:{temp_path}");

        let result = load_file_import(&file_url, "/some/base").await?;

        assert_eq!(result.import_type, ImportType::File);
        assert_eq!(result.resolved_location, temp_path);
        assert!(result.data.contains("prefixed: value"));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_file_import_nonexistent() {
        let result = load_file_import("/nonexistent/file.yaml", "/base").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("bad import"));
    }

    #[tokio::test]
    async fn test_load_filehash_import() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "hash test content")?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let location = format!("filehash:{temp_path}");
        let result = load_filehash_import(&location, "/base", false).await?;

        assert_eq!(result.import_type, ImportType::Filehash);
        assert_eq!(result.data.len(), 64); // SHA256 hex length

        // Verify the hash is consistent
        let expected_hash = sha256_digest("hash test content\n");
        assert_eq!(result.data, expected_hash);

        Ok(())
    }

    #[tokio::test]
    async fn test_load_filehash_import_base64() -> Result<()> {
        let mut temp_file = NamedTempFile::new()?;
        writeln!(temp_file, "base64 test content")?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let location = format!("filehash-base64:{temp_path}");
        let result = load_filehash_import(&location, "/base", true).await?;

        assert_eq!(result.import_type, ImportType::FilehashBase64);
        // Base64 encoded SHA256 should be different length than hex
        assert_ne!(result.data.len(), 64);

        Ok(())
    }

    #[tokio::test]
    async fn test_load_filehash_import_missing_allowed() -> Result<()> {
        let location = "filehash:?/nonexistent/file.txt";
        let result = load_filehash_import(location, "/base", false).await?;

        assert_eq!(result.import_type, ImportType::Filehash);
        assert_eq!(result.data, "FILE_MISSING");

        Ok(())
    }

    #[tokio::test]
    async fn test_load_filehash_import_missing_not_allowed() {
        let location = "filehash:/nonexistent/file.txt";
        let result = load_filehash_import(location, "/base", false).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid location"));
    }

    #[tokio::test]
    async fn test_load_filehash_import_invalid_format() {
        let result = load_filehash_import("invalid-format", "/base", false).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Invalid filehash import format")
        );
    }

    #[test]
    fn test_sha256_digest() {
        let input = "test string";
        let hash = sha256_digest(input);

        // Verify it's a valid hex string of correct length
        assert_eq!(hash.len(), 64);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));

        // Verify consistency
        assert_eq!(hash, sha256_digest(input));
    }
}
