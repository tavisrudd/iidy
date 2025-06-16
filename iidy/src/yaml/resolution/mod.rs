//! YAML tag resolution module
//!
//! Contains the tag resolution logic for processing custom preprocessing tags
//! and converting them to final YAML values.
//!
//! Includes both StandardTagResolver and SplitArgsResolver
//! (28% faster, available for performance-critical applications).

pub mod context;
pub mod resolver;

// Re-export key types
pub use context::{TagContext, VariableSource};

pub use resolver::{Resolver, TagResolver, resolve_ast};
