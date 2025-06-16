//! YAML preprocessing module for iidy
//!
//! This module provides YAML preprocessing capabilities with custom tags,
//! imports, and template composition for CloudFormation and other YAML documents.

pub mod detection;
pub mod emitter;
pub mod engine;
pub mod errors;
pub mod handlebars;
pub mod imports;
pub mod location;
pub mod parsing;
pub mod parsing_w_loc;
pub mod resolution;
pub mod tree_sitter_location;

// Core preprocessing API
pub use engine::{preprocess_yaml, preprocess_yaml_v11};

// YAML specification and document type detection
pub use detection::{
    YamlSpecDetection, detect_yaml_spec, is_cloudformation_template, is_kubernetes_manifest,
};

// Error IDs for error handling
pub use errors::ErrorId;
