//! Snapshot tests for error example templates
//! 
//! This test suite captures the enhanced error outputs for various error 
//! conditions using the error examples in example-templates/errors/

use iidy::yaml::preprocess_yaml_with_spec;
use iidy::cli::YamlSpec;
use insta::assert_snapshot;

/// Test helper to render an error template file and capture the error output
async fn capture_error_output(template_path: &str) -> String {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    
    let full_path = format!("example-templates/errors/{}", template_path);
    let content = std::fs::read_to_string(&full_path)
        .unwrap_or_else(|e| panic!("Failed to read {}: {}", full_path, e));
    
    match preprocess_yaml_with_spec(&content, &full_path, &YamlSpec::Auto).await {
        Ok(_) => panic!("Expected {} to fail but it succeeded", template_path),
        Err(e) => {
            // Convert error to string to capture the enhanced error display
            format!("{}", e)
        }
    }
}

#[tokio::test]
async fn test_yaml_syntax_malformed_mapping_error() {
    let error_output = capture_error_output("yaml-syntax-malformed-mapping.yaml").await;
    assert_snapshot!("yaml_syntax_malformed_mapping_error", error_output);
}

#[tokio::test]
async fn test_yaml_syntax_unexpected_end_error() {
    let error_output = capture_error_output("yaml-syntax-unexpected-end.yaml").await;
    assert_snapshot!("yaml_syntax_unexpected_end_error", error_output);
}

#[tokio::test]
async fn test_tag_map_uses_source_error() {
    let error_output = capture_error_output("tag-map-uses-source.yaml").await;
    assert_snapshot!("tag_map_uses_source_error", error_output);
}

#[tokio::test]
async fn test_tag_map_uses_transform_error() {
    let error_output = capture_error_output("tag-map-uses-transform.yaml").await;
    assert_snapshot!("tag_map_uses_transform_error", error_output);
}

#[tokio::test]
async fn test_tag_missing_required_field_error() {
    let error_output = capture_error_output("tag-missing-required-field.yaml").await;
    assert_snapshot!("tag_missing_required_field_error", error_output);
}

#[tokio::test]
async fn test_unknown_tag_typo_error() {
    let error_output = capture_error_output("unknown-tag-typo.yaml").await;
    assert_snapshot!("unknown_tag_typo_error", error_output);
}

#[tokio::test]
async fn test_variable_not_found_error() {
    let error_output = capture_error_output("variable-not-found.yaml").await;
    assert_snapshot!("variable_not_found_error", error_output);
}

#[tokio::test]
async fn test_variable_include_not_found_error() {
    let error_output = capture_error_output("variable-include-not-found.yaml").await;
    assert_snapshot!("variable_include_not_found_error", error_output);
}
