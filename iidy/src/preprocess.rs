use anyhow::Result;
use serde::de::DeserializeOwned;
use serde_yaml::Value;

use crate::yaml::{parse_yaml_with_custom_tags, TagContext};

/// YAML preprocessing system that processes iidy custom tags and handlebars templates.
///
/// This function converts a serde_yaml::Value to a YAML string, processes it through
/// the custom tag parser, and then deserializes the result to the requested type.
/// 
/// Currently implements AST parsing but not full resolution - that will be added
/// when the AST resolution system is completed.
pub fn preprocess<T: DeserializeOwned>(value: Value) -> Result<T> {
    // Convert Value back to YAML string for parsing with custom tags
    let yaml_string = serde_yaml::to_string(&value)?;
    
    // Parse with our custom tag system
    let ast = parse_yaml_with_custom_tags(&yaml_string)?;
    
    // TODO: Once AST resolution is implemented, add this:
    // let mut preprocessor = YamlPreprocessor::new();
    // let context = create_preprocessing_context()?;
    // let resolved = preprocessor.resolve_ast_with_context(ast, &context)?;
    // Ok(serde_yaml::from_value(resolved)?)
    
    // For now, convert the AST back to a Value and deserialize
    // This at least exercises the parsing logic and validates the syntax
    let processed_value = ast_to_value(ast)?;
    Ok(serde_yaml::from_value(processed_value)?)
}

/// Convert AST back to serde_yaml::Value for deserialization
/// This is a temporary bridge until full AST resolution is implemented
fn ast_to_value(ast: crate::yaml::YamlAst) -> Result<Value> {
    use crate::yaml::YamlAst;
    
    match ast {
        YamlAst::Null => Ok(Value::Null),
        YamlAst::Bool(b) => Ok(Value::Bool(b)),
        YamlAst::Number(n) => {
            // Convert f64 back to appropriate number type
            if n.fract() == 0.0 && n >= i64::MIN as f64 && n <= i64::MAX as f64 {
                Ok(Value::Number(serde_yaml::Number::from(n as i64)))
            } else {
                Ok(Value::Number(serde_yaml::Number::from(n)))
            }
        },
        YamlAst::String(s) => Ok(Value::String(s)),
        YamlAst::Sequence(seq) => {
            let mut vec = Vec::new();
            for item in seq {
                vec.push(ast_to_value(item)?);
            }
            Ok(Value::Sequence(vec))
        },
        YamlAst::Mapping(pairs) => {
            let mut map = serde_yaml::Mapping::new();
            for (key, value) in pairs {
                let key_value = ast_to_value(key)?;
                let value_value = ast_to_value(value)?;
                map.insert(key_value, value_value);
            }
            Ok(Value::Mapping(map))
        },
        YamlAst::PreprocessingTag(_tag) => {
            // For now, preprocessing tags are not resolved, so we represent them as null
            // TODO: Once AST resolution is implemented, this should never be reached
            // because all preprocessing tags should be resolved before this conversion
            Ok(Value::Null)
        },
        YamlAst::UnknownYamlTag(tag) => {
            // Return the tag's value as-is for unknown tags by recursively converting it
            ast_to_value(*tag.value)
        }
    }
}

/// Create a preprocessing context with environment variables and default settings
/// This will be expanded once the TagContext system is fully wired up
fn _create_preprocessing_context() -> Result<TagContext> {
    let mut context = TagContext::new();
    
    // Add common environment variables that might be used in stack-args
    if let Ok(env) = std::env::var("ENVIRONMENT") {
        context = context.with_variable("environment", Value::String(env));
    }
    if let Ok(app_name) = std::env::var("APP_NAME") {
        context = context.with_variable("app_name", Value::String(app_name));
    }
    if let Ok(region) = std::env::var("AWS_REGION") {
        context = context.with_variable("region", Value::String(region));
    }
    
    Ok(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    
    #[derive(Debug, Deserialize, PartialEq)]
    struct TestConfig {
        name: String,
        value: i32,
    }
    
    #[test]
    fn test_preprocess_simple_yaml() -> Result<()> {
        let yaml_value = serde_yaml::from_str::<Value>(r#"
name: "test"
value: 42
"#)?;
        
        let result: TestConfig = preprocess(yaml_value)?;
        assert_eq!(result.name, "test");
        assert_eq!(result.value, 42);
        
        Ok(())
    }
    
    #[test]
    fn test_preprocess_with_custom_tags() -> Result<()> {
        // Test that YAML with custom tags can be parsed (even if not resolved yet)
        let yaml_value = serde_yaml::from_str::<Value>(r#"
name: "test"
stack_name: !$join
  array: ["my-app", "production"]
  delimiter: "-"
"#)?;
        
        // Should not panic and should parse the structure
        let result = preprocess::<std::collections::HashMap<String, Value>>(yaml_value);
        assert!(result.is_ok(), "Custom tag parsing should not fail");
        
        Ok(())
    }
    
    #[test]
    fn test_ast_to_value_conversion() -> Result<()> {
        use crate::yaml::YamlAst;
        
        // Test basic scalar conversions
        assert_eq!(ast_to_value(YamlAst::Null)?, Value::Null);
        assert_eq!(ast_to_value(YamlAst::Bool(true))?, Value::Bool(true));
        assert_eq!(ast_to_value(YamlAst::String("test".to_string()))?, Value::String("test".to_string()));
        
        // Test number conversion
        let number_ast = YamlAst::Number(42.0);
        let number_value = ast_to_value(number_ast)?;
        assert!(matches!(number_value, Value::Number(_)));
        
        Ok(())
    }
    
    #[test]
    fn test_complex_yaml_structure() -> Result<()> {
        let yaml_value = serde_yaml::from_str::<Value>(r#"
database:
  host: "localhost"
  port: 5432
  settings:
    - "ssl=true"
    - "timeout=30"
features:
  enabled: true
  count: 10
"#)?;
        
        let result = preprocess::<std::collections::HashMap<String, Value>>(yaml_value)?;
        assert!(result.contains_key("database"));
        assert!(result.contains_key("features"));
        
        Ok(())
    }
}
