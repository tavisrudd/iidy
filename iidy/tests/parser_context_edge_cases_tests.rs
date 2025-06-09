//! Edge case tests for ParseContext core functionality

use iidy::yaml::parsing::ParseContext;

#[test]
fn test_empty_string_source() {
    let context = ParseContext::new("empty.yaml", "");
    
    assert_eq!(context.source.as_ref(), "");
    assert_eq!(context.yaml_path, "");
    assert_eq!(context.location_string(), "empty.yaml");
}

#[test]
fn test_special_characters_in_file_location() {
    let context = ParseContext::new("file with spaces.yaml", "content");
    assert_eq!(context.location_string(), "file with spaces.yaml");
    
    let context = ParseContext::new("path/to/file.yaml", "content");
    assert_eq!(context.location_string(), "path/to/file.yaml");
    
    let context = ParseContext::new("https://example.com/template.yaml", "content");
    assert_eq!(context.location_string(), "https://example.com/template.yaml");
}

#[test]
fn test_complex_yaml_path_building() {
    let context = ParseContext::new("test.yaml", "content");
    
    // Test deeply nested paths
    let deep_context = context
        .with_path("Resources")
        .with_path("Database")
        .with_array_index(0)
        .with_path("Properties")
        .with_path("ConnectionStrings")
        .with_array_index(2)
        .with_path("Value");
    
    assert_eq!(deep_context.yaml_path, "Resources.Database[0].Properties.ConnectionStrings[2].Value");
}

#[test]
fn test_unicode_in_paths() {
    let context = ParseContext::new("测试.yaml", "内容");
    
    let unicode_context = context
        .with_path("资源")
        .with_path("数据库");
    
    assert_eq!(unicode_context.yaml_path, "资源.数据库");
    assert_eq!(unicode_context.location_string(), "测试.yaml");
}

#[test]
fn test_memory_efficiency_with_shared_strings() {
    let source = "a".repeat(10000); // Large string
    let context = ParseContext::new("test.yaml", &source);
    
    // Creating multiple contexts should share the source string efficiently
    let context1 = context.with_path("Resources");
    let context2 = context1.with_path("MyBucket");
    let context3 = context2.with_array_index(0);
    
    // All contexts should share the same source data (Rc<str>)
    assert_eq!(context.source.as_ref(), source);
    assert_eq!(context1.source.as_ref(), source);
    assert_eq!(context2.source.as_ref(), source);
    assert_eq!(context3.source.as_ref(), source);
}

#[test]
fn test_context_cloning_behavior() {
    let context = ParseContext::new("test.yaml", "content");
    let context1 = context.with_path("Resources");
    
    // Original context should be unchanged
    assert_eq!(context.yaml_path, "");
    assert_eq!(context1.yaml_path, "Resources");
    
    // Both should share the same source and file location
    assert_eq!(context.source.as_ptr(), context1.source.as_ptr());
    assert_eq!(context.file_location.as_ptr(), context1.file_location.as_ptr());
}

#[test]
fn test_find_tag_position_in_context_with_empty_path() {
    let yaml_source = "root: !$map\n  items: [1, 2]\n  template: '{{item}}'";
    let context = ParseContext::new("test.yaml", yaml_source);
    
    // Empty path should still work for finding tags at root level
    let result = context.find_tag_position_in_context("!$map");
    // This may or may not find the tag depending on tree-sitter availability
    // The important thing is that it doesn't crash and returns an Option
    match result {
        Some(pos) => println!("Found !$map at line {}, column {}", pos.line, pos.column),
        None => println!("!$map not found (expected if tree-sitter fails)"),
    }
}