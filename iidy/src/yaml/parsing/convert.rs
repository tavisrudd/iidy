//! Parsing and diagnostic utilities for YAML with custom tags
//!
//! This module provides the main parsing functions and diagnostic APIs.

use super::ast::YamlAst;
use super::error::ParseDiagnostics;
use super::parser::YamlParser;

/// Parse YAML with custom tags and return the AST
pub fn parse_and_convert_to_original(source: &str, file_path: &str) -> anyhow::Result<YamlAst> {
    let uri = if file_path.starts_with("file://") {
        url::Url::parse(file_path)?
    } else {
        url::Url::from_file_path(file_path).unwrap_or_else(|_| {
            url::Url::parse(&format!("file://{}", file_path)).expect("Failed to create file URI")
        })
    };
    
    let mut parser = YamlParser::new()?;
    Ok(parser.parse(source, uri)?)
}

/// Parse YAML with custom tags and return diagnostics
pub fn parse_and_convert_to_original_with_diagnostics(source: &str, file_path: &str) -> ParseDiagnostics {
    let uri = if file_path.starts_with("file://") {
        url::Url::parse(file_path).expect("Invalid file URI")
    } else {
        url::Url::from_file_path(file_path).unwrap_or_else(|_| {
            url::Url::parse(&format!("file://{}", file_path)).expect("Failed to create file URI")
        })
    };
    
    let mut parser = YamlParser::new().expect("Failed to create YAML parser");
    parser.validate_with_diagnostics(source, uri)
}

/// Validate YAML syntax only (no semantic validation)
pub fn validate_yaml_only(source: &str, file_path: &str) -> ParseDiagnostics {
    // For now, use the same validation as the full parser
    // This could be optimized to skip semantic validation in the future
    parse_and_convert_to_original_with_diagnostics(source, file_path)
}