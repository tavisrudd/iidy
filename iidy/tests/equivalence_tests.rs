//! Tests to verify equivalence between handlebars {{var}} and include !$ var syntax
//! 
//! These tests ensure that `{{variable}}` and `!$ variable` produce identical results
//! for all scalar values. This is a critical invariant of the iidy preprocessing system.

use anyhow::Result;
use iidy::yaml::preprocess_yaml_v11;
use serde_yaml::Value;
use std::collections::HashMap;

fn format_yaml_value(var_value: &Value) -> String {
    match var_value {
        Value::String(s) if s.ends_with("\n") => format!("|\n    {}", s.replace("\n", "\n    ")),
        Value::String(s) if s.contains("\n") => format!("|-\n    {}", s.replace("\n", "\n    ")),
        Value::String(s) => format!("\"{}\"", s.replace("\"", "\\\"")),
        _ => serde_yaml::to_string(var_value).unwrap().trim().to_string(),
    }
}

/// Test handlebars interpolation 
async fn test_handlebars_interpolation(var_name: &str, var_value: &Value) -> Result<Value> {
    // For handlebars, we always use string interpolation
    let yaml_value_str = format_yaml_value(var_value);
    
    let yaml_content = format!(
        r#"
$defs:
  {}: {}
result: '{{{{{}}}}}'
"#, 
        var_name, 
        yaml_value_str,
        var_name
    );
    
    let processed = preprocess_yaml_v11(&yaml_content, "equivalence-test.yaml").await?;
    
    if let Value::Mapping(map) = processed {
        if let Some(result_value) = map.get(&Value::String("result".to_string())) {
            Ok(result_value.clone())
        } else {
            Err(anyhow::anyhow!("Expected 'result' key in processed YAML"))
        }
    } else {
        Err(anyhow::anyhow!("Expected mapping from processed YAML"))
    }
}

/// Test handlebars with parseYaml for non-string values
async fn test_handlebars_with_parse_yaml(var_name: &str, var_value: &Value) -> Result<Value> {
    let yaml_value_str = format_yaml_value(var_value);    
    let yaml_content = format!(
        r#"
$defs:
  {}: {}
result: !$parseYaml '{{{{{}}}}}'
"#, 
        var_name, 
        yaml_value_str,
        var_name
    );
    
    let processed = preprocess_yaml_v11(&yaml_content, "equivalence-test.yaml").await?;
    
    if let Value::Mapping(map) = processed {
        if let Some(result_value) = map.get(&Value::String("result".to_string())) {
            Ok(result_value.clone())
        } else {
            Err(anyhow::anyhow!("Expected 'result' key in processed YAML"))
        }
    } else {
        Err(anyhow::anyhow!("Expected mapping from processed YAML"))
    }
}

/// Test include tag syntax using process_yaml
async fn test_include_syntax(var_name: &str, var_value: &Value) -> Result<Value> {
    // Create YAML with embedded variable definition and include usage
    let yaml_value_str = format_yaml_value(var_value);    
    let yaml_content = format!(
        r#"
$defs:
  {}: {}
result: !$ {}
"#, 
        var_name, 
        yaml_value_str,
        var_name
    );
    eprint!("{}", yaml_content);
    let processed = preprocess_yaml_v11(&yaml_content, "equivalence-test.yaml").await?;
    
    // Extract the "result" field from the processed YAML
    if let Value::Mapping(map) = processed {
        if let Some(result_value) = map.get(&Value::String("result".to_string())) {
            //Ok(result_value.clone())
            Ok(result_value.clone())
        } else {
            Err(anyhow::anyhow!("Expected 'result' key in processed YAML"))
        }
    } else {
        Err(anyhow::anyhow!("Expected mapping from processed YAML"))
    }
}

async fn run_test_cases(cases: Vec<(&str, Value)>) -> Result<()> {
    for (var_name, var_value) in cases {
        let include_result = test_include_syntax(var_name, &var_value).await?;
        
        match &var_value {
            Value::String(_) => {
                let handlebars_result = test_handlebars_interpolation(var_name, &var_value).await?;
                assert_eq!(handlebars_result, include_result,
                           "String case '{}' handlebars_result != include_result", var_name);
            }
            _ => {
                let handlebars_parsed = test_handlebars_with_parse_yaml(var_name, &var_value).await?;
                assert_eq!(handlebars_parsed, include_result,
                           "Non-string case '{}' handlebars_result != include_result", var_name);
            }
        }
        assert_eq!(include_result, var_value, "'{}' include_result != original_value", var_name);
    }
    
    Ok(())
}

#[cfg(test)]
mod scalar_equivalence_tests {
    use super::*;
    #[tokio::test]
    async fn test_complex_scalar_cases() -> Result<()> {
        let cases = vec![
            ("simple_string", Value::String("hello".to_string())),
            ("empty_string", Value::String("".to_string())),
            ("string_with_spaces", Value::String("hello world".to_string())),
            ("string_with_special_chars", Value::String("hello-world_test.example".to_string())),
            ("integer_zero", Value::Number(0.into())),
            ("positive_integer", Value::Number(42.into())),
            ("negative_integer", Value::Number((-123).into())),
            ("float_value", Value::Number(serde_yaml::Number::from(3.14))),
            ("negative_float", Value::Number(serde_yaml::Number::from(-2.5))),
            ("boolean_true", Value::Bool(true)),
            ("boolean_false", Value::Bool(false)),
            ("null_value", Value::Null),            
            ("url", Value::String("https://api.example.com/v1/users".to_string())),
            ("version", Value::String("1.2.3-beta.4".to_string())),
            ("large_number", Value::Number(9223372036854775807i64.into())),
            ("small_float", Value::Number(serde_yaml::Number::from(0.000001))),
            ("scientific_notation", Value::Number(serde_yaml::Number::from(1.23e10))),
            ("unicode_string", Value::String("Hello 世界 🌍".to_string())),
            ("json_like_string", Value::String(r#"{"key": "value"}"#.to_string())),
        ];
        run_test_cases(cases).await
    }

    #[tokio::test]
    async fn test_edge_cases() -> Result<()> {
        let edge_cases = vec![
            // Strings that look like other types - these reveal bugs in string preservation
            ("string_that_looks_like_number", Value::String("123".to_string())),
            ("string_that_looks_like_null", Value::String("null".to_string())),
            ("string_that_looks_like_bool", Value::String("true".to_string())),
            
            // Values with special formatting that should remain strings
            ("zero_padded_number", Value::String("007".to_string())),
            ("hex_like_string", Value::String("0xFF".to_string())),
            
            // Empty/whitespace handling
            ("whitespace_only", Value::String("   ".to_string())),
            ("newline_string", Value::String("line1\nline2".to_string())),
            ("tab_string", Value::String("col1\tcol2".to_string())),
        ];
        run_test_cases(edge_cases).await
    }
}

/// Tests for advanced resolution scenarios
#[cfg(test)]
mod inside_string_join_tests {
    use super::*;

    #[tokio::test]
    async fn test_single_value() -> Result<()> {
        let test_variables = HashMap::from([
            ("environment".to_string(), Value::String("production".to_string())),
            ("app_name".to_string(), Value::String("my-app".to_string())),
            ("port".to_string(), Value::Number(3000.into())),
            ("enabled".to_string(), Value::Bool(true)),
        ]);
        
        for (var_name, var_value) in &test_variables {
            let handlebars_yaml = format!(
                r#"
$defs:
  {}: {}
result: 'Value is: {{{{{}}}}}'
"#, 
                var_name, 
                serde_yaml::to_string(var_value)?,
                var_name
            );
            let handlebars_processed = preprocess_yaml_v11(&handlebars_yaml, "test.yaml").await?;
            
            let include_yaml = format!(
                r#"
$defs:
  {}: {}
result: !$join ['', ['Value is: ', !$ {}]]
"#, 
                var_name, 
                serde_yaml::to_string(var_value)?,
                var_name
            );
            let include_processed = preprocess_yaml_v11(&include_yaml, "test.yaml").await?;
            
                assert_eq!(
                handlebars_processed, include_processed,
                "Full workflow equivalence failed for variable '{}'", var_name
            );
        }
        
        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_values() -> Result<()> {
        
        let yaml_handlebars = r#"
$defs:
  environment: "staging"
  app_name: "web-service"
stack_name: "{{app_name}}-{{environment}}"
database:
  host: "{{app_name}}-db.{{environment}}.local"
  name: "{{app_name}}_{{environment}}"
tags:
  Environment: "{{environment}}"
  Application: "{{app_name}}"
"#;
        
        let yaml_includes = r#"
$defs:
  environment: "staging"
  app_name: "web-service"
stack_name: !$join ["-", [!$ app_name, !$ environment]]
database:
  host: !$join ["", [!$ app_name, "-db.", !$ environment, ".local"]]
  name: !$join ["_", [!$ app_name, !$ environment]]
tags:
  Environment: !$ environment
  Application: !$ app_name
"#;
        
        let handlebars_result = preprocess_yaml_v11(yaml_handlebars, "handlebars.yaml").await?;
        let includes_result = preprocess_yaml_v11(yaml_includes, "includes.yaml").await?;
        
        assert_eq!(
            handlebars_result, includes_result,
            "Complex YAML structure equivalence test failed"
        );
        
        if let Value::Mapping(map) = &handlebars_result {
            assert_eq!(
                map.get(&Value::String("stack_name".to_string())),
                Some(&Value::String("web-service-staging".to_string()))
            );
        }
        
        Ok(())
    }
}
