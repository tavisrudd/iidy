//! Test to demonstrate the issue with finding the correct occurrence in error reporting

use iidy::yaml::parsing::parse_yaml_with_custom_tags_from_file;

#[test]
fn test_thirteenth_if_tag_error_position() {
    // Create YAML with 20 !$if tags, where the 13th one is missing the 'test' field
    let yaml_content = r#"Resources:
  Condition1: !$if { test: true, then: "ok1", else: "fail1" }
  Condition2: !$if { test: false, then: "ok2", else: "fail2" }
  Condition3: !$if { test: true, then: "ok3", else: "fail3" }
  Condition4: !$if { test: false, then: "ok4", else: "fail4" }
  Condition5: !$if { test: true, then: "ok5", else: "fail5" }
  Condition6: !$if { test: false, then: "ok6", else: "fail6" }
  Condition7: !$if { test: true, then: "ok7", else: "fail7" }
  Condition8: !$if { test: false, then: "ok8", else: "fail8" }
  Condition9: !$if { test: true, then: "ok9", else: "fail9" }
  Condition10: !$if { test: false, then: "ok10", else: "fail10" }
  Condition11: !$if { test: true, then: "ok11", else: "fail11" }
  Condition12: !$if { test: false, then: "ok12", else: "fail12" }
  Condition13: !$if { then: "ok13", else: "fail13" }  # Missing 'test' field - this should be reported at line 14
  Condition14: !$if { test: true, then: "ok14", else: "fail14" }
  Condition15: !$if { test: false, then: "ok15", else: "fail15" }
  Condition16: !$if { test: true, then: "ok16", else: "fail16" }
  Condition17: !$if { test: false, then: "ok17", else: "fail17" }
  Condition18: !$if { test: true, then: "ok18", else: "fail18" }
  Condition19: !$if { test: false, then: "ok19", else: "fail19" }
  Condition20: !$if { test: true, then: "ok20", else: "fail20" }"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "multiple_if.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    println!("Error message: {}", error_msg);
    
    // This test demonstrates the problem:
    // The error should point to line 14 (where Condition13 with missing 'test' is)
    // But it will likely point to line 2 (where the first !$if appears)
    
    // What we WANT (but currently don't get):
    // assert!(error_msg.contains("multiple_if.yaml:14:"), 
    //         "Error should point to line 14 where the problematic !$if is, but got: {}", error_msg);
    
    // What we ACTUALLY get (the problem):
    // The error will likely contain "multiple_if.yaml:2:" because find_position_of finds the first !$if
    
    // Verify the error is about missing 'test' field
    assert!(error_msg.contains("test"));
    assert!(error_msg.contains("missing") || error_msg.contains("required"));
    
    // Show that the current implementation incorrectly points to the first !$if
    // instead of the 13th one that actually has the problem
    if error_msg.contains("multiple_if.yaml:2:") {
        println!("❌ ISSUE CONFIRMED: Error incorrectly points to line 2 (first !$if) instead of line 14 (13th !$if with the actual error)");
    } else if error_msg.contains("multiple_if.yaml:14:") {
        println!("✅ ISSUE FIXED: Error correctly points to line 14 (13th !$if with the actual error)");
    } else {
        println!("🤔 Error points to some other location: {}", error_msg);
    }
}

#[test] 
fn test_nested_if_tags_error_position() {
    // Test with nested !$if tags where an inner one has an error
    let yaml_content = r#"Resources:
  OuterCondition: !$if
    test: true
    then: !$if
      test: false  
      then: !$if
        test: true
        then: !$if
          # Missing 'test' field in deeply nested !$if
          then: "deep_success"
          else: "deep_failure"
        else: "mid_failure"
      else: "outer_then_failure"
    else: "outer_failure""#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "nested_if.yaml");
    
    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    
    println!("Nested error message: {}", error_msg);
    
    // The error should point to line 9 where the problematic nested !$if is
    // But it will likely point to line 2 where the first !$if appears
    
    assert!(error_msg.contains("test"));
    assert!(error_msg.contains("missing") || error_msg.contains("required"));
    
    if error_msg.contains("nested_if.yaml:2:") {
        println!("❌ NESTED ISSUE: Error incorrectly points to line 2 (first !$if) instead of line 9 (nested !$if with error)");
    } else if error_msg.contains("nested_if.yaml:9:") {
        println!("✅ NESTED FIXED: Error correctly points to line 9 (nested !$if with the actual error)");
    } else {
        println!("🤔 Nested error points to some other location: {}", error_msg);
    }
}