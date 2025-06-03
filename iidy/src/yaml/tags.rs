//! Tag resolution and processing for YAML preprocessing
//! 
//! Contains the implementation logic for each custom preprocessing tag

use anyhow::{anyhow, Result};
use serde_yaml::Value;
use std::collections::HashMap;

use crate::yaml::ast::*;

/// Context for resolving preprocessing tags
#[derive(Debug, Default)]
pub struct TagContext {
    /// Variable bindings for current scope
    pub variables: HashMap<String, Value>,
    /// Base path for resolving relative includes
    pub base_path: Option<std::path::PathBuf>,
}

impl TagContext {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new context with additional variable bindings
    pub fn with_bindings(&self, bindings: HashMap<String, Value>) -> Self {
        let mut new_vars = self.variables.clone();
        new_vars.extend(bindings);
        Self {
            variables: new_vars,
            base_path: self.base_path.clone(),
        }
    }

    /// Get a variable value by name
    pub fn get_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    /// Set base path for includes
    pub fn with_base_path<P: Into<std::path::PathBuf>>(mut self, path: P) -> Self {
        self.base_path = Some(path.into());
        self
    }
}

/// Resolve an include tag
pub fn resolve_include_tag(tag: &IncludeTag, context: &TagContext) -> Result<Value> {
    // TODO: Implement actual file reading
    // For now, return a placeholder
    let path = &tag.path;
    
    // Handle different include formats:
    // - File paths: "./config.yaml", "/abs/path.yaml"
    // - URLs: "https://example.com/config.yaml"
    // - Special imports: "AWS::EC2::Instance", etc.
    
    if path.starts_with("http://") || path.starts_with("https://") {
        // TODO: HTTP includes
        Err(anyhow!("HTTP includes not yet implemented"))
    } else if path.contains("::") {
        // AWS CloudFormation resource type or similar
        // TODO: Handle special imports
        Err(anyhow!("Special imports not yet implemented"))
    } else {
        // File include
        let resolved_path = if let Some(base) = &context.base_path {
            base.join(path)
        } else {
            std::path::PathBuf::from(path)
        };
        
        // TODO: Read and parse the file
        // For now, return placeholder
        Ok(Value::String(format!("TODO: Include content from {}", resolved_path.display())))
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

/// Trait for resolving AST nodes (used to avoid circular dependencies)
pub trait AstResolver {
    fn resolve_ast(&self, ast: &YamlAst, context: &TagContext) -> Result<Value>;
}