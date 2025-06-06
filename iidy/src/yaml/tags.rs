//! Tag resolution and processing for YAML preprocessing
//! 
//! Contains the implementation logic for each custom preprocessing tag

use anyhow::{anyhow, Context, Result};
use serde_yaml::Value;
use std::collections::HashMap;

use crate::yaml::ast::*;

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

/// Processing environment that tracks state during YAML preprocessing
/// Modeled after iidy-js Env but more flexible for non-CFN documents
#[derive(Debug, Clone)]
pub struct ProcessingEnv {
    /// Global accumulator (optional - only used for CloudFormation templates or docs that need it)
    pub global_accumulator: Option<GlobalAccumulator>,
    /// Current scope variables (imports, defs, template parameters, loop variables)
    pub env_values: HashMap<String, Value>,
    /// Call stack for error reporting
    pub stack: Vec<StackFrame>,
}

impl Default for ProcessingEnv {
    fn default() -> Self {
        Self {
            global_accumulator: None,
            env_values: HashMap::new(),
            stack: Vec::new(),
        }
    }
}

impl ProcessingEnv {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create a new environment with CloudFormation global accumulator
    pub fn new_with_cfn_accumulator() -> Self {
        Self {
            global_accumulator: Some(GlobalAccumulator::default()),
            env_values: HashMap::new(),
            stack: Vec::new(),
        }
    }
    
    /// Create sub-environment with extended variables and stack (mimics iidy-js mkSubEnv)
    pub fn mk_sub_env(&self, new_values: HashMap<String, Value>, frame: StackFrame) -> Self {
        let mut env_values = self.env_values.clone();
        env_values.extend(new_values);
        
        let mut stack = self.stack.clone();
        stack.push(StackFrame {
            location: frame.location.or_else(|| self.current_location()),
            path: frame.path,
        });
        
        Self {
            global_accumulator: self.global_accumulator.clone(),
            env_values,
            stack,
        }
    }
    
    /// Get current location from stack (like iidy-js)
    pub fn current_location(&self) -> Option<String> {
        self.stack.last().and_then(|f| f.location.clone())
    }
    
    /// Get current path from stack
    pub fn current_path(&self) -> String {
        self.stack.last().map(|f| f.path.clone()).unwrap_or_default()
    }
    
    /// Add variable to current scope
    pub fn add_variable(&mut self, key: String, value: Value) {
        self.env_values.insert(key, value);
    }
    
    /// Get variable from current scope
    pub fn get_variable(&self, key: &str) -> Option<&Value> {
        self.env_values.get(key)
    }
}

/// Context for resolving preprocessing tags (lighter weight than ProcessingEnv)
#[derive(Debug, Default)]
pub struct TagContext {
    /// Variable bindings for current scope
    pub variables: HashMap<String, Value>,
    /// Base path for resolving relative includes
    pub base_path: Option<std::path::PathBuf>,
    /// Stack frames for error reporting
    pub stack: Vec<StackFrame>,
}

impl TagContext {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Create TagContext from ProcessingEnv for backward compatibility
    pub fn from_processing_env(env: &ProcessingEnv) -> Self {
        Self {
            variables: env.env_values.clone(),
            base_path: None, // TODO: Should be derived from current location if it's a file path
            stack: env.stack.clone(),
        }
    }

    /// Create a new context with additional variable bindings
    pub fn with_bindings(&self, bindings: HashMap<String, Value>) -> Self {
        let mut new_vars = self.variables.clone();
        new_vars.extend(bindings);
        Self {
            variables: new_vars,
            base_path: self.base_path.clone(),
            stack: self.stack.clone(),
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

    /// Set base path for includes
    pub fn with_base_path<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.base_path = Some(path.into());
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
    pub fn with_path_segment(&self, segment: &str) -> Self {
        let current_path = self.current_path();
        let new_path = if current_path.is_empty() {
            segment.to_string()
        } else {
            format!("{}.{}", current_path, segment)
        };
        
        let mut new_stack = self.stack.clone();
        if let Some(last_frame) = new_stack.last_mut() {
            last_frame.path = new_path;
        } else {
            // Create a new stack frame if none exists
            new_stack.push(StackFrame {
                location: self.current_location(),
                path: new_path,
            });
        }
        
        Self {
            variables: self.variables.clone(),
            base_path: self.base_path.clone(),
            stack: new_stack,
        }
    }
    
    /// Create a new context with an array index path segment
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
        
        Self {
            variables: self.variables.clone(),
            base_path: self.base_path.clone(),
            stack: new_stack,
        }
    }
}

/// Resolve an include tag
/// !$ tags only access variables in scope (from $defs, $imports, and local scoped variables)
/// They never perform file loading - that only happens during $imports processing
pub fn resolve_include_tag(tag: &IncludeTag, context: &TagContext) -> Result<Value> {
    let path = &tag.path;
    
    // Parse path and query
    let (base_path, query) = parse_path_and_query(path, &tag.query);
    
    // Try to resolve the variable from the environment
    if let Some(mut value) = resolve_dot_notation_path(&base_path, context) {
        // Apply query selector if present
        if let Some(query_str) = query {
            value = apply_query_selector(value, &query_str)?;
        }
        return Ok(value);
    }
    
    // Variable not found - error immediately with location context
    // Only variables from $defs, $imports, and local scoped variables are allowed
    let root_var = base_path.split('.').next().unwrap_or(&base_path).split('[').next().unwrap_or(&base_path);
    
    // Get location information - file name from base_path if available
    let location_info = if let Some(base_path) = &context.base_path {
        format!("in file '{}'", base_path.display())
    } else {
        context.current_location()
            .map(|loc| format!("in '{}'", loc))
            .unwrap_or_else(|| "in unknown location".to_string())
    };
    
    // Get YAML path information if available
    let yaml_path = context.current_path();
    let path_info = if !yaml_path.is_empty() {
        format!(" at path '{}'", yaml_path)
    } else {
        String::new()
    };
    
    Err(anyhow!(
        "Variable '{}' not found in environment {}{}\nOnly variables from $defs, $imports, and local scoped variables (like 'item' in !$map) are available.", 
        root_var, location_info, path_info
    ))
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
fn apply_query_selector(value: Value, query: &str) -> Result<Value> {
    match value {
        Value::Mapping(map) => {
            if query.starts_with('.') {
                // Handle nested path query like ".database.host"
                let path = &query[1..]; // Remove leading dot
                apply_nested_path_query(Value::Mapping(map), path)
            } else if query.contains(',') {
                // Handle multiple property selection like "database,host"
                let properties: Vec<&str> = query.split(',').map(|s| s.trim()).collect();
                let mut result = serde_yaml::Mapping::new();
                
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
fn apply_nested_path_query(value: Value, path: &str) -> Result<Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current_value = value;
    
    for part in parts {
        if part.is_empty() {
            continue;
        }
        
        match current_value {
            Value::Mapping(ref map) => {
                let key = Value::String(part.to_string());
                if let Some(next_value) = map.get(&key) {
                    current_value = next_value.clone();
                } else {
                    return Err(anyhow!("Property '{}' not found in path", part));
                }
            }
            _ => return Err(anyhow!("Cannot traverse path further at '{}'", part)),
        }
    }
    
    Ok(current_value)
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
    let mut current_value = context.get_variable(root_var)?.clone();
    
    // Traverse the path segments
    for segment in &path_segments[1..] {
        match current_value {
            Value::Mapping(ref map) => {
                let key = Value::String(segment.clone());
                current_value = map.get(&key)?.clone();
            }
            _ => return None, // Can't traverse further
        }
    }
    
    Some(current_value)
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

/// Resolve an if tag
pub fn resolve_if_tag(tag: &IfTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let condition_result = resolver.resolve_ast(&tag.condition, context)?;
    
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
        resolver.resolve_ast(&tag.then_value, context)
    } else if let Some(ref else_value) = tag.else_value {
        resolver.resolve_ast(else_value, context)
    } else {
        Ok(Value::Null)
    }
}

/// Resolve a map tag
pub fn resolve_map_tag(tag: &MapTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let source_result = resolver.resolve_ast(&tag.source, context)?;
    
    match source_result {
        Value::Sequence(seq) => {
            let mut result = Vec::new();
            let var_name = tag.var_name.as_deref().unwrap_or("item");
            
            for item in seq {
                // Create new context with the current item bound to the variable
                let mut item_bindings = HashMap::new();
                item_bindings.insert(var_name.to_string(), item);
                let item_context = context.with_bindings(item_bindings);
                
                let transformed = resolver.resolve_ast(&tag.transform, &item_context)?;
                result.push(transformed);
            }
            
            Ok(Value::Sequence(result))
        }
        _ => Err(anyhow!("Map source must be a sequence")),
    }
}

/// Resolve a merge tag
pub fn resolve_merge_tag(tag: &MergeTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let mut result = serde_yaml::Mapping::new();
    
    for source in &tag.sources {
        let source_result = resolver.resolve_ast(source, context)?;
        match source_result {
            Value::Mapping(map) => {
                result.extend(map);
            }
            _ => return Err(anyhow!("Merge source must be a mapping")),
        }
    }
    
    Ok(Value::Mapping(result))
}

/// Resolve a concat tag
pub fn resolve_concat_tag(tag: &ConcatTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let mut result = Vec::new();
    
    for source in &tag.sources {
        let source_result = resolver.resolve_ast(source, context)?;
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

/// Resolve a let tag
pub fn resolve_let_tag(tag: &LetTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let mut bindings = HashMap::new();
    
    // Resolve all variable bindings
    for (var_name, var_expr) in &tag.bindings {
        let var_value = resolver.resolve_ast(var_expr, context)?;
        bindings.insert(var_name.clone(), var_value);
    }
    
    // Create new context with bindings and resolve expression
    let new_context = context.with_bindings(bindings);
    resolver.resolve_ast(&tag.expression, &new_context)
}

/// Resolve an eq tag
pub fn resolve_eq_tag(tag: &EqTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let left = resolver.resolve_ast(&tag.left, context)?;
    let right = resolver.resolve_ast(&tag.right, context)?;
    
    let is_equal = values_equal(&left, &right);
    Ok(Value::Bool(is_equal))
}

/// Resolve a not tag
pub fn resolve_not_tag(tag: &NotTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let expr_result = resolver.resolve_ast(&tag.expression, context)?;
    
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

/// Resolve a split tag
pub fn resolve_split_tag(tag: &SplitTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let string_result = resolver.resolve_ast(&tag.string, context)?;
    
    match string_result {
        Value::String(s) => {
            let parts: Vec<Value> = s
                .split(&tag.delimiter)
                .map(|part| Value::String(part.to_string()))
                .collect();
            Ok(Value::Sequence(parts))
        }
        _ => Err(anyhow!("Split string must be a string value")),
    }
}

/// Resolve a join tag
pub fn resolve_join_tag(tag: &JoinTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let array_result = resolver.resolve_ast(&tag.array, context)?;
    
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
            
            let joined = strings?.join(&tag.delimiter);
            Ok(Value::String(joined))
        }
        _ => Err(anyhow!("Join array must be a sequence")),
    }
}

/// Compare two YAML values for equality
fn values_equal(left: &Value, right: &Value) -> bool {
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

/// Resolve a concatMap tag (map followed by concat)
pub fn resolve_concat_map_tag(tag: &ConcatMapTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let source_result = resolver.resolve_ast(&tag.source, context)?;
    
    match source_result {
        Value::Sequence(seq) => {
            let mut result = Vec::new();
            let var_name = tag.var_name.as_deref().unwrap_or("item");
            
            for item in seq {
                // Create new context with the current item bound to the variable
                let mut item_bindings = HashMap::new();
                item_bindings.insert(var_name.to_string(), item);
                let item_context = context.with_bindings(item_bindings);
                
                let transformed = resolver.resolve_ast(&tag.transform, &item_context)?;
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
        _ => Err(anyhow!("ConcatMap source must be a sequence")),
    }
}

/// Resolve a mergeMap tag (map followed by merge)
pub fn resolve_merge_map_tag(tag: &MergeMapTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let source_result = resolver.resolve_ast(&tag.source, context)?;
    
    match source_result {
        Value::Sequence(seq) => {
            let mut result = serde_yaml::Mapping::new();
            let var_name = tag.var_name.as_deref().unwrap_or("item");
            
            for item in seq {
                // Create new context with the current item bound to the variable
                let mut item_bindings = HashMap::new();
                item_bindings.insert(var_name.to_string(), item);
                let item_context = context.with_bindings(item_bindings);
                
                let transformed = resolver.resolve_ast(&tag.transform, &item_context)?;
                match transformed {
                    Value::Mapping(map) => {
                        result.extend(map);
                    }
                    _ => return Err(anyhow!("MergeMap transform must produce mappings")),
                }
            }
            
            Ok(Value::Mapping(result))
        }
        _ => Err(anyhow!("MergeMap source must be a sequence")),
    }
}

/// Resolve a mapListToHash tag (convert list of key-value pairs to hash)
pub fn resolve_map_list_to_hash_tag(tag: &MapListToHashTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let source_result = resolver.resolve_ast(&tag.source, context)?;
    
    match source_result {
        Value::Sequence(seq) => {
            let mut result = serde_yaml::Mapping::new();
            let key_field = tag.key_field.as_deref().unwrap_or("key");
            let value_field = tag.value_field.as_deref().unwrap_or("value");
            
            for item in seq {
                match item {
                    Value::Mapping(ref map) => {
                        let key_value = map.get(&Value::String(key_field.to_string()));
                        let value_value = map.get(&Value::String(value_field.to_string()));
                        
                        if let (Some(key), Some(value)) = (key_value, value_value) {
                            result.insert(key.clone(), value.clone());
                        }
                    }
                    _ => return Err(anyhow!("MapListToHash requires sequence of mappings with {} and {} fields", key_field, value_field)),
                }
            }
            
            Ok(Value::Mapping(result))
        }
        _ => Err(anyhow!("MapListToHash source must be a sequence")),
    }
}

/// Resolve a mapValues tag (transform object values while preserving keys)
pub fn resolve_map_values_tag(tag: &MapValuesTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let source_result = resolver.resolve_ast(&tag.source, context)?;
    
    match source_result {
        Value::Mapping(map) => {
            let mut result = serde_yaml::Mapping::new();
            let var_name = tag.var_name.as_deref().unwrap_or("value");
            
            for (key, value) in map {
                // Create new context with the current value and key bound to variables
                let mut value_bindings = HashMap::new();
                value_bindings.insert(var_name.to_string(), value);
                
                // Add the key as a string (convert from Value to string)
                let key_str = match &key {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    _ => format!("{:?}", key),
                };
                value_bindings.insert("key".to_string(), Value::String(key_str));
                
                let value_context = context.with_bindings(value_bindings);
                
                let transformed = resolver.resolve_ast(&tag.transform, &value_context)?;
                result.insert(key, transformed);
            }
            
            Ok(Value::Mapping(result))
        }
        _ => Err(anyhow!("MapValues source must be a mapping")),
    }
}

/// Resolve a groupBy tag (group items by key)
pub fn resolve_group_by_tag(tag: &GroupByTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let source_result = resolver.resolve_ast(&tag.source, context)?;
    
    match source_result {
        Value::Sequence(seq) => {
            let mut groups: std::collections::HashMap<String, Vec<Value>> = std::collections::HashMap::new();
            let var_name = tag.var_name.as_deref().unwrap_or("item");
            
            for item in seq {
                // Create new context with the current item bound to the variable
                let mut item_bindings = HashMap::new();
                item_bindings.insert(var_name.to_string(), item.clone());
                let item_context = context.with_bindings(item_bindings);
                
                let key_result = resolver.resolve_ast(&tag.key, &item_context)?;
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

/// Resolve a fromPairs tag (convert key-value pairs to object)
pub fn resolve_from_pairs_tag(tag: &FromPairsTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let source_result = resolver.resolve_ast(&tag.source, context)?;
    
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

/// Resolve a toYamlString tag (convert data to YAML string)
pub fn resolve_to_yaml_string_tag(tag: &ToYamlStringTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let data_result = resolver.resolve_ast(&tag.data, context)?;
    
    let yaml_string = serde_yaml::to_string(&data_result)
        .map_err(|e| anyhow!("Failed to convert data to YAML string: {}", e))?;
    
    // Remove trailing newline that serde_yaml adds
    let trimmed = yaml_string.trim_end().to_string();
    Ok(Value::String(trimmed))
}

/// Resolve a parseYaml tag (parse YAML string back to data)
pub fn resolve_parse_yaml_tag(tag: &ParseYamlTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let yaml_string_result = resolver.resolve_ast(&tag.yaml_string, context)?;
    
    match yaml_string_result {
        Value::String(yaml_str) => {
            serde_yaml::from_str(&yaml_str)
                .map_err(|e| anyhow!("Failed to parse YAML string: {}", e))
        }
        _ => Err(anyhow!("ParseYaml requires a string input")),
    }
}

/// Resolve a toJsonString tag (convert data to JSON string)
pub fn resolve_to_json_string_tag(tag: &ToJsonStringTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let data_result = resolver.resolve_ast(&tag.data, context)?;
    
    // Convert serde_yaml::Value to serde_json::Value
    let json_value = yaml_value_to_json_value(&data_result)?;
    
    let json_string = serde_json::to_string(&json_value)
        .map_err(|e| anyhow!("Failed to convert data to JSON string: {}", e))?;
    
    Ok(Value::String(json_string))
}

/// Resolve a parseJson tag (parse JSON string back to data)
pub fn resolve_parse_json_tag(tag: &ParseJsonTag, context: &TagContext, resolver: &dyn AstResolver) -> Result<Value> {
    let json_string_result = resolver.resolve_ast(&tag.json_string, context)?;
    
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

/// Resolve an escape tag (prevent preprocessing on child tree)
pub fn resolve_escape_tag(tag: &EscapeTag, _context: &TagContext, _resolver: &dyn AstResolver) -> Result<Value> {
    // For the escape tag, we need to convert the AST to Value without any preprocessing
    // This means we manually convert the AST while preserving any preprocessing tags as regular YAML
    escape_ast_to_value(&tag.content)
}

/// Convert AST to Value while escaping preprocessing tags (convert them to regular YAML)
fn escape_ast_to_value(ast: &YamlAst) -> Result<Value> {
    match ast {
        YamlAst::Null => Ok(Value::Null),
        YamlAst::Bool(b) => Ok(Value::Bool(*b)),
        YamlAst::Number(n) => Ok(Value::Number(n.clone())),
        YamlAst::String(s) => Ok(Value::String(s.clone())),
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
        YamlAst::UnknownYamlTag(tag) => {
            // Convert unknown tags to a string representation  
            Ok(Value::String(format!("__ESCAPED_TAG_{}__", tag.tag)))
        }
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
        Value::Tagged(_) => Err(anyhow!("Tagged values not supported in JSON conversion")),
    }
}

/// Trait for resolving AST nodes (used to avoid circular dependencies)
pub trait AstResolver {
    fn resolve_ast(&self, ast: &YamlAst, context: &TagContext) -> Result<Value>;
}

/// Trait for resolving preprocessing tags with different implementation strategies
pub trait TagResolver {
    // Core tag resolution methods
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value>;
    fn resolve_if(&self, tag: &IfTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_map(&self, tag: &MapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_merge(&self, tag: &MergeTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_let(&self, tag: &LetTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_eq(&self, tag: &EqTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_not(&self, tag: &NotTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_split(&self, tag: &SplitTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_join(&self, tag: &JoinTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    
    // Advanced transformation tags
    fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    
    // String processing tags
    fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
    fn resolve_escape(&self, tag: &EscapeTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value>;
}

/// Standard implementation of TagResolver
pub struct StandardTagResolver;

impl TagResolver for StandardTagResolver {
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value> {
        resolve_include_tag(tag, context)
    }
    
    fn resolve_if(&self, tag: &IfTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_if_tag(tag, context, ast_resolver)
    }
    
    fn resolve_map(&self, tag: &MapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_map_tag(tag, context, ast_resolver)
    }
    
    fn resolve_merge(&self, tag: &MergeTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_merge_tag(tag, context, ast_resolver)
    }
    
    fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_concat_tag(tag, context, ast_resolver)
    }
    
    fn resolve_let(&self, tag: &LetTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_let_tag(tag, context, ast_resolver)
    }
    
    fn resolve_eq(&self, tag: &EqTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_eq_tag(tag, context, ast_resolver)
    }
    
    fn resolve_not(&self, tag: &NotTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_not_tag(tag, context, ast_resolver)
    }
    
    fn resolve_split(&self, tag: &SplitTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_split_tag(tag, context, ast_resolver)
    }
    
    fn resolve_join(&self, tag: &JoinTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_join_tag(tag, context, ast_resolver)
    }
    
    fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_concat_map_tag(tag, context, ast_resolver)
    }
    
    fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_merge_map_tag(tag, context, ast_resolver)
    }
    
    fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_map_list_to_hash_tag(tag, context, ast_resolver)
    }
    
    fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_map_values_tag(tag, context, ast_resolver)
    }
    
    fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_group_by_tag(tag, context, ast_resolver)
    }
    
    fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_from_pairs_tag(tag, context, ast_resolver)
    }
    
    fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_to_yaml_string_tag(tag, context, ast_resolver)
    }
    
    fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_parse_yaml_tag(tag, context, ast_resolver)
    }
    
    fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_to_json_string_tag(tag, context, ast_resolver)
    }
    
    fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_parse_json_tag(tag, context, ast_resolver)
    }
    
    fn resolve_escape(&self, tag: &EscapeTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        resolve_escape_tag(tag, context, ast_resolver)
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
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value> {
        let result = self.inner.resolve_include(tag, context);
        self.log_resolution("include", tag, &result);
        result
    }
    
    fn resolve_if(&self, tag: &IfTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_if(tag, context, ast_resolver);
        self.log_resolution("if", tag, &result);
        result
    }
    
    fn resolve_map(&self, tag: &MapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_map(tag, context, ast_resolver);
        self.log_resolution("map", tag, &result);
        result
    }
    
    fn resolve_merge(&self, tag: &MergeTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_merge(tag, context, ast_resolver);
        self.log_resolution("merge", tag, &result);
        result
    }
    
    fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_concat(tag, context, ast_resolver);
        self.log_resolution("concat", tag, &result);
        result
    }
    
    fn resolve_let(&self, tag: &LetTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_let(tag, context, ast_resolver);
        self.log_resolution("let", tag, &result);
        result
    }
    
    fn resolve_eq(&self, tag: &EqTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_eq(tag, context, ast_resolver);
        self.log_resolution("eq", tag, &result);
        result
    }
    
    fn resolve_not(&self, tag: &NotTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_not(tag, context, ast_resolver);
        self.log_resolution("not", tag, &result);
        result
    }
    
    fn resolve_split(&self, tag: &SplitTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_split(tag, context, ast_resolver);
        self.log_resolution("split", tag, &result);
        result
    }
    
    fn resolve_join(&self, tag: &JoinTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_join(tag, context, ast_resolver);
        self.log_resolution("join", tag, &result);
        result
    }
    
    fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_concat_map(tag, context, ast_resolver);
        self.log_resolution("concatMap", tag, &result);
        result
    }
    
    fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_merge_map(tag, context, ast_resolver);
        self.log_resolution("mergeMap", tag, &result);
        result
    }
    
    fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_map_list_to_hash(tag, context, ast_resolver);
        self.log_resolution("mapListToHash", tag, &result);
        result
    }
    
    fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_map_values(tag, context, ast_resolver);
        self.log_resolution("mapValues", tag, &result);
        result
    }
    
    fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_group_by(tag, context, ast_resolver);
        self.log_resolution("groupBy", tag, &result);
        result
    }
    
    fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_from_pairs(tag, context, ast_resolver);
        self.log_resolution("fromPairs", tag, &result);
        result
    }
    
    fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_to_yaml_string(tag, context, ast_resolver);
        self.log_resolution("toYamlString", tag, &result);
        result
    }
    
    fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_parse_yaml(tag, context, ast_resolver);
        self.log_resolution("parseYaml", tag, &result);
        result
    }
    
    fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_to_json_string(tag, context, ast_resolver);
        self.log_resolution("toJsonString", tag, &result);
        result
    }
    
    fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_parse_json(tag, context, ast_resolver);
        self.log_resolution("parseJson", tag, &result);
        result
    }
    
    fn resolve_escape(&self, tag: &EscapeTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        let result = self.inner.resolve_escape(tag, context, ast_resolver);
        self.log_resolution("escape", tag, &result);
        result
    }
}

/// Tracing implementation that collects performance metrics
pub struct TracingTagResolver {
    inner: StandardTagResolver,
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
    fn resolve_include(&self, tag: &IncludeTag, context: &TagContext) -> Result<Value> {
        self.trace_resolution("include", tag, || self.inner.resolve_include(tag, context))
    }
    
    fn resolve_if(&self, tag: &IfTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("if", tag, || self.inner.resolve_if(tag, context, ast_resolver))
    }
    
    fn resolve_map(&self, tag: &MapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("map", tag, || self.inner.resolve_map(tag, context, ast_resolver))
    }
    
    fn resolve_merge(&self, tag: &MergeTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("merge", tag, || self.inner.resolve_merge(tag, context, ast_resolver))
    }
    
    fn resolve_concat(&self, tag: &ConcatTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("concat", tag, || self.inner.resolve_concat(tag, context, ast_resolver))
    }
    
    fn resolve_let(&self, tag: &LetTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("let", tag, || self.inner.resolve_let(tag, context, ast_resolver))
    }
    
    fn resolve_eq(&self, tag: &EqTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("eq", tag, || self.inner.resolve_eq(tag, context, ast_resolver))
    }
    
    fn resolve_not(&self, tag: &NotTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("not", tag, || self.inner.resolve_not(tag, context, ast_resolver))
    }
    
    fn resolve_split(&self, tag: &SplitTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("split", tag, || self.inner.resolve_split(tag, context, ast_resolver))
    }
    
    fn resolve_join(&self, tag: &JoinTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("join", tag, || self.inner.resolve_join(tag, context, ast_resolver))
    }
    
    fn resolve_concat_map(&self, tag: &ConcatMapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("concatMap", tag, || self.inner.resolve_concat_map(tag, context, ast_resolver))
    }
    
    fn resolve_merge_map(&self, tag: &MergeMapTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("mergeMap", tag, || self.inner.resolve_merge_map(tag, context, ast_resolver))
    }
    
    fn resolve_map_list_to_hash(&self, tag: &MapListToHashTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("mapListToHash", tag, || self.inner.resolve_map_list_to_hash(tag, context, ast_resolver))
    }
    
    fn resolve_map_values(&self, tag: &MapValuesTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("mapValues", tag, || self.inner.resolve_map_values(tag, context, ast_resolver))
    }
    
    fn resolve_group_by(&self, tag: &GroupByTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("groupBy", tag, || self.inner.resolve_group_by(tag, context, ast_resolver))
    }
    
    fn resolve_from_pairs(&self, tag: &FromPairsTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("fromPairs", tag, || self.inner.resolve_from_pairs(tag, context, ast_resolver))
    }
    
    fn resolve_to_yaml_string(&self, tag: &ToYamlStringTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("toYamlString", tag, || self.inner.resolve_to_yaml_string(tag, context, ast_resolver))
    }
    
    fn resolve_parse_yaml(&self, tag: &ParseYamlTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("parseYaml", tag, || self.inner.resolve_parse_yaml(tag, context, ast_resolver))
    }
    
    fn resolve_to_json_string(&self, tag: &ToJsonStringTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("toJsonString", tag, || self.inner.resolve_to_json_string(tag, context, ast_resolver))
    }
    
    fn resolve_parse_json(&self, tag: &ParseJsonTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("parseJson", tag, || self.inner.resolve_parse_json(tag, context, ast_resolver))
    }
    
    fn resolve_escape(&self, tag: &EscapeTag, context: &TagContext, ast_resolver: &dyn AstResolver) -> Result<Value> {
        self.trace_resolution("escape", tag, || self.inner.resolve_escape(tag, context, ast_resolver))
    }
}