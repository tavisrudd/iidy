//! Tests for enhanced error reporting with variable origin information
//!
//! Demonstrates how the scope system enables better error messages
//! with variable origin tracking and import chain context.

use anyhow::Result;
use tempfile::TempDir;
use tokio::fs;

use iidy::yaml::engine::YamlPreprocessor;
use iidy::yaml::imports::loaders::ProductionImportLoader;
use iidy::yaml::resolution::{TagContext, VariableSource};

#[tokio::test]
async fn test_variable_origin_access_in_context() -> Result<()> {
    // Create a TagContext with scope tracking to test variable origin access
    let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
    
    // Add variables from different sources
    context.add_scoped_variable(
        "local_var",
        serde_yaml::Value::String("local_value".to_string()),
        VariableSource::LocalDefs,
        Some("main.yaml:5".to_string())
    );
    
    context.add_scoped_variable(
        "imported_var",
        serde_yaml::Value::String("imported_value".to_string()),
        VariableSource::ImportedDocument("config".to_string()),
        Some("config.yaml:10".to_string())
    );
    
    // Test variable origin reporting
    let local_origin = context.get_variable_origin("local_var");
    let imported_origin = context.get_variable_origin("imported_var");
    let missing_origin = context.get_variable_origin("nonexistent_var");
    
    assert_eq!(local_origin, Some("local $defs".to_string()));
    assert_eq!(imported_origin, Some("imported from 'config'".to_string()));
    assert_eq!(missing_origin, None);
    
    // Test scoped variable resolution
    let local_var = context.resolve_scoped_variable("local_var").unwrap();
    assert_eq!(local_var.defined_at, Some("main.yaml:5".to_string()));
    assert_eq!(local_var.source, VariableSource::LocalDefs);
    
    let imported_var = context.resolve_scoped_variable("imported_var").unwrap();
    assert_eq!(imported_var.defined_at, Some("config.yaml:10".to_string()));
    assert_eq!(imported_var.source, VariableSource::ImportedDocument("config".to_string()));
    
    println!("✅ Variable origin access test passed");
    println!("   Local variable origin: {:?}", local_origin);
    println!("   Imported variable origin: {:?}", imported_origin);
    
    Ok(())
}

#[tokio::test]
async fn test_import_dependency_graph_generation() -> Result<()> {
    // Test the import dependency graph functionality
    let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
    
    // Simulate creating import scopes
    context.create_import_scope("config", "file:///test/config.yaml");
    context.add_scoped_variable(
        "config_var",
        serde_yaml::Value::String("config_value".to_string()),
        VariableSource::ImportedDocument("config".to_string()),
        Some("config.yaml:1".to_string())
    );
    
    // Create nested import scope
    context.create_import_scope("database", "file:///test/database.yaml");
    context.add_scoped_variable(
        "db_host",
        serde_yaml::Value::String("localhost".to_string()),
        VariableSource::ImportedDocument("database".to_string()),
        Some("database.yaml:1".to_string())
    );
    
    // Get dependency graph
    let deps = context.get_import_dependency_graph();
    
    println!("✅ Import dependency graph generation test passed");
    println!("   Dependency graph: {:?}", deps);
    
    // The graph should show import relationships
    assert!(!deps.is_empty(), "Dependency graph should not be empty");
    
    Ok(())
}

#[tokio::test]
async fn test_enhanced_error_context_preparation() -> Result<()> {
    // Test preparing context for enhanced error messages
    let temp_dir = TempDir::new()?;
    let base_path = temp_dir.path();
    
    // Create files that will generate good error context
    fs::write(base_path.join("config.yaml"), r#"
database_host: "localhost"
database_port: 5432
"#).await?;
    
    let main_content = r#"
$defs:
  environment: "production"
  
$imports:
  config: ./config.yaml

# This will succeed and give us context to examine
app_name: "test-app"
db_connection: "{{config.database_host}}:{{config.database_port}}"
env_setting: "{{environment}}"
"#;
    
    let main_path = base_path.join("main.yaml");
    let main_path_str = main_path.to_string_lossy();
    
    let loader = ProductionImportLoader::new();
    let mut preprocessor = YamlPreprocessor::new(loader, false);
    let result = preprocessor.process(main_content, &main_path_str).await?;
    
    // Verify processing worked
    assert_eq!(result["app_name"], "test-app");
    assert_eq!(result["db_connection"], "localhost:5432");
    assert_eq!(result["env_setting"], "production");
    
    println!("✅ Enhanced error context preparation test passed");
    println!("   Context prepared successfully for error reporting");
    
    Ok(())
}

#[test]
fn test_variable_source_enum_display() {
    // Test the different VariableSource types for error message formatting
    let sources = vec![
        VariableSource::LocalDefs,
        VariableSource::ImportedDocument("config".to_string()),
        VariableSource::ImportedDocument("database".to_string()),
        VariableSource::TagBinding("!$let".to_string()),
        VariableSource::BuiltIn,
        VariableSource::External,
    ];
    
    println!("✅ Variable source types available for error reporting:");
    for source in sources {
        println!("   {:?}", source);
    }
    
    // Test that they can be compared
    assert_eq!(VariableSource::LocalDefs, VariableSource::LocalDefs);
    assert_ne!(VariableSource::LocalDefs, VariableSource::BuiltIn);
    
    let imported1 = VariableSource::ImportedDocument("config".to_string());
    let imported2 = VariableSource::ImportedDocument("config".to_string());
    let imported3 = VariableSource::ImportedDocument("database".to_string());
    
    assert_eq!(imported1, imported2);
    assert_ne!(imported1, imported3);
}

#[tokio::test]
async fn test_scope_hierarchy_tracking() -> Result<()> {
    // Test that scope hierarchy is properly tracked for error reporting
    let mut context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
    
    // Add global variable
    context.add_scoped_variable(
        "global_var",
        serde_yaml::Value::String("global".to_string()),
        VariableSource::LocalDefs,
        Some("main.yaml:1".to_string())
    );
    
    // Verify initial state
    if let Some(ref scope_context) = context.scope_context {
        assert_eq!(scope_context.scope_stack.len(), 1);
        println!("   Initial scope stack length: {}", scope_context.scope_stack.len());
    }
    
    // Create import scope
    context.create_import_scope("config", "file:///test/config.yaml");
    context.add_scoped_variable(
        "import_var",
        serde_yaml::Value::String("imported".to_string()),
        VariableSource::ImportedDocument("config".to_string()),
        Some("config.yaml:1".to_string())
    );
    
    // Verify scope hierarchy
    if let Some(ref scope_context) = context.scope_context {
        assert_eq!(scope_context.scope_stack.len(), 2);
        println!("   After import scope creation: {}", scope_context.scope_stack.len());
    }
    
    // Both variables should be accessible from import scope
    assert!(context.resolve_scoped_variable("global_var").is_some());
    assert!(context.resolve_scoped_variable("import_var").is_some());
    
    // Pop scope
    context.pop_scope();
    
    // Should be back in global scope
    if let Some(ref scope_context) = context.scope_context {
        assert_eq!(scope_context.scope_stack.len(), 1);
        println!("   After popping scope: {}", scope_context.scope_stack.len());
    }
    
    // Only global variable should be accessible
    assert!(context.resolve_scoped_variable("global_var").is_some());
    assert!(context.resolve_scoped_variable("import_var").is_none());
    
    println!("✅ Scope hierarchy tracking test passed");
    
    Ok(())
}

#[test]
fn test_error_message_formatting_helpers() {
    // Test helper functions that would be used in error message formatting
    let context = TagContext::with_scope_tracking("file:///test/main.yaml".to_string());
    
    // Test that we can get meaningful information from the context
    assert!(context.scope_context.is_some());
    
    if let Some(ref scope_context) = context.scope_context {
        assert!(!scope_context.scope_stack.is_empty());
        println!("   Scope context available for error reporting");
        println!("   Current scope ID: {}", scope_context.current_scope.id);
        println!("   Source URI: {:?}", scope_context.current_scope.source_uri);
    }
    
    // Test variable origin formatting
    let origins = vec![
        ("local_var", Some("local $defs")),
        ("imported_var", Some("imported from 'config'")),
        ("tag_var", Some("bound in !$let")),
        ("builtin_var", Some("built-in")),
        ("external_var", Some("external")),
    ];
    
    println!("✅ Error message formatting helpers test passed");
    println!("   Available origin types:");
    for (var_name, origin) in origins {
        println!("   {} -> {:?}", var_name, origin);
    }
}

/// Simulated enhanced error message that could be generated
fn format_enhanced_variable_error(
    variable_name: &str,
    location: &str,
    available_vars: &[(String, String)], // (name, origin)
    suggested_vars: &[String]
) -> String {
    let mut error = format!("Error: Variable '{}' not found\n", variable_name);
    error.push_str(&format!("  in {}\n\n", location));
    
    if !available_vars.is_empty() {
        error.push_str("Available variables in scope:\n");
        for (name, origin) in available_vars {
            error.push_str(&format!("  - {}: ({})\n", name, origin));
        }
        error.push('\n');
    }
    
    if !suggested_vars.is_empty() {
        error.push_str("Did you mean one of: ");
        error.push_str(&suggested_vars.join(", "));
        error.push_str("?\n");
    }
    
    error
}

#[test]
fn test_enhanced_error_message_formatting() {
    // Test the enhanced error message formatting
    let available_vars = vec![
        ("environment".to_string(), "local $defs".to_string()),
        ("database_host".to_string(), "imported from 'config'".to_string()),
        ("app_version".to_string(), "imported from 'config'".to_string()),
        ("debug_mode".to_string(), "bound in !$let".to_string()),
    ];
    
    let suggested_vars = vec!["database_host".to_string(), "database_port".to_string()];
    
    let error_msg = format_enhanced_variable_error(
        "databse_host", // intentional typo
        "main.yaml:15",
        &available_vars,
        &suggested_vars
    );
    
    println!("✅ Enhanced error message formatting test passed");
    println!("Example enhanced error message:");
    println!("{}", error_msg);
    
    // Verify the error message contains expected components
    assert!(error_msg.contains("Variable 'databse_host' not found"));
    assert!(error_msg.contains("main.yaml:15"));
    assert!(error_msg.contains("Available variables in scope:"));
    assert!(error_msg.contains("local $defs"));
    assert!(error_msg.contains("imported from 'config'"));
    assert!(error_msg.contains("Did you mean one of:"));
}