//! Tag resolution context and scope management
//! 
//! Contains types and functionality for managing variable scopes and context
//! during YAML preprocessing tag resolution.

use serde_yaml::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Performance optimization: Global counter for scope ID generation
static SCOPE_ID_COUNTER: AtomicUsize = AtomicUsize::new(1);

/// Generate unique scope ID without expensive UUID generation
fn next_scope_id() -> usize {
    SCOPE_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Unique identifier for a scope
pub type ScopeId = String;

/// Context for resolving preprocessing tags
#[derive(Debug, Default)]
pub struct TagContext {
    /// Variable bindings for current scope
    pub variables: HashMap<String, Value>,
    /// URI of the input document being processed (for error reporting and relative imports)
    pub input_uri: Option<String>,
    /// Global accumulator (optional - only used for CloudFormation templates or docs that need it)
    pub global_accumulator: Option<GlobalAccumulator>,
    /// Enhanced scope tracking (optional - for advanced variable origin tracking)
    pub scope_context: Option<ScopeContext>,
}

/// Global accumulator for document-wide state (optional, not all docs need this)
#[derive(Debug, Clone, Default)]
pub struct GlobalAccumulator {
    /// CloudFormation global sections (Parameters, Outputs, etc.) if processing CFN templates
    pub cfn_globals: HashMap<String, Value>,
    /// Custom accumulator data for other document types
    pub custom_data: HashMap<String, Value>,
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
            global_accumulator: None,
            scope_context: None,
        }
    }
    
    /// Create TagContext with CloudFormation global accumulator
    pub fn new_with_cfn_accumulator() -> Self {
        Self {
            variables: HashMap::new(),
            input_uri: None,
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

#[cfg(test)]
mod tests {
    use super::*;
    /// Derive base path from location for testing
    fn derive_base_path_from_location(location: &str) -> Option<std::path::PathBuf> {
        use std::path::Path;
        
        if location.is_empty() {
            return None;
        }
        
        // Try to parse as URL first (S3, HTTP, HTTPS)
        if let Ok(url) = url::Url::parse(location) {
            let mut url_path = url.path();
            
            // Remove the filename to get the directory
            if let Some(last_slash) = url_path.rfind('/') {
                url_path = &url_path[..last_slash + 1];
            } else {
                // No path component, just the root
                url_path = "/";
            }
            
            // Reconstruct the URL with just the directory path
            let mut base_url = url.clone();
            base_url.set_path(url_path);
            
            return Some(Path::new(base_url.as_str()).to_path_buf());
        }
        
        // Handle local file paths
        let path = Path::new(location);
        path.parent().map(|p| p.to_path_buf())
    }

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

    // Scope system tests moved from tests/scope_system_tests.rs
    #[test]
    fn test_scope_context_creation() {
        // Test creating a basic scope context
        let input_uri = "file:///test/main.yaml".to_string();
        let context = TagContext::with_scope_tracking(input_uri.clone());
        
        assert!(context.scope_context.is_some());
        let scope_context = context.scope_context.unwrap();
        
        // Should have a global scope
        assert_eq!(scope_context.current_scope.scope_type, ScopeType::Global);
        assert_eq!(scope_context.current_scope.source_uri, Some(input_uri));
        assert_eq!(scope_context.scope_stack.len(), 1);
        
        // Should have one scope in the scopes map
        assert_eq!(scope_context.scopes.len(), 1);
    }

    #[test] 
    fn test_scoped_variable_addition_and_resolution() {
        let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
        
        // Add variables with different sources
        context.add_scoped_variable(
            "env", 
            Value::String("production".to_string()),
            VariableSource::LocalDefs,
            Some("main.yaml:5".to_string())
        );
        
        context.add_scoped_variable(
            "app_name",
            Value::String("my-app".to_string()),
            VariableSource::ImportedDocument("config".to_string()),
            Some("config.yaml:2".to_string())
        );
        
        // Test legacy variable access (backward compatibility)
        assert_eq!(context.variables.get("env").unwrap(), &Value::String("production".to_string()));
        assert_eq!(context.variables.get("app_name").unwrap(), &Value::String("my-app".to_string()));
        
        // Test scoped variable resolution
        let env_var = context.resolve_scoped_variable("env").unwrap();
        assert_eq!(env_var.value, Value::String("production".to_string()));
        assert_eq!(env_var.source, VariableSource::LocalDefs);
        assert_eq!(env_var.defined_at, Some("main.yaml:5".to_string()));
        
        let app_var = context.resolve_scoped_variable("app_name").unwrap();
        assert_eq!(app_var.value, Value::String("my-app".to_string()));
        assert_eq!(app_var.source, VariableSource::ImportedDocument("config".to_string()));
        assert_eq!(app_var.defined_at, Some("config.yaml:2".to_string()));
    }

    #[test]
    fn test_variable_origin_tracking() {
        let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
        
        // Add variables from different sources
        context.add_scoped_variable(
            "local_var", 
            Value::String("local".to_string()),
            VariableSource::LocalDefs,
            Some("main.yaml:10".to_string())
        );
        
        context.add_scoped_variable(
            "imported_var",
            Value::String("imported".to_string()),
            VariableSource::ImportedDocument("database".to_string()),
            Some("database.yaml:3".to_string())
        );
        
        context.add_scoped_variable(
            "tag_var",
            Value::String("tag_bound".to_string()),
            VariableSource::TagBinding("!$let".to_string()),
            None
        );
        
        // Test origin reporting
        assert_eq!(context.get_variable_origin("local_var"), Some("local $defs".to_string()));
        assert_eq!(context.get_variable_origin("imported_var"), Some("imported from 'database'".to_string()));
        assert_eq!(context.get_variable_origin("tag_var"), Some("bound in !$let".to_string()));
        assert_eq!(context.get_variable_origin("nonexistent"), None);
    }

    #[test]
    fn test_hierarchical_scope_creation() {
        let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
        
        // Add a variable to global scope
        context.add_scoped_variable(
            "global_var",
            Value::String("global".to_string()),
            VariableSource::LocalDefs,
            Some("main.yaml:1".to_string())
        );
        
        // Create an import scope
        context.create_import_scope("config", "file:///test/config.yaml");
        
        // Add a variable to import scope
        context.add_scoped_variable(
            "import_var",
            Value::String("from_import".to_string()),
            VariableSource::ImportedDocument("config".to_string()),
            Some("config.yaml:1".to_string())
        );
        
        // Should be able to resolve both variables
        let global_var = context.resolve_scoped_variable("global_var").unwrap();
        assert_eq!(global_var.value, Value::String("global".to_string()));
        assert_eq!(global_var.source, VariableSource::LocalDefs);
        
        let import_var = context.resolve_scoped_variable("import_var").unwrap();
        assert_eq!(import_var.value, Value::String("from_import".to_string()));
        assert_eq!(import_var.source, VariableSource::ImportedDocument("config".to_string()));
        
        // Verify scope hierarchy
        if let Some(ref scope_context) = context.scope_context {
            assert_eq!(scope_context.scope_stack.len(), 2); // global + import
            assert_eq!(scope_context.current_scope.scope_type, ScopeType::ImportedDocument("config".to_string()));
            assert!(scope_context.current_scope.parent_scope_id.is_some());
        }
    }

    #[test]
    fn test_scope_popping() {
        let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
        
        // Add global variable
        context.add_scoped_variable(
            "global_var",
            Value::String("global".to_string()),
            VariableSource::LocalDefs,
            None
        );
        
        // Create import scope and add variable
        context.create_import_scope("config", "file:///test/config.yaml");
        context.add_scoped_variable(
            "import_var",
            Value::String("imported".to_string()),
            VariableSource::ImportedDocument("config".to_string()),
            None
        );
        
        // Should be in import scope
        if let Some(ref scope_context) = context.scope_context {
            assert_eq!(scope_context.scope_stack.len(), 2);
            assert_eq!(scope_context.current_scope.scope_type, ScopeType::ImportedDocument("config".to_string()));
        }
        
        // Pop back to global scope
        context.pop_scope();
        
        // Should be back in global scope
        if let Some(ref scope_context) = context.scope_context {
            assert_eq!(scope_context.scope_stack.len(), 1);
            assert_eq!(scope_context.current_scope.scope_type, ScopeType::Global);
        }
        
        // Should still be able to resolve global variable
        assert!(context.resolve_scoped_variable("global_var").is_some());
        // Should NOT be able to resolve import variable from global scope
        assert!(context.resolve_scoped_variable("import_var").is_none());
    }

    #[test]
    fn test_import_dependency_graph() {
        let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
        
        // Create a chain: main -> config -> database
        context.create_import_scope("config", "file:///test/config.yaml");
        context.create_import_scope("database", "file:///test/database.yaml");
        
        // Get dependency graph
        let deps = context.get_import_dependency_graph();
        
        // Should have dependencies
        assert!(!deps.is_empty());
        println!("Dependency graph: {:?}", deps);
        
        // The graph should show the import relationships
        // Note: The exact structure depends on how we build scopes
    }

    #[test]
    fn test_backward_compatibility() {
        // Test that old TagContext methods still work without scope tracking
        let mut context = TagContext::new();
        
        // Add variable the old way
        context.variables.insert("test_var".to_string(), Value::String("test".to_string()));
        
        // Should work as before
        assert_eq!(context.variables.get("test_var"), Some(&Value::String("test".to_string())));
        
        // Scope methods should handle None scope_context gracefully
        assert_eq!(context.resolve_scoped_variable("test_var"), None);
        assert_eq!(context.get_variable_origin("test_var"), None);
    }

    #[test]
    fn test_variable_shadowing_in_scopes() {
        let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
        
        // Add variable to global scope
        context.add_scoped_variable(
            "env",
            Value::String("global_env".to_string()),
            VariableSource::LocalDefs,
            Some("main.yaml:1".to_string())
        );
        
        // Create import scope with same variable name
        context.create_import_scope("config", "file:///test/config.yaml");
        context.add_scoped_variable(
            "env",
            Value::String("imported_env".to_string()),
            VariableSource::ImportedDocument("config".to_string()),
            Some("config.yaml:1".to_string())
        );
        
        // Should resolve to import scope version (current scope wins)
        let resolved = context.resolve_scoped_variable("env").unwrap();
        assert_eq!(resolved.value, Value::String("imported_env".to_string()));
        assert_eq!(resolved.source, VariableSource::ImportedDocument("config".to_string()));
        
        // Pop scope - should now resolve to global version
        context.pop_scope();
        let resolved = context.resolve_scoped_variable("env").unwrap();
        assert_eq!(resolved.value, Value::String("global_env".to_string()));
        assert_eq!(resolved.source, VariableSource::LocalDefs);
    }

    #[test]
    fn test_scoped_variable_creation_helpers() {
        // Test ScopedVariable helper methods
        let var = ScopedVariable::new(
            Value::String("test".to_string()),
            VariableSource::LocalDefs,
            Some("test.yaml:10".to_string())
        );
        
        assert_eq!(var.value, Value::String("test".to_string()));
        assert_eq!(var.source, VariableSource::LocalDefs);
        assert_eq!(var.defined_at, Some("test.yaml:10".to_string()));
        assert_eq!(var.line_number, None);
        assert_eq!(var.column_number, None);
    }

    #[test]
    fn test_scope_creation_helpers() {
        // Test Scope helper methods
        let scope = Scope::new(
            ScopeType::ImportedDocument("test".to_string()),
            Some("file:///test.yaml".to_string())
        );
        
        assert!(scope.id.starts_with("import_test_"));
        assert_eq!(scope.scope_type, ScopeType::ImportedDocument("test".to_string()));
        assert_eq!(scope.source_uri, Some("file:///test.yaml".to_string()));
        assert!(scope.variables.is_empty());
        assert_eq!(scope.parent_scope_id, None);
        assert!(scope.child_scope_ids.is_empty());
    }

    #[test]
    fn test_multiple_import_scopes() {
        let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
        
        // Create multiple sibling import scopes
        context.create_import_scope("database", "file:///test/database.yaml");
        context.add_scoped_variable(
            "db_host",
            Value::String("localhost".to_string()),
            VariableSource::ImportedDocument("database".to_string()),
            None
        );
        
        // Pop back to global and create another import scope
        context.pop_scope();
        context.create_import_scope("features", "file:///test/features.yaml");
        context.add_scoped_variable(
            "auth_enabled",
            Value::String("true".to_string()),
            VariableSource::ImportedDocument("features".to_string()),
            None
        );
        
        // Should only see features variable in current scope
        assert!(context.resolve_scoped_variable("auth_enabled").is_some());
        assert!(context.resolve_scoped_variable("db_host").is_none());
        
        // Pop back to global - should see neither
        context.pop_scope();
        assert!(context.resolve_scoped_variable("auth_enabled").is_none());
        assert!(context.resolve_scoped_variable("db_host").is_none());
    }
}