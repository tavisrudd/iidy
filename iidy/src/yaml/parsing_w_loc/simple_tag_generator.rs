//! Simplified configurable YAML tag generator for property testing
//!
//! This module provides flexible generators for creating YAML documents
//! with specific custom tags for comprehensive testing.

use proptest::prelude::*;
use proptest::strategy::BoxedStrategy;
use std::collections::HashSet;

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
    /// Create a config with common CloudFormation tags
    pub fn with_cloudformation_tags() -> Self {
        let mut cf_tags = HashSet::new();
        cf_tags.insert("Ref".to_string());
        cf_tags.insert("Sub".to_string());
        cf_tags.insert("GetAtt".to_string());
        
        Self {
            cloudformation_tags: cf_tags,
            ..Default::default()
        }
    }

    /// Create a config with common preprocessing tags
    pub fn with_preprocessing_tags() -> Self {
        let mut prep_tags = HashSet::new();
        prep_tags.insert("$".to_string());
        prep_tags.insert("$include".to_string());
        prep_tags.insert("$not".to_string());
        prep_tags.insert("$parseYaml".to_string());
        prep_tags.insert("$parseJson".to_string());
        
        Self {
            preprocessing_tags: prep_tags,
            ..Default::default()
        }
    }

    /// Create a config with both CloudFormation and preprocessing tags
    pub fn with_all_tags() -> Self {
        let mut config = Self::with_cloudformation_tags();
        config.preprocessing_tags = Self::with_preprocessing_tags().preprocessing_tags;
        config
    }

    /// Add a specific CloudFormation tag
    pub fn add_cloudformation_tag(mut self, tag: &str) -> Self {
        self.cloudformation_tags.insert(tag.to_string());
        self
    }

    /// Add a specific preprocessing tag
    pub fn add_preprocessing_tag(mut self, tag: &str) -> Self {
        self.preprocessing_tags.insert(tag.to_string());
        self
    }

    /// Set the tag probability
    pub fn with_tag_probability(mut self, prob: f64) -> Self {
        self.tag_probability = prob.clamp(0.0, 1.0);
        self
    }

    /// Set maximum items
    pub fn with_limits(mut self, max_items: usize) -> Self {
        self.max_items = max_items;
        self
    }
}

/// Generate simple scalar values
pub fn simple_scalar_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z][a-zA-Z0-9_-]{0,8}".prop_map(|s| s),
        "\"[a-zA-Z0-9 _-]{0,15}\"".prop_map(|s| s),
        prop::num::i32::ANY.prop_filter("small", |n| n.abs() < 1000).prop_map(|n| n.to_string()),
        prop::bool::ANY.prop_map(|b| b.to_string()),
        Just("null".to_string()),
    ]
}

/// Generate CloudFormation tag values
pub fn cloudformation_tag_value_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        simple_scalar_strategy().prop_map(|s| format!("!Ref {}", s)),
        simple_scalar_strategy().prop_map(|param| format!("!Sub \"Value: ${{{}}}\"", param)),
        ("[a-zA-Z][a-zA-Z0-9]*", "[a-zA-Z][a-zA-Z0-9]*")
            .prop_map(|(res, attr)| format!("!GetAtt {}.{}", res, attr)),
    ]
}

/// Generate preprocessing tag values
pub fn preprocessing_tag_value_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z][a-zA-Z0-9_.]*".prop_map(|path| format!("!$ {}", path)),
        "[a-zA-Z][a-zA-Z0-9_.]*".prop_map(|path| format!("!$include {}", path)),
        prop::bool::ANY.prop_map(|b| format!("!$not {}", b)),
        Just("!$parseYaml [\"key: value\"]".to_string()),
        Just("!$parseJson [\"{\\\"key\\\": \\\"value\\\"}\"]".to_string()),
    ]
}

/// Generate a value that could be a scalar or a tag
pub fn yaml_value_strategy(config: TagConfig) -> BoxedStrategy<String> {
    let scalar_weight = ((1.0 - config.tag_probability) * 100.0) as u32;
    let cf_weight = if config.cloudformation_tags.is_empty() { 0 } else { (config.tag_probability * 50.0) as u32 };
    let prep_weight = if config.preprocessing_tags.is_empty() { 0 } else { (config.tag_probability * 50.0) as u32 };
    
    // Ensure we always have at least scalars available
    let scalar_weight = scalar_weight.max(10);
    
    if cf_weight > 0 && prep_weight > 0 {
        prop_oneof![
            scalar_weight => simple_scalar_strategy(),
            cf_weight => cloudformation_tag_value_strategy(),
            prep_weight => preprocessing_tag_value_strategy(),
        ].boxed()
    } else if cf_weight > 0 {
        prop_oneof![
            scalar_weight => simple_scalar_strategy(),
            cf_weight => cloudformation_tag_value_strategy(),
        ].boxed()
    } else if prep_weight > 0 {
        prop_oneof![
            scalar_weight => simple_scalar_strategy(),
            prep_weight => preprocessing_tag_value_strategy(),
        ].boxed()
    } else {
        simple_scalar_strategy().boxed()
    }
}

/// Generate a complete YAML document
pub fn yaml_document_strategy(config: TagConfig) -> impl Strategy<Value = String> {
    let max_items = config.max_items;
    prop::collection::vec(
        (
            "[a-zA-Z][a-zA-Z0-9_]*", // key
            yaml_value_strategy(config.clone())
        ),
        1..max_items
    )
    .prop_map(|entries| {
        entries.into_iter()
            .map(|(key, value)| format!("{}: {}", key, value))
            .collect::<Vec<_>>()
            .join("\n")
    })
}

/// Generate block-style documents with complex tags
pub fn block_style_document_strategy(config: TagConfig) -> impl Strategy<Value = String> {
    let max_items = config.max_items;
    prop::collection::vec(
        "[a-zA-Z][a-zA-Z0-9_]*".prop_flat_map(move |key| {
            let key1 = key.clone();
            let key2 = key.clone();
            let key3 = key.clone();
            let key4 = key.clone();
            prop_oneof![
                // Regular key-value
                yaml_value_strategy(config.clone()).prop_map(move |v| format!("{}: {}", key1, v)),
                
                // Block-style preprocessing tags  
                Just(format!("{}: !$if\n  test: true\n  then: \"yes\"\n  else: \"no\"", key2)),
                Just(format!("{}: !$let\n  x: 42\n  in: \"Value is {{{{x}}}}\"", key3)),
                Just(format!("{}: !$map\n  items: [1, 2, 3]\n  template: \"item-{{{{item}}}}\"", key4)),
            ]
        }),
        1..max_items
    )
    .prop_map(|lines| lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::strategy::ValueTree;

    #[test]
    fn test_tag_config_builder() {
        let config = TagConfig::with_cloudformation_tags()
            .add_preprocessing_tag("$custom")
            .with_tag_probability(0.5)
            .with_limits(3);
            
        assert!(config.cloudformation_tags.contains("Ref"));
        assert!(config.preprocessing_tags.contains("$custom"));
        assert_eq!(config.tag_probability, 0.5);
        assert_eq!(config.max_items, 3);
    }

    #[test]
    fn test_simple_scalar_generation() {
        let strategy = simple_scalar_strategy();
        let mut runner = proptest::test_runner::TestRunner::default();
        
        for _ in 0..3 {
            let value = strategy.new_tree(&mut runner).unwrap();
            let scalar = value.current();
            println!("Generated scalar: {}", scalar);
            assert!(!scalar.is_empty());
        }
    }

    #[test]
    fn test_cloudformation_tag_generation() {
        let strategy = cloudformation_tag_value_strategy();
        let mut runner = proptest::test_runner::TestRunner::default();
        
        for _ in 0..3 {
            let value = strategy.new_tree(&mut runner).unwrap();
            let tag = value.current();
            println!("Generated CF tag: {}", tag);
            assert!(tag.starts_with("!"));
        }
    }

    #[test]
    fn test_document_generation() {
        let config = TagConfig::with_all_tags()
            .with_tag_probability(0.7)
            .with_limits(3);
            
        let strategy = yaml_document_strategy(config);
        let mut runner = proptest::test_runner::TestRunner::default();
        
        for _ in 0..2 {
            let value = strategy.new_tree(&mut runner).unwrap();
            let doc = value.current();
            println!("Generated document:\n{}\n---", doc);
            assert!(doc.contains(":"));
        }
    }
}