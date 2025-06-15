//! Debug block scalar final newline issue

use url::Url;

use crate::yaml::parsing::parser as original_parser;
use super::{parse_yaml_ast, convert::to_original_ast};
use super::compatibility_test::asts_equal;

#[test]
fn debug_str_with_newlines_case() {
    // Test the exact case from include-handlebars-equivalence.yaml
    let yaml = r#"str_with_newlines: |    
  a
  b
  c
  d

  e
str_with_newlines_expl: "a\nb"
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing str_with_newlines case ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if asts_equal(&converted, &original_ast) {
                println!("✓ str_with_newlines case works");
            } else {
                println!("✗ str_with_newlines case failed");
                
                // Extract the specific content for comparison
                if let crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs) = &converted {
                    for (key, value) in conv_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "str_with_newlines" {
                                if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = value {
                                    println!("Tree-sitter 'str_with_newlines' content: {:?}", content);
                                    println!("Tree-sitter length: {}", content.len());
                                }
                            }
                        }
                    }
                }
                
                if let crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs) = &original_ast {
                    for (key, value) in orig_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "str_with_newlines" {
                                if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = value {
                                    println!("Original 'str_with_newlines' content: {:?}", content);
                                    println!("Original length: {}", content.len());
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
fn debug_tree_sitter_raw_output() {
    // Debug what tree-sitter gives us for the block
    let yaml = r#"str_with_newlines: |    
  a
  b
  c
  d

  e
str_with_newlines_expl: "a\nb"
"#;
    
    let _uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Debugging tree-sitter raw output ===");
    
    // Use tree-sitter directly to see what it gives us
    use tree_sitter::{Parser, Node};
    use tree_sitter_yaml::LANGUAGE;
    
    let mut parser = Parser::new();
    parser.set_language(&LANGUAGE.into()).unwrap();
    let tree = parser.parse(yaml, None).unwrap();
    
    fn debug_node(node: Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = node.utf8_text(source.as_bytes()).unwrap_or("<invalid>");
        
        // Focus on block_scalar nodes
        if node.kind() == "block_scalar" {
            println!("{}BLOCK_SCALAR found:", indent);
            println!("{}  Raw text: {:?}", indent, text);
            println!("{}  Text ends with newline: {}", indent, text.ends_with('\n'));
            println!("{}  Text length: {}", indent, text.len());
        } else {
            println!("{}{}[{}]: {:?}", indent, node.kind(), node.id(), text);
        }
        
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                debug_node(child, source, depth + 1);
            }
        }
    }
    
    debug_node(tree.root_node(), yaml, 0);
}

#[test]
fn debug_stack_urls_tree_sitter() {
    let yaml = r#"StackUrls:
  Description: "Useful stack URLs"
  Value: !Sub |
    {
      "s3_console": "https://s3.console.aws.amazon.com/s3/buckets/${ApplicationBucket}",
      "rds_console": "https://console.aws.amazon.com/rds/home?region=${AWS::Region}#database:id={{app_name}}-${AWS::Region}-db",
      "lambda_console": "https://console.aws.amazon.com/lambda/home?region=${AWS::Region}#/functions/{{app_name}}-${Environment}-processor",
      "cloudformation_console": "https://console.aws.amazon.com/cloudformation/home?region=${AWS::Region}#/stacks/stackinfo?stackId=${AWS::StackId}"
    }"#;
    
    println!("=== Debugging StackUrls tree-sitter raw output ===");
    
    // Use tree-sitter directly to see what it gives us
    use tree_sitter::{Parser, Node};
    use tree_sitter_yaml::LANGUAGE;
    
    let mut parser = Parser::new();
    parser.set_language(&LANGUAGE.into()).unwrap();
    let tree = parser.parse(yaml, None).unwrap();
    
    fn debug_node(node: Node, source: &str, depth: usize) {
        let indent = "  ".repeat(depth);
        let text = node.utf8_text(source.as_bytes()).unwrap_or("<invalid>");
        
        // Focus on block_scalar nodes
        if node.kind() == "block_scalar" {
            println!("{}BLOCK_SCALAR found:", indent);
            println!("{}  Raw text: {:?}", indent, text);
            println!("{}  Text ends with newline: {}", indent, text.ends_with('\n'));
            println!("{}  Text length: {}", indent, text.len());
        } else if depth <= 3 {
            println!("{}{}[{}]: {:?}", indent, node.kind(), node.id(), text);
        }
        
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                debug_node(child, source, depth + 1);
            }
        }
    }
    
    debug_node(tree.root_node(), yaml, 0);
}

#[test]
fn debug_processing_function_case() {
    // Test the ProcessingFunction case - should have final newline
    let yaml = r#"Code:
  ZipFile: !Sub |
    import json
    def handler():
        pass

# Other stuff
Environment:
  Variables:
    KEY: value
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing ProcessingFunction case ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if asts_equal(&converted, &original_ast) {
                println!("✓ ProcessingFunction case works");
            } else {
                println!("✗ ProcessingFunction case failed");
                
                // Extract the ZipFile content
                if let crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs) = &converted {
                    for (key, value) in conv_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "Code" {
                                if let crate::yaml::parsing::ast::YamlAst::Mapping(code_pairs) = value {
                                    for (sub_key, sub_value) in code_pairs {
                                        if let crate::yaml::parsing::ast::YamlAst::PlainString(sub_key_name) = sub_key {
                                            if sub_key_name == "ZipFile" {
                                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = sub_value {
                                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                                        println!("Tree-sitter ZipFile content: {:?}", content);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                if let crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs) = &original_ast {
                    for (key, value) in orig_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "Code" {
                                if let crate::yaml::parsing::ast::YamlAst::Mapping(code_pairs) = value {
                                    for (sub_key, sub_value) in code_pairs {
                                        if let crate::yaml::parsing::ast::YamlAst::PlainString(sub_key_name) = sub_key {
                                            if sub_key_name == "ZipFile" {
                                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = sub_value {
                                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                                        println!("Original ZipFile content: {:?}", content);
                                                    }
                                                }
                                            }
                                        }
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
fn debug_stack_urls_case() {
    // Test the StackUrls case - should NOT have final newline
    let yaml = r#"StackUrls:
  Description: "Useful stack URLs"
  Value: !Sub |
    {
      "s3_console": "https://s3.console.aws.amazon.com/s3/buckets/${ApplicationBucket}",
      "rds_console": "https://console.aws.amazon.com/rds/home?region=${AWS::Region}#database:id={{app_name}}-${AWS::Region}-db",
      "lambda_console": "https://console.aws.amazon.com/lambda/home?region=${AWS::Region}#/functions/{{app_name}}-${Environment}-processor",
      "cloudformation_console": "https://console.aws.amazon.com/cloudformation/home?region=${AWS::Region}#/stacks/stackinfo?stackId=${AWS::StackId}"
    }"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing StackUrls case ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if asts_equal(&converted, &original_ast) {
                println!("✓ StackUrls case works");
            } else {
                println!("✗ StackUrls case failed");
                
                // Extract the Value content
                if let crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs) = &converted {
                    for (key, value) in conv_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "StackUrls" {
                                if let crate::yaml::parsing::ast::YamlAst::Mapping(stack_pairs) = value {
                                    for (sub_key, sub_value) in stack_pairs {
                                        if let crate::yaml::parsing::ast::YamlAst::PlainString(sub_key_name) = sub_key {
                                            if sub_key_name == "Value" {
                                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = sub_value {
                                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                                        println!("Tree-sitter Value content: {:?}", content);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                
                if let crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs) = &original_ast {
                    for (key, value) in orig_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "StackUrls" {
                                if let crate::yaml::parsing::ast::YamlAst::Mapping(stack_pairs) = value {
                                    for (sub_key, sub_value) in stack_pairs {
                                        if let crate::yaml::parsing::ast::YamlAst::PlainString(sub_key_name) = sub_key {
                                            if sub_key_name == "Value" {
                                                if let crate::yaml::parsing::ast::YamlAst::CloudFormationTag(crate::yaml::parsing::ast::CloudFormationTag::Sub(sub_content)) = sub_value {
                                                    if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = sub_content.as_ref() {
                                                        println!("Original Value content: {:?}", content);
                                                    }
                                                }
                                            }
                                        }
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
fn debug_simple_block_newline() {
    // Test simple case 
    let yaml = r#"test: |
  line1
  line2
"#;
    
    let uri = Url::parse("file:///test.yaml").unwrap();
    
    println!("=== Testing simple block newline ===");
    
    match (parse_yaml_ast(yaml, uri.clone()), original_parser::parse_yaml_with_custom_tags_from_file(yaml, uri.as_str())) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            let converted = to_original_ast(&tree_sitter_ast);
            
            if asts_equal(&converted, &original_ast) {
                println!("✓ Simple block newline works");
            } else {
                println!("✗ Simple block newline failed");
                
                // Extract the specific content for comparison
                if let crate::yaml::parsing::ast::YamlAst::Mapping(conv_pairs) = &converted {
                    for (key, value) in conv_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "test" {
                                if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = value {
                                    println!("Tree-sitter content: {:?}", content);
                                }
                            }
                        }
                    }
                }
                
                if let crate::yaml::parsing::ast::YamlAst::Mapping(orig_pairs) = &original_ast {
                    for (key, value) in orig_pairs {
                        if let crate::yaml::parsing::ast::YamlAst::PlainString(key_name) = key {
                            if key_name == "test" {
                                if let crate::yaml::parsing::ast::YamlAst::PlainString(content) = value {
                                    println!("Original content: {:?}", content);
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