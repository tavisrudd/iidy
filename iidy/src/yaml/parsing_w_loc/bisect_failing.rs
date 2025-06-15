//! Bisect failing files to find the exact problematic section

use std::fs;
use url::Url;

use crate::yaml::parsing::parser as original_parser;
use super::{parse_yaml_ast, convert::to_original_ast};
use super::compatibility_test::asts_equal;

#[test]
fn bisect_cloudformation_tags_demo() {
    let content = fs::read_to_string("example-templates/cloudformation-tags-demo.yaml").unwrap();
    let lines: Vec<&str> = content.lines().collect();
    
    println!("Full file has {} lines", lines.len());
    
    // Test progressively larger prefixes to find where it breaks
    for line_count in (10..lines.len()).step_by(10) {
        let prefix = lines[0..line_count].join("\n");
        let uri = Url::parse("file:///test.yaml").unwrap();
        
        match (parse_yaml_ast(&prefix, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(&prefix, uri.as_str())) {
            (Ok(tree_sitter_ast), Ok(original_ast)) => {
                let converted = to_original_ast(&tree_sitter_ast);
                if !asts_equal(&converted, &original_ast) {
                    println!("❌ MISMATCH at line {}", line_count);
                    println!("Last 5 lines added:");
                    for i in (line_count.saturating_sub(5))..line_count {
                        if i < lines.len() {
                            println!("  {}: {}", i+1, lines[i]);
                        }
                    }
                    return;
                } else {
                    println!("✓ Match at line {}", line_count);
                }
            }
            (Err(tree_sitter_err), Ok(_)) => {
                println!("⚠ Tree-sitter failed at line {}: {}", line_count, tree_sitter_err.message);
                return;
            }
            (Ok(_), Err(original_err)) => {
                println!("⚠ Original parser failed at line {}: {}", line_count, original_err);
                return;
            }
            (Err(tree_sitter_err), Err(original_err)) => {
                println!("⚠ Both failed at line {}: TS={}, Orig={}", line_count, tree_sitter_err.message, original_err);
                return;
            }
        }
    }
    
    println!("All prefixes match - this shouldn't happen!");
}

#[test]
fn test_multiline_sub_issue() {
    // Test the specific multiline !Sub that might be causing issues
    let yaml = r#"
test: !Sub |
  import json
  def handler():
      print("{{app_name}}")
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing multiline !Sub ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            if asts_equal(&converted, &original_ast) {
                println!("✓ Multiline !Sub works");
            } else {
                println!("✗ Multiline !Sub failed");
                println!("Converted: {:#?}", converted);
                println!("Original: {:#?}", original_ast);
            }
        }
        (Err(tree_sitter_err), Ok(_)) => {
            println!("✗ Tree-sitter failed: {}", tree_sitter_err.message);
        }
        (Ok(_), Err(original_err)) => {
            println!("✗ Original parser failed: {}", original_err);
        }
        (Err(tree_sitter_err), Err(original_err)) => {
            println!("✗ Both parsers failed: TS={}, Orig={}", tree_sitter_err.message, original_err);
        }
    }
}

#[test]
fn test_getatt_arn_issue() {
    // Test !GetAtt with nested properties that might be causing issues
    let yaml = r#"
test: !GetAtt LambdaExecutionRole.Arn
other: !GetAtt Database.Endpoint.Address
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing !GetAtt nested properties ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            if asts_equal(&converted, &original_ast) {
                println!("✓ !GetAtt nested properties works");
            } else {
                println!("✗ !GetAtt nested properties failed");
                println!("Converted: {:#?}", converted);
                println!("Original: {:#?}", original_ast);
            }
        }
        (Err(tree_sitter_err), Ok(_)) => {
            println!("✗ Tree-sitter failed: {}", tree_sitter_err.message);
        }
        (Ok(_), Err(original_err)) => {
            println!("✗ Original parser failed: {}", original_err);
        }
        (Err(tree_sitter_err), Err(original_err)) => {
            println!("✗ Both parsers failed: TS={}, Orig={}", tree_sitter_err.message, original_err);
        }
    }
}

#[test]
fn test_minimal_failing_segment() {
    // Test the exact segment that was identified as problematic
    let yaml = r#"
Resources:
  ProcessingFunction:
    Type: "AWS::Lambda::Function"
    Properties:
      FunctionName: !Sub "app-${Environment}-processor"
      Code:
        ZipFile: !Sub |
          import json
          def handler(event, context):
              return {'message': 'Hello'}
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing minimal failing segment ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            if asts_equal(&converted, &original_ast) {
                println!("✓ Minimal segment works");
            } else {
                println!("✗ Minimal segment failed");
                println!("Converted: {:#?}", converted);
                println!("Original: {:#?}", original_ast);
            }
        }
        (Err(tree_sitter_err), Ok(_)) => {
            println!("✗ Tree-sitter failed: {}", tree_sitter_err.message);
        }
        (Ok(_), Err(original_err)) => {
            println!("✗ Original parser failed: {}", original_err);
        }
        (Err(tree_sitter_err), Err(original_err)) => {
            println!("✗ Both parsers failed: TS={}, Orig={}", tree_sitter_err.message, original_err);
        }
    }
}