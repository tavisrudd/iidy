//! Handlebars template interpolation for YAML preprocessing
//! 
//! This module provides handlebars template processing for import locations
//! and other string values in the preprocessing system. It includes a comprehensive
//! set of helpers for string manipulation, data serialization, and object access.

pub mod engine;
pub mod helpers;

#[cfg(test)]
mod tests;

// Re-export the main public API
pub use engine::interpolate_handlebars_string;