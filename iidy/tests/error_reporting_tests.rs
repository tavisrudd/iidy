use anyhow::Result;
use iidy::yaml::preprocess_yaml_with_base_location;

/// Tests for enhanced error reporting with YAML path tracking
/// These tests verify that error messages include precise file location and document path information

#[tokio::test]
async fn test_variable_not_found_error_with_object_path() -> Result<()> {
    let yaml_input = r#"
$defs:
  allowed_var: "this_works"

section1:
  subsection:
    bad_access: !$ nonexistent_var
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_message = error.to_string();
    
    // Verify error contains the variable name
    assert!(error_message.contains("Variable 'nonexistent_var' not found"));
    
    // Verify error contains the file name
    assert!(error_message.contains("in file 'test.yaml'"));
    
    // Verify error contains the YAML path
    assert!(error_message.contains("at path '<root>.section1.subsection.bad_access'"));
    
    // Verify error contains helpful guidance
    assert!(error_message.contains("Only variables from $defs, $imports, and local scoped variables"));
    
    Ok(())
}

#[tokio::test]
async fn test_variable_not_found_error_with_array_path() -> Result<()> {
    let yaml_input = r#"
$defs:
  allowed_var: "this_works"

section2:
  items:
    - name: "item1"
    - name: "item2"  
    - bad_field: !$ another_nonexistent_var
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "array_test.yaml").await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_message = error.to_string();
    
    // Verify error contains the variable name
    assert!(error_message.contains("Variable 'another_nonexistent_var' not found"));
    
    // Verify error contains the file name
    assert!(error_message.contains("in file 'array_test.yaml'"));
    
    // Verify error contains the YAML path with array index
    assert!(error_message.contains("at path '<root>.section2.items[2].bad_field'"));
    
    Ok(())
}

#[tokio::test]
async fn test_variable_not_found_error_with_deeply_nested_path() -> Result<()> {
    let yaml_input = r#"
$defs:
  allowed_var: "this_works"

section3:
  config:
    database:
      settings:
        invalid: !$ missing_variable
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "nested_test.yaml").await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_message = error.to_string();
    
    // Verify error contains the variable name
    assert!(error_message.contains("Variable 'missing_variable' not found"));
    
    // Verify error contains the file name
    assert!(error_message.contains("in file 'nested_test.yaml'"));
    
    // Verify error contains the deeply nested YAML path
    assert!(error_message.contains("at path '<root>.section3.config.database.settings.invalid'"));
    
    Ok(())
}

#[tokio::test]
async fn test_variable_not_found_error_with_complex_mixed_structure() -> Result<()> {
    let yaml_input = r#"
$defs:
  app_name: "my-app"

service_configs:
  - name: "api"
    replicas: 2
    settings:
      invalid_ref: !$ nonexistent_service_var
  - name: "web" 
    replicas: 1
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "complex_test.yaml").await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_message = error.to_string();
    
    // Verify error contains the variable name
    assert!(error_message.contains("Variable 'nonexistent_service_var' not found"));
    
    // Verify error contains the file name
    assert!(error_message.contains("in file 'complex_test.yaml'"));
    
    // Verify error contains the complex mixed path (array + object)
    assert!(error_message.contains("at path '<root>.service_configs[0].settings.invalid_ref'"));
    
    Ok(())
}

#[tokio::test] 
async fn test_showcase_example_error_path() -> Result<()> {
    let yaml_input = r#"
$defs:
  app_name: "iidy-showcase"
  environment: "demo"

app_info:
  name: "test app"

complete_config: !$merge
  - app: !$ app_info
  - database: "test"
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "showcase_example.yaml").await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_message = error.to_string();
    
    // This should show the exact path where the merge operation tries to access app_info
    assert!(error_message.contains("Variable 'app_info' not found"));
    assert!(error_message.contains("in file 'showcase_example.yaml'"));
    // The path should indicate it's inside the merge operation
    assert!(error_message.contains("at path '<root>.complete_config"));
    
    Ok(())
}

#[tokio::test]
async fn test_valid_variable_access_succeeds() -> Result<()> {
    let yaml_input = r#"
$defs:
  allowed_var: "this_works"
  app_name: "test-app"

section1:
  subsection:
    valid_access: !$ allowed_var
    
section2:
  items:
    - name: !$ app_name
    - value: !$ allowed_var
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "valid_test.yaml").await;
    
    // This should succeed without errors
    assert!(result.is_ok());
    let processed = result.unwrap();
    
    // Verify the valid variable references were resolved correctly
    let section1 = processed.get("section1").unwrap().as_mapping().unwrap();
    let subsection = section1.get(&serde_yaml::Value::String("subsection".to_string())).unwrap().as_mapping().unwrap();
    assert_eq!(
        subsection.get(&serde_yaml::Value::String("valid_access".to_string())).unwrap(),
        &serde_yaml::Value::String("this_works".to_string())
    );
    
    Ok(())
}

#[tokio::test]
async fn test_error_message_format_consistency() -> Result<()> {
    let yaml_input = r#"
$defs:
  valid_var: "works"

test_section:
  error_here: !$ invalid_var
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "format_test.yaml").await;
    
    assert!(result.is_err());
    let error = result.unwrap_err();
    let error_message = error.to_string();
    
    // Verify the error message format is consistent and helpful
    
    // Should start with the error type
    assert!(error_message.starts_with("Variable '"));
    
    // Should contain all required components
    assert!(error_message.contains("not found in environment"));
    assert!(error_message.contains("in file '"));
    assert!(error_message.contains("at path '"));
    
    // Should have helpful guidance on a new line
    assert!(error_message.contains("\nOnly variables from $defs, $imports, and local scoped variables"));
    
    Ok(())
}