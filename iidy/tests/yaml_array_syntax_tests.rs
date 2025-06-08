//! Tests for YAML tag array syntax support
//! 
//! Tests that tags support both direct syntax (!Tag value) and array syntax (!Tag [value])
//! This ensures compatibility with iidy-js behavior where single-element arrays are unpacked.

use anyhow::Result;
use iidy::yaml::preprocess_yaml_with_base_location;
use serde_yaml::Value;

#[tokio::test]
async fn test_not_tag_array_syntax() -> Result<()> {
    let yaml_input = r#"
test_not_direct: !$not true
test_not_array: !$not [true]
test_not_false_direct: !$not false
test_not_false_array: !$not [false]
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(map) = result {
        // Both syntaxes should produce identical results
        assert_eq!(
            map.get(&Value::String("test_not_direct".to_string())),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            map.get(&Value::String("test_not_array".to_string())),
            Some(&Value::Bool(false))
        );
        assert_eq!(
            map.get(&Value::String("test_not_false_direct".to_string())),
            Some(&Value::Bool(true))
        );
        assert_eq!(
            map.get(&Value::String("test_not_false_array".to_string())),
            Some(&Value::Bool(true))
        );
    } else {
        panic!("Expected mapping result");
    }

    Ok(())
}

#[tokio::test]
async fn test_escape_tag_array_syntax() -> Result<()> {
    let yaml_input = r#"
$defs:
  test_var: "processed value"

test_escape_direct: !$escape "{{test_var}}"
test_escape_array: !$escape ["{{test_var}}"]
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(map) = result {
        // Both should escape handlebars processing (not process the template)
        assert_eq!(
            map.get(&Value::String("test_escape_direct".to_string())),
            Some(&Value::String("{{test_var}}".to_string()))
        );
        assert_eq!(
            map.get(&Value::String("test_escape_array".to_string())),
            Some(&Value::String("{{test_var}}".to_string()))
        );
    } else {
        panic!("Expected mapping result");
    }

    Ok(())
}

#[tokio::test]
async fn test_to_yaml_string_tag_array_syntax() -> Result<()> {
    let yaml_input = r#"
$defs:
  test_data:
    key: "value"
    number: 42

test_yaml_direct: !$toYamlString 
  key: "value"
  number: 42
test_yaml_array: !$toYamlString 
  - key: "value"
    number: 42
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(map) = result {
        let direct_result = map.get(&Value::String("test_yaml_direct".to_string()));
        let array_result = map.get(&Value::String("test_yaml_array".to_string()));
        
        // Both should produce YAML strings (array should unpack to single element)
        assert!(direct_result.is_some());
        assert!(array_result.is_some());
        
        // Should contain YAML representation
        if let Some(Value::String(yaml_str)) = direct_result {
            assert!(yaml_str.contains("key: value"));
            assert!(yaml_str.contains("number: 42"));
        }
        if let Some(Value::String(yaml_str)) = array_result {
            assert!(yaml_str.contains("key: value"));
            assert!(yaml_str.contains("number: 42"));
        }
    } else {
        panic!("Expected mapping result");
    }

    Ok(())
}

#[tokio::test]
async fn test_to_json_string_tag_array_syntax() -> Result<()> {
    let yaml_input = r#"
$defs:
  test_data:
    key: "value"
    number: 42

test_json_direct: !$toJsonString 
  key: "value"
  number: 42
test_json_array: !$toJsonString 
  - key: "value"
    number: 42
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(map) = result {
        let direct_result = map.get(&Value::String("test_json_direct".to_string()));
        let array_result = map.get(&Value::String("test_json_array".to_string()));
        
        // Both should produce JSON strings (array should unpack to single element)
        assert!(direct_result.is_some());
        assert!(array_result.is_some());
        
        // Should contain JSON representation
        if let Some(Value::String(json_str)) = direct_result {
            assert!(json_str.contains("\"key\":\"value\""));
            assert!(json_str.contains("\"number\":42"));
        }
        if let Some(Value::String(json_str)) = array_result {
            assert!(json_str.contains("\"key\":\"value\""));
            assert!(json_str.contains("\"number\":42"));
        }
    } else {
        panic!("Expected mapping result");
    }

    Ok(())
}

#[tokio::test]
async fn test_parse_yaml_tag_array_syntax() -> Result<()> {
    let yaml_input = r#"
test_parse_direct: !$parseYaml "key: value\nnumber: 42"
test_parse_array: !$parseYaml ["key: value\nnumber: 42"]
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(map) = result {
        let direct_result = map.get(&Value::String("test_parse_direct".to_string()));
        let array_result = map.get(&Value::String("test_parse_array".to_string()));
        
        // Both should produce identical parsed structures
        assert!(direct_result.is_some());
        assert!(array_result.is_some());
        assert_eq!(direct_result, array_result);
        
        // Should be a parsed mapping
        if let Some(Value::Mapping(parsed)) = direct_result {
            assert_eq!(
                parsed.get(&Value::String("key".to_string())),
                Some(&Value::String("value".to_string()))
            );
            assert_eq!(
                parsed.get(&Value::String("number".to_string())),
                Some(&Value::Number(serde_yaml::Number::from(42)))
            );
        }
    } else {
        panic!("Expected mapping result");
    }

    Ok(())
}

#[tokio::test]
async fn test_parse_json_tag_array_syntax() -> Result<()> {
    let yaml_input = r#"
test_parse_direct: !$parseJson '{"key": "value", "number": 42}'
test_parse_array: !$parseJson ['{"key": "value", "number": 42}']
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(map) = result {
        let direct_result = map.get(&Value::String("test_parse_direct".to_string()));
        let array_result = map.get(&Value::String("test_parse_array".to_string()));
        
        // Both should produce identical parsed structures
        assert!(direct_result.is_some());
        assert!(array_result.is_some());
        assert_eq!(direct_result, array_result);
        
        // Should be a parsed mapping
        if let Some(Value::Mapping(parsed)) = direct_result {
            assert_eq!(
                parsed.get(&Value::String("key".to_string())),
                Some(&Value::String("value".to_string()))
            );
            assert_eq!(
                parsed.get(&Value::String("number".to_string())),
                Some(&Value::Number(serde_yaml::Number::from(42)))
            );
        }
    } else {
        panic!("Expected mapping result");
    }

    Ok(())
}

#[tokio::test]
async fn test_cloudformation_tags_array_syntax() -> Result<()> {
    let yaml_input = r#"
# CloudFormation tags with array syntax
test_ref_direct: !Ref "MyResource" 
test_ref_array: !Ref ["MyResource"]

test_sub_direct: !Sub "Hello ${param}"
test_sub_array: !Sub ["Hello ${param}"]

test_getatt_direct: !GetAtt "MyResource.Property"
test_getatt_array: !GetAtt ["MyResource.Property"]
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    // Convert to string to check tag preservation
    let output = serde_yaml::to_string(&result)?;
    
    // Both syntaxes should produce identical CloudFormation tag structures
    assert!(output.contains("'!Ref': MyResource"));
    assert!(output.contains("'!Sub': Hello ${param}"));
    assert!(output.contains("'!GetAtt': MyResource.Property"));
    
    // Should not contain array structures
    assert!(!output.contains("!Ref\n- MyResource"));
    assert!(!output.contains("!Sub\n- Hello"));
    assert!(!output.contains("!GetAtt\n- MyResource"));

    Ok(())
}

#[tokio::test]
async fn test_filter_with_array_syntax() -> Result<()> {
    let yaml_input = r#"
$defs:
  items: ["api", "web", "worker"]

# Test filter with array syntax (the original bug case)
test_filter: !$map
  items: !$ items
  filter: !$not [!$eq ["{{item}}", "worker"]]
  template: "service: {{item}}"
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(map) = result {
        if let Some(Value::Sequence(filtered)) = map.get(&Value::String("test_filter".to_string())) {
            // Should exclude "worker" and include "api" and "web"
            assert_eq!(filtered.len(), 2);
            assert_eq!(filtered[0], Value::String("service: api".to_string()));
            assert_eq!(filtered[1], Value::String("service: web".to_string()));
        } else {
            panic!("Expected filtered sequence");
        }
    } else {
        panic!("Expected mapping result");
    }

    Ok(())
}

#[tokio::test]
async fn test_array_syntax_edge_cases() -> Result<()> {
    let yaml_input = r#"
# Test edge cases for array syntax
test_empty_array: !$not []
test_multi_element_array: !$not [true, false]
test_nested_array: !$not [[true]]
"#;

    let result = preprocess_yaml_with_base_location(yaml_input, "test.yaml").await?;
    
    if let Value::Mapping(map) = result {
        // Empty array should be treated as direct empty array (not unpacked)
        if let Some(Value::Bool(empty_result)) = map.get(&Value::String("test_empty_array".to_string())) {
            // Empty array is falsy, so !$not [] should be true
            assert_eq!(*empty_result, true);
        }
        
        // Multi-element array should be treated as direct array (not unpacked)
        if let Some(Value::Bool(multi_result)) = map.get(&Value::String("test_multi_element_array".to_string())) {
            // Non-empty array is truthy, so !$not [true, false] should be false
            assert_eq!(*multi_result, false);
        }
        
        // Nested array should be treated as direct array (not unpacked)
        if let Some(Value::Bool(nested_result)) = map.get(&Value::String("test_nested_array".to_string())) {
            // Non-empty array is truthy, so !$not [[true]] should be false
            assert_eq!(*nested_result, false);
        }
    } else {
        panic!("Expected mapping result");
    }

    Ok(())
}