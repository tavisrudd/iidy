//! Definitive proof that the context-aware position finding works correctly
//! This test proves that when the 13th !$if out of 20 is missing the 'test' field,
//! the error points to the correct line (13th occurrence), not the first occurrence.

use iidy::yaml::parser::parse_yaml_with_custom_tags_from_file;

#[test]
fn proof_thirteenth_if_error_points_to_correct_line() {
    // Exactly 20 !$if tags where ONLY the 13th one is missing 'test:'
    let yaml_content = r#"Resources:
  Condition01: !$if { test: true, then: "result01", else: "fail01" }
  Condition02: !$if { test: false, then: "result02", else: "fail02" }
  Condition03: !$if { test: true, then: "result03", else: "fail03" }
  Condition04: !$if { test: false, then: "result04", else: "fail04" }
  Condition05: !$if { test: true, then: "result05", else: "fail05" }
  Condition06: !$if { test: false, then: "result06", else: "fail06" }
  Condition07: !$if { test: true, then: "result07", else: "fail07" }
  Condition08: !$if { test: false, then: "result08", else: "fail08" }
  Condition09: !$if { test: true, then: "result09", else: "fail09" }
  Condition10: !$if { test: false, then: "result10", else: "fail10" }
  Condition11: !$if { test: true, then: "result11", else: "fail11" }
  Condition12: !$if { test: false, then: "result12", else: "fail12" }
  Condition13: !$if { then: "result13", else: "fail13" }
  Condition14: !$if { test: true, then: "result14", else: "fail14" }
  Condition15: !$if { test: false, then: "result15", else: "fail15" }
  Condition16: !$if { test: true, then: "result16", else: "fail16" }
  Condition17: !$if { test: false, then: "result17", else: "fail17" }
  Condition18: !$if { test: true, then: "result18", else: "fail18" }
  Condition19: !$if { test: false, then: "result19", else: "fail19" }
  Condition20: !$if { test: true, then: "result20", else: "fail20" }"#;

    // Count lines to verify which line Condition13 is on
    let lines: Vec<&str> = yaml_content.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        if line.contains("Condition13") {
            println!("Condition13 (the problematic one) is on line {}", i + 1);
            break;
        }
    }

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "proof.yaml");
    
    assert!(result.is_err(), "Should have an error due to missing 'test' field");
    
    let error_msg = result.unwrap_err().to_string();
    println!("\n=== ERROR MESSAGE ===");
    println!("{}", error_msg);
    println!("====================\n");
    
    // Verify the error is about missing 'test' field
    assert!(error_msg.contains("test"), "Error should mention missing 'test' field");
    assert!(error_msg.contains("missing") || error_msg.contains("required"), 
            "Error should indicate field is missing/required");
    
    // THE CRITICAL TEST: Error should point to line 14 (where Condition13 is)
    // NOT to line 2 (where Condition01 is)
    assert!(error_msg.contains("proof.yaml:14:"), 
            "❌ FAIL: Error should point to line 14 (Condition13), but got: {}", error_msg);
    
    // Verify it's NOT pointing to the first occurrence
    assert!(!error_msg.contains("proof.yaml:2:"), 
            "❌ FAIL: Error incorrectly points to line 2 (first !$if) instead of line 14 (13th !$if)");
    
    println!("✅ SUCCESS: Error correctly points to line 14 (13th !$if) where the actual problem is!");
    println!("✅ SUCCESS: Error does NOT point to line 2 (1st !$if) which would be wrong!");
}

#[test]
fn proof_different_occurrence_different_error() {
    // Similar test but the 7th occurrence has the error instead
    let yaml_content = r#"Resources:
  Cond1: !$if { test: true, then: "ok1", else: "fail1" }
  Cond2: !$if { test: false, then: "ok2", else: "fail2" }
  Cond3: !$if { test: true, then: "ok3", else: "fail3" }
  Cond4: !$if { test: false, then: "ok4", else: "fail4" }
  Cond5: !$if { test: true, then: "ok5", else: "fail5" }
  Cond6: !$if { test: false, then: "ok6", else: "fail6" }
  Cond7: !$if { then: "ok7", else: "fail7" }
  Cond8: !$if { test: true, then: "ok8", else: "fail8" }
  Cond9: !$if { test: false, then: "ok9", else: "fail9" }
  Cond10: !$if { test: true, then: "ok10", else: "fail10" }"#;

    let result = parse_yaml_with_custom_tags_from_file(yaml_content, "proof7.yaml");
    
    assert!(result.is_err(), "Should have an error due to missing 'test' field in 7th !$if");
    
    let error_msg = result.unwrap_err().to_string();
    println!("\n=== 7TH OCCURRENCE ERROR ===");
    println!("{}", error_msg);
    println!("===========================\n");
    
    // Should point to line 8 (where Cond7 is), NOT line 2 (where Cond1 is)
    assert!(error_msg.contains("proof7.yaml:8:"), 
            "❌ FAIL: Error should point to line 8 (Cond7), but got: {}", error_msg);
    
    assert!(!error_msg.contains("proof7.yaml:2:"), 
            "❌ FAIL: Error incorrectly points to line 2 (first !$if) instead of line 8 (7th !$if)");
    
    println!("✅ SUCCESS: Error correctly points to line 8 (7th !$if) where the actual problem is!");
}