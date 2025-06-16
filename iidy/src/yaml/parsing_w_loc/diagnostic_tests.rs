#[cfg(test)]
mod diagnostic_tests {
    use url::Url;
    
    // Import the diagnostic API functions
    use crate::yaml::parsing_w_loc::{
        YamlParser, parse_yaml_ast_with_diagnostics, 
        parse_and_convert_to_original_with_diagnostics, validate_yaml_only,
        error_codes
    };

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
        assert!(diagnostics.errors.iter().any(|e| e.message.contains("unknownTag1")));
        assert!(diagnostics.errors.iter().any(|e| e.message.contains("unknownTag2")));
        
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
        let missing_in_errors: Vec<_> = diagnostics.errors.iter()
            .filter(|e| e.message.contains("Missing required 'in' field"))
            .collect();
        assert_eq!(missing_in_errors.len(), 2);
        
        // Verify error codes
        assert!(missing_in_errors.iter().all(|e| 
            e.code.as_ref().map(|c| c == error_codes::MISSING_FIELD).unwrap_or(false)
        ));
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
        let syntax_errors: Vec<_> = diagnostics.errors.iter()
            .filter(|e| e.code.as_ref().map(|c| c == error_codes::SYNTAX_ERROR).unwrap_or(false))
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
        assert!(diagnostics.warnings.iter().any(|w| w.message.contains("unexpected_field")));
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
        assert!(diagnostics.errors.iter().any(|e| e.message.contains("Missing required 'in' field")));
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
        assert!(diagnostics.errors[0].message.contains("Missing required 'in' field"));
    }
}