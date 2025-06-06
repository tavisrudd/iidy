//! Import system for YAML preprocessing
//! 
//! This module implements the `$imports` functionality that allows importing
//! data from various sources including files, environment variables, AWS services,
//! and more.

pub mod loaders;


use std::collections::HashMap;
use anyhow::{Result, anyhow};
use serde_yaml::Value;
use async_trait::async_trait;
use sha2::{Sha256, Digest};


/// Types of imports supported by the system
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportType {
    File,
    Env,
    Git,
    Random,
    Filehash,
    FilehashBase64,
    Cfn,
    Ssm,
    SsmPath,
    S3,
    Http,
}

impl ImportType {
    /// Parse import type from location string
    pub fn from_location(location: &str, base_location: &str) -> Result<Self> {
        parse_import_type(location, base_location)
    }

    /// Check if this import type requires network access
    pub fn requires_network(&self) -> bool {
        matches!(self, ImportType::Http | ImportType::S3 | ImportType::Cfn | ImportType::Ssm | ImportType::SsmPath)
    }

    /// Check if this import type is local-only (security restriction)
    pub fn is_local_only(&self) -> bool {
        matches!(self, ImportType::File | ImportType::Env)
    }
}

/// Data returned from an import operation
#[derive(Debug, Clone)]
pub struct ImportData {
    pub import_type: ImportType,
    pub resolved_location: String,
    pub data: String,
    pub doc: Value,
}

/// Record of an import for metadata tracking
#[derive(Debug, Clone)]
pub struct ImportRecord {
    pub key: Option<String>,
    pub from: String,
    pub imported: String,
    pub sha256_digest: String,
}

/// Environment values that can be used in handlebars templates and includes
pub type EnvValues = HashMap<String, Value>;

/// Trait for loading imports from different sources
#[async_trait]
pub trait ImportLoader: Send + Sync {
    async fn load(&self, location: &str, base_location: &str) -> Result<ImportData>;
}

/// Parse the import type from a location string
/// 
/// Handles security restrictions where local-only imports cannot be used
/// from remote templates (S3, HTTP).
pub fn parse_import_type(location: &str, base_location: &str) -> Result<ImportType> {
    // Parse explicit type from location (e.g., "s3:", "env:", etc.)
    let has_explicit_type = location.contains(':');
    let import_type_str = if has_explicit_type {
        location.to_lowercase()
            .split(':')
            .next()
            .unwrap()
            .replace("https", "http") // Normalize https to http
    } else {
        "file".to_string()
    };

    let import_type = match import_type_str.as_str() {
        "file" => ImportType::File,
        "env" => ImportType::Env,
        "git" => ImportType::Git,
        "random" => ImportType::Random,
        "filehash" => ImportType::Filehash,
        "filehash-base64" => ImportType::FilehashBase64,
        "cfn" => ImportType::Cfn,
        "ssm" => ImportType::Ssm,
        "ssm-path" => ImportType::SsmPath,
        "s3" => ImportType::S3,
        "http" => ImportType::Http,
        _ => return Err(anyhow!("Unknown import type '{}' in {}", location, base_location)),
    };

    // Parse base location type for security validation
    let base_import_type = if base_location.contains(':') {
        let base_location_lower = base_location.to_lowercase();
        let base_type_str = base_location_lower
            .split(':')
            .next()
            .unwrap();
        match base_type_str {
            "s3" => Some(ImportType::S3),
            "http" | "https" => Some(ImportType::Http),
            _ => None,
        }
    } else {
        None
    };

    // Security check: local-only imports cannot be used from remote templates
    if let Some(base_type) = base_import_type {
        if base_type.requires_network() {
            if !has_explicit_type {
                // For implicit imports (no prefix), inherit the base type
                // unless the location looks explicitly local (starts with ./ or /)
                if location.starts_with("./") || location.starts_with("../") || location.starts_with("/") {
                    return Err(anyhow!(
                        "Import type '{}' in '{}' not allowed from remote template",
                        location, base_location
                    ));
                }
                // Inherit the base type for relative paths without explicit local indicators
                return Ok(base_type);
            } else if import_type.is_local_only() {
                return Err(anyhow!(
                    "Import type '{}' in '{}' not allowed from remote template",
                    location, base_location
                ));
            }
        }
    }

    Ok(import_type)
}

/// Calculate SHA256 digest of content for import tracking
pub fn sha256_digest(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Main function to load imports recursively
/// 
/// This processes the `$imports` and `$defs` sections of a YAML document,
/// loading all imported data and making it available in the environment.
/// 
/// TODO: This function is being replaced by the two-phase processing approach
/// in the main yaml/mod.rs. For now, returning a placeholder to avoid compilation errors.
pub async fn load_imports(
    _doc: &mut Value,
    _base_location: &str,
    _imports_accum: &mut Vec<ImportRecord>,
    _loader: &dyn ImportLoader,
) -> Result<()> {
    // TODO: This function is being replaced by the two-phase processing approach
    // in yaml/mod.rs. Returning Ok for now to avoid compilation errors.
    Ok(())
}

