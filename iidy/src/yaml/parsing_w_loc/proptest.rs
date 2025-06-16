//! Property-based testing for YAML parser compatibility
//!
//! This module provides configurable generators for creating YAML documents
//! with specific custom tags and comprehensive compatibility testing.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use std::collections::HashSet;
use url::Url;

use super::compatibility_test::{asts_equal, compare_asts};
use super::{convert::to_original_ast, parse_yaml_ast};
use crate::yaml::parsing::parser as original_parser;

/// Tag type selection for configuration
#[derive(Debug, Clone, Copy)]
pub enum TagTypes {
    CloudFormation,
    Preprocessing,
    All,
}

/// Preset configurations for common use cases
#[derive(Debug, Clone, Copy)]
pub enum ConfigPreset {
    CloudFormationLight,
    PreprocessingHeavy,
    Mixed,
}

/// Common CloudFormation tags
fn common_cf_tags() -> HashSet<String> {
    ["Ref", "Sub", "GetAtt"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Common preprocessing tags  
fn common_prep_tags() -> HashSet<String> {
    ["$", "$include", "$not", "$parseYaml", "$parseJson"]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Configuration for which tags should appear in generated YAML documents
#[derive(Debug, Clone)]
pub struct TagConfig {
    /// CloudFormation tags to include
    pub cloudformation_tags: HashSet<String>,
    /// Preprocessing tags to include  
    pub preprocessing_tags: HashSet<String>,
    /// Probability that any given value will be a tag (0.0 to 1.0)
    pub tag_probability: f64,
    /// Maximum number of items in sequences/mappings
    pub max_items: usize,
}

impl Default for TagConfig {
    fn default() -> Self {
        Self {
            cloudformation_tags: HashSet::new(),
            preprocessing_tags: HashSet::new(),
            tag_probability: 0.3,
            max_items: 5,
        }
    }
}

impl TagConfig {
    /// Create a config with specified tag types and settings
    pub fn new(tag_types: TagTypes, probability: f64, max_items: usize) -> Self {
        let (cf_tags, prep_tags) = match tag_types {
            TagTypes::CloudFormation => (common_cf_tags(), HashSet::new()),
            TagTypes::Preprocessing => (HashSet::new(), common_prep_tags()),
            TagTypes::All => (common_cf_tags(), common_prep_tags()),
        };

        Self {
            cloudformation_tags: cf_tags,
            preprocessing_tags: prep_tags,
            tag_probability: probability.clamp(0.0, 1.0),
            max_items,
        }
    }

    /// Quick preset configurations
    pub fn preset(preset: ConfigPreset) -> Self {
        match preset {
            ConfigPreset::CloudFormationLight => Self::new(TagTypes::CloudFormation, 0.3, 3),
            ConfigPreset::PreprocessingHeavy => Self::new(TagTypes::Preprocessing, 0.5, 5),
            ConfigPreset::Mixed => Self::new(TagTypes::All, 0.4, 5),
        }
    }
}

/// Generate simple scalar values
pub fn simple_scalar_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z][a-zA-Z0-9_]{0,5}".prop_map(|s| s),
        "\"[a-zA-Z0-9 ]{0,10}\"".prop_map(|s| s),
        (1i32..100).prop_map(|n| n.to_string()),
        prop::bool::ANY.prop_map(|b| b.to_string()),
        Just("null".to_string()),
    ]
}

/// Generate CloudFormation tag values
pub fn cloudformation_tag_value_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z][a-zA-Z0-9_]{1,8}".prop_map(|s| format!("!Ref {}", s)),
        "[a-zA-Z][a-zA-Z0-9_]{1,8}".prop_map(|param| format!("!Sub \"Value: ${{{}}}\"", param)),
        ("[a-zA-Z][a-zA-Z0-9_]{1,5}", "[a-zA-Z][a-zA-Z0-9_]{1,5}")
            .prop_map(|(res, attr)| format!("!GetAtt {}.{}", res, attr)),
    ]
}

/// Generate preprocessing tag values
pub fn preprocessing_tag_value_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z][a-zA-Z0-9_]{1,8}".prop_map(|path| format!("!$ {}", path)),
        "[a-zA-Z][a-zA-Z0-9_]{1,8}".prop_map(|path| format!("!$include {}", path)),
        prop::bool::ANY.prop_map(|b| format!("!$not {}", b)),
        Just("!$parseYaml [\"key: value\"]".to_string()),
    ]
}

/// Generate a value that could be a scalar or a tag
pub fn yaml_value_strategy(config: TagConfig) -> BoxedStrategy<String> {
    let scalar_weight = ((1.0 - config.tag_probability) * 100.0) as u32;
    let cf_weight = if config.cloudformation_tags.is_empty() {
        0
    } else {
        (config.tag_probability * 50.0) as u32
    };
    let prep_weight = if config.preprocessing_tags.is_empty() {
        0
    } else {
        (config.tag_probability * 50.0) as u32
    };

    // Ensure we always have meaningful weights
    let scalar_weight = scalar_weight.max(10);
    let cf_weight = if cf_weight > 0 { cf_weight.max(5) } else { 0 };
    let prep_weight = if prep_weight > 0 {
        prep_weight.max(5)
    } else {
        0
    };

    if cf_weight > 0 && prep_weight > 0 {
        prop_oneof![
            scalar_weight => simple_scalar_strategy(),
            cf_weight => cloudformation_tag_value_strategy(),
            prep_weight => preprocessing_tag_value_strategy(),
        ]
        .boxed()
    } else if cf_weight > 0 {
        prop_oneof![
            scalar_weight => simple_scalar_strategy(),
            cf_weight => cloudformation_tag_value_strategy(),
        ]
        .boxed()
    } else if prep_weight > 0 {
        prop_oneof![
            scalar_weight => simple_scalar_strategy(),
            prep_weight => preprocessing_tag_value_strategy(),
        ]
        .boxed()
    } else {
        simple_scalar_strategy().boxed()
    }
}

/// Generate a complete YAML document
pub fn yaml_document_strategy(config: TagConfig) -> impl Strategy<Value = String> {
    let max_items = config.max_items.min(3); // Keep it small to avoid rejects
    prop::collection::vec(
        (
            "[a-zA-Z][a-zA-Z0-9_]{1,5}", // key - simplified
            yaml_value_strategy(config.clone()),
        ),
        1..=max_items,
    )
    .prop_map(|entries| {
        entries
            .into_iter()
            .map(|(key, value)| format!("{}: {}", key, value))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Helper function to test compatibility for generated YAML documents
fn test_generated_yaml_compatibility(
    yaml_doc: String,
    test_name: &str,
) -> Result<(), TestCaseError> {
    // Skip empty documents
    if yaml_doc.trim().is_empty() {
        return Ok(());
    }

    let test_uri = Url::parse("file:///proptest_generated.yaml").unwrap();

    // Try to parse with both parsers
    let tree_sitter_result = parse_yaml_ast(&yaml_doc, test_uri.clone());
    let original_result =
        original_parser::parse_yaml_with_custom_tags_from_file(&yaml_doc, test_uri.as_str());

    match (tree_sitter_result, original_result) {
        (Ok(tree_sitter_ast), Ok(original_ast)) => {
            // Both parsed successfully - compare results
            let converted_ast = to_original_ast(&tree_sitter_ast);

            if !asts_equal(&converted_ast, &original_ast) {
                // Create a temporary file path for error reporting
                let temp_path = std::path::Path::new("proptest_generated.yaml");

                // This will fail the test with detailed output
                compare_asts(&converted_ast, &original_ast, temp_path).map_err(|e| {
                    TestCaseError::Fail(
                        format!(
                            "{} AST mismatch in generated YAML:\n{}\n\nError: {}",
                            test_name, yaml_doc, e
                        )
                        .into(),
                    )
                })?;
            }
        }
        (Err(_tree_sitter_err), Err(_original_err)) => {
            // Both failed to parse - this is acceptable (both consistent)
        }
        (Ok(_), Err(original_err)) => {
            // Tree-sitter succeeded but original failed - could be acceptable
            println!(
                "{}: Tree-sitter parsed successfully but original parser failed: {}",
                test_name, original_err
            );
        }
        (Err(tree_sitter_err), Ok(_)) => {
            // Original succeeded but tree-sitter failed - this is a problem
            return Err(TestCaseError::Fail(format!("{}: Tree-sitter failed to parse YAML that original parser handled:\n{}\n\nTree-sitter error: {}", test_name, yaml_doc, tree_sitter_err.message).into()));
        }
    }

    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig {
        // Run fewer cases initially to keep test times reasonable
        cases: 50,
        // Enable shrinking to find minimal failing examples
        max_shrink_iters: 100,
        .. ProptestConfig::default()
    })]

    /// Property test: CloudFormation tags only
    #[test]
    fn prop_cloudformation_tags_only(yaml_doc in yaml_document_strategy(TagConfig::preset(ConfigPreset::CloudFormationLight))) {
        test_generated_yaml_compatibility(yaml_doc, "CloudFormation tags")?;
    }

    /// Property test: Preprocessing tags only
    #[test]
    fn prop_preprocessing_tags_only(yaml_doc in yaml_document_strategy(TagConfig::preset(ConfigPreset::PreprocessingHeavy))) {
        test_generated_yaml_compatibility(yaml_doc, "Preprocessing tags")?;
    }

    /// Property test: Mixed tags with moderate probability
    #[test]
    fn prop_mixed_tags(yaml_doc in yaml_document_strategy(TagConfig::preset(ConfigPreset::Mixed))) {
        test_generated_yaml_compatibility(yaml_doc, "Mixed tags")?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::strategy::ValueTree;

    #[test]
    fn test_tag_config_presets() {
        let cf_config = TagConfig::preset(ConfigPreset::CloudFormationLight);
        assert!(cf_config.cloudformation_tags.contains("Ref"));
        assert_eq!(cf_config.tag_probability, 0.3);
        assert_eq!(cf_config.max_items, 3);

        let mixed_config = TagConfig::preset(ConfigPreset::Mixed);
        assert!(mixed_config.cloudformation_tags.contains("Ref"));
        assert!(mixed_config.preprocessing_tags.contains("$"));
        assert_eq!(mixed_config.tag_probability, 0.4);
    }

    #[test]
    fn test_simple_scalar_generation() {
        let strategy = simple_scalar_strategy();
        let mut runner = proptest::test_runner::TestRunner::default();

        for _ in 0..3 {
            let value = strategy.new_tree(&mut runner).unwrap();
            let scalar = value.current();
            assert!(!scalar.is_empty());
        }
    }

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
            let original_result =
                original_parser::parse_yaml_with_custom_tags_from_file(yaml_doc, test_uri.as_str());

            match (tree_sitter_result, original_result) {
                (Ok(tree_sitter_ast), Ok(original_ast)) => {
                    let converted_ast = to_original_ast(&tree_sitter_ast);
                    if !asts_equal(&converted_ast, &original_ast) {
                        panic!(
                            "Manual test case failed for: {}\nConverted: {:#?}\nOriginal: {:#?}",
                            yaml_doc, converted_ast, original_ast
                        );
                    }
                }
                (tree_sitter_result, original_result) => {
                    panic!(
                        "Parser inconsistency for manual test: {}\nTree-sitter: {:?}\nOriginal: {:?}",
                        yaml_doc,
                        tree_sitter_result.map(|_| "Success").map_err(|e| e.message),
                        original_result.map(|_| "Success")
                    );
                }
            }
        }
    }
}
