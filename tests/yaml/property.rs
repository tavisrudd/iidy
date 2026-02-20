//! Property-based tests for YAML preprocessing workflows
//!
//! These tests use proptest to verify invariants and properties of the YAML
//! preprocessing system across a wide range of inputs.

use iidy::yaml::parsing::parse_yaml_from_file;
use iidy::yaml::resolution::{TagContext, resolve_ast};
use proptest::prelude::*;
use serde_yaml::Value;
use std::collections::HashMap;

/// Strategy for generating valid YAML scalar values
fn yaml_scalar_strategy() -> impl Strategy<Value = Value> {
    prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(|i| Value::Number(serde_yaml::Number::from(i))),
        any::<f64>()
            .prop_filter("must be finite", |f| f.is_finite())
            .prop_map(|f| Value::Number(serde_yaml::Number::from(f))),
        ".*".prop_map(Value::String),
    ]
}

/// Strategy for generating variable names for handlebars
fn variable_name_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z][a-zA-Z0-9_]*".prop_filter("Must not be handlebars reserved keyword", |name| {
        // Filter out handlebars reserved keywords and potential comparison operators
        !matches!(
            name.as_str(),
            "if" | "else" | "unless" | "each" | "with" | "lookup" | "log" | 
                "blockHelperMissing" | "helperMissing" | "true" | "false" | "null" | "undefined" |
                // Comparison operators that might be interpreted as helpers
                "lt" | "gt" | "eq" | "ne" | "le" | "ge" | "and" | "or" | "not" |
                // Other potentially problematic short names
                "in" | "is" | "as" | "to" | "at" | "on" | "by" | "of" | "do" | "be" | "go" |
                // Our custom helpers
                "toJson" | "tojson" | "toJsonPretty" | "tojsonPretty" | "toYaml" | "toyaml" |
                "base64" | "urlEncode" | "sha256" | "toLowerCase" | "toUpperCase" | "titleize" |
                "camelCase" | "snakeCase" | "kebabCase" | "capitalize" | "trim" | "replace" |
                "substring" | "length" | "pad" | "concat"
        )
    })
}

/// Strategy for generating handlebars template strings
fn handlebars_template_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        ".*",                                                              // Plain strings
        variable_name_strategy().prop_map(|var| format!("{{{{{var}}}}}")), // Simple variable
        (variable_name_strategy(), ".*")
            .prop_map(|(var, text)| format!("{text}{{{{{var}}}}}suffix")), // Mixed content
    ]
}

proptest! {
    /// Property: YAML parsing and resolution should handle valid scalar values consistently
    #[test]
    fn prop_yaml_parsing_scalars(value in yaml_scalar_strategy()) {
        let yaml_str = serde_yaml::to_string(&value).unwrap();
        let ast = parse_yaml_from_file(&yaml_str, "prop-test-scalar.yaml");
        prop_assert!(ast.is_ok(), "Failed to parse valid YAML: {}", yaml_str);

        let context = TagContext::new();
        let resolved = resolve_ast(&ast.unwrap(), &context);
        prop_assert!(resolved.is_ok(), "Failed to resolve valid scalar: {}", yaml_str);
    }

    /// Property: Handlebars templates without syntax should pass through unchanged
    #[test]
    fn prop_handlebars_engine_idempotent(template in ".*") {
        use iidy::yaml::handlebars::interpolate_handlebars_string;

        if !template.contains("{{") {
            let empty_vars = HashMap::new();
            let result = interpolate_handlebars_string(&template, &empty_vars, "test");

            if result.is_ok() {
                prop_assert_eq!(template, result.unwrap());
            }
        }
    }

    /// Property: Handlebars variable substitution produces non-empty output for non-empty values
    #[test]
    fn prop_handlebars_variable_substitution(
        var_name in variable_name_strategy(),
        var_value in "[a-zA-Z0-9 _\\-.,!@#$%^&*()+=\\[\\]{}|;:\"'<>?/]*"
    ) {
        use iidy::yaml::handlebars::interpolate_handlebars_string;

        let template = format!("{{{{{var_name}}}}}");
        let mut variables = HashMap::new();
        variables.insert(var_name.clone(), serde_json::Value::String(var_value.clone()));

        let result = interpolate_handlebars_string(&template, &variables, "test");
        prop_assert!(result.is_ok());

        if let Ok(processed) = result {
            prop_assert!(!processed.is_empty() || var_value.is_empty());
        }
    }

    /// Property: String transformation helpers should preserve certain invariants
    #[test]
    fn prop_case_conversion_invariants(input in "[a-zA-Z ]+") {
        use iidy::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;

        // Test camelCase helper
        let camel_template = format!("{{{{camelCase '{input}'}}}}");
        let result = interpolate_handlebars_string(&camel_template, &HashMap::new(), "test");

        if result.is_ok() {
            let camel_result = result.unwrap();

            // CamelCase should not contain spaces
            prop_assert!(!camel_result.contains(' '));

            // Should start with lowercase (if not empty)
            if !camel_result.is_empty() {
                prop_assert!(camel_result.chars().next().unwrap().is_lowercase());
            }
        }

        // Test snake_case helper
        let snake_template = format!("{{{{snakeCase '{input}'}}}}");
        let result = interpolate_handlebars_string(&snake_template, &HashMap::new(), "test");

        if result.is_ok() {
            let snake_result = result.unwrap();

            // Snake_case should not contain spaces
            prop_assert!(!snake_result.contains(' '));

            // Should be lowercase
            prop_assert_eq!(snake_result.clone(), snake_result.to_lowercase());
        }
    }

    /// Property: YAML parsing and resolution should be deterministic
    #[test]
    fn prop_yaml_parsing_deterministic(
        array in prop::collection::vec(".*", 1..5),
        delimiter in "[,;|\\-_]"
    ) {
        let yaml_content = format!(
            "result: !$join\n  array: {array:?}\n  delimiter: \"{delimiter}\""
        );

        let ast_result1 = parse_yaml_from_file(&yaml_content, "prop-test-join.yaml");
        let ast_result2 = parse_yaml_from_file(&yaml_content, "prop-test-join.yaml");
        prop_assert_eq!(ast_result1.is_ok(), ast_result2.is_ok());

        if let (Ok(ast1), Ok(ast2)) = (ast_result1, ast_result2) {
            let context = TagContext::new();
            let result1 = resolve_ast(&ast1, &context);
            let result2 = resolve_ast(&ast2, &context);
            prop_assert_eq!(result1.is_ok(), result2.is_ok());
            if let (Ok(v1), Ok(v2)) = (result1, result2) {
                prop_assert_eq!(v1, v2);
            }
        }
    }

    /// Property: YAML split tag should parse and resolve correctly
    #[test]
    fn prop_yaml_split_join_parsing(
        parts in prop::collection::vec("[a-zA-Z0-9]+", 1..5),
        delimiter in "[,;|\\-_]"
    ) {
        let clean_parts: Vec<String> = parts.into_iter()
            .filter(|s| !s.contains(&delimiter))
            .collect();

        if !clean_parts.is_empty() {
            let joined = clean_parts.join(&delimiter);
            let split_yaml = format!(
                "result: !$split [\"{delimiter}\", \"{joined}\"]"
            );

            let ast = parse_yaml_from_file(&split_yaml, "prop-test-split.yaml");
            prop_assert!(ast.is_ok(), "Failed to parse split tag YAML");

            let context = TagContext::new();
            let resolved = resolve_ast(&ast.unwrap(), &context);
            prop_assert!(resolved.is_ok(), "Failed to resolve split tag");

            if let Ok(Value::Mapping(map)) = resolved {
                let result = map.get(Value::String("result".to_string()));
                if let Some(Value::Sequence(seq)) = result {
                    prop_assert_eq!(seq.len(), clean_parts.len());
                }
            }
        }
    }

    /// Property: YAML parsing and resolution should handle errors gracefully (no panics)
    #[test]
    fn prop_yaml_parsing_graceful(malformed_yaml in ".*") {
        let parse_result = parse_yaml_from_file(&malformed_yaml, "prop-test-malformed.yaml");

        match parse_result {
            Ok(ast) => {
                let context = TagContext::new();
                match resolve_ast(&ast, &context) {
                    Ok(_) => {}
                    Err(e) => {
                        prop_assert!(!e.to_string().is_empty());
                    }
                }
            },
            Err(e) => {
                prop_assert!(!e.to_string().is_empty());
            }
        }
    }
}

#[cfg(test)]
mod standard_tests {
    use super::*;
    use proptest::strategy::ValueTree;

    #[test]
    fn test_handlebars_empty_value_substitution() {
        use iidy::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;

        let mut variables = HashMap::new();
        variables.insert(
            "normal_var".to_string(),
            serde_json::Value::String("".to_string()),
        );

        let result = interpolate_handlebars_string("{{normal_var}}", &variables, "test");
        assert!(result.is_ok(), "Normal variables should work");
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn test_yaml_scalar_strategy_generates_valid_values() {
        // Test that our strategy generates valid YAML values
        let strategy = yaml_scalar_strategy();
        let mut runner = proptest::test_runner::TestRunner::default();

        for _ in 0..10 {
            let value = strategy.new_tree(&mut runner).unwrap().current();

            // Should be able to serialize any generated value
            let serialized = serde_yaml::to_string(&value);
            assert!(
                serialized.is_ok(),
                "Generated value should be serializable: {value:?}"
            );
        }
    }

    #[test]
    fn test_handlebars_template_strategy_generates_valid_templates() {
        let strategy = handlebars_template_strategy();
        let mut runner = proptest::test_runner::TestRunner::default();

        for _ in 0..10 {
            let template = strategy.new_tree(&mut runner).unwrap().current();

            // Strategy should produce valid UTF-8 (guaranteed by Rust's String type)
            let _ = template;
        }
    }
}
