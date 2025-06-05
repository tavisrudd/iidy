//! File system import loaders
//! 
//! Provides functionality for loading files and computing file hashes

use std::path::{Path, PathBuf};
use anyhow::{Result, anyhow};
use tokio::fs;
use serde_json::Value;
use sha2::{Sha256, Digest};

use crate::yaml::imports::{ImportData, ImportType};
use super::utils::resolve_doc_from_import_data;

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

/// Compute SHA256 hash of a string
fn sha256_digest(data: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data.as_bytes());
    hex::encode(hasher.finalize())
}