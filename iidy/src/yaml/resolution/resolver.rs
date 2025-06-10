//! Tag resolution and processing for YAML preprocessing
//! 
//! Contains the implementation logic for each custom preprocessing tag and
//! base path derivation for relative imports.
//!
//! # Base Path Derivation for Relative Imports
//!
//! The `derive_base_path_from_location()` function derives base paths to enable
//! relative imports across different contexts:
//!
//! ## Local File Paths
//! - `/Users/app/configs/main.yaml` → `/Users/app/configs/`
//! - `./configs/app.yaml` → `./configs/`
//! - `config.yaml` → `` (empty - current directory)
//!
//! ## S3 URLs
//! - `s3://bucket/file.yaml` → `s3://bucket/`
//! - `s3://bucket/configs/app.yaml` → `s3://bucket/configs/`
//! - `s3://bucket/configs/env/prod.yaml` → `s3://bucket/configs/env/`
//!
//! ## HTTP/HTTPS URLs
//! - `https://example.com/file.yaml` → `https://example.com/`
//! - `https://example.com/configs/app.yaml` → `https://example.com/configs/`
//! - `http://api.com/templates/base.yaml` → `http://api.com/templates/`
//!
//! This enables relative imports to work correctly:
//! ```yaml
//! # From s3://bucket/configs/app.yaml
//! Resources:
//!   Database: !$ database.config  # Resolves to s3://bucket/configs/database.yaml
//! ```
//!
//! The base path derivation respects the security model - see the `imports` module
//! documentation for details on import type restrictions.

use anyhow::{anyhow, Context, Result};
use serde_yaml::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::yaml::parsing::ast::*;

/// Performance optimization: Global counter for scope ID generation
static SCOPE_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Generate unique scope ID without expensive UUID generation
fn next_scope_id() -> usize {
    SCOPE_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Create a small HashMap with pre-allocated capacity for common cases
#[inline(always)]
fn small_hashmap<K, V>() -> HashMap<K, V> {
    HashMap::with_capacity(2)
}

/// Fast path optimization helpers for AST processing
/// 
/// These helpers enable fast paths for common simple values that don't require
/// complex processing (no handlebars, preprocessing tags, etc.)

/// Check if an AST value is simple (no processing needed)
#[inline(always)]
fn is_simple_ast_value(ast: &YamlAst) -> bool {
    match ast {
        YamlAst::Null | YamlAst::Bool(_) | YamlAst::Number(_) => true,
        YamlAst::PlainString(_) => true, // All plain strings are simple
        YamlAst::TemplatedString(_) => false, // Templated strings are never simple
        _ => false,
    }
}

/// Check if all items in a sequence are simple values
#[inline(always)]
fn is_simple_sequence(seq: &[YamlAst]) -> bool {
    seq.iter().all(is_simple_ast_value)
}

/// Check if a mapping contains only simple string keys and simple values
/// Excludes preprocessing directive keys (starting with '$')
#[inline(always)]
fn is_simple_mapping(pairs: &[(YamlAst, YamlAst)]) -> bool {
    pairs.iter().all(|(key, value)| {
        match key {
            YamlAst::PlainString(s) | YamlAst::TemplatedString(s) if !s.starts_with('$') => is_simple_ast_value(value),
            _ => false,
        }
    })
}

/// Convert a simple AST value directly to serde_yaml::Value
/// Panics if called on non-simple AST (should be checked with is_simple_ast_value first)
#[inline(always)]
fn simple_ast_to_value(ast: &YamlAst) -> Value {
    match ast {
        YamlAst::Null => Value::Null,
        YamlAst::Bool(b) => Value::Bool(*b),
        YamlAst::Number(n) => Value::Number(n.clone()),
        YamlAst::PlainString(s) | YamlAst::TemplatedString(s) => Value::String(s.clone()),
        _ => unreachable!("simple_ast_to_value called on non-simple AST"),
    }
}


/// Enhanced error types for preprocessing with stack frame context
#[derive(thiserror::Error, Debug)]
pub enum PreprocessError {
    #[error("Could not find '{key}' at {path}")]
    VariableNotFound { key: String, path: String },
    
    #[error("Include path '{path}' not found\n  at {location}")]
    IncludeNotFound { path: String, location: String },
    
    #[error("Invalid template parameter '{param}' in {context}\n  at {location}")]
    ParameterValidation { param: String, context: String, location: String },
    
    #[error("Import error: {message}\n  importing {import_location}\n  from {base_location}")]
    ImportError { message: String, import_location: String, base_location: String },
    
    #[error("Tag resolution error: {message}\n  at {path}\n  in {location}")]
    TagResolutionError { message: String, path: String, location: String },
}

/// Helper trait for adding stack frame context to errors
pub trait WithStackContext<T> {
    fn with_stack_context(self, context: &TagContext, operation: &str) -> Result<T>;
}

impl<T> WithStackContext<T> for Result<T> {
    fn with_stack_context(self, context: &TagContext, operation: &str) -> Result<T> {
        self.with_context(|| {
            let current_path = context.current_path();
            let current_location = context.current_location().unwrap_or_else(|| "unknown".to_string());
            format!("{} at {} in {}", operation, current_path, current_location)
        })
    }
}

/// Stack frame for error reporting and debugging (matches iidy-js StackFrame)
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Location of the operation (file path or description) - optional like iidy-js
    pub location: Option<String>,
    /// Path within the document (e.g., "config.database.host")
    pub path: String,
}

/// Global accumulator for document-wide state (optional, not all docs need this)
#[derive(Debug, Clone, Default)]
pub struct GlobalAccumulator {
    /// CloudFormation global sections (Parameters, Outputs, etc.) if processing CFN templates
    pub cfn_sections: Option<serde_yaml::Mapping>,
    /// Other document-wide accumulations as needed
    pub metadata: serde_yaml::Mapping,
}


/// Context for resolving preprocessing tags
#[derive(Debug, Default)]
pub struct TagContext {
    /// Variable bindings for current scope
    pub variables: HashMap<String, Value>,
    /// URI of the input document being processed (for error reporting and relative imports)
    pub input_uri: Option<String>,
    /// Stack frames for error reporting
    pub stack: Vec<StackFrame>,
    /// Global accumulator (optional - only used for CloudFormation templates or docs that need it)
    pub global_accumulator: Option<GlobalAccumulator>,
    /// Enhanced scope tracking (optional - for advanced variable origin tracking)
    pub scope_context: Option<ScopeContext>,
}

/// Enhanced scope system for tracking variable origins and hierarchical scopes
#[derive(Debug, Clone, PartialEq)]
pub struct ScopeContext {
    /// Current active scope
    pub current_scope: Scope,
    /// All scopes by ID for cross-references
    pub scopes: HashMap<ScopeId, Scope>,
    /// Stack of scope IDs showing nesting hierarchy
    pub scope_stack: Vec<ScopeId>,
}

/// Unique identifier for a scope
pub type ScopeId = String;

/// Represents a variable scope with hierarchical parent relationships
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    /// Unique identifier for this scope
    pub id: ScopeId,
    /// Type of scope (document, import, tag execution, etc.)
    pub scope_type: ScopeType,
    /// URI of the document this scope belongs to
    pub source_uri: Option<String>,
    /// Variables defined in this scope with origin tracking
    pub variables: HashMap<String, ScopedVariable>,
    /// Parent scope ID (for hierarchical resolution)
    pub parent_scope_id: Option<ScopeId>,
    /// Child scope IDs (for debugging and visualization)
    pub child_scope_ids: Vec<ScopeId>,
}

/// Types of scopes in the variable resolution system
#[derive(Debug, Clone, PartialEq)]
pub enum ScopeType {
    /// Global/root document scope
    Global,
    /// Local definitions within a document ($defs)
    LocalDefs,
    /// Imported document scope (contains import key)
    ImportedDocument(String),
    /// Tag execution scope (e.g., within !$let or !$map)
    TagExecution(String),
    /// Built-in system variables
    BuiltIn,
}

/// Variable with origin and metadata tracking
#[derive(Debug, Clone, PartialEq)]
pub struct ScopedVariable {
    /// The variable's value
    pub value: Value,
    /// Source information for this variable
    pub source: VariableSource,
    /// File/location where this variable was defined
    pub defined_at: Option<String>,
    /// Line number where defined (if available)
    pub line_number: Option<usize>,
    /// Column number where defined (if available)
    pub column_number: Option<usize>,
}

/// Source type for a variable
#[derive(Debug, Clone, PartialEq)]
pub enum VariableSource {
    /// From $defs in the local document
    LocalDefs,
    /// From an imported document (includes import key)
    ImportedDocument(String),
    /// From tag binding (e.g., !$let variable: value)
    TagBinding(String),
    /// Built-in system variable
    BuiltIn,
    /// Command line or environment variable
    External,
}

/// Derive base path from a file location
/// Supports local file paths, S3 URLs, and HTTP/HTTPS URLs
pub fn derive_base_path_from_location(location: &str) -> Option<std::path::PathBuf> {
    // Check if it's a URL by looking for common URL schemes
    if location.starts_with("http://") || location.starts_with("https://") || location.starts_with("s3://") {
        // Parse as URL
        if let Ok(url) = url::Url::parse(location) {
            match url.scheme() {
                "s3" => {
                    // For S3 URLs: s3://bucket/path/file.yaml -> s3://bucket/path/
                    // For root files: s3://bucket/file.yaml -> s3://bucket/
                    let path = url.path();
                    if let Some(last_slash) = path.rfind('/') {
                        let dir_path = &path[..last_slash + 1];
                        let base_url = format!("s3://{}{}", url.host_str().unwrap_or(""), dir_path);
                        return Some(std::path::PathBuf::from(base_url));
                    }
                    // If no slash found, return bucket root
                    let base_url = format!("s3://{}/", url.host_str().unwrap_or(""));
                    Some(std::path::PathBuf::from(base_url))
                }
                "http" | "https" => {
                    // For HTTP URLs: https://example.com/configs/app/file.yaml -> https://example.com/configs/app/
                    // For root files: https://example.com/file.yaml -> https://example.com/
                    let path = url.path();
                    if let Some(last_slash) = path.rfind('/') {
                        let dir_path = &path[..last_slash + 1];
                        let mut base_url = url.clone();
                        base_url.set_path(dir_path);
                        return Some(std::path::PathBuf::from(base_url.as_str()));
                    }
                    // If no slash found, return domain root
                    let mut base_url = url.clone();
                    base_url.set_path("/");
                    Some(std::path::PathBuf::from(base_url.as_str()))
                }
                _ => None // Unknown URL scheme
            }
        } else {
            None // Failed to parse URL
        }
    } else {
        // Treat as local file path and use proper path parsing
        let path = std::path::Path::new(location);
        path.parent().map(|p| p.to_path_buf())
    }
}

impl TagContext {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create TagContext with input URI for testing
    /// Useful for testing and when you need to set the input URI
    #[cfg(test)]
    pub fn from_file_location(location: &str) -> Self {
        Self {
            variables: HashMap::new(),
            input_uri: Some(location.to_string()),
            stack: vec![StackFrame {
                location: Some(location.to_string()),
                path: "Root".to_string(),
            }],
            global_accumulator: None,
            scope_context: None,
        }
    }
    
    /// Create TagContext with both input URI and variables for comprehensive testing
    #[cfg(test)]
    pub fn from_location_and_vars(location: &str, variables: HashMap<String, Value>) -> Self {
        Self {
            variables,
            input_uri: Some(location.to_string()),
            stack: vec![StackFrame {
                location: Some(location.to_string()),
                path: "Root".to_string(),
            }],
            global_accumulator: None,
            scope_context: None,
        }
    }
    
    /// Create TagContext with CloudFormation global accumulator
    pub fn new_with_cfn_accumulator() -> Self {
        Self {
            variables: HashMap::new(),
            input_uri: None,
            stack: Vec::new(),
            global_accumulator: Some(GlobalAccumulator::default()),
            scope_context: None,
        }
    }

    /// Create a new context with additional variable bindings
    pub fn with_bindings(&self, bindings: HashMap<String, Value>) -> Self {
        let mut new_vars = self.variables.clone();
        new_vars.extend(bindings);
        Self {
            variables: new_vars,
            input_uri: self.input_uri.clone(),
            stack: self.stack.clone(),
            global_accumulator: self.global_accumulator.clone(),
            scope_context: self.scope_context.clone(),
        }
    }
    
    /// Create a new context with additional variable bindings (optimized for references)
    pub fn with_bindings_ref(&self, bindings: &HashMap<String, Value>) -> Self {
        // For small binding sets, the original extend approach is faster
        if bindings.len() <= 2 && self.variables.len() <= 10 {
            let mut new_vars = self.variables.clone();
            new_vars.extend(bindings.iter().map(|(k, v)| (k.clone(), v.clone())));
            Self {
                variables: new_vars,
                input_uri: self.input_uri.clone(),
                stack: self.stack.clone(),
                global_accumulator: self.global_accumulator.clone(),
                scope_context: self.scope_context.clone(),
            }
        } else {
            let mut new_vars = HashMap::with_capacity(self.variables.len() + bindings.len());
            new_vars.extend(self.variables.iter().map(|(k, v)| (k.clone(), v.clone())));
            new_vars.extend(bindings.iter().map(|(k, v)| (k.clone(), v.clone())));
            Self {
                variables: new_vars,
                input_uri: self.input_uri.clone(),
                stack: self.stack.clone(),
                global_accumulator: self.global_accumulator.clone(),
                scope_context: self.scope_context.clone(),
            }
        }
    }

    /// Get a variable value by name
    pub fn get_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    /// Add a variable to the context
    pub fn with_variable(mut self, name: &str, value: Value) -> Self {
        self.variables.insert(name.to_string(), value);
        self
    }

    /// Set input URI for the context
    pub fn with_input_uri(mut self, uri: String) -> Self {
        self.input_uri = Some(uri);
        self
    }
    
    /// Add a stack frame for error reporting
    pub fn with_stack_frame(mut self, frame: StackFrame) -> Self {
        self.stack.push(frame);
        self
    }
    
    /// Get current location from stack
    pub fn current_location(&self) -> Option<String> {
        self.stack.last().and_then(|f| f.location.clone())
    }
    
    /// Get current path from stack
    pub fn current_path(&self) -> String {
        self.stack.last().map(|f| f.path.clone()).unwrap_or_default()
    }
    
    /// Create a new context with an extended path for nested document traversal
    /// Optimized to minimize expensive cloning operations
    pub fn with_path_segment(&self, segment: &str) -> Self {
        let new_path = match self.stack.last() {
            Some(frame) if !frame.path.is_empty() => {
                let mut path = String::with_capacity(frame.path.len() + 1 + segment.len());
                path.push_str(&frame.path);
                path.push('.');
                path.push_str(segment);
                path
            }
            _ => segment.to_string()
        };
        
        // PERFORMANCE: Minimize stack cloning by reusing when possible
        let mut new_stack = self.stack.clone();
        if let Some(last_frame) = new_stack.last_mut() {
            last_frame.path = new_path;
        } else {
            new_stack.push(StackFrame {
                location: self.current_location(),
                path: new_path,
            });
        }
        
        // PERFORMANCE: Conditional cloning - only clone scope context if actually used
        Self {
            variables: self.variables.clone(),
            input_uri: self.input_uri.clone(),
            stack: new_stack,
            global_accumulator: self.global_accumulator.clone(),
            scope_context: if self.scope_context.is_some() { 
                // Only clone if actually being used
                self.scope_context.clone() 
            } else { 
                None 
            },
        }
    }
    
    /// Create a new context with an array index path segment  
    /// Optimized to minimize expensive cloning operations
    pub fn with_array_index(&self, index: usize) -> Self {
        let current_path = self.current_path();
        let new_path = if current_path.is_empty() {
            format!("[{}]", index)
        } else {
            format!("{}[{}]", current_path, index)
        };
        
        let mut new_stack = self.stack.clone();
        if let Some(last_frame) = new_stack.last_mut() {
            last_frame.path = new_path;
        } else {
            new_stack.push(StackFrame {
                location: self.current_location(),
                path: new_path,
            });
        }
        
        // PERFORMANCE: Conditional cloning - only clone scope context if actually used
        Self {
            variables: self.variables.clone(),
            input_uri: self.input_uri.clone(),
            stack: new_stack,
            global_accumulator: self.global_accumulator.clone(),
            scope_context: if self.scope_context.is_some() { 
                // Only clone if actually being used
                self.scope_context.clone() 
            } else { 
                None 
            },
        }
    }
    
    /// Create a new TagContext with enhanced scope tracking
    pub fn with_scope_tracking(input_uri: String) -> Self {
        let scope_id = format!("global_{}", next_scope_id());
        let global_scope = Scope {
            id: scope_id.clone(),
            scope_type: ScopeType::Global,
            source_uri: Some(input_uri.clone()),
            variables: HashMap::new(),
            parent_scope_id: None,
            child_scope_ids: Vec::new(),
        };
        
        let mut scopes = HashMap::new();
        scopes.insert(scope_id.clone(), global_scope.clone());
        
        let scope_context = Some(ScopeContext {
            current_scope: global_scope,
            scopes,
            scope_stack: vec![scope_id],
        });
        
        Self {
            variables: HashMap::new(),
            input_uri: Some(input_uri),
            stack: Vec::new(),
            global_accumulator: None,
            scope_context,
        }
    }
    
    /// Add a variable with scope tracking
    pub fn add_scoped_variable(
        &mut self, 
        name: &str, 
        value: Value, 
        source: VariableSource,
        defined_at: Option<String>,
    ) {
        // Add to legacy variables for backward compatibility
        self.variables.insert(name.to_string(), value.clone());
        
        // Add to scope system if enabled
        if let Some(ref mut scope_context) = self.scope_context {
            let scoped_var = ScopedVariable {
                value,
                source,
                defined_at,
                line_number: None,
                column_number: None,
            };
            
            scope_context.current_scope.variables.insert(name.to_string(), scoped_var.clone());
            
            // Update the scope in the scopes map
            let scope_id = scope_context.current_scope.id.clone();
            scope_context.scopes.insert(scope_id, scope_context.current_scope.clone());
        }
    }
    
    /// Resolve a variable with full origin information (test helper)
    #[doc(hidden)]
    pub fn resolve_scoped_variable(&self, name: &str) -> Option<&ScopedVariable> {
        if let Some(ref scope_context) = self.scope_context {
            // First check current scope
            if let Some(var) = scope_context.current_scope.variables.get(name) {
                return Some(var);
            }
            
            // Walk up the scope hierarchy
            let mut current_scope_id = scope_context.current_scope.parent_scope_id.as_ref();
            while let Some(scope_id) = current_scope_id {
                if let Some(scope) = scope_context.scopes.get(scope_id) {
                    if let Some(var) = scope.variables.get(name) {
                        return Some(var);
                    }
                    current_scope_id = scope.parent_scope_id.as_ref();
                } else {
                    break;
                }
            }
        }
        None
    }
    
    /// Get human-readable variable origin description (test helper)
    #[doc(hidden)]
    pub fn get_variable_origin(&self, name: &str) -> Option<String> {
        if let Some(var) = self.resolve_scoped_variable(name) {
            match &var.source {
                VariableSource::LocalDefs => Some("local $defs".to_string()),
                VariableSource::ImportedDocument(key) => Some(format!("imported from '{}'", key)),
                VariableSource::TagBinding(tag) => Some(format!("bound in {}", tag)),
                VariableSource::BuiltIn => Some("built-in".to_string()),
                VariableSource::External => Some("external".to_string()),
            }
        } else {
            None
        }
    }
    
    /// Create a new import scope (test helper)
    #[doc(hidden)]
    pub fn create_import_scope(&mut self, import_key: &str, source_uri: &str) -> &mut Self {
        if let Some(ref mut scope_context) = self.scope_context {
            let scope_id = format!("import_{}_{}", import_key, next_scope_id());
            let parent_id = scope_context.current_scope.id.clone();
            
            let import_scope = Scope {
                id: scope_id.clone(),
                scope_type: ScopeType::ImportedDocument(import_key.to_string()),
                source_uri: Some(source_uri.to_string()),
                variables: HashMap::new(),
                parent_scope_id: Some(parent_id.clone()),
                child_scope_ids: Vec::new(),
            };
            
            // Add child reference to parent
            if let Some(parent_scope) = scope_context.scopes.get_mut(&parent_id) {
                parent_scope.child_scope_ids.push(scope_id.clone());
            }
            
            // Add new scope to scopes map
            scope_context.scopes.insert(scope_id.clone(), import_scope.clone());
            
            // Update current scope and stack
            scope_context.current_scope = import_scope;
            scope_context.scope_stack.push(scope_id);
        }
        self
    }
    
    /// Pop current scope back to parent (test helper)
    #[doc(hidden)]
    pub fn pop_scope(&mut self) {
        if let Some(ref mut scope_context) = self.scope_context {
            if scope_context.scope_stack.len() > 1 {
                scope_context.scope_stack.pop();
                if let Some(parent_scope_id) = scope_context.scope_stack.last() {
                    if let Some(parent_scope) = scope_context.scopes.get(parent_scope_id) {
                        scope_context.current_scope = parent_scope.clone();
                    }
                }
            }
        }
    }
    
    /// Get import dependency graph for debugging (test helper)
    #[doc(hidden)]
    pub fn get_import_dependency_graph(&self) -> HashMap<String, Vec<String>> {
        let mut deps = HashMap::new();
        
        if let Some(ref scope_context) = self.scope_context {
            for (_scope_id, scope) in &scope_context.scopes {
                if let Some(ref source_uri) = scope.source_uri {
                    let children: Vec<String> = scope.child_scope_ids
                        .iter()
                        .filter_map(|child_id| {
                            scope_context.scopes.get(child_id)
                                .and_then(|child| child.source_uri.clone())
                        })
                        .collect();
                    
                    if !children.is_empty() {
                        deps.insert(source_uri.clone(), children);
                    }
                }
            }
        }
        
        deps
    }
}

impl ScopeContext {
    /// Create a new scope context with global scope
    pub fn new(source_uri: String) -> Self {
        let scope_id = format!("global_{}", next_scope_id());
        let global_scope = Scope {
            id: scope_id.clone(),
            scope_type: ScopeType::Global,
            source_uri: Some(source_uri),
            variables: HashMap::new(),
            parent_scope_id: None,
            child_scope_ids: Vec::new(),
        };
        
        let mut scopes = HashMap::new();
        scopes.insert(scope_id.clone(), global_scope.clone());
        
        Self {
            current_scope: global_scope,
            scopes,
            scope_stack: vec![scope_id],
        }
    }
}

impl Scope {
    /// Create a new scope
    pub fn new(scope_type: ScopeType, source_uri: Option<String>) -> Self {
        let type_prefix = match &scope_type {
            ScopeType::Global => "global".to_string(),
            ScopeType::LocalDefs => "defs".to_string(),
            ScopeType::ImportedDocument(key) => format!("import_{}", key),
            ScopeType::TagExecution(tag) => format!("tag_{}", tag),
            ScopeType::BuiltIn => "builtin".to_string(),
        };
        
        let id = format!("{}_{}", type_prefix, next_scope_id());
        
        Self {
            id,
            scope_type,
            source_uri,
            variables: HashMap::new(),
            parent_scope_id: None,
            child_scope_ids: Vec::new(),
        }
    }
}

impl ScopedVariable {
    /// Create a new scoped variable
    pub fn new(value: Value, source: VariableSource, defined_at: Option<String>) -> Self {
        Self {
            value,
            source,
            defined_at,
            line_number: None,
            column_number: None,
        }
    }
}


/// Parse path and query from include path
/// Supports formats like "config?database" or "config?database,host"
fn parse_path_and_query(path: &str, explicit_query: &Option<String>) -> (String, Option<String>) {
    // If there's an explicit query in the tag, use that
    if let Some(q) = explicit_query {
        return (path.to_string(), Some(q.clone()));
    }
    
    // Otherwise, check if path contains query syntax
    if let Some(query_index) = path.find('?') {
        let base_path = path[..query_index].to_string();
        let query = path[query_index + 1..].to_string();
        (base_path, Some(query))
    } else {
        (path.to_string(), None)
    }
}


/// Apply query selector to a value
/// Supported query formats:
/// - "property" - select single property
/// - "prop1,prop2" - select multiple properties  
/// - ".nested.path" - select nested property (same as dot notation)
fn apply_query_selector(value: &Value, query: &str) -> Result<Value> {
    match value {
        Value::Mapping(map) => {
            if query.starts_with('.') {
                // Handle nested path query like ".database.host"
                let path = &query[1..]; // Remove leading dot
                apply_nested_path_query(value, path)
            } else if query.contains(',') {
                // Handle multiple property selection like "database,host"
                let properties: Vec<&str> = query.split(',').map(|s| s.trim()).collect();
                let mut result = serde_yaml::Mapping::with_capacity(properties.len());
                
                for prop in properties {
                    if let Some(prop_value) = map.get(&Value::String(prop.to_string())) {
                        result.insert(Value::String(prop.to_string()), prop_value.clone());
                    }
                }
                
                Ok(Value::Mapping(result))
            } else {
                // Handle single property selection like "database"
                if let Some(prop_value) = map.get(&Value::String(query.to_string())) {
                    Ok(prop_value.clone())
                } else {
                    Err(anyhow!("Property '{}' not found in mapping", query))
                }
            }
        }
        _ => Err(anyhow!("Query selectors can only be applied to mappings"))
    }
}


/// Apply nested path query to a value
fn apply_nested_path_query(value: &Value, path: &str) -> Result<Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current_value = value;
    
    for part in parts {
        if part.is_empty() {
            continue;
        }
        
        match current_value {
            Value::Mapping(map) => {
                let key = Value::String(part.to_string());
                if let Some(next_value) = map.get(&key) {
                    current_value = next_value;
                } else {
                    return Err(anyhow!("Property '{}' not found in path", part));
                }
            }
            _ => return Err(anyhow!("Cannot traverse path further at '{}'", part)),
        }
    }
    
    Ok(current_value.clone())
}

/// Resolve dot notation path in variables (e.g., "config.database_host")
/// Also supports bracket notation (e.g., "config[environment]", "config['literal']")
fn resolve_dot_notation_path(path: &str, context: &TagContext) -> Option<Value> {
    // Parse the path to handle both dot notation and bracket notation
    let path_segments = parse_path_segments(path, context)?;
    
    if path_segments.is_empty() {
        return None;
    }
    
    // Start with the root variable
    let root_var = &path_segments[0];
    let mut current_value = context.get_variable(root_var)?;
    
    // Traverse the path segments using references until the end
    for segment in &path_segments[1..] {
        match current_value {
            Value::Mapping(map) => {
                let key = Value::String(segment.clone());
                current_value = map.get(&key)?;
            }
            _ => return None, // Can't traverse further
        }
    }
    
    // Only clone at the final step
    Some(current_value.clone())
}

/// Parse path segments handling both dot notation and bracket notation
/// Examples:
/// - "config.database_host" -> ["config", "database_host"]
/// - "config[environment]" -> ["config", "prod"] (if environment="prod")
/// - "config['literal']" -> ["config", "literal"]
/// - "config[env.stage]" -> ["config", "production"] (if env.stage="production")
fn parse_path_segments(path: &str, context: &TagContext) -> Option<Vec<String>> {
    let mut segments = Vec::new();
    let mut current_segment = String::new();
    let mut chars = path.chars().peekable();
    
    while let Some(ch) = chars.next() {
        match ch {
            '.' => {
                if !current_segment.is_empty() {
                    segments.push(current_segment.clone());
                    current_segment.clear();
                }
            }
            '[' => {
                // End current segment if any
                if !current_segment.is_empty() {
                    segments.push(current_segment.clone());
                    current_segment.clear();
                }
                
                // Parse bracket content
                let bracket_content = parse_bracket_content(&mut chars, context)?;
                segments.push(bracket_content);
            }
            _ => {
                current_segment.push(ch);
            }
        }
    }
    
    // Add final segment if any
    if !current_segment.is_empty() {
        segments.push(current_segment);
    }
    
    if segments.is_empty() {
        None
    } else {
        Some(segments)
    }
}


/// Parse the content inside brackets and resolve it
/// Supports:
/// - Variable references: [environment] -> resolves variable "environment"
/// - String literals: ['literal'] or ["literal"] -> returns "literal"
/// - Nested paths: [env.stage] -> resolves "env.stage" as a path
fn parse_bracket_content(chars: &mut std::iter::Peekable<std::str::Chars>, context: &TagContext) -> Option<String> {
    let mut bracket_content = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';
    let mut was_quoted = false;
    
    while let Some(ch) = chars.next() {
        match ch {
            ']' if !in_quotes => {
                break;
            }
            '\'' | '"' if !in_quotes => {
                in_quotes = true;
                was_quoted = true;
                quote_char = ch;
                // Don't include the opening quote in the content
            }
            c if in_quotes && c == quote_char => {
                in_quotes = false;
                // Don't include the closing quote in the content
            }
            _ => {
                bracket_content.push(ch);
            }
        }
    }
    
    if bracket_content.is_empty() {
        return None;
    }
    
    // If it was quoted, return the literal string
    if was_quoted {
        return Some(bracket_content);
    }
    
    // Otherwise, try to resolve as a variable or path
    if bracket_content.contains('.') {
        // It's a nested path reference
        if let Some(resolved_value) = resolve_dot_notation_path(&bracket_content, context) {
            match resolved_value {
                Value::String(s) => Some(s),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None, // Can't use complex types as keys
            }
        } else {
            None
        }
    } else {
        // It's a simple variable reference
        if let Some(var_value) = context.get_variable(&bracket_content) {
            match var_value {
                Value::String(s) => Some(s.clone()),
                Value::Number(n) => Some(n.to_string()),
                Value::Bool(b) => Some(b.to_string()),
                _ => None, // Can't use complex types as keys
            }
        } else {
            None
        }
    }
}




/// Helper function to determine if a value is truthy
fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
        Value::String(s) => !s.is_empty(),
        Value::Sequence(seq) => !seq.is_empty(),
        Value::Mapping(map) => !map.is_empty(),
        Value::Tagged(_) => true, // Tagged values are generally truthy
    }
}




/// Compare two YAML values for equality
fn values_equal(left: &Value, right: &Value) -> bool {
    // Fast path: check discriminants first to avoid expensive comparisons
    if std::mem::discriminant(left) != std::mem::discriminant(right) {
        return false;
    }
    
    match (left, right) {
        (Value::Null, Value::Null) => true,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        (Value::Number(a), Value::Number(b)) => {
            // Compare as f64 for consistency
            a.as_f64() == b.as_f64()
        }
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Sequence(a), Value::Sequence(b)) => {
            a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| values_equal(x, y))
        }
        (Value::Mapping(a), Value::Mapping(b)) => {
            a.len() == b.len()
                && a.iter().all(|(k, v)| {
                    b.get(k).map_or(false, |v2| values_equal(v, v2))
                })
        }
        _ => false,
    }
}


/// Convert serde_json::Value to serde_yaml::Value for JSON parsing results
fn json_value_to_yaml_value(json_value: &serde_json::Value) -> Result<Value> {
    match json_value {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(Value::Number(serde_yaml::Number::from(i)))
            } else if let Some(u) = n.as_u64() {
                Ok(Value::Number(serde_yaml::Number::from(u)))
            } else if let Some(f) = n.as_f64() {
                Ok(Value::Number(serde_yaml::Number::from(f)))
            } else {
                Err(anyhow!("Invalid JSON number"))
            }
        }
        serde_json::Value::String(s) => Ok(Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let mut yaml_seq = Vec::new();
            for item in arr {
                yaml_seq.push(json_value_to_yaml_value(item)?);
            }
            Ok(Value::Sequence(yaml_seq))
        }
        serde_json::Value::Object(obj) => {
            let mut yaml_map = serde_yaml::Mapping::new();
            for (k, v) in obj {
                let yaml_value = json_value_to_yaml_value(v)?;
                yaml_map.insert(Value::String(k.clone()), yaml_value);
            }
            Ok(Value::Mapping(yaml_map))
        }
    }
}

/// Convert serde_yaml::Value to serde_json::Value for JSON string conversion
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
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| anyhow!("Invalid float value for JSON"))
            } else {
                Err(anyhow!("Invalid YAML number"))
            }
        }
        Value::String(s) => Ok(serde_json::Value::String(s.clone())),
        Value::Sequence(seq) => {
            let mut json_arr = Vec::new();
            for item in seq {
                json_arr.push(yaml_value_to_json_value(item)?);
            }
            Ok(serde_json::Value::Array(json_arr))
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
        Value::Tagged(_) => Err(anyhow!("Tagged values not supported in JSON conversion")),
    }
}

/// Convert AST to Value while escaping preprocessing tags (convert them to regular YAML)
fn escape_ast_to_value(ast: &YamlAst) -> Result<Value> {
    match ast {
        YamlAst::Null => Ok(Value::Null),
        YamlAst::Bool(b) => Ok(Value::Bool(*b)),
        YamlAst::Number(n) => Ok(Value::Number(n.clone())),
        YamlAst::PlainString(s) => Ok(Value::String(s.clone())),
        YamlAst::TemplatedString(s) => Ok(Value::String(s.clone())),
        YamlAst::Sequence(seq) => {
            let mut result = Vec::new();
            for item in seq {
                result.push(escape_ast_to_value(item)?);
            }
            Ok(Value::Sequence(result))
        }
        YamlAst::Mapping(pairs) => {
            let mut result = serde_yaml::Mapping::new();
            for (key, value) in pairs {
                let key_val = escape_ast_to_value(key)?;
                let value_val = escape_ast_to_value(value)?;
                result.insert(key_val, value_val);
            }
            Ok(Value::Mapping(result))
        }
        YamlAst::PreprocessingTag(_) => {
            // Convert preprocessing tags to a string representation to "escape" them
            Ok(Value::String("__ESCAPED_PREPROCESSING_TAG__".to_string()))
        }
        YamlAst::CloudFormationTag(cfn_tag) => {
            // Convert CloudFormation tags to a string representation to "escape" them
            Ok(Value::String(format!("__ESCAPED_CFN_TAG_{}__", cfn_tag.tag_name())))
        }
        YamlAst::UnknownYamlTag(tag) => {
            // Convert unknown tags to a string representation  
            Ok(Value::String(format!("__ESCAPED_TAG_{}__", tag.tag)))
        }
        YamlAst::ImportedDocument(doc) => {
            // Escape the imported document's content
            escape_ast_to_value(&doc.content)
        }
    }
}

/// Trait for resolving preprocessing tags with different implementation strategies
pub trait TagResolver {
    // Core AST resolution method (replaces separate AstResolver trait)
    fn resolve_ast(&self, ast: &YamlAst, context: &TagContext) -> Result<Value>;
    
    // Helper method for handlebars processing
    fn yaml_value_to_json_value(&self, yaml_value: &Value) -> Result<serde_json::Value>;
    
    // Core tag resolution methods
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value>;
    fn resolve_if(&self, tag: &IfTag, context: &TagContext) -> Result<Value>;
    fn resolve_map(&self, tag: &MapTag, context: &TagContext) -> Result<Value>;
    fn resolve_merge(&self, tag: &MergeTag, context: &TagContext) -> Result<Value>;
    fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext) -> Result<Value>;
    fn resolve_let(&self, tag: &LetTag, context: &TagContext) -> Result<Value>;
    fn resolve_eq(&self, tag: &EqTag, context: &TagContext) -> Result<Value>;
    fn resolve_not(&self, tag: &NotTag, context: &TagContext) -> Result<Value>;
    fn resolve_split(&self, tag: &SplitTag, context: &TagContext) -> Result<Value>;
    fn resolve_join(&self, tag: &JoinTag, context: &TagContext) -> Result<Value>;
    
    // Advanced transformation tags
    fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext) -> Result<Value>;
    fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext) -> Result<Value>;
    fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext) -> Result<Value>;
    fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext) -> Result<Value>;
    fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext) -> Result<Value>;
    fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext) -> Result<Value>;
    
    // String processing tags
    fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext) -> Result<Value>;
    fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext) -> Result<Value>;
    fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext) -> Result<Value>;
    fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext) -> Result<Value>;
    fn resolve_escape(&self, tag: &EscapeTag, context: &TagContext) -> Result<Value>;
}

/// Standard implementation of TagResolver
pub struct StandardTagResolver;

impl TagResolver for StandardTagResolver {
    fn yaml_value_to_json_value(&self, yaml_value: &Value) -> Result<serde_json::Value> {
        // Call the implementation method directly from the impl block below
        StandardTagResolver::yaml_value_to_json_value(self, yaml_value)
    }
    
    fn resolve_ast(&self, ast: &YamlAst, context: &TagContext) -> Result<Value> {
        match ast {
            YamlAst::Null => Ok(Value::Null),
            YamlAst::Bool(b) => Ok(Value::Bool(*b)),
            YamlAst::Number(n) => Ok(Value::Number(n.clone())),
            YamlAst::PlainString(s) | YamlAst::TemplatedString(s) => {
                // Process handlebars templates in strings
                self.process_string_with_handlebars(s.clone(), context)
            },
            YamlAst::Sequence(seq) => {
                let mut result = Vec::with_capacity(seq.len());
                
                if is_simple_sequence(seq) {
                    // Fast path: convert simple values directly without context creation
                    for item in seq {
                        result.push(simple_ast_to_value(item));
                    }
                } else {
                    // Complex path: need full processing with context
                    for (index, item) in seq.iter().enumerate() {
                        let item_context = context.with_array_index(index);
                        result.push(self.resolve_ast(item, &item_context)?);
                    }
                }
                Ok(Value::Sequence(result))
            }
            YamlAst::Mapping(pairs) => {
                let mut result = serde_yaml::Mapping::with_capacity(pairs.len());
                
                if is_simple_mapping(pairs) {
                    // Fast path: simple key-value pairs with no processing needed
                    for (key, value) in pairs {
                        let key_val = simple_ast_to_value(key);
                        let value_val = simple_ast_to_value(value);
                        result.insert(key_val, value_val);
                    }
                } else {
                    // Complex path: need full processing
                    for (key, value) in pairs {
                        let key_val = self.resolve_ast(key, context)?;
                    
                    // Check for YAML 1.1 merge keys which are not supported in YAML 1.2
                    if let Value::String(key_str) = &key_val {
                        if key_str == "<<" {
                            let location_info = if let Some(input_uri) = &context.input_uri {
                                format!("in file '{}'", input_uri)
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
                        context.with_path_segment(&key_str)
                    } else {
                        // For non-string keys, use the key's string representation
                        let key_str = match &key_val {
                            Value::Number(n) => n.as_f64().unwrap_or(0.0).to_string(),
                            Value::Bool(b) => b.to_string(),
                            _ => format!("{:?}", key_val),
                        };
                        context.with_path_segment(&key_str)
                    };
                    
                        let value_val = self.resolve_ast(value, &value_context)?;
                        result.insert(key_val, value_val);
                    }
                }
                Ok(Value::Mapping(result))
            }
            YamlAst::PreprocessingTag(tag) => {
                self.resolve_preprocessing_tag_with_context(tag, context)
            },
            YamlAst::CloudFormationTag(cfn_tag) => {
                // Process CloudFormation intrinsic functions with proper YAML tag generation
                // The inner AST may contain handlebars templates or preprocessing directives
                let resolved_value = self.resolve_ast(cfn_tag.inner_value(), context)?;
                self.create_cfn_expression(cfn_tag, resolved_value)
            },
            YamlAst::UnknownYamlTag(tag) => {
                // For unknown tags, preserve the tag structure while processing the content
                // Based on iidy-js behavior: handlebars/preprocessing happens INSIDE tag values
                let resolved_value = self.resolve_ast(&tag.value, context)?;
                self.create_tagged_value(&tag.tag, resolved_value)
            }
            YamlAst::ImportedDocument(doc) => {
                // Process the imported document's content with its original context
                // The input_uri should already be set correctly for this document
                self.resolve_ast(&doc.content, context)
            }
        }
    }
    
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value> {
        let path = &tag.path;
        
        // Parse path and query
        let (base_path, query) = parse_path_and_query(path, &tag.query);
        
        // Try to resolve the variable from the environment
        if let Some(mut value) = resolve_dot_notation_path(&base_path, context) {
            // Apply query selector if present
            if let Some(query_str) = query {
                value = apply_query_selector(&value, &query_str)?;
            }
            return Ok(value);
        }
        
        // Variable not found - provide more specific error context
        // Check if the root variable exists to give a better error message
        let root_var = base_path.split('.').next().unwrap_or(&base_path).split('[').next().unwrap_or(&base_path);
        
        // Get file path
        let file_path = if let Some(input_uri) = &context.input_uri {
            input_uri.clone()
        } else {
            context.current_location()
                .unwrap_or_else(|| "unknown location".to_string())
        };
        
        // Get YAML path information
        let yaml_path = context.current_path();
        
        // Get available variables
        let available_vars: Vec<String> = context.variables.keys().cloned().collect();
        
        // Check if root variable exists
        if let Some(_root_value) = context.get_variable(root_var) {
            // Root variable exists, but path resolution failed - this means a property doesn't exist
            let property_path = if base_path.contains('.') {
                base_path.split('.').skip(1).collect::<Vec<_>>().join(".")
            } else if base_path.contains('[') {
                base_path.split('[').skip(1).collect::<Vec<_>>().join("[")
            } else {
                base_path.to_string()
            };
            
            {
                
                // Try to find the line number by searching for the include reference
                let include_pattern = format!("!$ {}", base_path);
                let location = if let Ok(content) = std::fs::read_to_string(&file_path) {
                    let line_number = content.lines().enumerate().find_map(|(idx, line)| {
                        if line.contains(&include_pattern) {
                            Some(idx + 1)
                        } else {
                            None
                        }
                    }).unwrap_or(0);
                    
                    if line_number > 0 {
                        format!("{}:{}", file_path, line_number)
                    } else {
                        file_path.clone()
                    }
                } else {
                    file_path.clone()
                };
                
                use crate::yaml::errors::variable_not_found_error;
                return Err(variable_not_found_error(&format!("{}.{}", root_var, property_path), &location, &yaml_path, available_vars));
            }
        } else {
            // Root variable doesn't exist
            use crate::yaml::errors::variable_not_found_error;
            Err(variable_not_found_error(root_var, &file_path, &yaml_path, available_vars))
        }
    }
    
    fn resolve_if(&self, tag: &IfTag, context: &TagContext) -> Result<Value> {
        let condition_result = self.resolve_ast(&tag.test, context)?;
        
        let is_truthy = match condition_result {
            Value::Bool(b) => b,
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
            Value::Sequence(ref seq) => !seq.is_empty(),
            Value::Mapping(ref map) => !map.is_empty(),
            _ => true,
        };

        if is_truthy {
            self.resolve_ast(&tag.then_value, context)
        } else if let Some(ref else_value) = tag.else_value {
            self.resolve_ast(else_value, context)
        } else {
            Ok(Value::Null)
        }
    }
    
    fn resolve_map(&self, tag: &MapTag, context: &TagContext) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.items, context)?;
        
        match items_result {
            Value::Sequence(seq) => {
                let mut result = Vec::with_capacity(seq.len());
                let var_name = tag.var.as_deref().unwrap_or("item");
                
                for (idx, item) in seq.into_iter().enumerate() {
                    // Create new context with the current item and index bound to variables
                    let mut item_bindings = small_hashmap();
                    item_bindings.insert(var_name.to_string(), item);
                    item_bindings.insert(format!("{}Idx", var_name), Value::Number(serde_yaml::Number::from(idx)));
                    let item_context = context.with_bindings_ref(&item_bindings);
                    
                    // Apply filter if present
                    if let Some(filter) = &tag.filter {
                        let filter_result = self.resolve_ast(filter, &item_context)?;
                        if !is_truthy(&filter_result) {
                            continue; // Skip this item
                        }
                    }
                    
                    let transformed = self.resolve_ast(&tag.template, &item_context)?;
                    result.push(transformed);
                }
                
                Ok(Value::Sequence(result))
            }
            _ => Err(anyhow!("Map items must be a sequence")),
        }
    }
    
    fn resolve_merge(&self, tag: &MergeTag, context: &TagContext) -> Result<Value> {
        // Pre-allocate with estimated capacity based on number of sources
        let mut result = serde_yaml::Mapping::with_capacity(tag.sources.len() * 4);
        
        for source in &tag.sources {
            let source_result = self.resolve_ast(source, context)?;
            match source_result {
                Value::Mapping(map) => {
                    result.extend(map);
                }
                _ => return Err(anyhow!("Merge source must be a mapping")),
            }
        }
        
        Ok(Value::Mapping(result))
    }
    
    fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext) -> Result<Value> {
        // Pre-allocate with estimated capacity
        let mut result = Vec::with_capacity(tag.sources.len() * 2);
        
        for source in &tag.sources {
            let source_result = self.resolve_ast(source, context)?;
            match source_result {
                Value::Sequence(mut seq) => {
                    result.append(&mut seq);
                }
                other => {
                    // Single item, add it to the result
                    result.push(other);
                }
            }
        }
        
        Ok(Value::Sequence(result))
    }
    
    fn resolve_let(&self, tag: &LetTag, context: &TagContext) -> Result<Value> {
        let mut bindings = HashMap::with_capacity(tag.bindings.len());
        
        // Resolve all variable bindings
        for (var_name, var_expr) in &tag.bindings {
            let var_value = self.resolve_ast(var_expr, context)?;
            bindings.insert(var_name.clone(), var_value);
        }
        
        // Create new context with bindings and resolve expression
        let new_context = context.with_bindings_ref(&bindings);
        self.resolve_ast(&tag.expression, &new_context)
    }
    
    fn resolve_eq(&self, tag: &EqTag, context: &TagContext) -> Result<Value> {
        let left = self.resolve_ast(&tag.left, context)?;
        let right = self.resolve_ast(&tag.right, context)?;
        
        let is_equal = values_equal(&left, &right);
        Ok(Value::Bool(is_equal))
    }
    
    fn resolve_not(&self, tag: &NotTag, context: &TagContext) -> Result<Value> {
        let expr_result = self.resolve_ast(&tag.expression, context)?;
        
        let is_truthy = match expr_result {
            Value::Bool(b) => b,
            Value::Null => false,
            Value::String(s) => !s.is_empty(),
            Value::Number(n) => n.as_f64().unwrap_or(0.0) != 0.0,
            Value::Sequence(ref seq) => !seq.is_empty(),
            Value::Mapping(ref map) => !map.is_empty(),
            _ => true,
        };
        
        Ok(Value::Bool(!is_truthy))
    }
    
    fn resolve_split(&self, tag: &SplitTag, context: &TagContext) -> Result<Value> {
        let delimiter_result = self.resolve_ast(&tag.delimiter, context)?;
        let string_result = self.resolve_ast(&tag.string, context)?;
        
        match (delimiter_result, string_result) {
            (Value::String(delimiter), Value::String(s)) => {
                let parts: Vec<Value> = s
                    .split(&delimiter)
                    .map(|part| Value::String(part.to_string()))
                    .collect();
                Ok(Value::Sequence(parts))
            }
            _ => Err(anyhow!("Split requires string delimiter and string to split")),
        }
    }
    
    fn resolve_join(&self, tag: &JoinTag, context: &TagContext) -> Result<Value> {
        let delimiter_result = self.resolve_ast(&tag.delimiter, context)?;
        let array_result = self.resolve_ast(&tag.array, context)?;
        
        // Extract delimiter as string
        let delimiter_str = match delimiter_result {
            Value::String(s) => s,
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => return Err(anyhow!("Join delimiter must be a string-convertible value")),
        };
        
        match array_result {
            Value::Sequence(seq) => {
                let strings: Result<Vec<String>, _> = seq
                    .into_iter()
                    .map(|v| match v {
                        Value::String(s) => Ok(s),
                        Value::Number(n) => Ok(n.to_string()),
                        Value::Bool(b) => Ok(b.to_string()),
                        _ => Err(anyhow!("Join array must contain string-convertible values")),
                    })
                    .collect();
                
                let joined = strings?.join(&delimiter_str);
                Ok(Value::String(joined))
            }
            _ => Err(anyhow!("Join array must be a sequence")),
        }
    }
    
    fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.items, context)?;
        
        match items_result {
            Value::Sequence(seq) => {
                // Pre-allocate with estimated capacity (assuming some expansion)
                let mut result = Vec::with_capacity(seq.len() * 2);
                let var_name = tag.var.as_deref().unwrap_or("item");
                
                for (idx, item) in seq.into_iter().enumerate() {
                    // Create new context with the current item and index bound to variables
                    let mut item_bindings = small_hashmap();
                    item_bindings.insert(var_name.to_string(), item);
                    item_bindings.insert(format!("{}Idx", var_name), Value::Number(serde_yaml::Number::from(idx)));
                    let item_context = context.with_bindings_ref(&item_bindings);
                    
                    // Apply filter if present
                    if let Some(filter) = &tag.filter {
                        let filter_result = self.resolve_ast(filter, &item_context)?;
                        if !is_truthy(&filter_result) {
                            continue; // Skip this item
                        }
                    }
                    
                    let transformed = self.resolve_ast(&tag.template, &item_context)?;
                    // Flatten the result - if it's a sequence, extend; otherwise push
                    match transformed {
                        Value::Sequence(mut sub_seq) => {
                            result.append(&mut sub_seq);
                        }
                        other => {
                            result.push(other);
                        }
                    }
                }
                
                Ok(Value::Sequence(result))
            }
            _ => Err(anyhow!("ConcatMap items must be a sequence")),
        }
    }
    
    fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.items, context)?;
        
        match items_result {
            Value::Sequence(seq) => {
                let mut result = serde_yaml::Mapping::new();
                let var_name = tag.var.as_deref().unwrap_or("item");
                
                for item in seq {
                    // Create new context with the current item bound to the variable
                    let mut item_bindings = HashMap::with_capacity(1);
                    item_bindings.insert(var_name.to_string(), item);
                    let item_context = context.with_bindings(item_bindings);
                    
                    let transformed = self.resolve_ast(&tag.template, &item_context)?;
                    match transformed {
                        Value::Mapping(map) => {
                            result.extend(map);
                        }
                        _ => return Err(anyhow!("MergeMap template must produce mappings")),
                    }
                }
                
                Ok(Value::Mapping(result))
            }
            _ => Err(anyhow!("MergeMap items must be a sequence")),
        }
    }
    
    fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext) -> Result<Value> {
        // Inline map resolution logic
        let items_result = self.resolve_ast(&tag.items, context)?;
        
        let mapped_result = match items_result {
            Value::Sequence(seq) => {
                let mut result = Vec::with_capacity(seq.len());
                let var_name = tag.var.as_deref().unwrap_or("item");
                
                for (idx, item) in seq.into_iter().enumerate() {
                    // Create new context with the current item and index bound to variables
                    let mut item_bindings = small_hashmap();
                    item_bindings.insert(var_name.to_string(), item);
                    item_bindings.insert(format!("{}Idx", var_name), Value::Number(serde_yaml::Number::from(idx)));
                    let item_context = context.with_bindings_ref(&item_bindings);
                    
                    // Apply filter if present
                    if let Some(filter) = &tag.filter {
                        let filter_result = self.resolve_ast(filter, &item_context)?;
                        if !is_truthy(&filter_result) {
                            continue; // Skip this item
                        }
                    }
                    
                    let transformed = self.resolve_ast(&tag.template, &item_context)?;
                    result.push(transformed);
                }
                
                Value::Sequence(result)
            }
            _ => return Err(anyhow!("MapListToHash items must be a sequence")),
        };
        
        match mapped_result {
            Value::Sequence(seq) => {
                let mut result = serde_yaml::Mapping::new();
                
                for item in seq {
                    match item {
                        Value::Sequence(ref pair) if pair.len() == 2 => {
                            let key = &pair[0];
                            let value = &pair[1];
                            result.insert(key.clone(), value.clone());
                        }
                        Value::Mapping(ref map) => {
                            // Handle object format with key/value fields
                            let key_value = map.get(&Value::String("key".to_string()));
                            let value_value = map.get(&Value::String("value".to_string()));
                            
                            if let (Some(key), Some(value)) = (key_value, value_value) {
                                result.insert(key.clone(), value.clone());
                            } else {
                                return Err(anyhow!("MapListToHash requires sequence of [key, value] pairs or objects with 'key' and 'value' fields"));
                            }
                        }
                        _ => return Err(anyhow!("MapListToHash requires sequence of [key, value] pairs or objects with 'key' and 'value' fields")),
                    }
                }
                
                Ok(Value::Mapping(result))
            }
            _ => Err(anyhow!("MapListToHash template must produce a sequence of key-value pairs")),
        }
    }
    
    fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.items, context)?;
        
        match items_result {
            Value::Mapping(map) => {
                let mut result = serde_yaml::Mapping::new();
                let var_name = tag.var.as_deref().unwrap_or("item");
                
                for (key, value) in map {
                    // Create lodash _.mapValues compatible context: item = {key: "keyname", value: actualValue}
                    let mut value_bindings = HashMap::with_capacity(1);
                    
                    // Convert key to string
                    let key_str = match &key {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => format!("{:?}", key),
                    };
                    
                    // Create the item object with key and value properties (lodash style)
                    let mut item_object = serde_yaml::Mapping::new();
                    item_object.insert(Value::String("key".to_string()), Value::String(key_str));
                    item_object.insert(Value::String("value".to_string()), value);
                    value_bindings.insert(var_name.to_string(), Value::Mapping(item_object));
                    
                    let value_context = context.with_bindings(value_bindings);
                    
                    let transformed = self.resolve_ast(&tag.template, &value_context)?;
                    result.insert(key, transformed);
                }
                
                Ok(Value::Mapping(result))
            }
            _ => Err(anyhow!("MapValues items must be a mapping")),
        }
    }
    
    fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext) -> Result<Value> {
        let items_result = self.resolve_ast(&tag.items, context)?;
        
        match items_result {
            Value::Sequence(seq) => {
                let mut groups: std::collections::HashMap<String, Vec<Value>> = std::collections::HashMap::with_capacity(seq.len() / 4);
                let var_name = tag.var.as_deref().unwrap_or("item");
                
                for item in seq {
                    // Create new context with the current item bound to the variable
                    let mut item_bindings = HashMap::with_capacity(1);
                    item_bindings.insert(var_name.to_string(), item.clone());
                    let item_context = context.with_bindings(item_bindings);
                    
                    let key_result = self.resolve_ast(&tag.key, &item_context)?;
                    let key_str = match key_result {
                        Value::String(s) => s,
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        _ => return Err(anyhow!("GroupBy key must resolve to a string-convertible value")),
                    };
                    
                    groups.entry(key_str).or_insert_with(Vec::new).push(item);
                }
                
                // Convert to YAML mapping
                let mut result = serde_yaml::Mapping::new();
                for (key, items) in groups {
                    result.insert(Value::String(key), Value::Sequence(items));
                }
                
                Ok(Value::Mapping(result))
            }
            _ => Err(anyhow!("GroupBy source must be a sequence")),
        }
    }
    
    fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext) -> Result<Value> {
        let source_result = self.resolve_ast(&tag.source, context)?;
        
        match source_result {
            Value::Sequence(seq) => {
                let mut result = serde_yaml::Mapping::new();
                
                for item in seq {
                    match item {
                        Value::Sequence(ref pair) if pair.len() == 2 => {
                            let key = &pair[0];
                            let value = &pair[1];
                            result.insert(key.clone(), value.clone());
                        }
                        _ => return Err(anyhow!("FromPairs requires sequence of [key, value] pairs")),
                    }
                }
                
                Ok(Value::Mapping(result))
            }
            _ => Err(anyhow!("FromPairs source must be a sequence")),
        }
    }
    
    fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext) -> Result<Value> {
        let data_result = self.resolve_ast(&tag.data, context)?;
        
        let yaml_string = serde_yaml::to_string(&data_result)
            .map_err(|e| anyhow!("Failed to convert data to YAML string: {}", e))?;
        
        // Remove trailing newline that serde_yaml adds
        let trimmed = yaml_string.trim_end().to_string();
        Ok(Value::String(trimmed))
    }
    
    fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext) -> Result<Value> {
        let yaml_string_result = self.resolve_ast(&tag.yaml_string, context)?;
        
        match yaml_string_result {
            Value::String(yaml_str) => {
                serde_yaml::from_str(&yaml_str)
                    .map_err(|e| anyhow!("Failed to parse YAML string: {}", e))
            }
            _ => Err(anyhow!("ParseYaml requires a string input")),
        }
    }
    
    fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext) -> Result<Value> {
        let data_result = self.resolve_ast(&tag.data, context)?;
        
        // Convert serde_yaml::Value to serde_json::Value
        let json_value = yaml_value_to_json_value(&data_result)?;
        
        let json_string = serde_json::to_string(&json_value)
            .map_err(|e| anyhow!("Failed to convert data to JSON string: {}", e))?;
        
        Ok(Value::String(json_string))
    }
    
    fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext) -> Result<Value> {
        let json_string_result = self.resolve_ast(&tag.json_string, context)?;
        
        match json_string_result {
            Value::String(json_str) => {
                let json_value: serde_json::Value = serde_json::from_str(&json_str)
                    .map_err(|e| anyhow!("Failed to parse JSON string: {}", e))?;
                
                // Convert serde_json::Value back to serde_yaml::Value
                json_value_to_yaml_value(&json_value)
            }
            _ => Err(anyhow!("ParseJson requires a string input")),
        }
    }
    
    fn resolve_escape(&self, tag: &EscapeTag, _context: &TagContext) -> Result<Value> {
        // For the escape tag, we need to convert the AST to Value without any preprocessing
        // This means we manually convert the AST while preserving any preprocessing tags as regular YAML
        escape_ast_to_value(&tag.content)
    }
}

impl StandardTagResolver {
    /// Convert serde_yaml::Value to serde_json::Value for handlebars processing
    pub fn yaml_value_to_json_value(&self, yaml_value: &Value) -> Result<serde_json::Value> {
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
                let mut json_seq = Vec::with_capacity(seq.len());
                for item in seq {
                    json_seq.push(self.yaml_value_to_json_value(item)?);
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
                    json_map.insert(key_str, self.yaml_value_to_json_value(v)?);
                }
                Ok(serde_json::Value::Object(json_map))
            }
            Value::Tagged(_) => Err(anyhow!("Tagged values not supported in handlebars conversion")),
        }
    }

    fn process_string_with_handlebars(&self, s: String, context: &TagContext) -> Result<Value> {
        use crate::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;
        
        // Check if string contains handlebars syntax
        if !s.contains("{{") {
            return Ok(Value::String(s));
        }
        
        // Convert TagContext variables from serde_yaml::Value to serde_json::Value
        let mut env_values: HashMap<String, serde_json::Value> = HashMap::with_capacity(context.variables.len());
        for (key, yaml_value) in &context.variables {
            let json_value = self.yaml_value_to_json_value(yaml_value)?;
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
                        let file_path = if let Some(input_uri) = &context.input_uri {
                            input_uri.clone()
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
                        use crate::yaml::errors::variable_not_found_error;
                        return Err(variable_not_found_error(var_name, &location, &context.current_path(), available_vars));
                    }
                }
                
                // Fallback to basic error
                Err(anyhow!("Handlebars processing failed: {}", e))
            }
        }
    }

    fn resolve_preprocessing_tag_with_context(&self, tag: &PreprocessingTag, context: &TagContext) -> Result<Value> {        
        match tag {
            PreprocessingTag::Include(include_tag) => {
                self.resolve_include(include_tag, context)
            }
            PreprocessingTag::If(if_tag) => {
                self.resolve_if(if_tag, context)
            }
            PreprocessingTag::Map(map_tag) => {
                self.resolve_map(map_tag, context)
            }
            PreprocessingTag::Merge(merge_tag) => {
                self.resolve_merge(merge_tag, context)
            }
            PreprocessingTag::Concat(concat_tag) => {
                self.resolve_concat(concat_tag, context)
            }
            PreprocessingTag::Let(let_tag) => {
                self.resolve_let(let_tag, context)
            }
            PreprocessingTag::Eq(eq_tag) => {
                self.resolve_eq(eq_tag, context)
            }
            PreprocessingTag::Not(not_tag) => {
                self.resolve_not(not_tag, context)
            }
            PreprocessingTag::Split(split_tag) => {
                self.resolve_split(split_tag, context)
            }
            PreprocessingTag::Join(join_tag) => {
                self.resolve_join(join_tag, context)
            }
            PreprocessingTag::ConcatMap(concat_map_tag) => {
                self.resolve_concat_map(concat_map_tag, context)
            }
            PreprocessingTag::MergeMap(merge_map_tag) => {
                self.resolve_merge_map(merge_map_tag, context)
            }
            PreprocessingTag::MapListToHash(map_list_to_hash_tag) => {
                self.resolve_map_list_to_hash(map_list_to_hash_tag, context)
            }
            PreprocessingTag::MapValues(map_values_tag) => {
                self.resolve_map_values(map_values_tag, context)
            }
            PreprocessingTag::GroupBy(group_by_tag) => {
                self.resolve_group_by(group_by_tag, context)
            }
            PreprocessingTag::FromPairs(from_pairs_tag) => {
                self.resolve_from_pairs(from_pairs_tag, context)
            }
            PreprocessingTag::ToYamlString(to_yaml_string_tag) => {
                self.resolve_to_yaml_string(to_yaml_string_tag, context)
            }
            PreprocessingTag::ParseYaml(parse_yaml_tag) => {
                self.resolve_parse_yaml(parse_yaml_tag, context)
            }
            PreprocessingTag::ToJsonString(to_json_string_tag) => {
                self.resolve_to_json_string(to_json_string_tag, context)
            }
            PreprocessingTag::ParseJson(parse_json_tag) => {
                self.resolve_parse_json(parse_json_tag, context)
            }
            PreprocessingTag::Escape(escape_tag) => {
                self.resolve_escape(escape_tag, context)
            }
        }
    }

    /// Create a CloudFormation expression that serializes to proper YAML tag syntax
    /// 
    /// This method converts CloudFormation AST nodes to serde mapping structures that
    /// can be serialized by serde_yaml. The output uses mapping format (`'!Ref': value`)
    /// which is later post-processed to proper YAML tags (`!Ref value`) in the render pipeline.
    fn create_cfn_expression(&self, cfn_tag: &crate::yaml::parsing::ast::CloudFormationTag, resolved_value: Value) -> Result<Value> {
        use crate::yaml::parsing::ast::CloudFormationTag;
        
        // Helper function to unpack single-element arrays (for array syntax support)
        let unpack_single_element_array = |value: Value| -> Value {
            match &value {
                Value::Sequence(seq) if seq.len() == 1 => seq[0].clone(),
                _ => value,
            }
        };
        
        // Convert the resolved value to the appropriate CloudFormation expression structure
        match cfn_tag {
            CloudFormationTag::Ref(_) => {
                // Ref expects a string - unpack single-element arrays for compatibility
                let unpacked = unpack_single_element_array(resolved_value);
                if let Value::String(resource) = unpacked {
                    let mut map = serde_yaml::Mapping::with_capacity(1);
                    map.insert(Value::String("!Ref".to_string()), Value::String(resource));
                    Ok(Value::Mapping(map))
                } else {
                    Err(anyhow::anyhow!("Ref function expects a string value, got: {:?}", unpacked))
                }
            },
            CloudFormationTag::Sub(_) => {
                // Sub expects a string or array [string, {substitutions}] - unpack single-element arrays
                let unpacked = unpack_single_element_array(resolved_value);
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Sub".to_string()), unpacked);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::GetAtt(_) => {
                // GetAtt expects [resource, attribute] or "resource.attribute" - unpack single-element arrays
                let unpacked = unpack_single_element_array(resolved_value);
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!GetAtt".to_string()), unpacked);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Join(_) => {
                // Join expects [delimiter, [values]]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Join".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Select(_) => {
                // Select expects [index, list]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Select".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Split(_) => {
                // Split expects [delimiter, string]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Split".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Base64(_) => {
                // Base64 expects a string
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Base64".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::GetAZs(_) => {
                // GetAZs expects a region string
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!GetAZs".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::ImportValue(_) => {
                // ImportValue expects a string
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!ImportValue".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::FindInMap(_) => {
                // FindInMap expects [MapName, TopLevelKey, SecondLevelKey]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!FindInMap".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Cidr(_) => {
                // Cidr expects [ipBlock, count, cidrBits]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Cidr".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Length(_) => {
                // Length expects a list
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Length".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::ToJsonString(_) => {
                // ToJsonString expects any data structure
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!ToJsonString".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Transform(_) => {
                // Transform expects a mapping with Name and Parameters
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Transform".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::ForEach(_) => {
                // ForEach expects a mapping with specific structure
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!ForEach".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::If(_) => {
                // If expects [condition, trueValue, falseValue]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!If".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Equals(_) => {
                // Equals expects [value1, value2]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Equals".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::And(_) => {
                // And expects [condition1, condition2, ...]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!And".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Or(_) => {
                // Or expects [condition1, condition2, ...]
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Or".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
            CloudFormationTag::Not(_) => {
                // Not expects a condition
                let mut map = serde_yaml::Mapping::with_capacity(1);
                map.insert(Value::String("!Not".to_string()), resolved_value);
                Ok(Value::Mapping(map))
            },
        }
    }

    /// Create a tagged value that preserves unknown YAML tags
    /// 
    /// Creates a mapping structure that can be serialized to both YAML and JSON.
    /// The output is post-processed to convert mapping format to proper YAML tags.
    /// 
    /// **Implementation Note**: This produces `'!Tag': value` format which is then
    /// converted to proper `!Tag value` format via `convert_cf_mappings_to_tags()`.
    /// This approach works around serde_yaml 0.9's inability to serialize `Value::Tagged`.
    fn create_tagged_value(&self, tag: &str, value: Value) -> Result<Value> {
        // Create a mapping with the tag as key - this works for both YAML and JSON serialization
        let mut map = serde_yaml::Mapping::with_capacity(1);
        map.insert(Value::String(format!("!{}", tag)), value);
        Ok(Value::Mapping(map))
    }
}

/// Debug implementation that logs all tag resolutions
pub struct DebugTagResolver {
    inner: StandardTagResolver,
}

impl DebugTagResolver {
    pub fn new() -> Self {
        Self {
            inner: StandardTagResolver,
        }
    }
    
    fn log_resolution<T: std::fmt::Debug>(&self, tag_name: &str, tag: &T, result: &Result<Value>) {
        match result {
            Ok(value) => {
                eprintln!("DEBUG: Resolved {} tag: {:?} -> {:?}", tag_name, tag, value);
            }
            Err(err) => {
                eprintln!("DEBUG: Failed to resolve {} tag: {:?} -> Error: {}", tag_name, tag, err);
            }
        }
    }
}

impl TagResolver for DebugTagResolver {
    fn resolve_ast(&self, ast: &YamlAst, context: &TagContext) -> Result<Value> {
        self.inner.resolve_ast(ast, context)
    }
    
    fn yaml_value_to_json_value(&self, yaml_value: &Value) -> Result<serde_json::Value> {
        self.inner.yaml_value_to_json_value(yaml_value)
    }
    
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_include(tag, context);
        self.log_resolution("include", tag, &result);
        result
    }
    
    fn resolve_if(&self, tag: &IfTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_if(tag, context);
        self.log_resolution("if", tag, &result);
        result
    }
    
    fn resolve_map(&self, tag: &MapTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_map(tag, context);
        self.log_resolution("map", tag, &result);
        result
    }
    
    fn resolve_merge(&self, tag: &MergeTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_merge(tag, context);
        self.log_resolution("merge", tag, &result);
        result
    }
    
    fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_concat(tag, context);
        self.log_resolution("concat", tag, &result);
        result
    }
    
    fn resolve_let(&self, tag: &LetTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_let(tag, context);
        self.log_resolution("let", tag, &result);
        result
    }
    
    fn resolve_eq(&self, tag: &EqTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_eq(tag, context);
        self.log_resolution("eq", tag, &result);
        result
    }
    
    fn resolve_not(&self, tag: &NotTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_not(tag, context);
        self.log_resolution("not", tag, &result);
        result
    }
    
    fn resolve_split(&self, tag: &SplitTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_split(tag, context);
        self.log_resolution("split", tag, &result);
        result
    }
    
    fn resolve_join(&self, tag: &JoinTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_join(tag, context);
        self.log_resolution("join", tag, &result);
        result
    }
    
    fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_concat_map(tag, context);
        self.log_resolution("concatMap", tag, &result);
        result
    }
    
    fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_merge_map(tag, context);
        self.log_resolution("mergeMap", tag, &result);
        result
    }
    
    fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_map_list_to_hash(tag, context);
        self.log_resolution("mapListToHash", tag, &result);
        result
    }
    
    fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_map_values(tag, context);
        self.log_resolution("mapValues", tag, &result);
        result
    }
    
    fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_group_by(tag, context);
        self.log_resolution("groupBy", tag, &result);
        result
    }
    
    fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_from_pairs(tag, context);
        self.log_resolution("fromPairs", tag, &result);
        result
    }
    
    fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_to_yaml_string(tag, context);
        self.log_resolution("toYamlString", tag, &result);
        result
    }
    
    fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_parse_yaml(tag, context);
        self.log_resolution("parseYaml", tag, &result);
        result
    }
    
    fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_to_json_string(tag, context);
        self.log_resolution("toJsonString", tag, &result);
        result
    }
    
    fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_parse_json(tag, context);
        self.log_resolution("parseJson", tag, &result);
        result
    }
    
    fn resolve_escape(&self, tag: &EscapeTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_escape(tag, context);
        self.log_resolution("escape", tag, &result);
        result
    }
}

/// Tracing implementation that collects performance metrics
pub struct TracingTagResolver {
    inner: StandardTagResolver,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tagcontext_input_uri_storage_local_file_paths() {
        // Test local file path input_uri storage
        let context = TagContext::from_file_location("/Users/test/configs/app/config.yaml");
        
        assert!(context.input_uri.is_some());
        assert_eq!(
            context.input_uri.unwrap(),
            "/Users/test/configs/app/config.yaml"
        );
        
        // Test that we can derive base path on demand
        let derived_base = derive_base_path_from_location("/Users/test/configs/app/config.yaml");
        assert!(derived_base.is_some());
        assert_eq!(
            derived_base.unwrap().to_string_lossy(),
            "/Users/test/configs/app"
        );
    }
    
    #[test]
    fn test_tagcontext_input_uri_storage_s3_urls() {
        // Test S3 URL input_uri storage
        let context = TagContext::from_file_location("s3://my-bucket/configs/app/config.yaml");
        
        assert!(context.input_uri.is_some());
        assert_eq!(
            context.input_uri.unwrap(),
            "s3://my-bucket/configs/app/config.yaml"
        );
        
        // Test that we can derive base path on demand
        let derived_base = derive_base_path_from_location("s3://my-bucket/configs/app/config.yaml");
        assert!(derived_base.is_some());
        assert_eq!(
            derived_base.unwrap().to_string_lossy(),
            "s3://my-bucket/configs/app/"
        );
    }
    
    #[test]
    fn test_tagcontext_input_uri_storage_s3_root_file() {
        // Test S3 URL with root file - input_uri storage and base path derivation
        let context = TagContext::from_file_location("s3://my-bucket/config.yaml");
        
        assert!(context.input_uri.is_some());
        assert_eq!(
            context.input_uri.unwrap(),
            "s3://my-bucket/config.yaml"
        );
        
        // Root files should have bucket root as base path to support relative imports
        let derived_base = derive_base_path_from_location("s3://my-bucket/config.yaml");
        assert!(derived_base.is_some());
        assert_eq!(
            derived_base.unwrap().to_string_lossy(),
            "s3://my-bucket/"
        );
    }
    
    #[test]
    fn test_tagcontext_input_uri_storage_https_urls() {
        // Test HTTPS URL input_uri storage
        let context = TagContext::from_file_location("https://example.com/configs/app/config.yaml");
        
        assert!(context.input_uri.is_some());
        assert_eq!(
            context.input_uri.unwrap(),
            "https://example.com/configs/app/config.yaml"
        );
        
        // Test that we can derive base path on demand
        let derived_base = derive_base_path_from_location("https://example.com/configs/app/config.yaml");
        assert!(derived_base.is_some());
        assert_eq!(
            derived_base.unwrap().to_string_lossy(),
            "https://example.com/configs/app/"
        );
    }
    
    #[test]
    fn test_tagcontext_input_uri_storage_http_urls() {
        // Test HTTP URL input_uri storage
        let context = TagContext::from_file_location("http://example.com/configs/app/config.yaml");
        
        assert!(context.input_uri.is_some());
        assert_eq!(
            context.input_uri.unwrap(),
            "http://example.com/configs/app/config.yaml"
        );
        
        // Test that we can derive base path on demand
        let derived_base = derive_base_path_from_location("http://example.com/configs/app/config.yaml");
        assert!(derived_base.is_some());
        assert_eq!(
            derived_base.unwrap().to_string_lossy(),
            "http://example.com/configs/app/"
        );
    }
    
    #[test]
    fn test_tagcontext_input_uri_storage_http_root_file() {
        // Test HTTP URL with root file - input_uri storage and base path derivation
        let context = TagContext::from_file_location("https://example.com/config.yaml");
        
        assert!(context.input_uri.is_some());
        assert_eq!(
            context.input_uri.unwrap(),
            "https://example.com/config.yaml"
        );
        
        // Root files should have domain root as base path to support relative imports
        let derived_base = derive_base_path_from_location("https://example.com/config.yaml");
        assert!(derived_base.is_some());
        assert_eq!(
            derived_base.unwrap().to_string_lossy(),
            "https://example.com/"
        );
    }
    
    #[test]
    fn test_tagcontext_comprehensive_relative_import_scenarios() {
        // Test all the scenarios you mentioned for relative imports
        let test_cases = vec![
            // S3 cases
            ("s3://bucket/file.yaml", "s3://bucket/"),
            ("s3://bucket/folder/file.yaml", "s3://bucket/folder/"),
            ("s3://bucket/folder/subfolder/file.yaml", "s3://bucket/folder/subfolder/"),
            // HTTPS cases  
            ("https://example.com/file.yaml", "https://example.com/"),
            ("https://example.com/folder/file.yaml", "https://example.com/folder/"),
            ("https://example.com/folder/subfolder/file.yaml", "https://example.com/folder/subfolder/"),
            // HTTP cases
            ("http://example.com/file.yaml", "http://example.com/"),
            ("http://example.com/folder/file.yaml", "http://example.com/folder/"),
            ("http://example.com/folder/subfolder/file.yaml", "http://example.com/folder/subfolder/"),
        ];
        
        for (location, expected_base) in test_cases {
            let context = TagContext::from_file_location(location);
            
            // Test that input_uri is stored correctly
            assert!(context.input_uri.is_some(), "Expected input_uri for location: {}", location);
            assert_eq!(
                context.input_uri.as_ref().unwrap(),
                location,
                "input_uri mismatch for location: {}", 
                location
            );
            
            // Test that base path can be derived on demand
            let derived_base = derive_base_path_from_location(location);
            assert!(derived_base.is_some(), "Expected derived base_path for location: {}", location);
            assert_eq!(
                derived_base.unwrap().to_string_lossy(),
                expected_base,
                "Derived base path mismatch for location: {}",
                location
            );
        }
    }
    
    #[test]
    fn test_derive_base_path_from_location_no_location() {
        // Test when there's no location provided
        let base_path = derive_base_path_from_location("");
        
        assert!(base_path.is_none());
    }
    
    #[test]
    fn test_tagcontext_input_uri_storage_relative_path() {
        // Test relative local file path
        let context = TagContext::from_file_location("configs/app/config.yaml");
        
        assert!(context.input_uri.is_some());
        assert_eq!(
            context.input_uri.unwrap(),
            "configs/app/config.yaml"
        );
        
        // Test base path derivation
        let derived_base = derive_base_path_from_location("configs/app/config.yaml");
        assert!(derived_base.is_some());
        assert_eq!(
            derived_base.unwrap().to_string_lossy(),
            "configs/app"
        );
    }
    
    #[test]
    fn test_tagcontext_input_uri_storage_native_paths() {
        // Test native paths for the current platform
        #[cfg(unix)]
        let test_path = "/Users/test/configs/app/config.yaml";
        #[cfg(windows)]
        let test_path = "C:\\Users\\test\\configs\\app\\config.yaml";
        
        let context = TagContext::from_file_location(test_path);
        
        // Test input_uri storage
        assert!(context.input_uri.is_some());
        assert_eq!(context.input_uri.unwrap(), test_path);
        
        // Test base path derivation
        let derived_base = derive_base_path_from_location(test_path);
        assert!(derived_base.is_some());
        let base_path = derived_base.unwrap();
        let base_path_str = base_path.to_string_lossy();
        
        // Should contain parent directory components
        #[cfg(unix)]
        assert!(base_path_str.contains("configs") && base_path_str.contains("app"));
        #[cfg(windows)]
        assert!(base_path_str.contains("C:") && base_path_str.contains("configs") && base_path_str.contains("app"));
    }
    
    #[test]
    fn test_tagcontext_from_location_and_vars() {
        // Test TagContext creation with location and variables
        let mut variables = HashMap::new();
        variables.insert("test_var".to_string(), Value::String("test_value".to_string()));
        variables.insert("config".to_string(), Value::String("production".to_string()));
        
        let context = TagContext::from_location_and_vars("test.yaml", variables);
        
        assert_eq!(context.get_variable("test_var"), Some(&Value::String("test_value".to_string())));
        assert_eq!(context.get_variable("config"), Some(&Value::String("production".to_string())));
        assert_eq!(context.current_location(), Some("test.yaml".to_string()));
    }
    
    #[test] 
    fn test_tagcontext_stack_operations() {
        // Test that stack frames work correctly
        let context = TagContext::from_file_location("test.yaml")
            .with_stack_frame(StackFrame {
                location: Some("imported.yaml".to_string()), 
                path: "Root.config.database".to_string(),
            });
        
        assert_eq!(context.stack.len(), 2);
        assert_eq!(context.current_location(), Some("imported.yaml".to_string()));
        assert_eq!(context.current_path(), "Root.config.database");
    }
    
    #[test]
    fn test_derive_base_path_from_location_comprehensive_edge_cases() {
        // Test various edge cases for URL and path parsing
        let test_cases = vec![
            // (location, expected_base_path)
            ("s3://bucket", Some("s3://bucket/")), // No slash after bucket -> bucket root
            ("s3://bucket/", Some("s3://bucket/")), // Just trailing slash -> bucket root  
            ("https://example.com", Some("https://example.com/")), // No path -> domain root
            ("https://example.com/", Some("https://example.com/")), // Just root path -> domain root
            ("http://example.com/file", Some("http://example.com/")), // Root file -> domain root
            ("file.yaml", Some("")), // Just filename -> empty parent (using std::path::Path)
            ("./config.yaml", Some(".")), // Current directory
            ("../config.yaml", Some("..")), // Parent directory
        ];
        
        for (location, expected) in test_cases {
            let base_path = derive_base_path_from_location(location);
            
            match expected {
                Some(expected_path) => {
                    assert!(base_path.is_some(), "Expected base_path for location: {}", location);
                    assert_eq!(
                        base_path.unwrap().to_string_lossy(),
                        expected_path,
                        "Mismatch for location: {}",
                        location
                    );
                }
                None => {
                    assert!(base_path.is_none(), "Expected no base_path for location: {}", location);
                }
            }
        }
    }
}

impl TracingTagResolver {
    pub fn new() -> Self {
        Self {
            inner: StandardTagResolver,
        }
    }
    
    fn trace_resolution<T: std::fmt::Debug, F>(&self, tag_name: &str, _tag: &T, operation: F) -> Result<Value>
    where
        F: FnOnce() -> Result<Value>,
    {
        let start = std::time::Instant::now();
        let result = operation();
        let duration = start.elapsed();
        
        match &result {
            Ok(_) => {
                eprintln!("TRACE: {} tag resolved in {:?}", tag_name, duration);
            }
            Err(err) => {
                eprintln!("TRACE: {} tag failed in {:?}: {}", tag_name, duration, err);
            }
        }
        
        result
    }
}

impl TagResolver for TracingTagResolver {
    fn resolve_ast(&self, ast: &YamlAst, context: &TagContext) -> Result<Value> {
        self.inner.resolve_ast(ast, context)
    }
    
    fn yaml_value_to_json_value(&self, yaml_value: &Value) -> Result<serde_json::Value> {
        self.inner.yaml_value_to_json_value(yaml_value)
    }
    
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("include", tag, || self.inner.resolve_include(tag, context))
    }
    
    fn resolve_if(&self, tag: &IfTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("if", tag, || self.inner.resolve_if(tag, context))
    }
    
    fn resolve_map(&self, tag: &MapTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("map", tag, || self.inner.resolve_map(tag, context))
    }
    
    fn resolve_merge(&self, tag: &MergeTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("merge", tag, || self.inner.resolve_merge(tag, context))
    }
    
    fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("concat", tag, || self.inner.resolve_concat(tag, context))
    }
    
    fn resolve_let(&self, tag: &LetTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("let", tag, || self.inner.resolve_let(tag, context))
    }
    
    fn resolve_eq(&self, tag: &EqTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("eq", tag, || self.inner.resolve_eq(tag, context))
    }
    
    fn resolve_not(&self, tag: &NotTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("not", tag, || self.inner.resolve_not(tag, context))
    }
    
    fn resolve_split(&self, tag: &SplitTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("split", tag, || self.inner.resolve_split(tag, context))
    }
    
    fn resolve_join(&self, tag: &JoinTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("join", tag, || self.inner.resolve_join(tag, context))
    }
    
    fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("concatMap", tag, || self.inner.resolve_concat_map(tag, context))
    }
    
    fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("mergeMap", tag, || self.inner.resolve_merge_map(tag, context))
    }
    
    fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("mapListToHash", tag, || self.inner.resolve_map_list_to_hash(tag, context))
    }
    
    fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("mapValues", tag, || self.inner.resolve_map_values(tag, context))
    }
    
    fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("groupBy", tag, || self.inner.resolve_group_by(tag, context))
    }
    
    fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("fromPairs", tag, || self.inner.resolve_from_pairs(tag, context))
    }
    
    fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("toYamlString", tag, || self.inner.resolve_to_yaml_string(tag, context))
    }
    
    fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("parseYaml", tag, || self.inner.resolve_parse_yaml(tag, context))
    }
    
    fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("toJsonString", tag, || self.inner.resolve_to_json_string(tag, context))
    }
    
    fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("parseJson", tag, || self.inner.resolve_parse_json(tag, context))
    }
    
    fn resolve_escape(&self, tag: &EscapeTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("escape", tag, || self.inner.resolve_escape(tag, context))
    }
}
