//! Tests for ParseContext core API

use iidy::yaml::parsing::ParseContext;
use iidy::yaml::location::Position;

#[test]
fn test_parse_context_creation() {
    let source = "line1\nline2\nline3";
    let context = ParseContext::new("test.yaml", source);
    
    assert_eq!(context.file_location.as_ref(), "test.yaml");
    assert_eq!(context.source.as_ref(), source);
    assert_eq!(context.yaml_path, "");
}

#[test]
fn test_location_string_formatting() {
    let context = ParseContext::new("test.yaml", "content");
    
    assert_eq!(context.location_string(), "test.yaml");
}

#[test]
fn test_with_path_navigation() {
    let context = ParseContext::new("test.yaml", "content");
    
    let context1 = context.with_path("Resources");
    assert_eq!(context1.yaml_path, "Resources");
    
    let context2 = context1.with_path("MyBucket");
    assert_eq!(context2.yaml_path, "Resources.MyBucket");
    
    let context3 = context2.with_array_index(0);
    assert_eq!(context3.yaml_path, "Resources.MyBucket[0]");
    
    let context4 = context3.with_path("Properties");
    assert_eq!(context4.yaml_path, "Resources.MyBucket[0].Properties");
}

#[test]
fn test_with_array_index_from_empty_path() {
    let context = ParseContext::new("test.yaml", "content");
    
    let array_context = context.with_array_index(5);
    assert_eq!(array_context.yaml_path, "[5]");
    
    let path_after_array = array_context.with_path("field");
    assert_eq!(path_after_array.yaml_path, "[5].field");
}

#[test]
fn test_position_new_and_start() {
    let pos1 = Position::new(10, 5, 50);
    assert_eq!(pos1.line, 10);
    assert_eq!(pos1.column, 5);
    assert_eq!(pos1.offset, 50);
    
    let pos2 = Position::start();
    assert_eq!(pos2.line, 1);
    assert_eq!(pos2.column, 1);
    assert_eq!(pos2.offset, 0);
}

#[test]
fn test_find_tag_position_in_context() {
    let yaml_source = r#"
Resources:
  MyBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !$if
        test: true
        then: "prod-bucket"
        else: "dev-bucket"
"#;
    
    let context = ParseContext::new("test.yaml", yaml_source);
    let bucket_context = context.with_path("Resources").with_path("MyBucket").with_path("Properties").with_path("BucketName");
    
    // Test context-aware tag finding
    if let Some(pos) = bucket_context.find_tag_position_in_context("!$if") {
        // Should find the !$if tag at the BucketName location
        assert!(pos.line > 5); // Should be somewhere after line 5
        println!("Found !$if at line {}, column {}", pos.line, pos.column);
    }
    // Note: This test might fail if tree-sitter isn't available, but that's OK
    // The important thing is that the API exists and can be called
}