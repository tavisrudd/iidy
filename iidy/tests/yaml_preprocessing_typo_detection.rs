//! Tests for YAML preprocessing tag typo detection
//! 
//! Validates that unknown !$ tags are caught as errors to prevent typos

use anyhow::Result;
use iidy::yaml::preprocess_yaml_v11;
use insta::assert_snapshot;

#[tokio::test]
async fn test_unknown_iidy_tag_detection() {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    
    let yaml_input = r#"
test_typo: !$typo "this should fail"
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await;
    
    // Should fail with unknown tag error
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert_snapshot!("unknown_iidy_tag_detection", error_msg);
}

#[tokio::test]
async fn test_unknown_and_or_tags_detected() {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    
    // Test !$and
    let result_and = preprocess_yaml_v11("test_and: !$and [true, false]", "test.yaml").await;
    assert!(result_and.is_err());
    let error_msg_and = result_and.unwrap_err().to_string();
    assert_snapshot!("unknown_and_tag_detected", error_msg_and);
    
    // Test !$or
    let result_or = preprocess_yaml_v11("test_or: !$or [true, false]", "test.yaml").await;
    assert!(result_or.is_err());
    let error_msg_or = result_or.unwrap_err().to_string();
    assert_snapshot!("unknown_or_tag_detected", error_msg_or);
}

#[tokio::test]
async fn test_other_common_typos() {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    
    let test_cases = vec![
        ("!$maps", "maps_typo"),     // typo of !$map
        ("!$iff", "iff_typo"),       // typo of !$if
        ("!$equ", "equ_typo"),       // typo of !$eq
        ("!$nott", "nott_typo"),     // typo of !$not
        ("!$merges", "merges_typo"), // typo of !$merge
        ("!$joins", "joins_typo"),   // typo of !$join
        ("!$splits", "splits_typo"), // typo of !$split
        ("!$lets", "lets_typo"),     // typo of !$let
    ];
    
    for (typo, snapshot_name) in test_cases {
        let yaml_input = format!("test: {} \"value\"", typo);
        let result = preprocess_yaml_v11(&yaml_input, "test.yaml").await;
        
        assert!(result.is_err(), "Should fail for typo: {}", typo);
        let error_msg = result.unwrap_err().to_string();
        assert_snapshot!(snapshot_name, error_msg);
    }
}

#[tokio::test]
async fn test_cloudformation_tags_still_work() -> Result<()> {
    let yaml_input = r#"
# CloudFormation tags should still work (they don't start with !$)
test_ref: !Ref "MyResource"
test_sub: !Sub "Hello ${World}"
test_getatt: !GetAtt "Resource.Property"
test_base64: !Base64 "content"
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await?;
    
    // Should succeed and preserve CloudFormation tags
    let output = serde_yaml::to_string(&result)?;
    assert!(output.contains("'!Ref': MyResource"));
    assert!(output.contains("'!Sub': Hello ${World}"));
    assert!(output.contains("'!GetAtt': Resource.Property"));
    assert!(output.contains("'!Base64': content"));
    
    Ok(())
}

#[tokio::test]
async fn test_valid_iidy_tags_still_work() -> Result<()> {
    let yaml_input = r#"
$defs:
  test_var: "value"

# Valid iidy tags should continue to work
test_if: !$if
  test: !$eq ["test", "test"]
  then: "success"
  else: "failure"

test_map: !$map
  items: ["a", "b"]
  template: "{{item}}"

test_not: !$not false
test_escape: !$escape "{{test_var}}"
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await?;
    
    // Should succeed and process correctly
    if let serde_yaml::Value::Mapping(map) = result {
        assert_eq!(
            map.get(&serde_yaml::Value::String("test_if".to_string())),
            Some(&serde_yaml::Value::String("success".to_string()))
        );
        assert_eq!(
            map.get(&serde_yaml::Value::String("test_not".to_string())),
            Some(&serde_yaml::Value::Bool(true))
        );
        assert_eq!(
            map.get(&serde_yaml::Value::String("test_escape".to_string())),
            Some(&serde_yaml::Value::String("{{test_var}}".to_string()))
        );
        
        // Check map result
        if let Some(serde_yaml::Value::Sequence(map_result)) = map.get(&serde_yaml::Value::String("test_map".to_string())) {
            assert_eq!(map_result.len(), 2);
            assert_eq!(map_result[0], serde_yaml::Value::String("a".to_string()));
            assert_eq!(map_result[1], serde_yaml::Value::String("b".to_string()));
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_invalid_extra_fields_detected() -> Result<()> {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    
    // Test !$map with invalid extra field
    let yaml_input = r#"
test_map: !$map
  items: [1, 2, 3]
  template: "{{item}}"
  invalid_field: "should_not_be_here"
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await;
    
    // Should fail with unexpected field error
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("unexpected field 'invalid_field'"));
    assert!(error_msg.contains("Valid fields are: items, template, var (optional), filter (optional)"));
    
    Ok(())
}

#[tokio::test]
async fn test_missing_required_fields_detected() -> Result<()> {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    
    // Test !$if with missing required field
    let yaml_input = r#"
test_if: !$if
  then: "success"
  # Missing required 'test' field
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await;
    
    // Should fail with missing required field error
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("'test' missing in !$if tag"));
    
    Ok(())
}

#[tokio::test]
async fn test_validation_comprehensive() -> Result<()> {
    // Force NO_COLOR to avoid ANSI codes in snapshots
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    
    // Test that we get helpful suggestion for old field names
    let yaml_old_source = r#"
test_map: !$map
  source: [1, 2, 3]
  template: "{{item}}"
"#;

    let result = preprocess_yaml_v11(yaml_old_source, "test.yaml").await;
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    // Now we get a simpler "missing required field" error instead of typo detection
    assert!(error_msg.contains("'items' missing in !$map tag"));
    assert!(error_msg.contains("add 'items' field to !$map tag"));
    
    // Test that we get specific error for completely invalid fields
    let yaml_invalid = r#"
test_map: !$map
  items: [1, 2, 3]
  template: "{{item}}"
  completely_invalid: "not allowed"
"#;

    let result = preprocess_yaml_v11(yaml_invalid, "test.yaml").await;
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("unexpected field 'completely_invalid'"));
    assert!(error_msg.contains("Valid fields are: items, template, var (optional), filter (optional)"));
    
    Ok(())
}

#[tokio::test]
async fn test_enhanced_error_messages_with_examples() -> Result<()> {
    // Force NO_COLOR to avoid ANSI codes
    unsafe {
        std::env::set_var("NO_COLOR", "1");
    }
    
    // Test that error messages include helpful examples
    let yaml_old_transform = r#"
test_map: !$map
  items: [1, 2, 3]
  transform: "{{item}}"
"#;

    let result = preprocess_yaml_v11(yaml_old_transform, "test.yaml").await;
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // With 'transform' instead of 'template', we get a "missing required field" error 
    // because validation reports missing 'template' before checking for extra fields
    assert!(error_msg.contains("'template' missing in !$map tag"));
    assert!(error_msg.contains("add 'template' field to !$map tag"));
    
    // Check that it includes a helpful example
    assert!(error_msg.contains("example:"));
    assert!(error_msg.contains("!$map"));
    assert!(error_msg.contains("items: [1, 2, 3]"));
    assert!(error_msg.contains("template: \"{{item}}\""));
    
    Ok(())
}