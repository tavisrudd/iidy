//! Debug specific failing advanced files

use std::path::Path;
use url::Url;

use crate::yaml::parsing::parser as original_parser;
use super::{parse_yaml_ast, convert::to_original_ast};
use super::compatibility_test::compare_asts;

#[test]
fn debug_cloudformation_tags_demo() {
    let file_path = Path::new("example-templates/cloudformation-tags-demo.yaml");
    if !file_path.exists() {
        println!("File not found: {}", file_path.display());
        return;
    }
    
    let content = std::fs::read_to_string(file_path).unwrap();
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing cloudformation-tags-demo.yaml ===");
    
    // Try parsing with both parsers
    let tree_sitter_result = parse_yaml_ast(&content, uri.clone());
    let original_result = original_parser::parse_yaml_with_custom_tags_from_file(&content, uri.as_str());
    
    match (tree_sitter_result, original_result) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            match compare_asts(&converted, &original_ast, file_path) {
                Ok(()) => {
                    println!("✓ cloudformation-tags-demo.yaml is compatible!");
                }
                Err(e) => {
                    println!("✗ cloudformation-tags-demo.yaml incompatible: {}", e);
                    
                    // Try to find the first structural difference
                    if let (crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs), crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs)) = (&converted, &original_ast) {
                        println!("Converted has {} top-level pairs", conv_pairs.len());
                        println!("Original has {} top-level pairs", orig_pairs.len());
                        
                        for i in 0..std::cmp::min(conv_pairs.len(), orig_pairs.len()) {
                            let (conv_key, conv_val) = &conv_pairs[i];
                            let (orig_key, orig_val) = &orig_pairs[i];
                            
                            if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = conv_key {
                                if !super::compatibility_test::asts_equal(conv_val, orig_val) {
                                    println!("❌ First value mismatch at key '{}'", key_name);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        (Err(tree_sitter_err), Ok(_)) => {
            println!("✗ Tree-sitter failed: {}", tree_sitter_err.message);
        }
        (Ok(_), Err(original_err)) => {
            println!("✗ Original parser failed: {}", original_err);
        }
        (Err(tree_sitter_err), Err(original_err)) => {
            println!("✗ Both parsers failed");
            println!("  Tree-sitter: {}", tree_sitter_err.message);
            println!("  Original: {}", original_err);
        }
    }
}

#[test]
fn debug_advanced_cloudformation() {
    let file_path = Path::new("example-templates/advanced-cloudformation.yaml");
    if !file_path.exists() {
        println!("File not found: {}", file_path.display());
        return;
    }
    
    let content = std::fs::read_to_string(file_path).unwrap();
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing advanced-cloudformation.yaml ===");
    
    // Try parsing with both parsers
    let tree_sitter_result = parse_yaml_ast(&content, uri.clone());
    let original_result = original_parser::parse_yaml_with_custom_tags_from_file(&content, uri.as_str());
    
    match (tree_sitter_result, original_result) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            match compare_asts(&converted, &original_ast, file_path) {
                Ok(()) => {
                    println!("✓ advanced-cloudformation.yaml is compatible!");
                }
                Err(e) => {
                    println!("✗ advanced-cloudformation.yaml incompatible: {}", e);
                    
                    // Try to find the first structural difference
                    if let (crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs), crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs)) = (&converted, &original_ast) {
                        println!("Converted has {} top-level pairs", conv_pairs.len());
                        println!("Original has {} top-level pairs", orig_pairs.len());
                        
                        for i in 0..std::cmp::min(conv_pairs.len(), orig_pairs.len()) {
                            let (conv_key, conv_val) = &conv_pairs[i];
                            let (orig_key, orig_val) = &orig_pairs[i];
                            
                            if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = conv_key {
                                if !super::compatibility_test::asts_equal(conv_val, orig_val) {
                                    println!("❌ First value mismatch at key '{}'", key_name);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
        (Err(tree_sitter_err), Ok(_)) => {
            println!("✗ Tree-sitter failed: {}", tree_sitter_err.message);
        }
        (Ok(_), Err(original_err)) => {
            println!("✗ Original parser failed: {}", original_err);
        }
        (Err(tree_sitter_err), Err(original_err)) => {
            println!("✗ Both parsers failed");
            println!("  Tree-sitter: {}", tree_sitter_err.message);
            println!("  Original: {}", original_err);
        }
    }
}

#[test] 
fn debug_minimal_multiline_sub() {
    // Test a minimal case that might be causing issues
    let yaml = r#"
test: !Sub |
  import json
  def handler():
      print("{{app_name}}")
      return "{{environment}}"
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing minimal multiline !Sub ===");
    
    let tree_sitter_result = parse_yaml_ast(yaml, uri.clone());
    let original_result = original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str());
    
    match (tree_sitter_result, original_result) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if super::compatibility_test::asts_equal(&converted, &original_ast) {
                println!("✓ Minimal multiline !Sub works");
            } else {
                println!("✗ Minimal multiline !Sub failed");
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
            println!("✗ Both parsers failed");
            println!("  Tree-sitter: {}", tree_sitter_err.message);
            println!("  Original: {}", original_err);
        }
    }
}