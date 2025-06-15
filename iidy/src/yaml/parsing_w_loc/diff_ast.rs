//! Compare AST structures to find exact differences

use std::fs;
use url::Url;

use crate::yaml::parsing::parser as original_parser;
use crate::yaml::parsing::ast as original_ast;
use super::{parse_yaml_ast, convert::to_original_ast};

fn find_first_difference(converted: &original_ast::YamlAst, original: &original_ast::YamlAst, path: String) -> Option<String> {
    match (converted, original) {
        // Same type matches
        (original_ast::YamlAst::PlainString(c), original_ast::YamlAst::PlainString(o)) => {
            if c != o {
                Some(format!("String mismatch at {}: '{}' vs '{}'", path, c, o))
            } else {
                None
            }
        }
        (original_ast::YamlAst::TemplatedString(c), original_ast::YamlAst::TemplatedString(o)) => {
            if c != o {
                Some(format!("TemplatedString mismatch at {}: '{}' vs '{}'", path, c, o))
            } else {
                None
            }
        }
        (original_ast::YamlAst::Number(c), original_ast::YamlAst::Number(o)) => {
            if format!("{:?}", c) != format!("{:?}", o) {
                Some(format!("Number mismatch at {}: '{:?}' vs '{:?}'", path, c, o))
            } else {
                None
            }
        }
        (original_ast::YamlAst::Bool(c), original_ast::YamlAst::Bool(o)) => {
            if c != o {
                Some(format!("Bool mismatch at {}: '{}' vs '{}'", path, c, o))
            } else {
                None
            }
        }
        (original_ast::YamlAst::Null, original_ast::YamlAst::Null) => None,
        
        // Mapping comparison
        (original_ast::YamlAst::Mapping(c_pairs), original_ast::YamlAst::Mapping(o_pairs)) => {
            if c_pairs.len() != o_pairs.len() {
                return Some(format!("Mapping length mismatch at {}: {} vs {}", path, c_pairs.len(), o_pairs.len()));
            }
            
            for (i, ((c_key, c_val), (o_key, o_val))) in c_pairs.iter().zip(o_pairs.iter()).enumerate() {
                // Check key difference
                if let Some(diff) = find_first_difference(c_key, o_key, format!("{}[{}].key", path, i)) {
                    return Some(diff);
                }
                
                // Check value difference
                let key_name = match c_key {
                    original_ast::YamlAst::PlainString(s) => s.clone(),
                    _ => format!("key_{}", i),
                };
                if let Some(diff) = find_first_difference(c_val, o_val, format!("{}.{}", path, key_name)) {
                    return Some(diff);
                }
            }
            None
        }
        
        // Sequence comparison
        (original_ast::YamlAst::Sequence(c_items), original_ast::YamlAst::Sequence(o_items)) => {
            if c_items.len() != o_items.len() {
                return Some(format!("Sequence length mismatch at {}: {} vs {}", path, c_items.len(), o_items.len()));
            }
            
            for (i, (c_item, o_item)) in c_items.iter().zip(o_items.iter()).enumerate() {
                if let Some(diff) = find_first_difference(c_item, o_item, format!("{}[{}]", path, i)) {
                    return Some(diff);
                }
            }
            None
        }
        
        // CloudFormation tag comparison
        (original_ast::YamlAst::CloudFormationTag(c_tag), original_ast::YamlAst::CloudFormationTag(o_tag)) => {
            match (c_tag, o_tag) {
                (original_ast::CloudFormationTag::Ref(c_val), original_ast::CloudFormationTag::Ref(o_val)) => {
                    find_first_difference(c_val, o_val, format!("{}.!Ref", path))
                }
                (original_ast::CloudFormationTag::Sub(c_val), original_ast::CloudFormationTag::Sub(o_val)) => {
                    find_first_difference(c_val, o_val, format!("{}.!Sub", path))
                }
                (original_ast::CloudFormationTag::GetAtt(c_val), original_ast::CloudFormationTag::GetAtt(o_val)) => {
                    find_first_difference(c_val, o_val, format!("{}.!GetAtt", path))
                }
                _ => Some(format!("CloudFormation tag type mismatch at {}: {:?} vs {:?}", path, c_tag, o_tag))
            }
        }
        
        // Preprocessing tag comparison
        (original_ast::YamlAst::PreprocessingTag(c_tag), original_ast::YamlAst::PreprocessingTag(o_tag)) => {
            use crate::yaml::parsing::ast::PreprocessingTag;
            match (c_tag, o_tag) {
                (PreprocessingTag::If(c_if), PreprocessingTag::If(o_if)) => {
                    // Compare test field
                    if let Some(diff) = find_first_difference(&c_if.test, &o_if.test, format!("{}.!$if.test", path)) {
                        return Some(diff);
                    }
                    // Compare then field
                    if let Some(diff) = find_first_difference(&c_if.then_value, &o_if.then_value, format!("{}.!$if.then", path)) {
                        return Some(diff);
                    }
                    // Compare else field
                    match (&c_if.else_value, &o_if.else_value) {
                        (Some(c_else), Some(o_else)) => {
                            find_first_difference(c_else, o_else, format!("{}.!$if.else", path))
                        }
                        (None, None) => None,
                        _ => Some(format!("!$if else field presence mismatch at {}", path))
                    }
                }
                (PreprocessingTag::Eq(c_eq), PreprocessingTag::Eq(o_eq)) => {
                    // Compare left and right fields
                    if let Some(diff) = find_first_difference(&c_eq.left, &o_eq.left, format!("{}.!$eq.left", path)) {
                        return Some(diff);
                    }
                    find_first_difference(&c_eq.right, &o_eq.right, format!("{}.!$eq.right", path))
                }
                _ => Some(format!("Preprocessing tag type mismatch at {}: {:?} vs {:?}", path, 
                                std::mem::discriminant(c_tag), std::mem::discriminant(o_tag)))
            }
        }
        
        // Type mismatches
        _ => Some(format!("Type mismatch at {}: {:?} vs {:?}", path, 
                         std::mem::discriminant(converted), 
                         std::mem::discriminant(original)))
    }
}

#[test]
fn debug_cloudformation_tags_demo_diff() {
    let content = fs::read_to_string("example-templates/cloudformation-tags-demo.yaml").unwrap();
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Finding exact difference in cloudformation-tags-demo.yaml ===");
    
    match (parse_yaml_ast(&content, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(&content, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if let Some(diff) = find_first_difference(&converted, &original_ast, "root".to_string()) {
                println!("❌ First difference found: {}", diff);
            } else {
                println!("✓ No differences found - this shouldn't happen!");
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
fn debug_include_handlebars_equivalence_diff() {
    let content = fs::read_to_string("example-templates/yaml-iidy-syntax/include-handlebars-equivalence.yaml").unwrap();
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Finding exact difference in include-handlebars-equivalence.yaml ===");
    
    match (parse_yaml_ast(&content, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(&content, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if let Some(diff) = find_first_difference(&converted, &original_ast, "root".to_string()) {
                println!("❌ First difference found: {}", diff);
            } else {
                println!("✓ No differences found - this shouldn't happen!");
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
fn debug_string_formatting_demo_diff() {
    let content = fs::read_to_string("example-templates/string-formatting-demo.yaml").unwrap();
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Finding exact difference in string-formatting-demo.yaml ===");
    
    match (parse_yaml_ast(&content, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(&content, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if let Some(diff) = find_first_difference(&converted, &original_ast, "root".to_string()) {
                println!("❌ First difference found: {}", diff);
            } else {
                println!("✓ No differences found - this shouldn't happen!");
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
fn debug_advanced_cloudformation_diff() {
    let content = fs::read_to_string("example-templates/advanced-cloudformation.yaml").unwrap();
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Finding exact difference in advanced-cloudformation.yaml ===");
    
    match (parse_yaml_ast(&content, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(&content, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if let Some(diff) = find_first_difference(&converted, &original_ast, "root".to_string()) {
                println!("❌ First difference found: {}", diff);
            } else {
                println!("✓ No differences found - this shouldn't happen!");
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