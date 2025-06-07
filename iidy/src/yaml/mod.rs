//! YAML preprocessing module for iidy
//! 
//! This module implements the custom YAML preprocessing language that allows
//! advanced template composition, data imports, and transformations.
//! 
//! ## Two-Phase Processing Architecture
//! 
//! The preprocessing follows the same two-phase approach as the original iidy-js:
//! 
//! **Phase 1 - Import Loading and Environment Building:**
//! - Parse YAML with custom schema recognition
//! - Copy `$defs` values to environment (unprocessed)
//! - Load all `$imports` with handlebars interpolation in paths
//! - Build complete environment with all imports and definitions
//! 
//! **Phase 2 - Tag Processing and Final Resolution:**
//! - Process all custom tags using visitor pattern
//! - Apply handlebars interpolation to final values
//! - Resolve `!$` includes with dot notation
//! - Generate final processed output

pub mod ast;
pub mod parser;
pub mod tags;
pub mod imports;
pub mod handlebars;
pub mod error_wrapper;

pub mod error_ids;
pub mod enhanced_errors;
#[cfg(test)]
pub mod error_spike_tests;

pub use ast::*;
pub use parser::{parse_yaml_with_custom_tags, parse_yaml_with_custom_tags_from_file};
pub use tags::{TagContext, StackFrame};
pub use error_wrapper::{EnhancedErrorWrapper, EnhancedError};

pub use error_ids::ErrorId;
pub use enhanced_errors::{EnhancedPreprocessingError, SourceLocation};

use anyhow::Result;
use serde_yaml::Value;
use std::path::PathBuf;

use crate::yaml::imports::{ImportLoader, ImportRecord, EnvValues};
use crate::yaml::imports::loaders::ProductionImportLoader;

/// Detect YAML specification version from document content
/// 
/// Checks for:
/// 1. Explicit %YAML directives (%YAML 1.1 or %YAML 1.2)
/// 2. CloudFormation-specific top-level keys (AWSTemplateFormatVersion, Resources, etc.)
/// 3. Kubernetes-specific patterns (apiVersion, kind, etc.)
pub fn detect_yaml_spec(input: &str) -> YamlSpecDetection {
    // Check for explicit YAML directive first
    let lines = input.lines().take(5); // YAML directive should be in first few lines
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("%YAML") {
            if trimmed.contains("1.1") {
                return YamlSpecDetection::ExplicitV11;
            } else if trimmed.contains("1.2") {
                return YamlSpecDetection::ExplicitV12;
            }
        }
    }
    
    // Check for CloudFormation indicators
    if is_cloudformation_template(input) {
        return YamlSpecDetection::CloudFormation;
    }
    
    // Check for Kubernetes indicators  
    if is_kubernetes_manifest(input) {
        return YamlSpecDetection::Kubernetes;
    }
    
    // Default to YAML 1.2 if no specific indicators found
    YamlSpecDetection::Unknown
}

/// Result of YAML specification detection
#[derive(Debug, Clone, PartialEq)]
pub enum YamlSpecDetection {
    /// Explicit %YAML 1.1 directive found
    ExplicitV11,
    /// Explicit %YAML 1.2 directive found  
    ExplicitV12,
    /// CloudFormation template detected (prefer YAML 1.1)
    CloudFormation,
    /// Kubernetes manifest detected (prefer YAML 1.2)
    Kubernetes,
    /// Could not determine type (default to YAML 1.2)
    Unknown,
}

impl YamlSpecDetection {
    /// Convert detection result to boolean for YAML 1.1 compatibility mode
    pub fn should_use_yaml_11_compatibility(&self) -> bool {
        match self {
            YamlSpecDetection::ExplicitV11 => true,
            YamlSpecDetection::ExplicitV12 => false,
            YamlSpecDetection::CloudFormation => true,  // CloudFormation uses YAML 1.1
            YamlSpecDetection::Kubernetes => false,     // Kubernetes uses YAML 1.2
            YamlSpecDetection::Unknown => false,        // Default to YAML 1.2 strict mode
        }
    }
}

/// Check if the document appears to be a CloudFormation template
fn is_cloudformation_template(input: &str) -> bool {
    // CloudFormation-specific top-level keys
    let cfn_indicators = [
        "AWSTemplateFormatVersion",
        "Transform:",
        "Resources:",
        "Parameters:",
        "Outputs:",
        "Conditions:",
        "Mappings:",
        "Metadata:",
    ];
    
    // Look for CloudFormation indicators in the first 50 lines
    let lines: Vec<&str> = input.lines().take(50).collect();
    let content = lines.join("\n");
    
    // Count how many CloudFormation indicators we find
    let cfn_count = cfn_indicators.iter()
        .filter(|&indicator| content.contains(indicator))
        .count();
    
    // If we find 2+ CloudFormation indicators, it's likely a CFN template
    cfn_count >= 2
}

/// Check if the document appears to be a Kubernetes manifest
fn is_kubernetes_manifest(input: &str) -> bool {
    // Kubernetes-specific patterns (not currently used in detection logic)
    let _k8s_indicators = [
        "apiVersion:",
        "kind:",
        "metadata:",
        "spec:",
        "status:",
    ];
    
    // Kubernetes API versions
    let k8s_api_versions = [
        "apps/v1",
        "v1",
        "extensions/v1beta1",
        "networking.k8s.io",
        "batch/v1",
        "autoscaling/v1",
        "rbac.authorization.k8s.io",
    ];
    
    // Look for Kubernetes indicators in the first 20 lines
    let lines: Vec<&str> = input.lines().take(20).collect();
    let content = lines.join("\n");
    
    // Check for apiVersion and kind (required for all K8s resources)
    let has_api_version = content.contains("apiVersion:");
    let has_kind = content.contains("kind:");
    
    // Check for known Kubernetes API versions
    let has_k8s_api = k8s_api_versions.iter()
        .any(|&api| content.contains(api));
    
    // If we have apiVersion + kind + known K8s API, it's likely Kubernetes
    has_api_version && has_kind && has_k8s_api
}

/// Main entry point for YAML preprocessing
/// 
/// Takes raw YAML text and processes all custom tags and preprocessing directives
/// to produce a final YAML document ready for standard deserialization.
pub async fn preprocess_yaml(input: &str) -> Result<Value> {
    preprocess_yaml_with_base_location(input, "input.yaml").await
}

/// Preprocess YAML with a specific base location for resolving relative imports
pub async fn preprocess_yaml_with_base_location(input: &str, base_location: &str) -> Result<Value> {
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader);
    preprocessor.process(input, base_location).await
}

/// Preprocess YAML with specific YAML specification mode
pub async fn preprocess_yaml_with_spec(input: &str, base_location: &str, yaml_spec: &crate::cli::YamlSpec) -> Result<Value> {
    let loader = ProductionImportLoader::new();
    
    let yaml_11_compatibility = match yaml_spec {
        crate::cli::YamlSpec::V11 => true,
        crate::cli::YamlSpec::V12 => false,
        crate::cli::YamlSpec::Auto => {
            let detection = detect_yaml_spec(input);
            detection.should_use_yaml_11_compatibility()
        }
    };
    
    let mut preprocessor = YamlPreprocessor::new(loader)
        .with_yaml_11_compatibility(yaml_11_compatibility);
    
    preprocessor.process(input, base_location).await
}

/// YAML preprocessor that handles the two-phase processing pipeline
pub struct YamlPreprocessor<L: ImportLoader> {
    import_loader: L,
    /// Enable YAML 1.1 boolean compatibility for CloudFormation
    yaml_11_compatibility: bool,
    /// Map of preprocessing tag unique identifiers to their actual tags
    preprocessing_tag_map: std::collections::HashMap<String, ast::PreprocessingTag>,
}

impl<L: ImportLoader> YamlPreprocessor<L> {
    pub fn new(import_loader: L) -> Self {
        Self { 
            import_loader,
            yaml_11_compatibility: true, // Default to CloudFormation compatibility
            preprocessing_tag_map: std::collections::HashMap::new(),
        }
    }
    
    /// Create a preprocessor with YAML 1.1 compatibility disabled
    pub fn new_yaml_12_mode(import_loader: L) -> Self {
        Self { 
            import_loader,
            yaml_11_compatibility: false,
            preprocessing_tag_map: std::collections::HashMap::new(),
        }
    }
    
    /// Enable or disable YAML 1.1 boolean compatibility
    pub fn with_yaml_11_compatibility(mut self, enabled: bool) -> Self {
        self.yaml_11_compatibility = enabled;
        self
    }

    /// Main processing entry point - implements the two-phase pipeline
    pub async fn process(&mut self, input: &str, base_location: &str) -> Result<Value> {
        // Parse YAML with custom tag support
        let ast = parser::parse_yaml_with_custom_tags_from_file(input, base_location)?;
        
        // Phase 1: Import loading and environment building
        let mut env_values = EnvValues::new();
        let mut import_records = Vec::new();
        self.load_imports_and_defs(&ast, base_location, &mut env_values, &mut import_records).await?;
        
        // Phase 2: Tag processing and final resolution
        let mut context = TagContext::new()
            .with_base_path(PathBuf::from(base_location))
            .with_stack_frame(StackFrame {
                location: Some(base_location.to_string()),
                path: "<root>".to_string(),
            });
        
        // Add all environment variables to context
        for (key, value) in env_values {
            context = context.with_variable(&key, value);
        }
            
        let result = self.resolve_ast_with_context(ast, &context)?;
        
        // Apply YAML 1.1 compatibility for CloudFormation if enabled
        if self.yaml_11_compatibility {
            Ok(self.convert_yaml_12_to_11_compatibility(result))
        } else {
            Ok(result)
        }
    }

    /// Phase 1: Load all imports and definitions to build the complete environment
    async fn load_imports_and_defs(
        &mut self,
        ast: &YamlAst,
        base_location: &str,
        env_values: &mut EnvValues,
        import_records: &mut Vec<ImportRecord>,
    ) -> Result<()> {
        // Look for $imports and $defs in the root mapping
        if let YamlAst::Mapping(pairs) = ast {
            for (key, value) in pairs {
                if let YamlAst::String(key_str) = key {
                    match key_str.as_str() {
                        "$defs" => {
                            self.process_defs(value, env_values)?;
                        }
                        "$imports" => {
                            self.process_imports(value, base_location, env_values, import_records).await?;
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    /// Process $defs by copying values to environment (unprocessed)
    fn process_defs(&mut self, defs_ast: &YamlAst, env_values: &mut EnvValues) -> Result<()> {
        if let YamlAst::Mapping(pairs) = defs_ast {
            for (key, value) in pairs {
                if let YamlAst::String(key_str) = key {
                    // Check for collisions with existing imports
                    if env_values.contains_key(key_str) {
                        return Err(anyhow::anyhow!(
                            "\"{}\" in $defs collides with the same name in $imports",
                            key_str
                        ));
                    }
                    
                    // Convert AST to Value for storage (will be processed later in Phase 2)
                    let value_raw = self.ast_to_value_unprocessed(value.clone())?;
                    env_values.insert(key_str.clone(), value_raw);
                }
            }
        }
        Ok(())
    }

    /// Process $imports by loading external data with handlebars interpolation
    async fn process_imports(
        &mut self,
        imports_ast: &YamlAst,
        base_location: &str,
        env_values: &mut EnvValues,
        import_records: &mut Vec<ImportRecord>,
    ) -> Result<()> {
        if let YamlAst::Mapping(pairs) = imports_ast {
            for (key, value) in pairs {
                if let (YamlAst::String(import_key), YamlAst::String(location)) = (key, value) {
                    // Check for collisions
                    if env_values.contains_key(import_key) {
                        return Err(anyhow::anyhow!(
                            "\"{}\" in $imports collides with the same name in $defs",
                            import_key
                        ));
                    }

                    // Apply handlebars interpolation to import location using current env_values
                    let resolved_location = self.interpolate_import_location(location, env_values)?;
                    
                    // Load the import
                    let import_data = self.import_loader.load(&resolved_location, base_location).await?;
                    
                    // CRITICAL: Recursively process the imported document if it has $imports or $defs
                    // This matches iidy-js loadImports() lines 524-527
                    let processed_doc = self.process_imported_document(
                        import_data.doc, 
                        &import_data.resolved_location,
                        import_records
                    ).await?;
                    
                    // Add the fully processed document to environment
                    env_values.insert(import_key.clone(), processed_doc);
                    
                    // Record for metadata
                    import_records.push(ImportRecord {
                        key: Some(import_key.clone()),
                        from: base_location.to_string(),
                        imported: import_data.resolved_location,
                        sha256_digest: self.compute_sha256(&import_data.data),
                    });
                    
                    // TODO: Recursively process nested imports in imported documents
                }
            }
        }
        Ok(())
    }

    /// Apply handlebars interpolation to import location
    fn interpolate_import_location(&self, location: &str, env_values: &EnvValues) -> Result<String> {
        use crate::yaml::handlebars::interpolate_handlebars_string;
        
        // Check if location contains handlebars syntax
        if location.contains("{{") && location.contains("}}") {
            // Convert env_values from serde_yaml::Value to serde_json::Value for handlebars
            let mut json_env = std::collections::HashMap::new();
            for (key, yaml_value) in env_values {
                let json_value = yaml_value_to_json_value(yaml_value)?;
                json_env.insert(key.clone(), json_value);
            }
            
            interpolate_handlebars_string(location, &json_env, "import-location")
                .map_err(|e| anyhow::anyhow!("Failed to interpolate import location '{}': {}", location, e))
        } else {
            Ok(location.to_string())
        }
    }

    /// Process an imported document recursively, matching iidy-js loadImports behavior
    /// This ensures that imported documents get their own $defs and $imports processed
    fn process_imported_document<'a>(
        &'a mut self,
        doc: Value,
        doc_location: &'a str,
        import_records: &'a mut Vec<ImportRecord>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value>> + 'a>> {
        Box::pin(async move {
            // Check if this document has preprocessing directives that need processing
            if let Value::Mapping(ref map) = doc {
                let has_imports = map.contains_key(&Value::String("$imports".to_string()));
                let has_defs = map.contains_key(&Value::String("$defs".to_string()));
                
                if has_imports || has_defs {
                    // This document needs recursive preprocessing - parse it back to AST and process
                    let doc_yaml = serde_yaml::to_string(&doc)?;
                    let doc_ast = parser::parse_yaml_with_custom_tags_from_file(&doc_yaml, doc_location)?;
                    
                    // Recursively process this document with its own environment
                    let mut doc_env_values = EnvValues::new();
                    self.load_imports_and_defs(&doc_ast, doc_location, &mut doc_env_values, import_records).await?;
                    
                    // Phase 2: Process the document with its own environment context
                    let mut doc_context = TagContext::new()
                        .with_base_path(PathBuf::from(doc_location));
                    
                    // Add the document's environment variables to context
                    for (key, value) in doc_env_values {
                        doc_context = doc_context.with_variable(&key, value);
                    }
                    
                    // Create a temporary mutable preprocessor for resolving this document
                    // Inherit configuration from parent preprocessor
                    let loader = ProductionImportLoader::new();
                    let mut temp_preprocessor = YamlPreprocessor::new(loader)
                        .with_yaml_11_compatibility(self.yaml_11_compatibility);
                    return temp_preprocessor.resolve_ast_with_context(doc_ast, &doc_context);
                }
            }
            
            // Document has no preprocessing directives, return as-is
            Ok(doc)
        })
    }

    /// Compute SHA256 hash for import tracking
    fn compute_sha256(&self, data: &str) -> String {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        hex::encode(hasher.finalize())
    }

    /// Convert AST to Value without processing (for Phase 1 storage)
    fn ast_to_value_unprocessed(&mut self, ast: YamlAst) -> Result<Value> {
        match ast {
            YamlAst::Null => Ok(Value::Null),
            YamlAst::Bool(b) => Ok(Value::Bool(b)),
            YamlAst::Number(n) => Ok(Value::Number(n)),
            YamlAst::String(s) => Ok(Value::String(s)),
            YamlAst::Sequence(seq) => {
                let mut result = Vec::new();
                for item in seq {
                    result.push(self.ast_to_value_unprocessed(item)?);
                }
                Ok(Value::Sequence(result))
            }
            YamlAst::Mapping(pairs) => {
                let mut result = serde_yaml::Mapping::new();
                for (key, value) in pairs {
                    let key_val = self.ast_to_value_unprocessed(key)?;
                    let value_val = self.ast_to_value_unprocessed(value)?;
                    result.insert(key_val, value_val);
                }
                Ok(Value::Mapping(result))
            }
            YamlAst::PreprocessingTag(tag) => {
                // Store preprocessing tags with unique identifiers to prevent collision
                let tag_id = format!("__PREPROCESSING_TAG_{}__", uuid::Uuid::new_v4().simple());
                self.preprocessing_tag_map.insert(tag_id.clone(), tag.clone());
                Ok(Value::String(tag_id))
            }
            YamlAst::UnknownYamlTag(tag) => {
                // Store unknown tags by converting their value
                self.ast_to_value_unprocessed(*tag.value)
            }
        }
    }

    /// Convert YAML 1.2 values to YAML 1.1 equivalents for CloudFormation compatibility
    /// 
    /// CloudFormation uses YAML 1.1 which auto-converts certain strings to booleans/null:
    /// - yes/Yes/YES/true/True/TRUE/on/On/ON → boolean true
    /// - no/No/NO/false/False/FALSE/off/Off/OFF → boolean false  
    /// - null/Null/NULL/~ → null
    ///
    /// This function uses heuristics to avoid converting strings that are likely intended 
    /// to remain as strings (like in Description fields or certain tag contexts).
    fn convert_yaml_12_to_11_compatibility(&self, value: Value) -> Value {
        self.convert_yaml_12_to_11_compatibility_with_context(value, &[])
    }
    
    /// Convert with context awareness to avoid inappropriate conversions
    fn convert_yaml_12_to_11_compatibility_with_context(&self, value: Value, path: &[String]) -> Value {
        match value {
            Value::String(s) => {
                // Check if we're in a context where strings should remain strings
                if self.should_preserve_as_string(&s, path) {
                    Value::String(s)
                } else {
                    match s.as_str() {
                        // YAML 1.1 true values
                        "yes" | "Yes" | "YES" | "true" | "True" | "TRUE" | "on" | "On" | "ON" => {
                            Value::Bool(true)
                        }
                        // YAML 1.1 false values  
                        "no" | "No" | "NO" | "false" | "False" | "FALSE" | "off" | "Off" | "OFF" => {
                            Value::Bool(false)
                        }
                        // YAML 1.1 null values (~ is already handled by serde_yaml)
                        "null" | "Null" | "NULL" => {
                            Value::Null
                        }
                        // Keep all other strings as strings
                        _ => Value::String(s)
                    }
                }
            }
            Value::Sequence(seq) => {
                // Recursively convert sequence elements
                let converted_seq = seq.into_iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let mut new_path = path.to_vec();
                        new_path.push(format!("[{}]", i));
                        self.convert_yaml_12_to_11_compatibility_with_context(item, &new_path)
                    })
                    .collect();
                Value::Sequence(converted_seq)
            }
            Value::Mapping(map) => {
                // Recursively convert mapping values with path context
                let mut converted_map = serde_yaml::Mapping::new();
                for (k, v) in map {
                    let key_str = match &k {
                        Value::String(s) => s.clone(),
                        _ => format!("{:?}", k),
                    };
                    let mut new_path = path.to_vec();
                    new_path.push(key_str);
                    let converted_value = self.convert_yaml_12_to_11_compatibility_with_context(v, &new_path);
                    converted_map.insert(k, converted_value);
                }
                Value::Mapping(converted_map)
            }
            // Keep other types as-is (Bool, Number, Null, Tagged)
            _ => value
        }
    }
    
    /// Determine if a string should be preserved as-is rather than converted to boolean
    /// Uses heuristics based on the path context to avoid inappropriate conversions
    fn should_preserve_as_string(&self, s: &str, path: &[String]) -> bool {
        // Don't convert boolean-like strings in these contexts:
        let preserve_contexts = [
            "Description",      // CloudFormation Description fields
            "Name",            // Name fields often contain descriptive text
            "Value",           // Tag values might be descriptive
            "Message",         // Message fields
            "Text",            // Text fields
            "Content",         // Content fields
            "Data",            // Data fields
        ];
        
        // Check if we're in a context that typically contains free-form text
        for context in &preserve_contexts {
            if path.iter().any(|p| p.contains(context)) {
                return true;
            }
        }
        
        // Additional heuristic: if the string is longer than a simple boolean word,
        // it's probably not intended as a boolean
        if s.len() > 5 {  // "false" is 5 characters, so longer strings are probably not booleans
            return true;
        }
        
        false
    }

    /// Phase 2: Resolve AST with complete environment context
    pub fn resolve_ast_with_context(&mut self, ast: YamlAst, context: &TagContext) -> Result<Value> {
        match ast {
            YamlAst::Null => Ok(Value::Null),
            YamlAst::Bool(b) => Ok(Value::Bool(b)),
            YamlAst::Number(n) => Ok(Value::Number(n)),
            YamlAst::String(s) => {
                // Process handlebars templates in strings
                self.process_string_with_handlebars(s, context)
            },
            YamlAst::Sequence(seq) => {
                let mut result = Vec::new();
                for (index, item) in seq.into_iter().enumerate() {
                    // Create context with array index for path tracking
                    let item_context = context.with_array_index(index);
                    result.push(self.resolve_ast_with_context(item, &item_context)?);
                }
                Ok(Value::Sequence(result))
            }
            YamlAst::Mapping(map) => {
                let mut result = serde_yaml::Mapping::new();
                for (key, value) in map {
                    let key_val = self.resolve_ast_with_context(key, context)?;
                    
                    // Check for YAML 1.1 merge keys which are not supported in YAML 1.2
                    if let Value::String(key_str) = &key_val {
                        if key_str == "<<" {
                            let location_info = if let Some(base_path) = &context.base_path {
                                format!("in file '{}'", base_path.display())
                            } else {
                                context.current_location()
                                    .map(|loc| format!("in '{}'", loc))
                                    .unwrap_or_else(|| "in unknown location".to_string())
                            };
                            let yaml_path = context.current_path();
                            let path_info = if !yaml_path.is_empty() {
                                format!(" at path '{}'", yaml_path)
                            } else {
                                String::new()
                            };
                            return Err(anyhow::anyhow!(
                                "YAML merge keys ('<<') are not supported in YAML 1.2 {}{}\n\
                                Consider using iidy's !$merge tag instead:\n\
                                  combined_config: !$merge\n\
                                    - *base_config\n\
                                    - additional_key: additional_value",
                                location_info, path_info
                            ));
                        }
                        
                        // Skip preprocessing directive keys in final output (matching iidy-js behavior)
                        if matches!(key_str.as_str(), "$imports" | "$defs" | "$envValues") {
                            continue;
                        }
                    }
                    
                    // Create context with object key for path tracking
                    let value_context = if let Value::String(key_str) = &key_val {
                        context.with_path_segment(key_str)
                    } else {
                        // For non-string keys, use the key's string representation
                        let key_str = match &key_val {
                            Value::Number(n) => n.as_f64().unwrap_or(0.0).to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => format!("{:?}", key_val),
                        };
                        context.with_path_segment(&key_str)
                    };
                    
                    let value_val = self.resolve_ast_with_context(value, &value_context)?;
                    result.insert(key_val, value_val);
                }
                Ok(Value::Mapping(result))
            }
            YamlAst::PreprocessingTag(tag) => {
                self.resolve_preprocessing_tag_with_context(tag, context)
            },
            YamlAst::UnknownYamlTag(tag) => {
                // For unknown tags like !Ref, !Sub, preserve the tag structure while processing the content
                // Based on iidy-js behavior: handlebars/preprocessing happens INSIDE tag values
                let resolved_value = self.resolve_ast_with_context(*tag.value, context)?;
                self.create_tagged_value(&tag.tag, resolved_value)
            }
        }
    }

    /// Create a tagged value that preserves CloudFormation tags like !Ref, !Sub
    /// Uses serde_yaml::value::TaggedValue to properly serialize YAML tags
    fn create_tagged_value(&self, tag: &str, value: Value) -> Result<Value> {
        // Use serde_yaml::value::TaggedValue for proper YAML tag serialization
        let tagged_value = serde_yaml::value::TaggedValue {
            tag: serde_yaml::value::Tag::new(format!("!{}", tag)),
            value,
        };
        Ok(Value::Tagged(Box::new(tagged_value)))
    }

    #[allow(dead_code)]
    fn resolve_preprocessing_tag(&mut self, tag: PreprocessingTag) -> Result<Value> {
        self.resolve_preprocessing_tag_with_context(tag, &TagContext::new())
    }

    fn process_string_with_handlebars(&self, s: String, context: &TagContext) -> Result<Value> {
        use crate::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;
        
        // Check if string contains handlebars syntax
        if !s.contains("{{") {
            return Ok(Value::String(s));
        }
        
        // Convert TagContext variables from serde_yaml::Value to serde_json::Value
        let mut env_values: HashMap<String, serde_json::Value> = HashMap::new();
        for (key, yaml_value) in &context.variables {
            let json_value = yaml_value_to_json_value(yaml_value)?;
            env_values.insert(key.clone(), json_value);
        }
        
        // Apply handlebars interpolation to the string
        match interpolate_handlebars_string(&s, &env_values, "yaml-string") {
            Ok(processed_string) => Ok(Value::String(processed_string)),
            Err(e) => {
                // Enhanced error handling for handlebars processing
                {
                    let error_msg = e.to_string();
                    
                    // Extract variable name from handlebars error if possible
                    if error_msg.contains("Variable") && error_msg.contains("not found") {
                        // Parse the variable name from the error message
                        let var_name = if let Some(start) = error_msg.find("Variable \"") {
                            let start = start + 10; // Skip 'Variable "'
                            if let Some(end) = error_msg[start..].find('"') {
                                &error_msg[start..start + end]
                            } else {
                                "unknown"
                            }
                        } else {
                            "unknown"
                        };
                        
                        // Get file path and try to find the line number
                        let file_path = if let Some(base_path) = &context.base_path {
                            base_path.display().to_string()
                        } else {
                            context.current_location().unwrap_or_else(|| "unknown location".to_string())
                        };
                        
                        let location = if let Ok(content) = std::fs::read_to_string(&file_path) {
                            let line_number = content.lines().enumerate().find_map(|(idx, line)| {
                                if line.contains(&format!("{{{{{}}}}}", var_name)) {
                                    Some(idx + 1)
                                } else {
                                    None
                                }
                            }).unwrap_or(0);
                            
                            if line_number > 0 {
                                format!("{}:{}", file_path, line_number)
                            } else {
                                file_path
                            }
                        } else {
                            file_path
                        };
                        
                        let available_vars: Vec<String> = env_values.keys().cloned().collect();
                        use crate::yaml::error_wrapper::variable_not_found_error;
                        return Err(variable_not_found_error(var_name, &location, &context.current_path(), available_vars));
                    }
                }
                
                // Fallback to basic error
                Err(anyhow::anyhow!("Handlebars processing failed: {}", e))
            }
        }
    }

    fn resolve_preprocessing_tag_with_context(&mut self, tag: PreprocessingTag, context: &TagContext) -> Result<Value> {
        use crate::yaml::tags::*;
        
        match tag {
            PreprocessingTag::Include(include_tag) => {
                resolve_include_tag(&include_tag, context)
            }
            PreprocessingTag::If(if_tag) => {
                resolve_if_tag(&if_tag, context, self)
            }
            PreprocessingTag::Map(map_tag) => {
                resolve_map_tag(&map_tag, context, self)
            }
            PreprocessingTag::Merge(merge_tag) => {
                resolve_merge_tag(&merge_tag, context, self)
            }
            PreprocessingTag::Concat(concat_tag) => {
                resolve_concat_tag(&concat_tag, context, self)
            }
            PreprocessingTag::Let(let_tag) => {
                resolve_let_tag(&let_tag, context, self)
            }
            PreprocessingTag::Eq(eq_tag) => {
                resolve_eq_tag(&eq_tag, context, self)
            }
            PreprocessingTag::Not(not_tag) => {
                resolve_not_tag(&not_tag, context, self)
            }
            PreprocessingTag::Split(split_tag) => {
                resolve_split_tag(&split_tag, context, self)
            }
            PreprocessingTag::Join(join_tag) => {
                resolve_join_tag(&join_tag, context, self)
            }
            PreprocessingTag::ConcatMap(concat_map_tag) => {
                resolve_concat_map_tag(&concat_map_tag, context, self)
            }
            PreprocessingTag::MergeMap(merge_map_tag) => {
                resolve_merge_map_tag(&merge_map_tag, context, self)
            }
            PreprocessingTag::MapListToHash(map_list_to_hash_tag) => {
                resolve_map_list_to_hash_tag(&map_list_to_hash_tag, context, self)
            }
            PreprocessingTag::MapValues(map_values_tag) => {
                resolve_map_values_tag(&map_values_tag, context, self)
            }
            PreprocessingTag::GroupBy(group_by_tag) => {
                resolve_group_by_tag(&group_by_tag, context, self)
            }
            PreprocessingTag::FromPairs(from_pairs_tag) => {
                resolve_from_pairs_tag(&from_pairs_tag, context, self)
            }
            PreprocessingTag::ToYamlString(to_yaml_string_tag) => {
                resolve_to_yaml_string_tag(&to_yaml_string_tag, context, self)
            }
            PreprocessingTag::ParseYaml(parse_yaml_tag) => {
                resolve_parse_yaml_tag(&parse_yaml_tag, context, self)
            }
            PreprocessingTag::ToJsonString(to_json_string_tag) => {
                resolve_to_json_string_tag(&to_json_string_tag, context, self)
            }
            PreprocessingTag::ParseJson(parse_json_tag) => {
                resolve_parse_json_tag(&parse_json_tag, context, self)
            }
            PreprocessingTag::Escape(escape_tag) => {
                resolve_escape_tag(&escape_tag, context, self)
            }
        }
    }
}

impl<L: ImportLoader> Default for YamlPreprocessor<L> 
where 
    L: Default 
{
    fn default() -> Self {
        Self::new(L::default())
    }
}

impl<L: ImportLoader> tags::AstResolver for YamlPreprocessor<L> {
    fn resolve_ast(&self, ast: &YamlAst, context: &tags::TagContext) -> Result<Value> {
        // Create a temporary preprocessor for synchronous resolution
        // TODO: Refactor AstResolver trait to support async properly
        let loader = ProductionImportLoader::new();
        let mut temp_preprocessor = YamlPreprocessor::new(loader);
        temp_preprocessor.resolve_ast_with_context(ast.clone(), context)
    }
}

/// Convert serde_yaml::Value to serde_json::Value for handlebars processing
fn yaml_value_to_json_value(yaml_value: &Value) -> Result<serde_json::Value> {
    match yaml_value {
        Value::Null => Ok(serde_json::Value::Null),
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(serde_json::Value::Number(serde_json::Number::from(i)))
            } else if let Some(u) = n.as_u64() {
                Ok(serde_json::Value::Number(serde_json::Number::from(u)))
            } else if let Some(f) = n.as_f64() {
                Ok(serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null))
            } else {
                Ok(serde_json::Value::Null)
            }
        }
        Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Sequence(seq) => {
            let mut json_seq = Vec::new();
            for item in seq {
                json_seq.push(yaml_value_to_json_value(item)?);
            }
            Ok(serde_json::Value::Array(json_seq))
        }
        Value::Mapping(map) => {
            let mut json_map = serde_json::Map::new();
            for (k, v) in map {
                let key_str = match k {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.as_f64().unwrap_or(0.0).to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => format!("{:?}", k), // fallback for other types
                };
                json_map.insert(key_str, yaml_value_to_json_value(v)?);
            }
            Ok(serde_json::Value::Object(json_map))
        }
        Value::Tagged(_) => Err(anyhow::anyhow!("Tagged values not supported in handlebars conversion")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml::imports::loaders::ProductionImportLoader;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[tokio::test]
    async fn test_two_phase_processing_with_defs() -> Result<()> {
        let yaml_input = r#"
$defs:
  environment: "test"
  app_name: "my-app"

stack_name: "{{app_name}}-{{environment}}"
region: "us-west-2"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        // Check that the environment variables were properly resolved
        if let Value::Mapping(map) = result {
            if let Some(Value::String(stack_name)) = map.get(&Value::String("stack_name".to_string())) {
                assert_eq!(stack_name, "my-app-test");
            } else {
                panic!("Expected stack_name to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_two_phase_processing_with_file_import() -> Result<()> {
        // Create a temporary file with some data
        let mut temp_file = NamedTempFile::with_suffix(".yaml")?;
        writeln!(temp_file, "database_host: db.example.com")?;
        writeln!(temp_file, "database_port: 5432")?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        let yaml_input = format!(r#"
$imports:
  config: "{}"

database:
  host: !$ config.database_host
  port: !$ config.database_port
"#, temp_path);

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(&yaml_input, "test.yaml").await?;

        // The database values should be resolved from the imported file
        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(database)) = map.get(&Value::String("database".to_string())) {
                if let Some(Value::String(host)) = database.get(&Value::String("host".to_string())) {
                    assert_eq!(host, "db.example.com");
                }
                if let Some(Value::String(port)) = database.get(&Value::String("port".to_string())) {
                    assert_eq!(port, "5432");
                }
            } else {
                panic!("Expected database mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_two_phase_processing_with_conditional() -> Result<()> {
        let yaml_input = r#"
$defs:
  environment: "prod"

database_host: !$if
  condition: !$eq ["prod", "{{environment}}"]
  then: "prod-db.example.com"
  else: "dev-db.example.com"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        // The conditional should resolve to the prod database
        if let Value::Mapping(map) = result {
            if let Some(Value::String(db_host)) = map.get(&Value::String("database_host".to_string())) {
                assert_eq!(db_host, "prod-db.example.com");
            } else {
                panic!("Expected database_host to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_concat_map_tag() -> Result<()> {
        let yaml_input = r#"
$defs:
  data: [1, 2, 3]

result: !$concatMap
  items: !$ data
  template: ["{{item}}", "{{item}}"]
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Sequence(result_seq)) = map.get(&Value::String("result".to_string())) {
                // Should flatten the sequences: [1, 1, 2, 2, 3, 3]
                assert_eq!(result_seq.len(), 6);
                assert_eq!(result_seq[0], Value::String("1".to_string()));
                assert_eq!(result_seq[1], Value::String("1".to_string()));
                assert_eq!(result_seq[2], Value::String("2".to_string()));
                assert_eq!(result_seq[3], Value::String("2".to_string()));
            } else {
                panic!("Expected result to be a sequence");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_merge_map_tag() -> Result<()> {
        let yaml_input = r#"
$defs:
  data: ["a", "b"]

result: !$mergeMap
  source: !$ data
  transform: 
    "prefix_{{item}}": "value_{{item}}"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                // Should merge the transformed mappings
                assert!(result_map.contains_key(&Value::String("prefix_a".to_string())));
                assert!(result_map.contains_key(&Value::String("prefix_b".to_string())));
                assert_eq!(result_map.get(&Value::String("prefix_a".to_string())), Some(&Value::String("value_a".to_string())));
                assert_eq!(result_map.get(&Value::String("prefix_b".to_string())), Some(&Value::String("value_b".to_string())));
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_map_list_to_hash_tag() -> Result<()> {
        let yaml_input = r#"
$defs:
  data: 
    - key: "name"
      value: "Alice"
    - key: "age" 
      value: 30

result: !$mapListToHash
  source: !$ data
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                assert_eq!(result_map.get(&Value::String("name".to_string())), Some(&Value::String("Alice".to_string())));
                // Check the age number properly
                if let Some(Value::Number(age)) = result_map.get(&Value::String("age".to_string())) {
                    assert_eq!(age.as_i64().unwrap(), 30);
                } else {
                    panic!("Expected age to be a number");
                }
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_map_values_tag() -> Result<()> {
        let yaml_input = r#"
$defs:
  data: 
    name: "alice"
    city: "boston"

result: !$mapValues
  items: !$ data  
  template: "{{toUpperCase item.value}}"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                assert_eq!(result_map.get(&Value::String("name".to_string())), Some(&Value::String("ALICE".to_string())));
                assert_eq!(result_map.get(&Value::String("city".to_string())), Some(&Value::String("BOSTON".to_string())));
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_group_by_tag() -> Result<()> {
        let yaml_input = r#"
$defs:
  data:
    - name: "Alice"
      category: "A"
    - name: "Bob"
      category: "B"
    - name: "Charlie"
      category: "A"

result: !$groupBy
  source: !$ data
  key: !$ item.category
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                // Should have groups "A" and "B"
                assert!(result_map.contains_key(&Value::String("A".to_string())));
                assert!(result_map.contains_key(&Value::String("B".to_string())));
                
                if let Some(Value::Sequence(group_a)) = result_map.get(&Value::String("A".to_string())) {
                    assert_eq!(group_a.len(), 2); // Alice and Charlie
                }
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_from_pairs_tag() -> Result<()> {
        let yaml_input = r#"
result: !$fromPairs
  - ["name", "Alice"]
  - ["age", 30]
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                assert_eq!(result_map.get(&Value::String("name".to_string())), Some(&Value::String("Alice".to_string())));
                // Numbers should preserve their original representation (integer in this case)
                if let Some(Value::Number(age)) = result_map.get(&Value::String("age".to_string())) {
                    assert_eq!(age.as_i64().unwrap(), 30);
                } else {
                    panic!("Expected age to be a number");
                }
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_to_yaml_string_tag() -> Result<()> {
        let yaml_input = r#"
result: !$toYamlString
  name: "Alice"
  age: 30
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::String(yaml_str)) = map.get(&Value::String("result".to_string())) {
                assert!(yaml_str.contains("name: Alice"));
                assert!(yaml_str.contains("age: 30"));
            } else {
                panic!("Expected result to be a YAML string");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_parse_yaml_tag() -> Result<()> {
        let yaml_input = r#"
result: !$parseYaml "name: Alice\nage: 30"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                assert_eq!(result_map.get(&Value::String("name".to_string())), Some(&Value::String("Alice".to_string())));
                assert_eq!(result_map.get(&Value::String("age".to_string())), Some(&Value::Number(serde_yaml::Number::from(30))));
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_to_json_string_tag() -> Result<()> {
        let yaml_input = r#"
result: !$toJsonString
  name: "Alice"
  age: 30
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::String(json_str)) = map.get(&Value::String("result".to_string())) {
                // Should be valid JSON
                let parsed: serde_json::Value = serde_json::from_str(json_str).expect("Should be valid JSON");
                assert_eq!(parsed["name"], "Alice");
                assert_eq!(parsed["age"], 30);
            } else {
                panic!("Expected result to be a JSON string");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_parse_json_tag() -> Result<()> {
        let yaml_input = r#"
result: !$parseJson '{"name": "Alice", "age": 30}'
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                assert_eq!(result_map.get(&Value::String("name".to_string())), Some(&Value::String("Alice".to_string())));
                assert_eq!(result_map.get(&Value::String("age".to_string())), Some(&Value::Number(serde_yaml::Number::from(30))));
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_escape_tag() -> Result<()> {
        let yaml_input = r#"
$defs:
  data: "test"

result: !$escape
  message: "{{data}}"
  nested: !$ data
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                // The escaped content should not be processed
                assert_eq!(result_map.get(&Value::String("message".to_string())), Some(&Value::String("{{data}}".to_string())));
                // The !$ tag should be escaped to a placeholder
                assert_eq!(result_map.get(&Value::String("nested".to_string())), Some(&Value::String("__ESCAPED_PREPROCESSING_TAG__".to_string())));
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_number_preservation_for_cloudformation() -> Result<()> {
        let yaml_input = r#"
$defs:
  port: 80
  timeout: 3.5
  count: 10

resources:
  server_port: !$ port
  request_timeout: !$ timeout  
  instance_count: !$ count
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        // Convert back to YAML string to verify serialization
        let yaml_output = serde_yaml::to_string(&result)?;
        
        // Verify that integers stay as integers (no .0 suffix)
        assert!(yaml_output.contains("server_port: 80"));
        assert!(!yaml_output.contains("server_port: 80.0"));
        
        // Verify that floats stay as floats  
        assert!(yaml_output.contains("request_timeout: 3.5"));
        
        // Verify that large integers stay as integers
        assert!(yaml_output.contains("instance_count: 10"));
        assert!(!yaml_output.contains("instance_count: 10.0"));

        Ok(())
    }

    #[tokio::test]
    async fn test_bracket_notation_variable_reference() -> Result<()> {
        let yaml_input = r#"
$defs:
  environment: "prod"
  config:
    prod: "prod-db.example.com"
    dev: "dev-db.example.com"

database_host: !$ config[environment]
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::String(db_host)) = map.get(&Value::String("database_host".to_string())) {
                assert_eq!(db_host, "prod-db.example.com");
            } else {
                panic!("Expected database_host to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_bracket_notation_literal_string() -> Result<()> {
        let yaml_input = r#"
$defs:
  config:
    "literal-key": "literal-value"
    'single-quote-key': "single-quote-value"

result1: !$ config["literal-key"]
result2: !$ config['single-quote-key']
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::String(result1)) = map.get(&Value::String("result1".to_string())) {
                assert_eq!(result1, "literal-value");
            } else {
                panic!("Expected result1 to be resolved");
            }
            
            if let Some(Value::String(result2)) = map.get(&Value::String("result2".to_string())) {
                assert_eq!(result2, "single-quote-value");
            } else {
                panic!("Expected result2 to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_bracket_notation_nested_path() -> Result<()> {
        let yaml_input = r#"
$defs:
  env:
    stage: "production"
  config:
    production: "prod-db.example.com"
    development: "dev-db.example.com"

database_host: !$ config[env.stage]
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::String(db_host)) = map.get(&Value::String("database_host".to_string())) {
                assert_eq!(db_host, "prod-db.example.com");
            } else {
                panic!("Expected database_host to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_mixed_dot_and_bracket_notation() -> Result<()> {
        let yaml_input = r#"
$defs:
  environment: "prod"
  region: "us-west-2"
  configs:
    prod:
      regions:
        "us-west-2": "prod-us-west-2-db.example.com"
        "us-east-1": "prod-us-east-1-db.example.com"

database_host: !$ configs[environment].regions[region]
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::String(db_host)) = map.get(&Value::String("database_host".to_string())) {
                assert_eq!(db_host, "prod-us-west-2-db.example.com");
            } else {
                panic!("Expected database_host to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_query_selector_single_property() -> Result<()> {
        let yaml_input = r#"
$defs:
  config:
    database: "db.example.com"
    cache: "cache.example.com"
    storage: "storage.example.com"

result: !$ config?database
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::String(result_value)) = map.get(&Value::String("result".to_string())) {
                assert_eq!(result_value, "db.example.com");
            } else {
                panic!("Expected result to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_query_selector_multiple_properties() -> Result<()> {
        let yaml_input = r#"
$defs:
  config:
    database: "db.example.com"
    cache: "cache.example.com"
    storage: "storage.example.com"

result: !$ config?database,cache
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                // Should contain only database and cache, not storage
                assert!(result_map.contains_key(&Value::String("database".to_string())));
                assert!(result_map.contains_key(&Value::String("cache".to_string())));
                assert!(!result_map.contains_key(&Value::String("storage".to_string())));
                
                assert_eq!(result_map.get(&Value::String("database".to_string())), Some(&Value::String("db.example.com".to_string())));
                assert_eq!(result_map.get(&Value::String("cache".to_string())), Some(&Value::String("cache.example.com".to_string())));
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_query_selector_nested_path() -> Result<()> {
        let yaml_input = r#"
$defs:
  config:
    database:
      host: "db.example.com"
      port: 5432
    cache:
      host: "cache.example.com"
      port: 6379

result: !$ config?.database.host
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::String(result_value)) = map.get(&Value::String("result".to_string())) {
                assert_eq!(result_value, "db.example.com");
            } else {
                panic!("Expected result to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_explicit_query_parameter() -> Result<()> {
        let yaml_input = r#"
$defs:
  config:
    database: "db.example.com"
    cache: "cache.example.com"
    storage: "storage.example.com"

result: !$
  path: config
  query: "database,cache"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(map) = result {
            if let Some(Value::Mapping(result_map)) = map.get(&Value::String("result".to_string())) {
                // Should contain only database and cache, not storage
                assert!(result_map.contains_key(&Value::String("database".to_string())));
                assert!(result_map.contains_key(&Value::String("cache".to_string())));
                assert!(!result_map.contains_key(&Value::String("storage".to_string())));
            } else {
                panic!("Expected result to be a mapping");
            }
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_enhanced_error_handling_infrastructure() -> Result<()> {
        // Test that we can create ProcessingEnv and TagContext with stack frames
        use crate::yaml::tags::{ProcessingEnv, TagContext, StackFrame};

        // Test ProcessingEnv creation and usage
        let mut env = ProcessingEnv::new();
        env.add_variable("test_var".to_string(), Value::String("test_value".to_string()));
        
        let sub_env = env.mk_sub_env(
            [("new_var".to_string(), Value::String("new_value".to_string()))].into(),
            StackFrame {
                location: Some("test.yaml".to_string()),
                path: "Root.config".to_string(),
            }
        );
        
        assert_eq!(sub_env.get_variable("test_var"), Some(&Value::String("test_value".to_string())));
        assert_eq!(sub_env.get_variable("new_var"), Some(&Value::String("new_value".to_string())));
        assert_eq!(sub_env.current_location(), Some("test.yaml".to_string()));
        assert_eq!(sub_env.current_path(), "Root.config");
        
        // Test CloudFormation environment
        let cfn_env = ProcessingEnv::new_with_cfn_accumulator();
        assert!(cfn_env.global_accumulator.is_some());
        
        // Test TagContext integration
        let context = TagContext::from_processing_env(&sub_env);
        assert_eq!(context.get_variable("test_var"), Some(&Value::String("test_value".to_string())));
        assert_eq!(context.current_location(), Some("test.yaml".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_successful_stack_frame_context() -> Result<()> {
        use crate::yaml::tags::{TagContext, StackFrame};

        let yaml_input = r#"
$defs:
  config:
    database: "db.example.com"

result: !$ config.database
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        // Should succeed and resolve the include
        if let Value::Mapping(map) = result {
            if let Some(Value::String(result_value)) = map.get(&Value::String("result".to_string())) {
                assert_eq!(result_value, "db.example.com");
            } else {
                panic!("Expected result to be resolved");
            }
        } else {
            panic!("Expected a mapping result");
        }

        // Test TagContext with stack frames
        let context = TagContext::new()
            .with_variable("test_var", Value::String("test_value".to_string()))
            .with_stack_frame(StackFrame {
                location: Some("test.yaml".to_string()),
                path: "Root.config".to_string(),
            });

        assert_eq!(context.current_location(), Some("test.yaml".to_string()));
        assert_eq!(context.current_path(), "Root.config");
        assert_eq!(context.get_variable("test_var"), Some(&Value::String("test_value".to_string())));

        Ok(())
    }

    #[tokio::test]
    async fn test_direct_handlebars_processing() -> Result<()> {
        // Test handlebars processing directly
        use crate::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;
        
        let mut variables = HashMap::new();
        variables.insert("environment".to_string(), serde_json::Value::String("production".to_string()));
        
        // Test different patterns
        let result1 = interpolate_handlebars_string("${{environment}}", &variables, "test")?;
        let result2 = interpolate_handlebars_string("${{{environment}}}", &variables, "test")?;
        let result3 = interpolate_handlebars_string("{{environment}}", &variables, "test")?;
        
        // Based on actual handlebars behavior:
        assert_eq!(result1, "$production");  // ${{env}} becomes $production  
        assert_eq!(result2, "$production");  // ${{{env}}} also becomes $production
        assert_eq!(result3, "production");   // {{env}} becomes production
        
        Ok(())
    }

    #[tokio::test]
    async fn test_handlebars_with_cloudformation_syntax() -> Result<()> {
        // Test that handlebars processing correctly handles CloudFormation ${} syntax
        let yaml_input = r#"
$defs:
  environment: production

test_values:
  simple: "{{environment}}"
  cf_syntax_correct: "${{environment}}"
  cf_syntax_triple: "${{{environment}}}"
  mixed: "prefix-${{environment}}-suffix"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        if let Value::Mapping(root) = &result {
            if let Some(Value::Mapping(test_values)) = root.get(&Value::String("test_values".to_string())) {
                let simple = test_values.get(&Value::String("simple".to_string()));
                assert_eq!(simple, Some(&Value::String("production".to_string())));
                
                let cf_syntax_correct = test_values.get(&Value::String("cf_syntax_correct".to_string()));
                // Based on actual handlebars behavior: both become $production
                assert_eq!(cf_syntax_correct, Some(&Value::String("$production".to_string())));
                
                let cf_syntax_triple = test_values.get(&Value::String("cf_syntax_triple".to_string()));
                assert_eq!(cf_syntax_triple, Some(&Value::String("$production".to_string())));
                
                let mixed = test_values.get(&Value::String("mixed".to_string()));
                assert_eq!(mixed, Some(&Value::String("prefix-$production-suffix".to_string())));
            }
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_preprocessing_directives_stripped_from_output() -> Result<()> {
        // Test that $imports, $defs, and $envValues are removed from final output
        let yaml_input = r#"
$defs:
  environment: "production"
  region: "us-west-2"

name: "test-{{environment}}"
region: "{{region}}"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        // Verify that preprocessing directives are not in the output
        if let Value::Mapping(map) = &result {
            assert!(!map.contains_key(&Value::String("$defs".to_string())));
            assert!(!map.contains_key(&Value::String("$imports".to_string())));
            assert!(!map.contains_key(&Value::String("$envValues".to_string())));
            
            // But processed values should be present
            assert_eq!(map.get(&Value::String("name".to_string())), Some(&Value::String("test-production".to_string())));
            assert_eq!(map.get(&Value::String("region".to_string())), Some(&Value::String("us-west-2".to_string())));
        } else {
            panic!("Expected a mapping result");
        }

        Ok(())
    }

    // TODO: Add tests for nested import processing and environment isolation
    // These tests are currently failing due to issues with import processing
    // and will be implemented in a separate commit once the underlying issues are fixed

    #[tokio::test]
    async fn test_cloudformation_tag_preservation_with_preprocessing() -> Result<()> {
        // Test that handlebars preprocessing works inside CloudFormation tags
        // This matches the behavior tested in iidy-js test-yaml-preprocessing.ts:219-247
        let yaml_input = r#"
$defs:
  environment: production
  param: MyParameter

Resources:
  MyBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !Sub "${{environment}}-my-bucket"
      Tags:
        - Key: Environment
          Value: !Ref "{{param}}"
        - Key: Name
          Value: !GetAtt "SomeResource.{{param}}"
"#;

        let loader = ProductionImportLoader::new();
        let mut preprocessor = YamlPreprocessor::new(loader);
        let result = preprocessor.process(yaml_input, "test.yaml").await?;

        // Verify that CloudFormation tags are preserved with processed content
        if let Value::Mapping(root) = &result {
            if let Some(Value::Mapping(resources)) = root.get(&Value::String("Resources".to_string())) {
                if let Some(Value::Mapping(bucket)) = resources.get(&Value::String("MyBucket".to_string())) {
                    if let Some(Value::Mapping(properties)) = bucket.get(&Value::String("Properties".to_string())) {
                        // Check that !Sub tag is preserved with processed handlebars
                        if let Some(bucket_name) = properties.get(&Value::String("BucketName".to_string())) {
                            if let Value::Tagged(tagged) = bucket_name {
                                assert_eq!(tagged.tag.to_string(), "!Sub");
                                // The handlebars {{environment}} should be processed to "production"  
                                if let Value::String(value) = &tagged.value {
                                    assert_eq!(value, "$production-my-bucket");
                                } else {
                                    panic!("Expected tagged value to be a string");
                                }
                            } else {
                                panic!("Expected !Sub to be preserved as tagged value");
                            }
                        }

                        // Check that !Ref tag preserves content with processed handlebars
                        if let Some(Value::Sequence(tags)) = properties.get(&Value::String("Tags".to_string())) {
                            if tags.len() >= 2 {
                                if let Value::Mapping(env_tag) = &tags[0] {
                                    if let Some(Value::Tagged(ref_tagged)) = env_tag.get(&Value::String("Value".to_string())) {
                                        assert_eq!(ref_tagged.tag.to_string(), "!Ref");
                                        // The handlebars {{param}} should be processed to "MyParameter"
                                        if let Value::String(ref_value) = &ref_tagged.value {
                                            assert_eq!(ref_value, "MyParameter");
                                        } else {
                                            panic!("Expected Ref tagged value to be a string");
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            panic!("Expected root to be a mapping");
        }

        Ok(())
    }
}
