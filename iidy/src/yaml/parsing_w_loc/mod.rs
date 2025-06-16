// Essential parsing functionality
pub use parser::parse_yaml_ast;
pub use error::{ParseError, ParseResult};

// AST types - need to match original parser API
pub use ast::*;

// Conversion utilities for compatibility
pub use convert::to_original_ast;

// Drop-in replacement function for original parser
pub use convert::parse_and_convert_to_original;

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