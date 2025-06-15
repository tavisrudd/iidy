//! Shared test utilities for YAML parsing tests

use url::Url;
use crate::yaml::parsing::parser as original_parser;
use super::{parse_yaml_ast, YamlAst, ParseError};
use super::convert::to_original_ast;

/// Standard test URI for consistency across tests
pub fn test_uri() -> Url {
    Url::parse("file:///test.yaml").unwrap()
}

/// Parse YAML with both parsers and return results for comparison
#[allow(dead_code)]
pub fn compare_parsers(yaml: &str) -> (Result<YamlAst, ParseError>, anyhow::Result<crate::yaml::parsing::ast::YamlAst>) {
    let uri = test_uri();
    let tree_sitter_result = parse_yaml_ast(yaml, uri.clone());
    let original_result = original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str());
    (tree_sitter_result, original_result)
}

/// Parse YAML with tree-sitter parser and convert to original format for easy comparison
#[allow(dead_code)]
pub fn parse_and_convert(yaml: &str) -> Result<crate::yaml::parsing::ast::YamlAst, ParseError> {
    let ast = parse_yaml_ast(yaml, test_uri())?;
    Ok(to_original_ast(&ast))
}