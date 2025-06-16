// Main public API - used by yaml::engine
pub use convert::parse_and_convert_to_original;

// Diagnostic APIs for future LSP/linter integration
pub use convert::{parse_and_convert_to_original_with_diagnostics, validate_yaml_only};
pub use error::{ParseDiagnostics, ParseWarning, error_codes};

// New diagnostic parsing function
pub fn parse_yaml_ast_with_diagnostics(source: &str, uri: url::Url) -> ParseDiagnostics {
    let mut parser = parser::YamlParser::new().expect("Failed to create YAML parser");
    parser.validate_with_diagnostics(source, uri)
}

// Internal modules - visible within crate for tests but not exported externally  
pub(crate) mod ast;
pub(crate) mod convert;
pub(crate) mod error;
pub(crate) mod parser;
pub(crate) mod validation;


#[cfg(test)]
mod compatibility_test;
#[cfg(test)]
mod diagnostic_tests;
#[cfg(test)]
mod proptest;
#[cfg(test)]
mod test;
