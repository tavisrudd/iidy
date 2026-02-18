
// Diagnostic APIs for future LSP/linter integration
pub use error::{ParseDiagnostics, ParseWarning, error_codes};

// Main public API - used by yaml::engine
pub fn parse_yaml_from_file(source: &str, file_path: &str) -> anyhow::Result<ast::YamlAst> {
    let uri = if file_path.starts_with("file://") {
        url::Url::parse(file_path)?
    } else {
        url::Url::from_file_path(file_path).unwrap_or_else(|_| {
            url::Url::parse(&format!("file://{}", file_path)).expect("Failed to create file URI")
        })
    };
    
    let mut parser = parser::YamlParser::new()?;
    Ok(parser.parse(source, uri)?)
}

// New diagnostic parsing function
pub fn parse_yaml_ast_with_diagnostics(source: &str, uri: url::Url) -> ParseDiagnostics {
    let mut parser = parser::YamlParser::new().expect("Failed to create YAML parser");
    parser.validate_with_diagnostics(source, uri)
}

// Internal modules - ast types needed for public API
pub mod ast;
pub(crate) mod error;
pub mod parser;


#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod test;

// Moved parsing-specific unit tests from tests/ directory
#[cfg(test)]
mod position_error_tests;
#[cfg(test)]
mod position_verification_tests;

// Property-based testing for finding parser bugs
#[cfg(test)]
mod proptest;
