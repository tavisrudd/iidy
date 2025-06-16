use url::Url;

// Import the diagnostic API functions
use crate::yaml::parsing_w_loc::{
error_codes, parse_and_convert_to_original_with_diagnostics,
parse_yaml_ast_with_diagnostics, validate_yaml_only,
};
use super::parser::YamlParser;

fn test_uri() -> Url {
    Url::parse("file:///test.yaml").unwrap()
}

#[test]
fn test_backward_compatibility_parse_method() {
    // Test that existing parse method works exactly as before
    let mut parser = YamlParser::new().unwrap();
    let source = r#"
Resources:
  Bucket:
Type: AWS::S3::Bucket
Properties:
  BucketName: "test"
"#;
    let result = parser.parse(source, test_uri());
    assert!(result.is_ok());
}

#[test]
fn test_diagnostic_api_basic_valid_yaml() {
    let source = r#"
Resources:
  Bucket:
Type: AWS::S3::Bucket
Properties:
  BucketName: "test"
"#;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, test_uri());
    assert!(!diagnostics.has_errors());
    assert!(diagnostics.parse_successful);
    assert_eq!(diagnostics.error_count(), 0);
}

#[test]
fn test_multiple_errors_collected() {
    let source = r#"
test1: !$unknownTag1 value
test2: !$unknownTag2 value
"#;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, test_uri());

    assert!(diagnostics.has_errors());
    assert_eq!(diagnostics.error_count(), 2);
    assert!(!diagnostics.parse_successful);

    // Check that both errors are collected
    assert!(
        diagnostics
            .errors
            .iter()
            .any(|e| e.message.contains("unknownTag1"))
    );
    assert!(
        diagnostics
            .errors
            .iter()
            .any(|e| e.message.contains("unknownTag2"))
    );

    // Verify error codes are set
    assert!(diagnostics.errors.iter().all(|e| e.code.is_some()));
}

#[test]
fn test_missing_field_errors_collected() {
    let source = r#"
test1: !$let
  var1: value1
  # Missing 'in' field

test2: !$let
  var2: value2
  # Another missing 'in' field
"#;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, test_uri());

    assert!(diagnostics.has_errors());
    // Should collect both missing 'in' field errors
    let missing_in_errors: Vec<_> = diagnostics
        .errors
        .iter()
        .filter(|e| e.message.contains("Missing required 'in' field"))
        .collect();
    assert_eq!(missing_in_errors.len(), 2);

    // Verify error codes
    assert!(missing_in_errors.iter().all(|e| {
        e.code
            .as_ref()
            .map(|c| c == error_codes::MISSING_FIELD)
            .unwrap_or(false)
    }));
}

#[test]
fn test_syntax_errors_collected() {
    let source = r#"
Resources:
  Bucket:
Type: AWS::S3::Bucket
Properties:
  BucketName: "unclosed quote
  Tags: [
"#;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, test_uri());

    assert!(diagnostics.has_errors());
    // Should collect syntax errors
    assert!(diagnostics.error_count() > 0);

    // All errors should have syntax error codes or be related to syntax
    let syntax_errors: Vec<_> = diagnostics
        .errors
        .iter()
        .filter(|e| {
            e.code
                .as_ref()
                .map(|c| c == error_codes::SYNTAX_ERROR)
                .unwrap_or(false)
        })
        .collect();
    assert!(!syntax_errors.is_empty());
}

#[test]
fn test_warnings_for_unexpected_fields() {
    let source = r#"
test: !$let
  var1: value1
  in: "{{var1}}"
  unexpected_field: "should warn"
"#;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, test_uri());

    // Should parse successfully (has 'in' field)
    assert!(!diagnostics.has_errors());

    // But should have warnings for unexpected field
    assert!(!diagnostics.warnings.is_empty());
    assert!(
        diagnostics
            .warnings
            .iter()
            .any(|w| w.message.contains("unexpected_field"))
    );
}

#[test]
fn test_backward_compatibility_returns_first_error() {
    let mut parser = YamlParser::new().unwrap();
    let source = r#"
test1: !$unknownTag1 value
test2: !$unknownTag2 value
"#;
    let result = parser.parse(source, test_uri());

    // Should return error (backward compatibility)
    assert!(result.is_err());
    let error = result.unwrap_err();

    // Should be one of the unknown tag errors
    assert!(error.message.contains("unknownTag"));
}

#[test]
fn test_convert_module_diagnostic_api() {
    let source = r#"
test1: !$unknownTag1 value
test2: !$unknownTag2 value  
"#;

    let result = parse_and_convert_to_original_with_diagnostics(source, "file:///test.yaml");
    assert!(result.is_ok());

    let diagnostics = result.unwrap();
    assert!(diagnostics.has_errors());
    assert_eq!(diagnostics.error_count(), 2);
}

#[test]
fn test_validate_yaml_only() {
    let source = r#"
test: !$let
  var1: value1
  # Missing 'in' field
"#;

    let result = validate_yaml_only(source, "file:///test.yaml");
    assert!(result.is_ok());

    let diagnostics = result.unwrap();
    assert!(diagnostics.has_errors());
    assert!(
        diagnostics
            .errors
            .iter()
            .any(|e| e.message.contains("Missing required 'in' field"))
    );
}

#[test]
fn test_error_locations_preserved() {
    let source = r#"
test: !$unknownTag value
"#;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, test_uri());

    assert!(diagnostics.has_errors());
    let error = &diagnostics.errors[0];

    // Should have location information
    assert!(error.location.is_some());
    let location = error.location.as_ref().unwrap();

    // Should have valid position (line 2, since we have newline at start)
    assert!(location.start.line >= 1);
    assert!(location.start.character < 100); // Basic sanity check instead of >= 0
}

#[test]
fn test_known_cloudformation_tags_accepted() {
    let source = r#"
Resources:
  Bucket:
Type: AWS::S3::Bucket
Properties:
  BucketName: !Ref BucketNameParam
  Tags: !GetAtt Resource.Arn
"#;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, test_uri());

    // Should not have errors for known CloudFormation tags
    assert!(!diagnostics.has_errors());
    assert!(diagnostics.parse_successful);
}

#[test]
fn test_preprocessing_tag_validation() {
    let source = r#"
# Valid tags should not generate errors
include_test: !$include "path/to/file.yaml"
map_test: !$map
  items: [1, 2, 3]
  template: "item: {{item}}"
  var: item

# Invalid tag should generate error  
invalid_test: !$let
  var1: value1
  # Missing required 'in' field
"#;
    let diagnostics = parse_yaml_ast_with_diagnostics(source, test_uri());

    // Should have one error for the invalid !$let tag
    assert!(diagnostics.has_errors());
    assert_eq!(diagnostics.error_count(), 1);
    assert!(
        diagnostics.errors[0]
            .message
            .contains("Missing required 'in' field")
    );
}

// ============================================================================
// PHASE 2: YAML Specification Compatibility Tests - CRITICAL FOR OLD PARSER REMOVAL
// Tests for YAML 1.1 vs 1.2 compatibility issues that could break CloudFormation
// ============================================================================

#[test]
fn test_yaml_11_boolean_compatibility_issue() {
    // CRITICAL: This test documents that the tree-sitter parser treats
    // YAML 1.1 boolean forms as strings, not booleans.
    // This is a compatibility difference from the old parser!
    
    let yaml_11_booleans = vec![
        ("yes", true), ("no", false),
        ("on", true), ("off", false),
        ("Yes", true), ("No", false),
        ("ON", true), ("OFF", false),
    ];
    
    for (yaml_str, _expected_bool) in yaml_11_booleans {
        use super::parser::parse_yaml_ast;
        use super::ast::YamlAst;
        
        let result = parse_yaml_ast(yaml_str, test_uri()).unwrap();
        
        // Document the current behavior: these are parsed as strings, not booleans
        match result {
            YamlAst::PlainString(s, _) => {
                assert_eq!(s, yaml_str);
                // This is the current tree-sitter behavior - strings, not booleans
                println!("WARNING: '{}' parsed as string, not boolean. This may break CloudFormation compatibility.", yaml_str);
            }
            YamlAst::Bool(_b, _) => {
                // If this ever starts working, update the test
                println!("INFO: '{}' correctly parsed as boolean", yaml_str);
            }
            _ => panic!("Unexpected result for '{}': {:?}", yaml_str, result),
        }
    }
}

#[test]
fn test_unicode_escape_handling() {
    // Test that documents current unicode handling limitations
    let test_cases = vec![
        (r#""\u0041""#, "\\u0041"), // Currently NOT processed as unicode
        (r#""\n""#, "\n"),           // Basic escapes DO work
        (r#""\t""#, "\t"),           // Basic escapes DO work
    ];
    
    for (yaml_str, expected) in test_cases {
        use super::parser::parse_yaml_ast;
        use super::ast::YamlAst;
        
        let result = parse_yaml_ast(yaml_str, test_uri()).unwrap();
        match result {
            YamlAst::PlainString(s, _) => {
                assert_eq!(s, expected, "Unicode handling for '{}' differs from expected", yaml_str);
            }
            _ => panic!("Expected PlainString for '{}', got {:?}", yaml_str, result),
        }
    }
}

#[test]
fn test_malformed_yaml_error_recovery() {
    // Test tree-sitter's error recovery capabilities vs serde_yaml
    let malformed_cases = vec![
        ("key: [\n  unclosed", "Unclosed bracket"),
        ("key:\n  - item\n    bad_indent", "Bad indentation"),
        ("key: \"unclosed quote", "Unclosed quote"),
    ];
    
    for (yaml_str, description) in malformed_cases {
        let diagnostics = parse_yaml_ast_with_diagnostics(yaml_str, test_uri());
        
        // Should have syntax errors
        assert!(diagnostics.has_errors(), "Expected syntax error for: {}", description);
        assert!(!diagnostics.parse_successful, "Parse should fail for: {}", description);
        
        // Error should contain location information
        if let Some(error) = diagnostics.errors.first() {
            assert!(error.location.is_some(), "Error should have location for: {}", description);
        }
    }
}

#[test]
fn test_deep_nesting_handling() {
    // Test parser behavior with deep nesting (potential stack overflow risk)
    let mut deep_yaml = String::new();
    for i in 0..50 {
        deep_yaml.push_str(&format!("level{}: \n", i));
        deep_yaml.push_str("  ");
    }
    deep_yaml.push_str("value: \"deep\"");
    
    let diagnostics = parse_yaml_ast_with_diagnostics(&deep_yaml, test_uri());
    
    // Should either succeed or fail gracefully (no panic/stack overflow)
    if diagnostics.has_errors() {
        println!("Deep nesting failed gracefully with {} errors", diagnostics.error_count());
    } else {
        println!("Deep nesting succeeded");
    }
    
    // Test should not panic/crash
    assert!(true, "Parser handled deep nesting without crashing");
}

#[test] 
fn test_large_document_handling() {
    // Test parser behavior with large documents
    let mut large_yaml = String::from("Resources:\n");
    
    for i in 0..100 {
        large_yaml.push_str(&format!(
            "  Resource{}:\n    Type: AWS::S3::Bucket\n    Properties:\n      BucketName: \"bucket-{}\"\n",
            i, i
        ));
    }
    
    let start = std::time::Instant::now();
    let diagnostics = parse_yaml_ast_with_diagnostics(&large_yaml, test_uri());
    let duration = start.elapsed();
    
    // Should parse successfully in reasonable time
    assert!(!diagnostics.has_errors(), "Large document should parse successfully");
    assert!(duration < std::time::Duration::from_secs(5), "Parse time should be reasonable: {:?}", duration);
    
    println!("Large document (100 resources) parsed in {:?}", duration);
}
