//! Edge case tests for ParseContext position tracking

use iidy::yaml::parser::ParseContext;

#[test]
fn test_empty_string_source() {
    let context = ParseContext::new("empty.yaml", "");
    
    assert_eq!(context.source, "");
    assert_eq!(context.position.line, 1);
    assert_eq!(context.position.column, 1);
    assert_eq!(context.position.offset, 0);
    
    // Finding anything in empty string should return None
    assert!(context.find_position_of("anything").is_none());
    assert!(context.find_position_of_from_offset("test", 0).is_none());
    
    // Line content should return None for any line in empty source
    assert!(context.get_line_content(1).is_none());
    assert!(context.current_line_content().is_none());
}

#[test]
fn test_single_character_source() {
    let context = ParseContext::new("single.yaml", "a");
    
    let pos = context.find_position_of("a").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
    assert_eq!(pos.offset, 0);
    
    assert_eq!(context.get_line_content(1), Some("a"));
    assert!(context.find_position_of("b").is_none());
}

#[test]
fn test_only_newlines_source() {
    let context = ParseContext::new("newlines.yaml", "\n\n\n");
    
    // Should have empty lines
    assert_eq!(context.get_line_content(1), Some(""));
    assert_eq!(context.get_line_content(2), Some(""));
    assert_eq!(context.get_line_content(3), Some(""));
    assert!(context.get_line_content(4).is_none());
    
    // Finding newlines should work but position calculation should be correct
    let pos = context.find_position_of("\n").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
    assert_eq!(pos.offset, 0);
}

#[test]
fn test_unicode_emoji_position_tracking() {
    // Test with various emoji and unicode characters
    let source = "🚀 Deploy:\n  🔧 Config: ☕\n  📦 Package: 🎯";
    let context = ParseContext::new("unicode.yaml", source);
    
    // Find rocket emoji
    let pos = context.find_position_of("🚀").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
    
    // Find "Deploy"
    let pos = context.find_position_of("Deploy").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 3); // "🚀 " = 2 chars (emoji counts as 1 char in Rust strings)
    
    // Find wrench emoji
    let pos = context.find_position_of("🔧").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 3); // "  " = 2 spaces
    
    // Find coffee emoji
    let pos = context.find_position_of("☕").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 13); // "  🔧 Config: " = 12 chars
    
    // Find target emoji
    let pos = context.find_position_of("🎯").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 14); // "  📦 Package: " = 13 chars
}

#[test]
fn test_unicode_combining_characters() {
    // Test with combining characters (é = e + ´)
    let source = "café\nnaïve\nrésumé";  // These use composed characters
    let context = ParseContext::new("combining.yaml", source);
    
    let pos = context.find_position_of("café").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
    
    let pos = context.find_position_of("naïve").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);
    
    let pos = context.find_position_of("résumé").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_asian_characters_position_tracking() {
    // Test with Chinese, Japanese, Korean characters
    let source = "中文: value\n日本語: test\n한국어: content";
    let context = ParseContext::new("asian.yaml", source);
    
    let pos = context.find_position_of("中文").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
    
    let pos = context.find_position_of("value").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 5); // "中文: " = 4 chars
    
    let pos = context.find_position_of("日本語").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);
    
    let pos = context.find_position_of("한국어").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_mixed_line_endings() {
    // Test with common line endings
    let source = "line1\nline2\r\nline3";
    let context = ParseContext::new("mixed.yaml", source);
    
    let pos = context.find_position_of("line2").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);
    
    let pos = context.find_position_of("line3").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_very_long_lines() {
    // Test with long lines
    let long_content = "x".repeat(1000);
    let source = format!("short\n{}\nend", long_content);
    let context = ParseContext::new("long.yaml", &source);
    
    let pos = context.find_position_of("short").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 1);
    
    // Find the beginning of the long content
    let pos = context.find_position_of("x").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);
    
    let pos = context.find_position_of("end").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
}

#[test]
fn test_tabs_vs_spaces_position_tracking() {
    // Test consistent handling of tabs vs spaces
    let source = "line1\n\tindented_with_tab\n    indented_with_spaces\n\t\tmixed\t\tindent";
    let context = ParseContext::new("tabs.yaml", source);
    
    let pos = context.find_position_of("indented_with_tab").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 2); // Tab counts as 1 character
    
    let pos = context.find_position_of("indented_with_spaces").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 5); // 4 spaces + 1
    
    let pos = context.find_position_of("mixed").unwrap();
    assert_eq!(pos.line, 4);
    assert_eq!(pos.column, 3); // 2 tabs = 2 chars
}

#[test]
fn test_zero_width_characters() {
    // Test with zero-width characters that might be invisible but affect position
    let source = "normal\u{200B}text\nmore\u{FEFF}content"; // Zero-width space and BOM
    let context = ParseContext::new("zerowidth.yaml", source);
    
    // Find text after zero-width space
    let pos = context.find_position_of("text").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 8); // "normal" + zero-width space + 1
    
    // Find content after BOM
    let pos = context.find_position_of("content").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 6); // "more" + BOM + 1
}

#[test]
fn test_boundary_offset_calculations() {
    let source = "abc\ndef\nghi";
    let context = ParseContext::new("boundary.yaml", source);
    
    // Test offset at exact boundaries
    let pos = context.find_position_of_from_offset("def", 4).unwrap(); // Start right at "def"
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 1);
    assert_eq!(pos.offset, 4);
    
    // Test offset right before target
    let pos = context.find_position_of_from_offset("ghi", 7).unwrap(); // Start right before "ghi"
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 1);
    assert_eq!(pos.offset, 8);
    
    // Test offset that would skip target entirely
    let pos = context.find_position_of_from_offset("def", 5); // Start after "def"
    assert!(pos.is_none());
}

#[test]
fn test_special_yaml_characters_in_search() {
    let source = "key: value\nspecial: \":[]{}|>\"\nanother: !tag value";
    let context = ParseContext::new("special.yaml", source);
    
    // Find YAML special characters
    let pos = context.find_position_of(":").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 4);
    
    let pos = context.find_position_of("!tag").unwrap();
    assert_eq!(pos.line, 3);
    assert_eq!(pos.column, 10);
    
    // Find brackets and braces
    let pos = context.find_position_of("[").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 12); // "special: \":" = 11 chars, so "[" starts at column 12
}

#[test]
fn test_malformed_utf8_handling() {
    // Create a string with valid UTF-8 
    // (Rust strings are always valid UTF-8, so we can't test invalid UTF-8 directly)
    let source = "valid utf8: 测试\nmore: content";
    let context = ParseContext::new("utf8.yaml", source);
    
    let pos = context.find_position_of("测试").unwrap();
    assert_eq!(pos.line, 1);
    assert_eq!(pos.column, 13); // "valid utf8: " = 12 chars
    
    let pos = context.find_position_of("content").unwrap();
    assert_eq!(pos.line, 2);
    assert_eq!(pos.column, 7);
}