// Essential parsing functionality
pub use parser::parse_yaml_ast;
pub use error::{ParseError, ParseResult};

// New diagnostic API
pub use error::{ParseDiagnostics, ParseWarning, ParseMode, error_codes};
pub use parser::YamlParser;

// AST types - need to match original parser API
pub use ast::*;

// Conversion utilities for compatibility
pub use convert::to_original_ast;

// Drop-in replacement function for original parser
pub use convert::parse_and_convert_to_original;

// New diagnostic conversion functions
pub use convert::{parse_and_convert_to_original_with_diagnostics, validate_yaml_only};

// New diagnostic parsing function
pub fn parse_yaml_ast_with_diagnostics(source: &str, uri: url::Url) -> ParseDiagnostics {
    let mut parser = YamlParser::new().expect("Failed to create YAML parser");
    parser.validate_with_diagnostics(source, uri)
}

// Private modules - implementation details
mod ast;
mod parser;
mod error;
mod convert;

#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod test;
#[cfg(test)]
mod compatibility_test;
#[cfg(test)]
mod proptest;
#[cfg(test)]
mod diagnostic_tests;