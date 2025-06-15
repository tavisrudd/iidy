//! Property-based compatibility testing using proptest
//! 
//! This module uses proptest to generate random YAML documents and test
//! compatibility between the tree-sitter parser and original parser.
//! Proptest helps find minimal failing examples automatically.

use proptest::prelude::*;
use std::collections::HashMap;
use url::Url;

use crate::yaml::parsing::parser as original_parser;
use crate::yaml::parsing::ast as original_ast;
use super::{parse_yaml_ast, convert::to_original_ast};
use super::compatibility_test::{asts_equal, compare_asts};
use super::simple_tag_generator::{TagConfig, yaml_document_strategy, block_style_document_strategy};

/// Helper function to test compatibility for generated YAML documents
fn test_generated_yaml_compatibility(yaml_doc: String, test_name: &str) -> Result<(), TestCaseError> {
    // Skip empty documents
    if yaml_doc.trim().is_empty() {
        return Ok(());
    }
    
    let test_uri = Url::parse("file:///proptest_generated.yaml").unwrap();
    
    // Try to parse with both parsers
    let tree_sitter_result = parse_yaml_ast(&yaml_doc, test_uri.clone());
    let original_result = original_parser::parse_yaml_with_custom_tags_from_file(&yaml_doc, test_uri.as_str());
    
    match (tree_sitter_result, original_result) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            // Both parsed successfully - compare results
            let converted_ast = to_original_ast(&tree_sitter_ast);
            
            if !asts_equal(&converted_ast, &original_ast) {
                // Create a temporary file path for error reporting
                let temp_path = std::path::Path::new("proptest_generated.yaml");
                
                // This will fail the test with detailed output
                compare_asts(&converted_ast, &original_ast, temp_path)
                    .map_err(|e| TestCaseError::Fail(format!("{} AST mismatch in generated YAML:\n{}\n\nError: {}", test_name, yaml_doc, e).into()))?;
            }
        }
        (Err(tree_sitter_err), Err(_original_err)) => {
            // Both failed to parse - this is acceptable (both consistent)
            // We might want to log this for debugging but it's not a test failure
        }
        (Ok(_), Err(original_err)) => {
            // Tree-sitter succeeded but original failed - this could be good or bad
            // For now, we'll consider this acceptable since tree-sitter might be more robust
            println!("{}: Tree-sitter parsed successfully but original parser failed: {}", test_name, original_err);
        }
        (Err(tree_sitter_err), Ok(_)) => {
            // Original succeeded but tree-sitter failed - this is a problem
            return Err(TestCaseError::Fail(format!("{}: Tree-sitter failed to parse YAML that original parser handled:\n{}\n\nTree-sitter error: {}", test_name, yaml_doc, tree_sitter_err.message).into()));
        }
    }
    
    Ok(())
}

/// Strategy for generating valid YAML scalar values
fn yaml_scalar_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // Simple strings
        "[a-zA-Z][a-zA-Z0-9_-]*".prop_map(|s| s),
        // Quoted strings
        "\"[^\"]*\"".prop_map(|s| s),
        // Numbers
        prop::num::i32::ANY.prop_map(|n| n.to_string()),
        // Booleans
        prop::bool::ANY.prop_map(|b| b.to_string()),
        // Null
        Just("null".to_string()),
        // Templated strings (simple)
        "[a-zA-Z][a-zA-Z0-9_-]*\\{\\{[a-zA-Z][a-zA-Z0-9_]*\\}\\}[a-zA-Z0-9_-]*".prop_map(|s| format!("\"{}\"", s)),
    ]
}

/// Strategy for generating simple YAML sequences
fn yaml_sequence_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(yaml_scalar_strategy(), 0..5)
        .prop_map(|items| {
            if items.is_empty() {
                "[]".to_string()
            } else {
                format!("[{}]", items.join(", "))
            }
        })
}

/// Strategy for generating simple flow-style tags that should work
fn flow_tag_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        // CloudFormation tags
        yaml_scalar_strategy().prop_map(|s| format!("!Ref {}", s)),
        yaml_scalar_strategy().prop_map(|s| format!("!Sub \"Hello ${{{}}}\"", s)),
        yaml_scalar_strategy().prop_map(|s| format!("!GetAtt {}.Property", s)),
        
        // Simple preprocessing tags
        prop::bool::ANY.prop_map(|b| format!("!$not {}", b)),
        yaml_scalar_strategy().prop_map(|s| format!("!$ {}", s)),
    ]
}

/// Strategy for generating simple YAML mapping entries  
fn yaml_mapping_entry_strategy() -> impl Strategy<Value = (String, String)> {
    (
        "[a-zA-Z][a-zA-Z0-9_]*", // key
        prop_oneof![
            yaml_scalar_strategy(),
            yaml_sequence_strategy(),
            flow_tag_strategy(),
        ]
    )
}

/// Strategy for generating simple YAML documents that should parse consistently
fn simple_yaml_document_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(yaml_mapping_entry_strategy(), 1..8)
        .prop_map(|entries| {
            let mut lines = Vec::new();
            for (key, value) in entries {
                lines.push(format!("{}: {}", key, value));
            }
            lines.join("\n")
        })
}

/// Strategy for generating more complex nested structures
fn nested_yaml_strategy() -> impl Strategy<Value = String> {
    // For now, keep it simple - we can expand this as we fix basic issues
    prop_oneof![
        simple_yaml_document_strategy(),
        // Add more complex structures later
    ]
}

proptest! {
    #![proptest_config(ProptestConfig {
        // Run fewer cases initially to keep test times reasonable
        cases: 50,
        // Enable shrinking to find minimal failing examples
        max_shrink_iters: 100,
        .. ProptestConfig::default()
    })]

    /// Property test: parsed and converted AST should equal original AST
    #[test]
    fn prop_tree_sitter_matches_original_parser(yaml_doc in simple_yaml_document_strategy()) {
        // Skip empty documents or documents that are known to be problematic
        if yaml_doc.trim().is_empty() {
            return Ok(());
        }
        
        let test_uri = Url::parse("file:///proptest.yaml").unwrap();
        
        // Try to parse with both parsers
        let tree_sitter_result = parse_yaml_ast(&yaml_doc, test_uri.clone());
        let original_result = original_parser::parse_yaml_with_custom_tags_from_file(&yaml_doc, test_uri.as_str());
        
        match (tree_sitter_result, original_result) {
            (Ok(tree_sitter_ast), Ok(original_ast)) => {
                // Both parsed successfully - compare results
                let converted_ast = to_original_ast(&tree_sitter_ast);
                
                if !asts_equal(&converted_ast, &original_ast) {
                    // Create a temporary file path for error reporting
                    let temp_path = std::path::Path::new("proptest_generated.yaml");
                    
                    // This will fail the test with detailed output
                    compare_asts(&converted_ast, &original_ast, temp_path)
                        .map_err(|e| TestCaseError::Fail(format!("AST mismatch in generated YAML:\n{}\n\nError: {}", yaml_doc, e).into()))?;
                }
            }
            (Err(tree_sitter_err), Err(_original_err)) => {
                // Both failed to parse - this is acceptable (both consistent)
                // We might want to log this for debugging but it's not a test failure
            }
            (Ok(_), Err(original_err)) => {
                // Tree-sitter succeeded but original failed - this could be good or bad
                // For now, we'll consider this acceptable since tree-sitter might be more robust
                // In the future, we might want to investigate these cases
                println!("Tree-sitter parsed successfully but original parser failed: {}", original_err);
            }
            (Err(tree_sitter_err), Ok(_)) => {
                // Original succeeded but tree-sitter failed - this is a problem
                return Err(TestCaseError::Fail(format!("Tree-sitter failed to parse YAML that original parser handled:\n{}\n\nTree-sitter error: {}", yaml_doc, tree_sitter_err.message).into()));
            }
        }
    }

    /// Property test focused on simple flow-style tags that should work
    #[test]
    fn prop_flow_tags_work_correctly(tag_expr in flow_tag_strategy()) {
        let yaml_doc = format!("test_value: {}", tag_expr);
        let test_uri = Url::parse("file:///proptest_flow.yaml").unwrap();
        
        let tree_sitter_result = parse_yaml_ast(&yaml_doc, test_uri.clone());
        let original_result = original_parser::parse_yaml_with_custom_tags_from_file(&yaml_doc, test_uri.as_str());
        
        match (tree_sitter_result, original_result) {
            (Ok(tree_sitter_ast), Ok(original_ast)) => {
                let converted_ast = to_original_ast(&tree_sitter_ast);
                if !asts_equal(&converted_ast, &original_ast) {
                    let temp_path = std::path::Path::new("proptest_flow_tag.yaml");
                    compare_asts(&converted_ast, &original_ast, temp_path)
                        .map_err(|e| TestCaseError::Fail(format!("Flow tag AST mismatch:\n{}\n\nError: {}", yaml_doc, e).into()))?;
                }
            }
            (tree_sitter_result, original_result) => {
                // If either parser fails, we want to know about it for flow tags
                return Err(TestCaseError::Fail(format!(
                    "Parser inconsistency for flow tag:\n{}\nTree-sitter: {:?}\nOriginal: {:?}", 
                    yaml_doc, 
                    tree_sitter_result.map(|_| "Success").map_err(|e| e.message),
                    original_result.map(|_| "Success")
                ).into()));
            }
        }
    }

    /// Property test: CloudFormation tags only
    #[test]
    fn prop_cloudformation_tags_only(yaml_doc in yaml_document_strategy(TagConfig::with_cloudformation_tags().with_tag_probability(0.3).with_limits(3))) {
        test_generated_yaml_compatibility(yaml_doc, "CloudFormation tags")?;
    }

    /// Property test: Preprocessing tags only
    #[test]
    fn prop_preprocessing_tags_only(yaml_doc in yaml_document_strategy(TagConfig::with_preprocessing_tags().with_tag_probability(0.7))) {
        test_generated_yaml_compatibility(yaml_doc, "Preprocessing tags")?;
    }

    /// Property test: Mixed tags with high probability
    #[test]
    fn prop_mixed_tags_high_probability(yaml_doc in yaml_document_strategy(TagConfig::with_all_tags().with_tag_probability(0.8))) {
        test_generated_yaml_compatibility(yaml_doc, "Mixed tags high prob")?;
    }

    /// Property test: Specific tag combinations
    #[test]
    fn prop_specific_tag_combinations(yaml_doc in yaml_document_strategy(
        TagConfig::default()
            .add_cloudformation_tag("Ref")
            .add_cloudformation_tag("Sub")
            .add_preprocessing_tag("$")
            .add_preprocessing_tag("$not")
            .add_preprocessing_tag("$parseYaml")
            .with_tag_probability(0.9)
            .with_limits(4)
    )) {
        test_generated_yaml_compatibility(yaml_doc, "Specific tag combinations")?;
    }

    /// Property test: Block-style tags (more complex structures)
    #[test]
    fn prop_block_style_tags(yaml_doc in block_style_document_strategy(
        TagConfig::default()
            .add_preprocessing_tag("$if")
            .add_preprocessing_tag("$let")
            .add_preprocessing_tag("$map")
            .add_preprocessing_tag("$groupBy")
            .with_tag_probability(0.5)
            .with_limits(3)
    )) {
        test_generated_yaml_compatibility(yaml_doc, "Block-style tags")?;
    }
}

/// Manual test cases for specific known patterns
#[cfg(test)]
mod manual_test_cases {
    use super::*;

    /// Test specific minimal cases that we know should work
    #[test]
    fn test_minimal_working_cases() {
        let test_cases = vec![
            // Basic scalar
            "key: value",
            // Basic number
            "key: 42", 
            // Basic boolean
            "key: true",
            // Basic sequence
            "key: [1, 2, 3]",
            // Simple CloudFormation tag
            "key: !Ref MyResource",
            // Simple preprocessing tag
            "key: !$not true",
            // Basic mapping
            "key1: value1\nkey2: value2",
        ];

        for yaml_doc in test_cases {
            let test_uri = Url::parse("file:///manual_test.yaml").unwrap();
            
            let tree_sitter_result = parse_yaml_ast(yaml_doc, test_uri.clone());
            let original_result = original_parser::parse_yaml_with_custom_tags_from_file(yaml_doc, test_uri.as_str());
            
            match (tree_sitter_result, original_result) {
                (Ok(tree_sitter_ast), Ok(original_ast)) => {
                    let converted_ast = to_original_ast(&tree_sitter_ast);
                    if !asts_equal(&converted_ast, &original_ast) {
                        panic!("Manual test case failed for: {}\nConverted: {:#?}\nOriginal: {:#?}", 
                               yaml_doc, converted_ast, original_ast);
                    } else {
                        println!("✓ Manual test passed: {}", yaml_doc);
                    }
                }
                (tree_sitter_result, original_result) => {
                    panic!("Parser inconsistency for manual test: {}\nTree-sitter: {:?}\nOriginal: {:?}", 
                           yaml_doc, 
                           tree_sitter_result.map(|_| "Success").map_err(|e| e.message),
                           original_result.map(|_| "Success"));
                }
            }
        }
    }

    /// Test specific cases that are known to fail (for documentation)
    #[test] 
    #[should_panic(expected = "block-style")]
    fn test_known_failing_block_style_tags() {
        let yaml_doc = r#"
test_tag: !$map
  items: [1, 2, 3]
  template: "item-{{item}}"
"#;
        
        let test_uri = Url::parse("file:///block_style_test.yaml").unwrap();
        
        let tree_sitter_result = parse_yaml_ast(yaml_doc, test_uri.clone());
        let original_result = original_parser::parse_yaml_with_custom_tags_from_file(yaml_doc, test_uri.as_str());
        
        match (tree_sitter_result, original_result) {
            (Ok(tree_sitter_ast), Ok(original_ast)) => {
                let converted_ast = to_original_ast(&tree_sitter_ast);
                if !asts_equal(&converted_ast, &original_ast) {
                    panic!("Expected failure: block-style tags not yet supported");
                }
            }
            _ => panic!("Unexpected parser behavior"),
        }
    }
}