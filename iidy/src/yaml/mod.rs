//! YAML preprocessing module for iidy
//! 
//! This module provides YAML preprocessing capabilities with custom tags,
//! imports, and template composition for CloudFormation and other YAML documents.

pub mod ast;
pub mod parser;
pub mod tags;
pub mod imports;
pub mod handlebars;
pub mod preprocessor;
pub mod spec_detection;
pub mod doc_type_predicates;
mod error_wrapper;
pub mod tree_sitter_location;
pub mod location;

pub mod error_ids;
pub mod enhanced_errors;

// Core preprocessing API
pub use preprocessor::{preprocess_yaml_v11, preprocess_yaml};

// YAML specification detection
pub use spec_detection::{detect_yaml_spec, YamlSpecDetection};
pub use doc_type_predicates::{is_cloudformation_template, is_kubernetes_manifest};

// Error IDs for error handling
pub use error_ids::ErrorId;







