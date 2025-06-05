//! Import system for YAML preprocessing
//! 
//! This module implements the `$imports` functionality that allows importing
//! data from various sources including files, environment variables, AWS services,
//! and more.

pub mod loaders;


use std::collections::HashMap;
use anyhow::{Result, anyhow};
use serde_json::Value;
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
pub async fn load_imports(
    doc: &mut Value,
    base_location: &str,
    imports_accum: &mut Vec<ImportRecord>,
    loader: &dyn ImportLoader,
) -> Result<()> {
    // Ensure $envValues exists
    if !doc.is_object() {
        return Ok(());
    }

    let doc_map = doc.as_object_mut().unwrap();
    
    // Initialize $envValues if not present
    if !doc_map.contains_key("$envValues") {
        doc_map.insert("$envValues".to_string(), Value::Object(serde_json::Map::new()));
    }

    // Process $defs into $envValues
    if let Some(defs) = doc_map.get("$defs").cloned() {
        if let Some(defs_obj) = defs.as_object() {
            let env_values = doc_map.get_mut("$envValues").unwrap().as_object_mut().unwrap();
            
            for (key, value) in defs_obj {
                if env_values.contains_key(key) {
                    return Err(anyhow!(
                        "\"{}\" in $defs collides with the same name in $imports of {}",
                        key, base_location
                    ));
                }
                env_values.insert(key.clone(), value.clone());
            }
        }
    }

    // Process $imports
    if let Some(imports) = doc_map.get("$imports").cloned() {
        if !imports.is_object() {
            return Err(anyhow!(
                "Invalid imports in {}: \"{}\". Should be mapping.",
                base_location, imports
            ));
        }

        let imports_obj = imports.as_object().unwrap();
        let env_values = doc_map.get_mut("$envValues").unwrap().as_object_mut().unwrap();

        for (as_key, location_value) in imports_obj {
            let location = location_value.as_str()
                .ok_or_else(|| anyhow!(
                    "\"{}\" has a bad import \"$imports: ... {}\". Import values must be strings but {}=\"{}\"",
                    base_location, as_key, as_key, location_value
                ))?;

            // Interpolate handlebars in location if it contains {{...}}
            let mut resolved_location = location.to_string();
            if location.contains("{{") {
                let env_values_map: std::collections::HashMap<String, Value> = env_values.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect();
                resolved_location = crate::yaml::handlebars::interpolate_handlebars_string(
                    location, 
                    &env_values_map, 
                    &format!("{}: {}", base_location, as_key)
                )?;
            }

            // Load the import
            let import_data = loader.load(&resolved_location, base_location).await?;

            // Add $location to imported document if it's an object
            let mut doc_with_location = import_data.doc.clone();
            if let Some(obj) = doc_with_location.as_object_mut() {
                obj.insert("$location".to_string(), Value::String(resolved_location.to_string()));
            }

            // Record this import
            let resolved_location_for_record = import_data.resolved_location.clone();
            imports_accum.push(ImportRecord {
                from: base_location.to_string(),
                imported: resolved_location_for_record,
                sha256_digest: sha256_digest(&import_data.data),
                key: Some(as_key.clone()),
            });

            // Check for name collision
            if env_values.contains_key(as_key) {
                return Err(anyhow!(
                    "\"{}\" in $imports collides with the same name in $defs of {}",
                    as_key, base_location
                ));
            }

            // Recursively process nested imports if the imported document has them
            if doc_with_location.get("$imports").is_some() || doc_with_location.get("$defs").is_some() {
                let mut nested_doc = doc_with_location.clone();
                Box::pin(load_imports(&mut nested_doc, &import_data.resolved_location, imports_accum, loader)).await?;
                // Update with the processed version
                env_values.insert(as_key.clone(), nested_doc);
            } else {
                // Add to environment as-is if no nested processing needed
                env_values.insert(as_key.clone(), doc_with_location);
            }
        }
    }

    // Validate $params don't collide with imports/defs
    if let Some(params) = doc_map.get("$params") {
        if let Some(params_array) = params.as_array() {
            let env_values = doc_map.get("$envValues").unwrap().as_object().unwrap();
            
            for param in params_array {
                if let Some(param_obj) = param.as_object() {
                    if let Some(name) = param_obj.get("Name").and_then(|n| n.as_str()) {
                        if env_values.contains_key(name) {
                            return Err(anyhow!(
                                "\"{}\" in $params collides with \"{}\" in $imports or $defs of {}",
                                name, name, base_location
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

