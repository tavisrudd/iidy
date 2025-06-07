//! Tests for ParseContext position tracking and error reporting accuracy

use iidy::yaml::parser::ParseContext;
use iidy::yaml::location::Position;

#[test]
fn test_parse_context_creation() {
    let source = "line1\nline2\nline3";
    let context = ParseContext::new("test.yaml", source);
    
    assert_eq!(context.file_location, "test.yaml");
    assert_eq!(context.source, source);
    assert_eq!(context.position.line, 1);
    assert_eq!(context.position.column, 1);
    assert_eq!(context.position.offset, 0);
    assert_eq!(context.yaml_path, "");
}

#[test]
fn test_location_string_formatting() {
    let context = ParseContext::new("test.yaml", "content")
        .with_position(5, 10, 25);
    
    assert_eq!(context.location_string(), "test.yaml:5:10");
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
fn test_find_position_of_from_offset() {
    let source = "test test test\nmore test content\nfinal test";
    let context = ParseContext::new("test.yaml", source);
    
    // First "test" at beginning
    let pos = context.find_position_of_from_offset("test", 0).unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
    assert_eq!(pos.offset, 0);
    
    // Second "test" (skip first one)
    let pos = context.find_position_of_from_offset("test", 1).unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 6); // "test " = 5 chars, so second "test" starts at column 6
    assert_eq!(pos.offset, 5);
    
    // Third "test" (skip first two)
    let pos = context.find_position_of_from_offset("test", 6).unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 11); // "test test " = 10 chars, so third "test" starts at column 11
    assert_eq!(pos.offset, 10);
    
    // "test" on second line
    let pos = context.find_position_of_from_offset("test", 15).unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 6); // "more " = 5 chars on line 2
    
    // "test" on third line  
    let pos = context.find_position_of_from_offset("test", 32).unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 7); // "final " = 6 chars on line 3
}

#[test]
fn test_find_position_of_from_offset_boundary_cases() {
    let source = "abc\ndef\nghi";
    let context = ParseContext::new("test.yaml", source);
    
    // Start from offset beyond source length
    let pos = context.find_position_of_from_offset("def", 100);
    assert!(pos.is_none());
    
    // Start from exact end of source
    let pos = context.find_position_of_from_offset("def", source.len());
    assert!(pos.is_none());
    
    // Start from offset equal to source length - 1 
    let pos = context.find_position_of_from_offset("i", source.len() - 1).unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 3);
}

#[test]
fn test_get_line_content() {
    let source = "line1\nline2 content\nline3 more\n";
    let context = ParseContext::new("test.yaml", source);
    
    assert_eq!(context.get_line_content(1), Some("line1"));
    assert_eq!(context.get_line_content(2), Some("line2 content"));
    assert_eq!(context.get_line_content(3), Some("line3 more"));
    assert_eq!(context.get_line_content(4), None); // Beyond end since there's no 4th line
    assert_eq!(context.get_line_content(5), None); // Beyond end
    assert_eq!(context.get_line_content(0), Some("line1")); // Line 0 maps to first line due to saturating_sub
}

#[test]
fn test_current_line_content() {
    let source = "line1\nline2 content\nline3 more";
    let context = ParseContext::new("test.yaml", source)
        .with_position(2, 5, 10);
    
    assert_eq!(context.current_line_content(), Some("line2 content"));
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
    
    // The issue: find_position_of will find the FIRST occurrence again
    // not the second occurrence at line 3
    let pos2 = context.find_position_of("!$map").unwrap();
    assert_eq!(pos2.line, 1); // This is the problem - always finds first occurrence
    assert_eq!(pos2.column, 1);
    
    // To find the second occurrence, we need to use find_position_of_from_offset
    let first_occurrence_end = pos1.offset + "!$map".len();
    let pos3 = context.find_position_of_from_offset("!$map", first_occurrence_end).unwrap();
    assert_eq!(pos3.line, 3);
    assert_eq!(pos3.column, 13); // "  template: " = 12 chars, so !$map starts at column 13
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