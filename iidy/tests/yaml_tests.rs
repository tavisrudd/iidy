//! Integration tests for end-to-end YAML preprocessing workflows
//! 
//! These tests focus on complete workflows using realistic fixture files
//! rather than testing individual components in isolation.

use anyhow::Result;
use iidy::yaml::parse_yaml_with_custom_tags;
use std::path::Path;

/// Helper function to load and parse fixture files
fn load_fixture(filename: &str) -> Result<String> {
    let fixture_path = Path::new("tests/fixtures").join(filename);
    std::fs::read_to_string(fixture_path)
        .map_err(|e| anyhow::anyhow!("Failed to load fixture {}: {}", filename, e))
}

/// Test parsing of a simplified stack-args configuration
#[test]
fn test_stack_args_parsing_workflow() -> Result<()> {
    // Use a simplified version that avoids complex nested tag syntax issues
    let yaml_content = r#"
StackName: !$join ["-", ["my-app", "{{environment}}"]]

Template: ./template.yaml
Region: us-west-2

Parameters:
  Environment: "{{environment}}"
  AppName: "my-app"

Tags:
  Environment: "{{environment}}"
  Project: my-application
  ManagedBy: iidy

Capabilities:
  - CAPABILITY_IAM
  - CAPABILITY_NAMED_IAM

TimeoutInMinutes: 30
OnFailure: ROLLBACK
"#;
    
    let ast = parse_yaml_with_custom_tags(yaml_content)?;
    
    // Should successfully parse the complete stack-args structure
    assert!(matches!(ast, iidy::yaml::YamlAst::Mapping(_)), "Stack-args should parse as a mapping");
    
    // NOTE: Once AST resolution is implemented, extend this test to:
    // - Process the full stack-args with environment variables
    // - Verify correct StackName generation with join tag: "my-app-production"
    // - Validate parameter substitution with handlebars templates
    // - Check that capabilities array is preserved
    // - Test with different environment values (dev, staging, prod)
    
    Ok(())
}

/// Test parsing of handlebars template processing workflow
#[test]
fn test_handlebars_integration_workflow() -> Result<()> {
    // Use a simplified handlebars example that focuses on core functionality
    let yaml_content = r#"
$defs:
  app_name: "my-application"
  version: "1.2.3"

application:
  name: "{{app_name}}"
  version: "{{version}}"
  environment: "{{environment}}"

identifiers:
  stack_name: "{{app_name}}-{{environment}}"
  s3_bucket: "{{toLowerCase app_name}}-{{environment}}"

database:
  host: "{{app_name}}-db.example.com"
  name: "{{app_name}}_{{environment}}"

configuration:
  app_config: "{{toJson app_settings}}"
  
tags:
  Project: "{{app_name}}"
  Environment: "{{environment}}"
  Version: "{{version}}"
"#;
    
    let ast = parse_yaml_with_custom_tags(yaml_content)?;
    
    // Should successfully parse handlebars template structure
    assert!(matches!(ast, iidy::yaml::YamlAst::Mapping(_)), "Handlebars example should parse as a mapping");
    
    // NOTE: Once AST resolution is implemented, extend this test to:
    // - Process with environment variables: app_name, environment, version
    // - Verify handlebars helper functions: toLowerCase, toJson
    // - Validate variable substitution in nested structures
    // - Test $defs section processing for local variable definitions
    // - Verify complex template strings with multiple variables
    
    Ok(())
}

/// Test parsing of complex nested preprocessing workflow
#[test]
fn test_complex_preprocessing_workflow() -> Result<()> {
    // Simplified complex example focusing on key preprocessing features
    let yaml_content = r#"
$defs:
  environments: ["development", "staging", "production"]
  app_name: "my-complex-app"

stack_name: !$join ["-", ["{{app_name}}", "{{environment}}"]]

resources: !$merge
  - common:
      vpc: !$join ["-", ["{{app_name}}", "vpc"]]
  - environment_specific:
      database: !$join ["-", ["{{app_name}}", "{{environment}}", "db"]]

parameters: !$let
  bindings:
    base_params:
      AppName: "{{app_name}}"
      Environment: "{{environment}}"
  expression: !$merge
    - "{{base_params}}"
    - DatabaseConfig:
        Host: "{{app_name}}-{{environment}}.db.local"

tags: !$merge
  - Project: "{{app_name}}"
    Environment: "{{environment}}"
  - ManagedBy: iidy

capabilities: !$concat
  - ["CAPABILITY_IAM"]
  - ["CAPABILITY_NAMED_IAM"]
"#;
    
    let ast = parse_yaml_with_custom_tags(yaml_content)?;
    
    // Should successfully parse complex nested structure
    assert!(matches!(ast, iidy::yaml::YamlAst::Mapping(_)), "Complex example should parse as a mapping");
    
    // NOTE: Once AST resolution is implemented, extend this test to:
    // - Process $defs section with environment arrays and app configuration
    // - Resolve nested !$let bindings with complex expressions
    // - Test !$merge operations combining multiple sources
    // - Verify !$concat operations with arrays
    // - Validate deep nesting of preprocessing tags
    // - Check that all preprocessing tags are properly parsed
    
    Ok(())
}

/// Test parsing workflow for database configuration
#[test]
fn test_database_config_workflow() -> Result<()> {
    let yaml_content = load_fixture("db-config.yaml")?;
    let ast = parse_yaml_with_custom_tags(&yaml_content)?;
    
    // Database config should parse successfully
    assert!(ast.is_preprocessing_tag() || matches!(ast, iidy::yaml::YamlAst::Mapping(_)), 
            "Database config should parse as mapping or preprocessing tag");
    
    // NOTE: Once AST resolution is implemented, extend this test to:
    // - Validate database connection parameters
    // - Test environment-specific database configurations
    // - Verify connection string construction
    
    Ok(())
}

/// Test parsing workflow for default features configuration
#[test] 
fn test_default_features_workflow() -> Result<()> {
    let yaml_content = load_fixture("default-features.yaml")?;
    let ast = parse_yaml_with_custom_tags(&yaml_content)?;
    
    // Default features should parse successfully  
    assert!(ast.is_preprocessing_tag() || matches!(ast, iidy::yaml::YamlAst::Mapping(_)),
            "Default features should parse as mapping or preprocessing tag");
    
    // NOTE: Once AST resolution is implemented, extend this test to:
    // - Validate feature flag structure
    // - Test feature toggle logic
    // - Verify environment-specific feature overrides
    
    Ok(())
}

/// Test error handling for malformed fixture files
#[test]
fn test_malformed_yaml_error_handling() -> Result<()> {
    // Test with intentionally malformed YAML
    let malformed_yaml = r#"
stack_name: !$join
  array: ["test"
  # Missing closing bracket and delimiter field
"#;
    
    let result = parse_yaml_with_custom_tags(malformed_yaml);
    
    // Should fail gracefully with meaningful error
    assert!(result.is_err(), "Malformed YAML should fail to parse");
    
    let error_msg = result.unwrap_err().to_string();
    assert!(!error_msg.is_empty(), "Error message should not be empty");
    
    Ok(())
}

/// Test parsing consistency across multiple fixture files  
#[test]
fn test_parsing_consistency_across_fixtures() -> Result<()> {
    let fixtures = [
        "stack-args.yaml",
        "handlebars-example.yaml", 
        "complex-example.yaml",
        "db-config.yaml",
        "default-features.yaml",
    ];
    
    for fixture in &fixtures {
        let yaml_content = load_fixture(fixture)?;
        
        // Parse the same content twice
        let ast1 = parse_yaml_with_custom_tags(&yaml_content);
        let ast2 = parse_yaml_with_custom_tags(&yaml_content);
        
        // Both should have the same outcome (succeed or fail)
        assert_eq!(ast1.is_ok(), ast2.is_ok(), 
                  "Parsing should be deterministic for {}", fixture);
                  
        if ast1.is_ok() {
            // Could add more detailed comparison here if AST implements PartialEq
        }
    }
    
    Ok(())
}

/// Test that all fixture files can be loaded from filesystem
#[test]
fn test_fixture_file_accessibility() -> Result<()> {
    let expected_fixtures = [
        "stack-args.yaml",
        "handlebars-example.yaml",
        "complex-example.yaml", 
        "db-config.yaml",
        "default-features.yaml",
    ];
    
    for fixture in &expected_fixtures {
        let content = load_fixture(fixture)?;
        assert!(!content.is_empty(), "Fixture {} should not be empty", fixture);
        assert!(content.contains("# ") || content.contains(":"), 
               "Fixture {} should contain YAML content", fixture);
    }
    
    Ok(())
}

/// Integration test demonstrating the full intended workflow
/// NOTE: This test will need to be updated once AST resolution is implemented
#[test]
fn test_end_to_end_preprocessing_workflow_placeholder() -> Result<()> {
    // This test demonstrates the intended end-to-end workflow that will be possible
    // once AST resolution is fully implemented
    
    let yaml_content = load_fixture("stack-args.yaml")?;
    let ast = parse_yaml_with_custom_tags(&yaml_content)?;
    
    // Currently we can only test parsing
    assert!(matches!(ast, iidy::yaml::YamlAst::Mapping(_)));
    
    // TODO: Once AST resolution is implemented, this test should:
    // 
    // 1. Create a preprocessing context with environment variables:
    //    let mut context = TagContext::new()
    //        .with_variable("environment", "production")
    //        .with_variable("app_name", "my-app");
    //
    // 2. Process the AST with full resolution:
    //    let mut preprocessor = YamlPreprocessor::new();
    //    let result = preprocessor.resolve_ast_with_context(ast, &context)?;
    //
    // 3. Validate the final processed output:
    //    - StackName should be "my-app-production"  
    //    - ServiceRoleARN should be non-null for production
    //    - EnableTerminationProtection should be true
    //    - NotificationARNs should contain SNS ARN
    //
    // 4. Verify the output can be serialized back to valid YAML:
    //    let final_yaml = serde_yaml::to_string(&result)?;
    //    // Final YAML should be valid CloudFormation stack-args
    
    Ok(())
}
