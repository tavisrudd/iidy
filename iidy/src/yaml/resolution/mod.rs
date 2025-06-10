//! YAML tag resolution module
//! 
//! Contains the tag resolution logic for processing custom preprocessing tags
//! and converting them to final YAML values.

pub mod resolver;

// Re-export key types
pub use resolver::{
    TagContext, StackFrame, TagResolver, StandardTagResolver, derive_base_path_from_location, GlobalAccumulator,
    ScopeContext, Scope, ScopeType, ScopedVariable, VariableSource
};
