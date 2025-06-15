//! Debug the exact failing case structure

use url::Url;

use crate::yaml::parsing::parser as original_parser;
use super::{parse_yaml_ast, convert::to_original_ast};
use super::compatibility_test::asts_equal;

#[test]
fn test_exact_zipfile_structure() {
    // Test the exact ZipFile structure that's failing
    let yaml = r#"
Code:
  ZipFile: !Sub |
    import json
    def handler(event, context):
        return {'message': 'Hello'}
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing exact ZipFile structure ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            if asts_equal(&converted, &original_ast) {
                println!("✓ Exact ZipFile structure works");
            } else {
                println!("✗ Exact ZipFile structure failed");
                
                // Debug the specific ZipFile value
                if let crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs) = &converted {
                    for (key, value) in conv_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "Code" {
                                println!("Code value in converted: {:#?}", value);
                            }
                        }
                    }
                }
                
                if let crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs) = &original_ast {
                    for (key, value) in orig_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "Code" {
                                println!("Code value in original: {:#?}", value);
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
            println!("✗ Both parsers failed: TS={}, Orig={}", tree_sitter_err.message, original_err);
        }
    }
}

#[test]
fn test_debug_tree_structure() {
    // Let's see what tree-sitter produces for our problematic case
    let yaml = r#"
Code:
  ZipFile: !Sub |
    import json
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Debugging tree-sitter structure ===");
    
    match parse_yaml_ast(yaml, uri.clone()) {
        Ok(tree_sitter_ast) => {
            println!("Tree-sitter AST: {:#?}", tree_sitter_ast);
            
            let converted = to_original_ast(&tree_sitter_ast);
            println!("Converted AST: {:#?}", converted);
        }
        Err(e) => {
            println!("Tree-sitter failed: {}", e.message);
        }
    }
}