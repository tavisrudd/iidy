//! YAML tag resolution module
//! 
//! Contains the tag resolution logic for processing custom preprocessing tags
//! and converting them to final YAML values.
//!
//! Includes both StandardTagResolver and SplitArgsResolver
//! (28% faster, available for performance-critical applications).

pub mod resolver;
pub mod resolver_split_args;

// Re-export key types
pub use resolver::{
    TagContext, StackFrame, TagResolver, StandardTagResolver, 
};

pub use resolver_split_args::{
    SplitArgsResolver,  resolve_ast_split_args,
};
