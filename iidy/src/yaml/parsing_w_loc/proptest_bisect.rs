use std::fs;
use std::path::Path;
use proptest::prelude::*;
use url::Url;

use crate::yaml::parsing::parser as original_parser;
use crate::yaml::parsing::ast as original_ast;
use super::{parse_yaml_ast, convert::to_original_ast};
use super::compatibility_test::compare_asts;

#[test]
fn test_groupby_bisect() {
    // Read the full groupby.yaml file
    let full_content = fs::read_to_string("example-templates/yaml-iidy-syntax/groupby.yaml").unwrap();
    let lines: Vec<&str> = full_content.lines().collect();
    
    println!("Full file has {} lines", lines.len());
    
    // Test progressively larger prefixes of the file to find where it breaks
    for line_count in (1..=lines.len()).step_by(10) {
        let prefix = lines[0..line_count].join("\n");
        let uri = Url::parse("file:///test.yaml").unwrap();
        
        match parse_yaml_ast(&prefix, uri.clone()) {
            Ok(tree_sitter_ast) => {
                match original_parser::parse_yaml_with_custom_tags_from_file(&prefix, uri.as_str()) {
                    Ok(original_ast) => {
                        let converted = to_original_ast(&tree_sitter_ast);
                        if compare_asts(&converted, &original_ast, Path::new("bisect_test")).is_err() {
                            println!("❌ MISMATCH at line {}", line_count);
                            println!("Last few lines:");
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
                    Err(_e) => {
                        println!("⚠ Original parser failed at line {}", line_count);
                    }
                }
            }
            Err(_e) => {
                println!("⚠ Tree-sitter parser failed at line {}", line_count);
            }
        }
    }
    
    println!("All prefixes match - this shouldn't happen!");
}

#[test]
fn test_minimal_failing_case() {
    // Test the exact failing case from line 111 area
    let minimal_yaml = r#"# Example demonstrating !$groupBy tag for grouping items by key
# The !$groupBy tag groups items in an array based on a key expression, similar to lodash groupBy

AWSTemplateFormatVersion: '2010-09-09'
Description: 'Example of !$groupBy tag for grouping operations'

$defs:
  resources:
    - name: "web-server-1"
      type: "EC2"
      environment: "production"
      team: "frontend"
    - name: "web-server-2"
      type: "EC2"
      environment: "production"
      team: "frontend"
    - name: "api-server-1"
      type: "EC2"
      environment: "production"
      team: "backend"
    - name: "database-1"
      type: "RDS"
      environment: "production"
      team: "backend"
    - name: "test-server-1"
      type: "EC2"
      environment: "staging"
      team: "qa"
    - name: "test-database"
      type: "RDS"
      environment: "staging"
      team: "qa"

Resources:
  # Example 1: Group resources by environment
  ResourcesByEnvironment:
    Type: AWS::SSM::Parameter
    Properties:
      Name: /app/config/resources-by-env
      Type: String
      Value: !$toJsonString
        - !$groupBy
            items: !$ resources
            key: !$ item.environment

  # Example 2: Group resources by type
  ResourcesByType:
    Type: AWS::SSM::Parameter
    Properties:
      Name: /app/config/resources-by-type
      Type: String
      Value: !$toJsonString
        - !$groupBy
            items: !$ resources
            key: !$ item.type

  # Example 3: Group resources by team
  ResourcesByTeam:
    Type: AWS::SSM::Parameter
    Properties:
      Name: /app/config/resources-by-team
      Type: String
      Value: !$toJsonString
        - !$groupBy
            items: !$ resources
            key: !$ item.team
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing minimal failing case ===");
    
    match parse_yaml_ast(minimal_yaml, uri.clone()) {
        Ok(tree_sitter_ast) => {
            println!("✓ Tree-sitter parsing succeeded");
            let converted = to_original_ast(&tree_sitter_ast);
            
            match original_parser::parse_yaml_with_custom_tags_from_file(minimal_yaml, uri.as_str()) {
                Ok(original_ast) => {
                    println!("✓ Original parsing succeeded");
                    
                    if compare_asts(&converted, &original_ast, Path::new("minimal_test")).is_ok() {
                        println!("✓ Minimal case compatible");
                    } else {
                        println!("✗ Minimal case incompatible - found the issue!");
                        
                        // Let's find the specific structural difference
                        if let (original_ast::YamlAst::Mapping(conv_pairs), original_ast::YamlAst::Mapping(orig_pairs)) = (&converted, &original_ast) {
                            println!("Converted has {} top-level pairs", conv_pairs.len());
                            println!("Original has {} top-level pairs", orig_pairs.len());
                            
                            // Find the first mismatch
                            for i in 0..std::cmp::min(conv_pairs.len(), orig_pairs.len()) {
                                let (conv_key, conv_val) = &conv_pairs[i];
                                let (orig_key, orig_val) = &orig_pairs[i];
                                
                                if !super::compatibility_test::asts_equal(conv_key, orig_key) || !super::compatibility_test::asts_equal(conv_val, orig_val) {
                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = conv_key {
                                        println!("❌ First mismatch at key '{}'", key_name);
                                        if !super::compatibility_test::asts_equal(conv_val, orig_val) {
                                            println!("Conv val: {:#?}", conv_val);
                                            println!("Orig val: {:#?}", orig_val);
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("✗ Original parser failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ Tree-sitter parser failed: {}", e.message);
        }
    }
}

#[test]
fn test_debug_tree_structure() {
    let simple_resources = r#"Resources:
  ResourcesByEnvironment:
    Type: AWS::SSM::Parameter
"#;
    
    let with_comments = r#"$defs:
  resources: []

Resources:
  # Example 1: Group resources by environment  
  ResourcesByEnvironment:
    Type: AWS::SSM::Parameter
"#;
    
    let comment_between_key_value = r#"Resources:
  ResourcesByEnvironment:
    # This is a comment inside the value
    Type: AWS::SSM::Parameter
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing simple Resources ===");
    match parse_yaml_ast(simple_resources, uri.clone()) {
        Ok(ast) => {
            let converted = to_original_ast(&ast);
            println!("Simple Resources AST: {:#?}", converted);
        }
        Err(e) => {
            println!("Failed: {}", e.message);
        }
    }
    
    println!("\n=== Testing with comments ===");
    match parse_yaml_ast(with_comments, uri.clone()) {
        Ok(ast) => {
            let converted = to_original_ast(&ast);
            println!("With comments AST: {:#?}", converted);
        }
        Err(e) => {
            println!("Failed: {}", e.message);
        }
    }
    
    println!("\n=== Testing comment between key and value ===");
    match parse_yaml_ast(comment_between_key_value, uri.clone()) {
        Ok(ast) => {
            let converted = to_original_ast(&ast);
            println!("Comment between key-value AST: {:#?}", converted);
        }
        Err(e) => {
            println!("Failed: {}", e.message);
        }
    }
}

#[test]
fn test_mapvalues_tag() {
    let mapvalues_yaml = r#"
Resources:
  ConfigMappings:
    Type: AWS::SSM::Parameter
    Properties:
      Value: !$mapValues
        items: !$ config
        template: "{{toUpperCase item.value}}"
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing !$mapValues tag ===");
    match parse_yaml_ast(mapvalues_yaml, uri.clone()) {
        Ok(tree_sitter_ast) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            match original_parser::parse_yaml_with_custom_tags_from_file(mapvalues_yaml, uri.as_str()) {
                Ok(original_ast) => {
                    if compare_asts(&converted, &original_ast, Path::new("mapvalues_test")).is_ok() {
                        println!("✓ mapValues compatible");
                    } else {
                        println!("✗ mapValues incompatible");
                        println!("Converted: {:#?}", converted);
                        println!("Original: {:#?}", original_ast);
                    }
                }
                Err(e) => {
                    println!("✗ Original parser failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ Tree-sitter parser failed: {}", e.message);
        }
    }
}

#[test]
fn test_all_preprocessing_tags() {
    println!("=== Running comprehensive preprocessing tag tests ===");
    test_mapvalues_tag();
    println!("Preprocessing tag tests completed!");
}

#[test]
fn test_remaining_issues() {
    // Test parseYaml with array syntax
    let parse_yaml_array = r#"
test: !$parseYaml ["key: value"]
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing parseYaml with array syntax ===");
    match parse_yaml_ast(parse_yaml_array, uri.clone()) {
        Ok(tree_sitter_ast) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            match original_parser::parse_yaml_with_custom_tags_from_file(parse_yaml_array, uri.as_str()) {
                Ok(original_ast) => {
                    if compare_asts(&converted, &original_ast, Path::new("parse_yaml_test")).is_ok() {
                        println!("✓ parseYaml array syntax compatible");
                    } else {
                        println!("✗ parseYaml array syntax incompatible");
                        println!("Converted: {:#?}", converted);
                        println!("Original: {:#?}", original_ast);
                    }
                }
                Err(e) => {
                    println!("✗ Original parser failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ Tree-sitter parser failed: {}", e.message);
        }
    }
    
    // Test include with query syntax
    let include_query = r#"
test: !$ config.database?host,port
"#;
    
    // Test include object form with query
    let include_object_query = r#"
test: !$
  path: config.database
  query: "host,port"
"#;
    
    println!("\n=== Testing include with query syntax ===");
    match parse_yaml_ast(include_query, uri.clone()) {
        Ok(tree_sitter_ast) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            match original_parser::parse_yaml_with_custom_tags_from_file(include_query, uri.as_str()) {
                Ok(original_ast) => {
                    if compare_asts(&converted, &original_ast, Path::new("include_query_test")).is_ok() {
                        println!("✓ include query syntax compatible");
                    } else {
                        println!("✗ include query syntax incompatible");
                        println!("Converted: {:#?}", converted);
                        println!("Original: {:#?}", original_ast);
                    }
                }
                Err(e) => {
                    println!("✗ Original parser failed: {}", e);
                }
            }
        }
        Err(e) => {
            println!("✗ Tree-sitter parser failed: {}", e.message);
        }
    }
}