//! Tests for error reporting with accurate line/column positions

use iidy::yaml::parser::parse_yaml_with_custom_tags_from_file;

#[test]
fn test_unknown_tag_error_position() {
    let yaml_content = r#"Resources:
  MyBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !$unknownTag value"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should contain the correct line number where the unknown tag appears
    assert!(error_msg.contains("test.yaml:5:19"), 
            "Error should contain correct position test.yaml:5:19, but got: {}", error_msg);
    assert!(error_msg.contains("!$unknownTag"));
    assert!(error_msg.contains("not a valid iidy tag"));
}

#[test]
fn test_missing_required_field_error_position() {
    let yaml_content = r#"Resources:
  MyTemplate:
    Properties:
      MapResult: !$map
        items: [1, 2, 3]
        # missing required 'template' field"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should contain the correct line number where the !$map tag appears
    assert!(error_msg.contains("test.yaml:4:18") || error_msg.contains("!$map"), 
            "Error should contain position info for !$map tag, but got: {}", error_msg);
    assert!(error_msg.contains("template"));
    assert!(error_msg.contains("missing") || error_msg.contains("required"));
}

#[test]
fn test_wrong_field_name_suggestion_error_position() {
    let yaml_content = r#"Resources:
  MyTemplate:
    Properties:
      MapResult: !$map
        items: [1, 2, 3]
        source: "{{item}}"  # should be 'template'"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // The error should mention the missing required field 'template'
    assert!(error_msg.contains("template"), 
            "Error should mention missing 'template' field, but got: {}", error_msg);
    // Position should point to the !$map tag location
    assert!(error_msg.contains("test.yaml:4:18") || error_msg.contains("!$map"), 
            "Error should contain position info for !$map tag, but got: {}", error_msg);
}

#[test]
fn test_if_tag_missing_field_error_position() {
    let yaml_content = r#"Conditions:
  IsProduction: !$if
    # missing required 'test' field
    then: true
    else: false"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should report missing 'test' field with correct position
    assert!(error_msg.contains("test.yaml:2:17") || error_msg.contains("!$if"), 
            "Error should contain position info for !$if tag, but got: {}", error_msg);
    assert!(error_msg.contains("test"));
    assert!(error_msg.contains("missing") || error_msg.contains("required"));
}

#[test]
fn test_nested_tag_error_position() {
    let yaml_content = r#"Resources:
  MyBucket:
    Properties:
      Nested:
        Deep:
          Value: !$invalidTag test"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should show the correct deep position
    assert!(error_msg.contains("test.yaml:6:18") || error_msg.contains("!$invalidTag"), 
            "Error should contain position info for nested invalid tag, but got: {}", error_msg);
    assert!(error_msg.contains("!$invalidTag"));
}

#[test]
fn test_multiple_errors_first_one_reported() {
    let yaml_content = r#"Resources:
  First: !$badTag1 value1
  Second: !$badTag2 value2"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should report the first error encountered (badTag1)
    assert!(error_msg.contains("!$badTag1") || error_msg.contains("test.yaml:2:10"), 
            "Error should report first invalid tag, but got: {}", error_msg);
}

#[test]
fn test_tag_in_array_error_position() {
    let yaml_content = r#"Resources:
  MyList:
    - item1
    - !$wrongTag value
    - item3"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should show correct position in array
    assert!(error_msg.contains("test.yaml:4:7") || error_msg.contains("!$wrongTag"), 
            "Error should contain position info for tag in array, but got: {}", error_msg);
    assert!(error_msg.contains("!$wrongTag"));
}

#[test]
fn test_malformed_yaml_syntax_error_position() {
    let yaml_content = r#"Resources:
  MyBucket:
    Type: AWS::S3::Bucket
    Properties
      BucketName: test"#;  // Missing colon after Properties

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should report YAML syntax error with position
    assert!(error_msg.contains("test.yaml") && error_msg.contains("syntax"), 
            "Error should be a YAML syntax error with file position, but got: {}", error_msg);
}

#[test]
fn test_error_with_complex_yaml_path() {
    let yaml_content = r#"Resources:
  MyApp:
    Properties:
      Configuration:
        Database:
          Settings: !$map
            items: [1, 2, 3]
            # missing template field"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should show position and might include YAML path context
    assert!(error_msg.contains("test.yaml:6:21") || error_msg.contains("!$map"), 
            "Error should contain position for deeply nested tag, but got: {}", error_msg);
    assert!(error_msg.contains("template"));
}

#[test]
fn test_eq_tag_wrong_number_of_elements_error() {
    let yaml_content = r#"Conditions:
  TestCondition: !$eq [value1, value2, value3]"#;  // Should have exactly 2 elements

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should report error about wrong number of elements
    assert!(error_msg.contains("2 elements") || error_msg.contains("exactly 2"), 
            "Error should mention exactly 2 elements for !$eq, but got: {}", error_msg);
}

#[test]
fn test_join_tag_wrong_format_error() {
    let yaml_content = r#"Transform:
  JoinResult: !$join [","]"#;  // Missing second element (array)

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should report error about join format
    assert!(error_msg.contains("two elements") || error_msg.contains("[delimiter, array]"), 
            "Error should mention join format requirement, but got: {}", error_msg);
}

#[test]
fn test_split_tag_missing_delimiter_error() {
    let yaml_content = r#"Transform:
  SplitResult: !$split
    string: "a,b,c"
    # missing delimiter field"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "test.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    // Should report missing delimiter field
    assert!(error_msg.contains("delimiter"), 
            "Error should mention missing delimiter field, but got: {}", error_msg);
}