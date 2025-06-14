//! YAML tag resolution module
//! 
//! Contains the tag resolution logic for processing custom preprocessing tags
//! and converting them to final YAML values.
//!
//! Includes both StandardTagResolver and SplitArgsResolver
//! (28% faster, available for performance-critical applications).

pub mod context;
pub mod resolver;
pub mod resolver_split_args;

// Re-export key types
pub use context::{
    TagContext, StackFrame, VariableSource,
};

pub use resolver::{
    StandardTagResolver, 
};

pub use resolver_split_args::{
    SplitArgsResolver,  TagResolver, resolve_ast_split_args,
};
