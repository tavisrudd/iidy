//! Test to verify the context-aware position finding works correctly

use super::parse_yaml_from_file;

#[test]
fn test_multiple_map_tags_with_missing_template() {
    let yaml_content = r#"Resources:
  MapOp1: !$map
    items: [1, 2, 3]
    template: "{{item}}"
  
  MapOp2: !$map
    items: [a, b, c]
    template: "prefix-{{item}}"
  
  MapOp3: !$map
    items: [x, y, z]
    # missing template field - this should be reported at line 12
  
  MapOp4: !$map
    items: [p, q, r]
    template: "suffix-{{item}}""#;

    let result = parse_yaml_from_file(yaml_content, "multiple_maps.yaml");

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();

    println!("Multiple maps error: {}", error_msg);

    // Should point to line 10 where MapOp3's !$map is (the one missing template)
    assert!(
        error_msg.contains("multiple_maps.yaml:10:"),
        "Error should point to line 10 where MapOp3 !$map is, but got: {}",
        error_msg
    );

    assert!(error_msg.contains("template"));
    assert!(error_msg.contains("missing") || error_msg.contains("required"));
}

#[test]
fn test_different_tag_types_mixed() {
    let yaml_content = r#"Operations:
  Condition1: !$if
    test: true
    then: "yes"
    else: "no"
  
  Transform1: !$map
    items: [1, 2, 3]
    template: "{{item}}"
  
  Condition2: !$if
    test: false
    then: "maybe"
    else: "no"
  
  Transform2: !$map
    items: [a, b, c]
    # missing template field - should point here, not to Transform1
  
  Condition3: !$if
    test: true
    then: "ok"
    else: "fail""#;

    let result = parse_yaml_from_file(yaml_content, "mixed_tags.yaml");

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();

    println!("Mixed tags error: {}", error_msg);

    // Should point to line 16 where Transform2's !$map is (the one missing template)
    // NOT to line 8 where Transform1's !$map is
    assert!(
        error_msg.contains("mixed_tags.yaml:16:"),
        "Error should point to line 16 where Transform2 !$map is, but got: {}",
        error_msg
    );

    assert!(error_msg.contains("template"));
}

#[test]
fn test_array_context_positioning() {
    let yaml_content = r#"ListOperations:
  - operation: !$map
      items: [1, 2]
      template: "{{item}}"
  - operation: !$if
      test: true
      then: "ok"
      else: "fail"
  - operation: !$map
      items: [a, b]
      # missing template - should point to this occurrence, not the first
  - operation: !$if
      test: false
      then: "maybe"
      else: "no""#;

    let result = parse_yaml_from_file(yaml_content, "array_context.yaml");

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();

    println!("Array context error: {}", error_msg);

    // Should point to line 9 where the problematic !$map is in the array
    assert!(
        error_msg.contains("array_context.yaml:9:"),
        "Error should point to line 9 where the problematic !$map is, but got: {}",
        error_msg
    );

    assert!(error_msg.contains("template"));
}
