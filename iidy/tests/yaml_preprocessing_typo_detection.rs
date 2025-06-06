//! Tests for YAML preprocessing tag typo detection
//! 
//! Validates that unknown !$ tags are caught as errors to prevent typos

use anyhow::Result;
use iidy::yaml::preprocess_yaml_with_base_location;

#[tokio::test]
async fn test_unknown_iidy_tag_detection() {
    let yaml_input = r#"
test_typo: !$typo "this should fail"
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await;
    
    // Should fail with unknown tag error
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Unknown iidy preprocessing tag '!$typo'"));
    assert!(error_msg.contains("likely a typo"));
}

#[tokio::test]
async fn test_unknown_and_or_tags_detected() {
    let yaml_input = r#"
test_and: !$and [true, false]
test_or: !$or [true, false]
"#;

    // Test !$and
    let result_and = preprocess_yaml_with_base_location("test_and: !$and [true, false]", "test.yaml").await;
    assert!(result_and.is_err());
    let error_msg = result_and.unwrap_err().to_string();
    assert!(error_msg.contains("Unknown iidy preprocessing tag '!$and'"));
    
    // Test !$or
    let result_or = preprocess_yaml_with_base_location("test_or: !$or [true, false]", "test.yaml").await;
    assert!(result_or.is_err());
    let error_msg = result_or.unwrap_err().to_string();
    assert!(error_msg.contains("Unknown iidy preprocessing tag '!$or'"));
}

#[tokio::test]
async fn test_other_common_typos() {
    let test_cases = vec![
        "!$maps",     // typo of !$map
        "!$iff",      // typo of !$if
        "!$equ",      // typo of !$eq
        "!$nott",     // typo of !$not
        "!$merges",   // typo of !$merge
        "!$joins",    // typo of !$join
        "!$splits",   // typo of !$split
        "!$lets",     // typo of !$let
    ];
    
    for typo in test_cases {
        let yaml_input = format!("test: {} \"value\"", typo);
        let result = preprocess_yaml_with_base_location(&yaml_input, "test.yaml").await;
        
        assert!(result.is_err(), "Should fail for typo: {}", typo);
        let error_msg = result.unwrap_err().to_string();
        assert!(error_msg.contains(&format!("Unknown iidy preprocessing tag '{}'", typo)));
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

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    // Should succeed and preserve CloudFormation tags
    let output = serde_yaml::to_string(&result)?;
    assert!(output.contains("!Ref MyResource"));
    assert!(output.contains("!Sub Hello ${World}"));
    assert!(output.contains("!GetAtt Resource.Property"));
    assert!(output.contains("!Base64 content"));
    
    Ok(())
}

#[tokio::test]
async fn test_valid_iidy_tags_still_work() -> Result<()> {
    let yaml_input = r#"
$defs:
  test_var: "value"

# Valid iidy tags should continue to work
test_if: !$if
  condition: !$eq ["test", "test"]
  then: "success"
  else: "failure"

test_map: !$map
  items: ["a", "b"]
  template: "{{item}}"

test_not: !$not false
test_escape: !$escape "{{test_var}}"
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
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