//! YAML preprocessing module for iidy
//! 
//! This module provides YAML preprocessing capabilities with custom tags,
//! imports, and template composition for CloudFormation and other YAML documents.

pub mod parsing;
pub mod resolution;
pub mod errors;
pub mod detection;
pub mod imports;
pub mod handlebars;
pub mod engine;
pub mod tree_sitter_location;
pub mod location;

// Core preprocessing API
pub use engine::{preprocess_yaml_v11, preprocess_yaml};

// YAML specification and document type detection
pub use detection::{detect_yaml_spec, YamlSpecDetection, is_cloudformation_template, is_kubernetes_manifest};

// Error IDs for error handling
pub use errors::ErrorId;







