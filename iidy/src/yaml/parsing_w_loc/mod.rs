pub mod ast;
pub mod parser;
pub mod error;
pub mod convert;

#[cfg(test)]
mod test;

#[cfg(test)]
mod compatibility_test;

#[cfg(test)]
mod proptest_compatibility;

#[cfg(test)]
mod debug_tree_sitter;

#[cfg(test)]
mod proptest_bisect;

#[cfg(test)]
mod debug_failing;

#[cfg(test)]
mod simple_tag_generator;

#[cfg(test)]
mod debug_failing_advanced;

#[cfg(test)]
mod bisect_failing;

#[cfg(test)]
mod debug_exact_case;

#[cfg(test)]
mod diff_ast;

#[cfg(test)]
mod test_whitespace_normalization;

#[cfg(test)]
mod debug_block_newline;


pub use ast::*;
pub use parser::*;
pub use error::*;
pub use convert::*;