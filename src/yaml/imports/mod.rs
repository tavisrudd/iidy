//! Import system for YAML preprocessing
//!
//! This module implements the `$imports` functionality that allows importing
//! data from various sources including files, environment variables, AWS services,
//! and more.
//!
//! # Security Model
//!
//! The import system implements a comprehensive security model to prevent malicious
//! remote templates from accessing local resources:
//!
//! ## Local vs Remote Templates
//!
//! - **Local templates**: Files loaded from the local filesystem (no URL scheme)
//!   - Can import from any source type
//!   - No security restrictions applied
//!
//! - **Remote templates**: Files loaded from S3, HTTP, or HTTPS URLs
//!   - Subject to security restrictions to prevent local resource access
//!   - Cannot use local-only import types
//!
//! ## Import Type Security Classification
//!
//! ### Local-Only Import Types (Forbidden from Remote Templates)
//!
//! - `file:` - Local filesystem access
//! - `env:` - Local environment variables
//! - `git:` - Local git repository access
//! - `filehash:` - File hashing (typically local files)
//! - `filehash-base64:` - File hashing with base64 encoding
//!
//! ### Remote-Allowed Import Types
//!
//! - `s3:` - S3 objects
//! - `http:`/`https:` - HTTP endpoints
//! - `cfn:` - CloudFormation stacks and exports
//! - `ssm:` - SSM parameters
//! - `ssm-path:` - SSM parameter paths
//! - `random:` - Random value generation
//!
//! ## Relative Import Behavior
//!
//! When a template uses relative imports (no explicit type prefix), the import
//! inherits the type of the parent template:
//!
//! ```yaml
//! # From s3://bucket/configs/app.yaml
//! $imports:
//!   database: "./database.yaml"  # Resolves to s3://bucket/configs/database.yaml
//! ```
//!
//! However, explicit local path indicators are forbidden from remote templates:
//!
//! ```yaml
//! # From s3://bucket/config.yaml - These will be REJECTED:
//! $imports:
//!   bad1: "./local.yaml"      # Error: local path from remote
//!   bad2: "../local.yaml"     # Error: local path from remote  
//!   bad3: "/abs/local.yaml"   # Error: local path from remote
//! ```
//!
//! ## Examples
//!
//! ### Allowed: Remote-to-Remote Imports
//!
//! ```yaml
//! # From https://example.com/configs/app.yaml
//! $imports:
//!   s3data: "s3://bucket/data.yaml"           # S3 import
//!   webdata: "https://api.com/config.json"    # HTTP import
//!   database: "./database.yaml"               # Relative (inherits HTTPS)
//!   cfnstack: "cfn:stack/MyStack/output"      # CloudFormation
//!   secret: "ssm:/app/secret"                 # SSM parameter
//! ```
//!
//! ### Forbidden: Remote-to-Local Imports
//!
//! ```yaml
//! # From s3://bucket/config.yaml - All of these are REJECTED:
//! $imports:
//!   localfile: "file:./local.yaml"      # REJECTED: File access
//!   envvar: "env:HOME"                  # REJECTED: Environment variable
//!   gitinfo: "git:branch"               # REJECTED: Git repository
//!   hash: "filehash:./data.txt"         # REJECTED: Local file hash
//!   localpath: "./local.yaml"           # REJECTED: Local path indicator
//! ```
//!
//! ### Allowed: Local Template Flexibility
//!
//! ```yaml
//! # From local file ./config.yaml - All imports allowed:
//! $imports:
//!   localfile: "file:./other.yaml"      # Local file
//!   envvar: "env:HOME"                  # Environment variable
//!   s3data: "s3://bucket/data.yaml"     # S3 object
//!   webdata: "https://api.com/data"     # HTTP endpoint
//! ```
//!
//! This security model prevents malicious remote templates from:
//! - Reading sensitive local files
//! - Accessing environment variables (potentially containing secrets)
//! - Extracting git repository information
//! - Scanning the local filesystem via file hashing
//!
//! While still allowing legitimate use cases like:
//! - Remote templates composing from other remote sources
//! - Relative imports within the same remote context
//! - Access to AWS services for dynamic configuration

pub mod loaders;

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_yaml::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

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
        matches!(
            self,
            ImportType::Http
                | ImportType::S3
                | ImportType::Cfn
                | ImportType::Ssm
                | ImportType::SsmPath
        )
    }

    /// Check if this import type is local-only (security restriction)
    /// Remote templates cannot use these import types for security reasons
    pub fn is_local_only(&self) -> bool {
        matches!(
            self,
            ImportType::File |           // Local filesystem access
            ImportType::Env |            // Local environment variables
            ImportType::Git |            // Local git repository access
            ImportType::Filehash |       // Typically hashes local files 
            ImportType::FilehashBase64 // Typically hashes local files
        )
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

/// Parse the import type from a location string with security validation
///
/// This function implements the core security model for imports:
///
/// 1. **Local templates** (no URL scheme) can import from any source
/// 2. **Remote templates** (S3, HTTP, HTTPS) cannot use local-only import types:
///    - `file:`, `env:`, `git:`, `filehash:`, `filehash-base64:`
/// 3. **Relative imports** from remote templates inherit the remote type
/// 4. **Local path indicators** (`./`, `../`, `/`) are blocked from remote templates
///
/// # Arguments
/// * `location` - The import location (e.g., "file:data.yaml", "s3://bucket/file.yaml")
/// * `base_location` - The location of the template making the import
///
/// # Returns
/// The validated `ImportType` or an error if security restrictions are violated
///
/// # Security Examples
///
/// - Local template can import anything: `parse_import_type("file:local.yaml", "config.yaml")` -> `Ok(ImportType::File)`
/// - Remote template cannot import local files: `parse_import_type("file:local.yaml", "s3://bucket/config.yaml")` -> `Err("not allowed from remote template")`
/// - Remote template can import other remote sources: `parse_import_type("s3://other/data.yaml", "https://example.com/config.yaml")` -> `Ok(ImportType::S3)`
pub fn parse_import_type(location: &str, base_location: &str) -> Result<ImportType> {
    // Parse explicit type from location (e.g., "s3:", "env:", etc.)
    let has_explicit_type = location.contains(':');
    let import_type_str = if has_explicit_type {
        location
            .to_lowercase()
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
        _ => {
            return Err(anyhow!(
                "Unknown import type '{}' in {}",
                location,
                base_location
            ));
        }
    };

    // Parse base location type for security validation
    let base_import_type = if base_location.contains(':') {
        let base_location_lower = base_location.to_lowercase();
        let base_type_str = base_location_lower.split(':').next().unwrap();
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
                if location.starts_with("./")
                    || location.starts_with("../")
                    || location.starts_with("/")
                {
                    return Err(anyhow!(
                        "Import type '{}' in '{}' not allowed from remote template",
                        location,
                        base_location
                    ));
                }
                // Inherit the base type for relative paths without explicit local indicators
                return Ok(base_type);
            } else if import_type.is_local_only() {
                return Err(anyhow!(
                    "Import type '{}' in '{}' not allowed from remote template",
                    location,
                    base_location
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

// Note: Import loading is handled by the two-phase processing approach
// in yaml/mod.rs through YamlPreprocessor::load_imports_and_defs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_restrictions_from_s3_template() {
        // Test that S3 templates cannot use local-only import types
        let base_location = "s3://bucket/config.yaml";

        // These should be rejected
        let forbidden_imports = vec![
            "file:local.yaml",
            "env:HOME",
            "git:branch",
            "filehash:local.txt",
            "filehash-base64:local.txt",
            "./local.yaml", // Local path indicators
            "../local.yaml",
            "/absolute/local.yaml",
        ];

        for import in forbidden_imports {
            let result = parse_import_type(import, base_location);
            assert!(
                result.is_err(),
                "Expected error for import '{import}' from S3 template"
            );
            let error_msg = result.unwrap_err().to_string();
            assert!(
                error_msg.contains("not allowed from remote template"),
                "Expected security error message for '{import}', got: {error_msg}"
            );
        }
    }

    #[test]
    fn test_security_restrictions_from_https_template() {
        // Test that HTTPS templates cannot use local-only import types
        let base_location = "https://example.com/config.yaml";

        // These should be rejected
        let forbidden_imports = vec![
            "file:local.yaml",
            "env:PATH",
            "git:sha",
            "filehash:local.txt",
            "filehash-base64:local.txt",
            "./local.yaml",
            "../local.yaml",
            "/absolute/local.yaml",
        ];

        for import in forbidden_imports {
            let result = parse_import_type(import, base_location);
            assert!(
                result.is_err(),
                "Expected error for import '{import}' from HTTPS template"
            );
        }
    }

    #[test]
    fn test_allowed_imports_from_remote_templates() {
        // Test that remote templates CAN use these import types
        let s3_base = "s3://bucket/config.yaml";
        let https_base = "https://example.com/config.yaml";

        let allowed_imports = vec![
            ("s3:bucket/other.yaml", ImportType::S3),
            ("http:https://other.com/data.yaml", ImportType::Http),
            ("cfn:stack/MyStack/output", ImportType::Cfn),
            ("ssm:/path/to/param", ImportType::Ssm),
            ("ssm-path:/path/to/params/", ImportType::SsmPath),
            ("random:dashed-name", ImportType::Random),
        ];

        for base_location in [s3_base, https_base] {
            for (import, expected_type) in &allowed_imports {
                let result = parse_import_type(import, base_location);
                assert!(
                    result.is_ok(),
                    "Expected success for import '{import}' from remote template '{base_location}'"
                );
                assert_eq!(result.unwrap(), *expected_type);
            }
        }
    }

    #[test]
    fn test_relative_imports_inherit_remote_type() {
        // Test that relative imports from remote templates inherit the remote type
        let test_cases = vec![
            (
                "s3://bucket/folder/config.yaml",
                "other.yaml",
                ImportType::S3,
            ),
            (
                "https://example.com/configs/app.yaml",
                "database.yaml",
                ImportType::Http,
            ),
            (
                "http://example.com/templates/base.yaml",
                "shared.yaml",
                ImportType::Http,
            ),
        ];

        for (base_location, relative_import, expected_type) in test_cases {
            let result = parse_import_type(relative_import, base_location);
            assert!(
                result.is_ok(),
                "Expected success for relative import '{relative_import}' from '{base_location}'"
            );
            assert_eq!(result.unwrap(), expected_type);
        }
    }

    #[test]
    fn test_local_templates_can_use_any_import_type() {
        // Test that local templates can use any import type (no security restrictions)
        let local_bases = vec![
            "config.yaml",
            "./config.yaml",
            "/absolute/path/config.yaml",
            "configs/app.yaml",
        ];

        let all_import_types = vec![
            ("file:other.yaml", ImportType::File),
            ("env:HOME", ImportType::Env),
            ("git:branch", ImportType::Git),
            ("filehash:data.txt", ImportType::Filehash),
            ("filehash-base64:data.txt", ImportType::FilehashBase64),
            ("s3:bucket/file.yaml", ImportType::S3),
            ("http:https://example.com/data.yaml", ImportType::Http),
            ("cfn:stack/MyStack/output", ImportType::Cfn),
            ("ssm:/param", ImportType::Ssm),
            ("random:name", ImportType::Random),
        ];

        for base_location in local_bases {
            for (import, expected_type) in &all_import_types {
                let result = parse_import_type(import, base_location);
                assert!(
                    result.is_ok(),
                    "Expected success for import '{import}' from local template '{base_location}'"
                );
                assert_eq!(result.unwrap(), *expected_type);
            }
        }
    }

    #[test]
    fn test_is_local_only_classification() {
        // Test the classification of import types
        let local_only_types = vec![
            ImportType::File,
            ImportType::Env,
            ImportType::Git,
            ImportType::Filehash,
            ImportType::FilehashBase64,
        ];

        let remote_allowed_types = vec![
            ImportType::S3,
            ImportType::Http,
            ImportType::Cfn,
            ImportType::Ssm,
            ImportType::SsmPath,
            ImportType::Random,
        ];

        for import_type in local_only_types {
            assert!(
                import_type.is_local_only(),
                "{import_type:?} should be local-only"
            );
        }

        for import_type in remote_allowed_types {
            assert!(
                !import_type.is_local_only(),
                "{import_type:?} should be allowed from remote"
            );
        }
    }
}
