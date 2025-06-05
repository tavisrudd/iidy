//! Integration tests for YAML preprocessing

use anyhow::Result;
use iidy::yaml::{parse_yaml_with_custom_tags, preprocess_yaml, YamlPreprocessor, YamlAst, PreprocessingTag, TagContext};
use serde_yaml::Value;

#[test]
fn test_parse_simple_scalar() -> Result<()> {
    let yaml = "hello world";
    let ast = parse_yaml_with_custom_tags(yaml)?;
    
    match ast {
        YamlAst::String(s) => assert_eq!(s, "hello world"),
        _ => panic!("Expected string scalar"),
    }
    Ok(())
}

#[test]
fn test_parse_different_scalar_types() -> Result<()> {
    // Test boolean
    let yaml = "true";
    let ast = parse_yaml_with_custom_tags(yaml)?;
    match ast {
        YamlAst::Bool(b) => assert_eq!(b, true),
        _ => panic!("Expected boolean"),
    }
    
    // Test number
    let yaml = "42.5";
    let ast = parse_yaml_with_custom_tags(yaml)?;
    match ast {
        YamlAst::Number(n) => assert_eq!(n, 42.5),
        _ => panic!("Expected number"),
    }
    
    // Test null
    let yaml = "null";
    let ast = parse_yaml_with_custom_tags(yaml)?;
    match ast {
        YamlAst::Null => {},
        _ => panic!("Expected null"),
    }
    
    Ok(())
}

#[test]
fn test_parse_include_tag_simple() -> Result<()> {
    let yaml = "!$ ./config.yaml";
    let ast = parse_yaml_with_custom_tags(yaml)?;
    
    match ast {
        YamlAst::PreprocessingTag(PreprocessingTag::Include(include_tag)) => {
            assert_eq!(include_tag.path, "./config.yaml");
            assert!(include_tag.query.is_none());
        }
        _ => panic!("Expected include tag"),
    }
    Ok(())
}

#[test]
fn test_preprocess_simple_yaml() -> Result<()> {
    let yaml = r#"
name: "test-app"
version: "1.0.0"
enabled: true
count: 42
"#;
    let result = preprocess_yaml(yaml)?;
    
    // Should parse as a mapping with proper types
    if let Value::Mapping(map) = result {
        assert_eq!(map.len(), 4);
        assert_eq!(map.get(&Value::String("name".to_string())), Some(&Value::String("test-app".to_string())));
        assert_eq!(map.get(&Value::String("version".to_string())), Some(&Value::String("1.0.0".to_string())));
        assert_eq!(map.get(&Value::String("enabled".to_string())), Some(&Value::Bool(true)));
        assert_eq!(map.get(&Value::String("count".to_string())), Some(&Value::Number(serde_yaml::Number::from(42.0))));
    } else {
        panic!("Expected mapping result");
    }
    
    Ok(())
}

#[test]
fn test_preprocess_with_join_tag() -> Result<()> {
    let yaml = r#"
stack_name: !$join
  array: ["my-app", "production", "v1"]
  delimiter: "-"
"#;
    let mut preprocessor = YamlPreprocessor::new();
    let ast = parse_yaml_with_custom_tags(yaml)?;
    let result = preprocessor.resolve_ast(ast)?;
    
    if let Value::Mapping(map) = result {
        let stack_name = map.get(&Value::String("stack_name".to_string()));
        if let Some(Value::String(s)) = stack_name {
            assert_eq!(s, "my-app-production-v1");
        }
    } else {
        panic!("Expected mapping result");
    }
    
    Ok(())
}

#[test]
fn test_preprocess_with_split_tag() -> Result<()> {
    let yaml = r#"
emails: !$split
  string: "user1@example.com,user2@example.com,user3@example.com"
  delimiter: ","
"#;
    let mut preprocessor = YamlPreprocessor::new();
    let ast = parse_yaml_with_custom_tags(yaml)?;
    let result = preprocessor.resolve_ast(ast)?;
    
    if let Value::Mapping(map) = result {
        let emails = map.get(&Value::String("emails".to_string()));
        if let Some(Value::Sequence(seq)) = emails {
            assert_eq!(seq.len(), 3);
            assert_eq!(seq[0], Value::String("user1@example.com".to_string()));
            assert_eq!(seq[1], Value::String("user2@example.com".to_string()));
            assert_eq!(seq[2], Value::String("user3@example.com".to_string()));
        }
    } else {
        panic!("Expected mapping result");
    }
    
    Ok(())
}

#[test]
fn test_preprocess_with_eq_tag() -> Result<()> {
    let yaml = r#"
is_production: !$eq
  - "production"
  - "production"
is_dev: !$eq
  - "production"
  - "development"
"#;
    let mut preprocessor = YamlPreprocessor::new();
    let ast = parse_yaml_with_custom_tags(yaml)?;
    let result = preprocessor.resolve_ast(ast)?;
    
    if let Value::Mapping(map) = result {
        let is_production = map.get(&Value::String("is_production".to_string()));
        if let Some(Value::Bool(b)) = is_production {
            assert_eq!(*b, true);
        }
        
        let is_dev = map.get(&Value::String("is_dev".to_string()));
        if let Some(Value::Bool(b)) = is_dev {
            assert_eq!(*b, false);
        }
    } else {
        panic!("Expected mapping result");
    }
    
    Ok(())
}

#[test]
fn test_tag_context_variables() -> Result<()> {
    use std::collections::HashMap;
    
    let context = TagContext::new();
    let mut vars = HashMap::new();
    vars.insert("environment".to_string(), Value::String("production".to_string()));
    vars.insert("app_name".to_string(), Value::String("my-app".to_string()));
    
    let context_with_vars = context.with_bindings(vars);
    
    // Test variable retrieval
    assert_eq!(
        context_with_vars.get_variable("environment"), 
        Some(&Value::String("production".to_string()))
    );
    assert_eq!(
        context_with_vars.get_variable("app_name"), 
        Some(&Value::String("my-app".to_string()))
    );
    assert_eq!(context_with_vars.get_variable("nonexistent"), None);
    
    Ok(())
}

#[test]
fn test_handlebars_in_yaml_values() -> Result<()> {
    use std::collections::HashMap;
    
    let yaml = r#"
app_name: "{{service}}-{{environment}}"
version: "{{version}}"
port: 8080
"#;
    
    let mut preprocessor = YamlPreprocessor::new();
    let ast = parse_yaml_with_custom_tags(yaml)?;
    
    // Create context with variables
    let mut variables = HashMap::new();
    variables.insert("service".to_string(), Value::String("api".to_string()));
    variables.insert("environment".to_string(), Value::String("production".to_string()));
    variables.insert("version".to_string(), Value::String("v1.2.3".to_string()));
    
    let context = TagContext::new().with_bindings(variables);
    let result = preprocessor.resolve_ast_with_context(ast, &context)?;
    
    if let Value::Mapping(map) = result {
        assert_eq!(
            map.get(&Value::String("app_name".to_string())),
            Some(&Value::String("api-production".to_string()))
        );
        assert_eq!(
            map.get(&Value::String("version".to_string())),
            Some(&Value::String("v1.2.3".to_string()))
        );
        assert_eq!(
            map.get(&Value::String("port".to_string())),
            Some(&Value::Number(serde_yaml::Number::from(8080.0)))
        );
    } else {
        panic!("Expected mapping result");
    }
    
    Ok(())
}

#[test]
fn test_handlebars_in_yaml_keys() -> Result<()> {
    use std::collections::HashMap;
    
    let yaml = r#"
"{{service}}_name": "my-api"
"{{environment}}_config":
  port: 8080
  debug: true
"#;
    
    let mut preprocessor = YamlPreprocessor::new();
    let ast = parse_yaml_with_custom_tags(yaml)?;
    
    // Create context with variables
    let mut variables = HashMap::new();
    variables.insert("service".to_string(), Value::String("api".to_string()));
    variables.insert("environment".to_string(), Value::String("production".to_string()));
    
    let context = TagContext::new().with_bindings(variables);
    let result = preprocessor.resolve_ast_with_context(ast, &context)?;
    
    if let Value::Mapping(map) = result {
        // Check that keys were processed with handlebars
        assert!(map.contains_key(&Value::String("api_name".to_string())));
        assert!(map.contains_key(&Value::String("production_config".to_string())));
        
        // Verify values
        assert_eq!(
            map.get(&Value::String("api_name".to_string())),
            Some(&Value::String("my-api".to_string()))
        );
        
        if let Some(Value::Mapping(config)) = map.get(&Value::String("production_config".to_string())) {
            assert_eq!(
                config.get(&Value::String("port".to_string())),
                Some(&Value::Number(serde_yaml::Number::from(8080.0)))
            );
            assert_eq!(
                config.get(&Value::String("debug".to_string())),
                Some(&Value::Bool(true))
            );
        } else {
            panic!("Expected production_config to be a mapping");
        }
    } else {
        panic!("Expected mapping result");
    }
    
    Ok(())
}

#[test]
fn test_handlebars_with_helpers_in_keys() -> Result<()> {
    use std::collections::HashMap;
    
    let yaml = r#"
"{{camelCase service_name}}": "my-api"
"{{snakeCase environment_name}}_config":
  enabled: true
"#;
    
    let mut preprocessor = YamlPreprocessor::new();
    let ast = parse_yaml_with_custom_tags(yaml)?;
    
    // Create context with variables
    let mut variables = HashMap::new();
    variables.insert("service_name".to_string(), Value::String("API Service".to_string()));
    variables.insert("environment_name".to_string(), Value::String("Production Environment".to_string()));
    
    let context = TagContext::new().with_bindings(variables);
    let result = preprocessor.resolve_ast_with_context(ast, &context)?;
    
    if let Value::Mapping(map) = result {
        // Check that keys were processed with handlebars helpers
        assert!(map.contains_key(&Value::String("apiService".to_string())));
        assert!(map.contains_key(&Value::String("production_environment_config".to_string())));
        
        // Verify values
        assert_eq!(
            map.get(&Value::String("apiService".to_string())),
            Some(&Value::String("my-api".to_string()))
        );
        
        if let Some(Value::Mapping(config)) = map.get(&Value::String("production_environment_config".to_string())) {
            assert_eq!(
                config.get(&Value::String("enabled".to_string())),
                Some(&Value::Bool(true))
            );
        } else {
            panic!("Expected production_environment_config to be a mapping");
        }
    } else {
        panic!("Expected mapping result");
    }
    
    Ok(())
}