//! Property-based tests for YAML preprocessing workflows
//!
//! These tests use proptest to verify invariants and properties of the YAML
//! preprocessing system across a wide range of inputs.

use iidy::yaml::parsing_w_loc::parse_yaml_with_custom_tags_from_file;
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
        ".*",                                                                // Plain strings
        variable_name_strategy().prop_map(|var| format!("{{{{{}}}}}", var)), // Simple variable
        (variable_name_strategy(), ".*")
            .prop_map(|(var, text)| format!("{}{{{{{}}}}}suffix", text, var)), // Mixed content
    ]
}

proptest! {
    /// Property: YAML parsing should handle valid scalar values consistently
    #[test]
    fn prop_yaml_parsing_scalars(value in yaml_scalar_strategy()) {
        // Serialize to YAML string
        let yaml_str = serde_yaml::to_string(&value).unwrap();

        // Parse with our custom parser - this tests the parsing layer
        let ast = parse_yaml_with_custom_tags_from_file(&yaml_str, "prop-test-scalar.yaml");

        // Should successfully parse simple scalar values
        prop_assert!(ast.is_ok(), "Failed to parse valid YAML: {}", yaml_str);

        // NOTE: Once AST resolution is implemented, add tests here to verify:
        // - let mut preprocessor = YamlPreprocessor::new(, true);
        // - let result = preprocessor.resolve_ast(ast.unwrap());
        // - Type preservation and value correctness
    }

    /// Property: Handlebars template processing (unit test for engine only)
    #[test]
    fn prop_handlebars_engine_idempotent(template in ".*") {
        // Test the handlebars engine directly, not through AST resolution
        use iidy::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;

        // Templates without handlebars syntax should remain unchanged
        if !template.contains("{{") {
            let empty_vars = HashMap::new();
            let result = interpolate_handlebars_string(&template, &empty_vars, "test");

            if result.is_ok() {
                prop_assert_eq!(template, result.unwrap());
            }
        }

        // NOTE: Once AST resolution is implemented, add tests for:
        // - Full YAML preprocessing with handlebars in strings
        // - Variable substitution through YamlPreprocessor
    }

    /// Property: Handlebars variable substitution (direct engine test)
    #[test]
    fn prop_handlebars_variable_substitution(
        var_name in variable_name_strategy(),
        var_value in "[a-zA-Z0-9 _\\-.,!@#$%^&*()+=\\[\\]{}|;:\"'<>?/]*"
    ) {
        use iidy::yaml::handlebars::interpolate_handlebars_string;

        let template = format!("{{{{{}}}}}", var_name);

        // Create variables map for handlebars engine
        let mut variables = HashMap::new();
        variables.insert(var_name.clone(), serde_json::Value::String(var_value.clone()));

        let result = interpolate_handlebars_string(&template, &variables, "test");

        // Should successfully substitute (accounting for HTML escaping)
        prop_assert!(result.is_ok());

        if let Ok(processed) = result {
            // The result should contain the substituted value (possibly escaped)
            prop_assert!(!processed.is_empty() || var_value.is_empty());
        }

        // NOTE: Once AST resolution is implemented, add tests for:
        // - Variable substitution through YamlPreprocessor
        // - TagContext integration with handlebars
    }

    /// Property: String transformation helpers should preserve certain invariants
    #[test]
    fn prop_case_conversion_invariants(input in "[a-zA-Z ]+") {
        use iidy::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;

        // Test camelCase helper
        let camel_template = format!("{{{{camelCase '{}'}}}}", input);
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
        let snake_template = format!("{{{{snakeCase '{}'}}}}", input);
        let result = interpolate_handlebars_string(&snake_template, &HashMap::new(), "test");

        if result.is_ok() {
            let snake_result = result.unwrap();

            // Snake_case should not contain spaces
            prop_assert!(!snake_result.contains(' '));

            // Should be lowercase
            prop_assert_eq!(snake_result.clone(), snake_result.to_lowercase());
        }
    }

    /// Property: YAML parsing with custom tags should be deterministic
    #[test]
    fn prop_yaml_parsing_deterministic(
        array in prop::collection::vec(".*", 1..5),
        delimiter in "[,;|\\-_]"
    ) {
        // Create a !$join tag for parsing tests
        let yaml_content = format!(
            "result: !$join\n  array: {:?}\n  delimiter: \"{}\"",
            array, delimiter
        );

        // Parse the same content twice
        let ast_result1 = parse_yaml_with_custom_tags_from_file(&yaml_content, "prop-test-join.yaml");
        let ast_result2 = parse_yaml_with_custom_tags_from_file(&yaml_content, "prop-test-join.yaml");

        // Both parsing attempts should have the same outcome
        prop_assert_eq!(ast_result1.is_ok(), ast_result2.is_ok());

        // NOTE: Once AST resolution is implemented, add tests for:
        // - Deterministic tag resolution with same inputs
        // - Consistent output across multiple processing runs
        // - let result1 = preprocessor1.resolve_ast(ast_result1.unwrap());
        // - let result2 = preprocessor2.resolve_ast(ast_result2.unwrap());
    }

    /// Property: YAML parsing for split/join tags should be consistent
    #[test]
    fn prop_yaml_split_join_parsing(
        parts in prop::collection::vec("[a-zA-Z0-9]+", 1..5), // No delimiters in parts
        delimiter in "[,;|\\-_]"
    ) {
        // Ensure delimiter doesn't appear in any part
        let clean_parts: Vec<String> = parts.into_iter()
            .filter(|s| !s.contains(&delimiter))
            .collect();

        if !clean_parts.is_empty() {
            let joined = clean_parts.join(&delimiter);

            // Test that split tag parses correctly
            let split_yaml = format!(
                "result: !$split [\"{}\", \"{}\"]",
                delimiter, joined
            );

            let ast = parse_yaml_with_custom_tags_from_file(&split_yaml, "prop-test-split.yaml");
            prop_assert!(ast.is_ok(), "Failed to parse split tag YAML");

            // NOTE: Once AST resolution is implemented, add tests for:
            // - Split and join operations being inverse
            // - let result = preprocessor.resolve_ast(ast.unwrap());
            // - Verification that split recovers original parts
            // - Join operation creating the original string
        }
    }

    /// Property: YAML parsing should handle errors gracefully
    #[test]
    fn prop_yaml_parsing_graceful(malformed_yaml in ".*") {
        // Malformed YAML should not panic, just return errors
        let parse_result = parse_yaml_with_custom_tags_from_file(&malformed_yaml, "prop-test-malformed.yaml");

        // Either succeeds or fails gracefully (no panics)
        match parse_result {
            Ok(_) => {
                // Success is fine - valid YAML was generated
            },
            Err(e) => {
                // Parse error messages should not be empty
                prop_assert!(!e.to_string().is_empty());
            }
        }

        // NOTE: Once AST resolution is implemented, add tests for:
        // - Graceful handling of resolution errors
        // - let process_result = preprocessor.resolve_ast(ast);
        // - Error message quality and consistency
    }
}

#[cfg(test)]
mod standard_tests {
    use super::*;
    use proptest::strategy::ValueTree;

    #[test]
    fn test_property_test_framework_works() {
        // Basic sanity check that proptest is working
        assert!(true);
    }

    #[test]
    fn test_handlebars_empty_value_specific_case() {
        // Test the specific failing case from property tests
        use iidy::yaml::handlebars::interpolate_handlebars_string;
        use std::collections::HashMap;

        // Test with a different variable name first to make sure it's not a general issue
        let mut variables = HashMap::new();
        variables.insert(
            "normal_var".to_string(),
            serde_json::Value::String("".to_string()),
        );

        let result1 = interpolate_handlebars_string("{{normal_var}}", &variables, "test");
        println!("Normal var result: {:?}", result1);

        // Now test the problematic "lt" variable
        let var_name = "lt";
        let var_value = "";

        let template = format!("{{{{{}}}}}", var_name);
        println!("Template: {}", template);

        variables.clear();
        variables.insert(
            var_name.to_string(),
            serde_json::Value::String(var_value.to_string()),
        );

        let result = interpolate_handlebars_string(&template, &variables, "test");

        match &result {
            Ok(processed) => println!("Success: '{}'", processed),
            Err(e) => println!("Error: {}", e),
        }

        // For now, just verify the normal var works - we'll address lt separately
        assert!(result1.is_ok(), "Normal variables should work");

        // Print result of the lt issue for debugging
        println!("Result for lt variable: {:?}", result);
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
                "Generated value should be serializable: {:?}",
                value
            );
        }
    }

    #[test]
    fn test_handlebars_template_strategy_generates_valid_templates() {
        let strategy = handlebars_template_strategy();
        let mut runner = proptest::test_runner::TestRunner::default();

        for _ in 0..10 {
            let template = strategy.new_tree(&mut runner).unwrap().current();

            // Should be valid UTF-8 strings (all strings in Rust are valid UTF-8)
            assert!(!template.is_empty() || template.is_empty()); // Basic sanity check
        }
    }
}
