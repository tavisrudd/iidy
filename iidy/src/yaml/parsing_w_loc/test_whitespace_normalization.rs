//! Test whitespace normalization in block scalars

use url::Url;

use crate::yaml::parsing::parser as original_parser;
use super::{parse_yaml_ast, convert::to_original_ast};
use super::compatibility_test::asts_equal;

#[test]
fn test_literal_block_final_newline() {
    // Test literal block without trailing newline
    let yaml = r#"
test: !Sub |
  {
    "key": "value"
  }"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing literal block final newline ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if asts_equal(&converted, &original_ast) {
                println!("✓ Literal block final newline works");
            } else {
                println!("✗ Literal block final newline failed");
                
                // Extract string contents to see the exact difference
                if let crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs) = &converted {
                    for (key, value) in conv_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "test" {
                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = value {
                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                        println!("Tree-sitter content: {:?}", content);
                                    }
                                }
                            }
                        }
                    }
                }
                
                if let crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs) = &original_ast {
                    for (key, value) in orig_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "test" {
                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = value {
                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                        println!("Original content: {:?}", content);
                                    }
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
            println!("✗ Both parsers failed: TS={}, Orig={}", tree_sitter_err.message, original_err);
        }
    }
}

#[test]
fn test_literal_block_with_following_content() {
    // Test literal block with content following it (like ProcessingFunction case)
    let yaml = r#"
test: !Sub |
  {
    "key": "value"
  }
      
other: "value"
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing literal block with following content ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if asts_equal(&converted, &original_ast) {
                println!("✓ Literal block with following content works");
            } else {
                println!("✗ Literal block with following content failed");
                
                // Extract string contents to see the exact difference
                if let crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs) = &converted {
                    for (key, value) in conv_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "test" {
                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = value {
                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                        println!("Tree-sitter content: {:?}", content);
                                    }
                                }
                            }
                        }
                    }
                }
                
                if let crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs) = &original_ast {
                    for (key, value) in orig_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "test" {
                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = value {
                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                        println!("Original content: {:?}", content);
                                    }
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
            println!("✗ Both parsers failed: TS={}, Orig={}", tree_sitter_err.message, original_err);
        }
    }
}

#[test]
fn test_empty_line_with_indentation() {
    // Test the exact case from cloudformation-tags-demo: empty line with indentation
    let yaml = r#"
test: !Sub |
  line1
  line2
              
  line4
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing empty line with indentation ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if asts_equal(&converted, &original_ast) {
                println!("✓ Empty line with indentation works");
            } else {
                println!("✗ Empty line with indentation failed");
                
                // Extract string contents to see the exact difference
                if let crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs) = &converted {
                    for (key, value) in conv_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "test" {
                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = value {
                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                        println!("Tree-sitter content: {:?}", content);
                                    }
                                }
                            }
                        }
                    }
                }
                
                if let crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs) = &original_ast {
                    for (key, value) in orig_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "test" {
                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = value {
                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                        println!("Original content: {:?}", content);
                                    }
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
            println!("✗ Both parsers failed: TS={}, Orig={}", tree_sitter_err.message, original_err);
        }
    }
}