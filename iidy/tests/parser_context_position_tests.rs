//! Tests for ParseContext position tracking and error reporting accuracy

use iidy::yaml::parser::ParseContext;
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
fn test_find_position_of_single_occurrence() {
    let source = "Resources:\n  MyBucket:\n    Type: AWS::S3::Bucket";
    let context = ParseContext::new("test.yaml", source);
    
    let pos = context.find_position_of("MyBucket").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 3); // "  MyBucket" - MyBucket starts at column 3
    assert_eq!(pos.offset, 13); // "Resources:\n  " = 13 chars
}

#[test]
fn test_find_position_of_at_line_start() {
    let source = "Resources:\nMyBucket:\n  Type: test";
    let context = ParseContext::new("test.yaml", source);
    
    let pos = context.find_position_of("MyBucket").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);
    assert_eq!(pos.offset, 11); // "Resources:\n" = 11 chars
}

#[test]
fn test_find_position_of_multiple_lines() {
    let source = "line1\nline2 with content\nline3 more content\nline4";
    let context = ParseContext::new("test.yaml", source);
    
    // Find "content" - should find first occurrence
    let pos = context.find_position_of("content").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 12); // "line2 with " = 11 chars, so "content" starts at 12
    
    // Find "line3" 
    let pos = context.find_position_of("line3").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_find_position_of_with_tabs() {
    let source = "Resources:\n\tMyBucket:\n\t\tType: AWS::S3::Bucket";
    let context = ParseContext::new("test.yaml", source);
    
    let pos = context.find_position_of("Type").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 3); // "\t\t" = 2 tab chars counted as 2 columns, so "Type" at column 3
}

#[test]
fn test_find_position_of_not_found() {
    let source = "Resources:\n  MyBucket:\n    Type: AWS::S3::Bucket";
    let context = ParseContext::new("test.yaml", source);
    
    let pos = context.find_position_of("NotFound");
    assert!(pos.is_none());
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
    
    let context1 = context.with_array_index(5);
    assert_eq!(context1.yaml_path, "[5]");
    
    let context2 = context1.with_array_index(10);
    assert_eq!(context2.yaml_path, "[5][10]");
}

#[test]
fn test_position_tracking_with_unicode() {
    // Test with multi-byte UTF-8 characters
    let source = "Resources:\n  café: ☕\n  naïve: test";
    let context = ParseContext::new("test.yaml", source);
    
    // Find "café" 
    let pos = context.find_position_of("café").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 3);
    
    // Find "☕" (coffee emoji)
    let pos = context.find_position_of("☕").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 9); // "  café: " = 8 chars
    
    // Find "naïve"
    let pos = context.find_position_of("naïve").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 3);
}

#[test]
fn test_position_tracking_with_empty_lines() {
    let source = "line1\n\n\nline4\n\nline6";
    let context = ParseContext::new("test.yaml", source);
    
    let pos = context.find_position_of("line4").unwrap();
    assert_eq!(pos.line, 4);
    assert_eq!(pos.column, 1);
    
    let pos = context.find_position_of("line6").unwrap();
    assert_eq!(pos.line, 6);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_position_tracking_with_windows_line_endings() {
    let source = "line1\r\nline2\r\nline3";
    let context = ParseContext::new("test.yaml", source);
    
    let pos = context.find_position_of("line2").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);
    
    let pos = context.find_position_of("line3").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_issue_with_multiple_occurrences_from_beginning() {
    // This test demonstrates the issue you mentioned
    // where find_position_of always searches from beginning
    let source = "!$map:\n  items: [1, 2, 3]\n  template: !$map:\n    items: [a, b]\n    template: value";
    let context = ParseContext::new("test.yaml", source);
    
    // Find first occurrence of "!$map" - should be at line 1
    let pos1 = context.find_position_of("!$map").unwrap();
    assert_eq!(pos1.line, 1);
    assert_eq!(pos1.column, 1);
    
    // The behavior: find_position_of will always find the FIRST occurrence
    // This is expected behavior - finding the first occurrence is the main use case
    let pos2 = context.find_position_of("!$map").unwrap();
    assert_eq!(pos2.line, 1); // Always finds first occurrence
    assert_eq!(pos2.column, 1);
    
    // For finding subsequent occurrences, we now rely on the context-aware methods
    // or the tree-sitter LocationFinder implementation which can navigate paths
}

#[test]
fn test_position_new_and_start() {
    let pos = Position::new(5, 10, 25);
    assert_eq!(pos.line, 5);
    assert_eq!(pos.column, 10);
    assert_eq!(pos.offset, 25);
    
    let start_pos = Position::start();
    assert_eq!(start_pos.line, 1);
    assert_eq!(start_pos.column, 1);
    assert_eq!(start_pos.offset, 0);
}