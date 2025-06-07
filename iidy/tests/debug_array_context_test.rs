//! Debug test to understand array context path generation

use iidy::yaml::parser::ParseContext;

#[test]
fn debug_array_context_paths() {
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
      # missing template - should point to this occurrence, not the first"#;

    let context = ParseContext::new("debug.yaml", yaml_content);
    
    // Simulate the path building that happens during parsing
    let list_ctx = context.with_path("ListOperations");
    println!("ListOperations path: '{}'", list_ctx.yaml_path);
    
    let first_item_ctx = list_ctx.with_array_index(0);
    println!("First array item path: '{}'", first_item_ctx.yaml_path);
    
    let first_operation_ctx = first_item_ctx.with_path("operation");
    println!("First operation path: '{}'", first_operation_ctx.yaml_path);
    
    let second_item_ctx = list_ctx.with_array_index(1);
    println!("Second array item path: '{}'", second_item_ctx.yaml_path);
    
    let third_item_ctx = list_ctx.with_array_index(2);
    println!("Third array item path: '{}'", third_item_ctx.yaml_path);
    
    let third_operation_ctx = third_item_ctx.with_path("operation");
    println!("Third operation path: '{}'", third_operation_ctx.yaml_path);
    
    // Test context-aware position finding
    println!("\nContext-aware position finding:");
    
    // Test context-aware tag finding
    if let Some(pos) = third_operation_ctx.find_tag_position_in_context("!$map") {
        println!("!$map in third operation context at line {}, column {}", pos.line, pos.column);
    }
    
    // Test that paths are built correctly
    assert_eq!(first_operation_ctx.yaml_path, "ListOperations[0].operation");
    assert_eq!(third_operation_ctx.yaml_path, "ListOperations[2].operation");
    println!("\nContext-aware finding:");
    if let Some(pos) = first_operation_ctx.find_tag_position_in_context("!$map") {
        println!("First operation context finds !$map at line {}, column {}", pos.line, pos.column);
    }
    
    if let Some(pos) = third_operation_ctx.find_tag_position_in_context("!$map") {
        println!("Third operation context finds !$map at line {}, column {}", pos.line, pos.column);
    }
}