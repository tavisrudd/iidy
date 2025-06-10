//! Tests for the hierarchical scope system and variable origin tracking
//!
//! Tests the enhanced scope system that tracks variable origins,
//! provides hierarchical variable resolution, and enables better
//! error reporting with import chain context.

use serde_yaml::Value;

use iidy::yaml::resolution::resolver::{
    TagContext, Scope, ScopeType, ScopedVariable, VariableSource
};

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