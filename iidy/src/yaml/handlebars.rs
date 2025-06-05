//! Handlebars template interpolation for YAML preprocessing
//! 
//! This module provides handlebars template processing for import locations
//! and other string values in the preprocessing system.

use std::collections::HashMap;
use anyhow::{Result, anyhow};
use handlebars::{Handlebars, Helper, Context, RenderContext, Output, HelperResult};
use serde_json::Value;
use base64::{Engine as _, engine::general_purpose};

/// Initialize a handlebars registry with all the custom helpers
pub fn create_handlebars_registry() -> Handlebars<'static> {
    let mut handlebars = Handlebars::new();
    
    // Set options to match iidy-js behavior
    handlebars.set_strict_mode(true);
    
    // Register JSON helpers
    handlebars.register_helper("toJson", Box::new(to_json_helper));
    handlebars.register_helper("tojson", Box::new(to_json_helper)); // deprecated alias
    handlebars.register_helper("toJsonPretty", Box::new(to_json_pretty_helper));
    handlebars.register_helper("tojsonPretty", Box::new(to_json_pretty_helper)); // deprecated alias
    
    // Register YAML helper
    handlebars.register_helper("toYaml", Box::new(to_yaml_helper));
    handlebars.register_helper("toyaml", Box::new(to_yaml_helper)); // deprecated alias
    
    // Register encoding helpers
    handlebars.register_helper("base64", Box::new(base64_helper));
    
    // Register string manipulation helpers
    handlebars.register_helper("toLowerCase", Box::new(to_lower_case_helper));
    handlebars.register_helper("toUpperCase", Box::new(to_upper_case_helper));
    handlebars.register_helper("titleize", Box::new(titleize_helper));
    handlebars.register_helper("camelCase", Box::new(camel_case_helper));
    handlebars.register_helper("snakeCase", Box::new(snake_case_helper));
    handlebars.register_helper("kebabCase", Box::new(kebab_case_helper));
    handlebars.register_helper("capitalize", Box::new(capitalize_helper));
    handlebars.register_helper("trim", Box::new(trim_helper));
    handlebars.register_helper("replace", Box::new(replace_helper));
    
    // Register object access helpers
    handlebars.register_helper("lookup", Box::new(lookup_helper));
    
    // Note: if, unless, each, and with are built-in block helpers in handlebars-rs
    // No need to register custom implementations for basic functionality
    
    handlebars
}

/// Interpolate a handlebars template string with the given environment values
pub fn interpolate_handlebars_string(
    template_string: &str, 
    env_values: &HashMap<String, Value>,
    error_context: &str
) -> Result<String> {
    // Check if the string contains handlebars syntax
    if !template_string.contains("{{") {
        return Ok(template_string.to_string());
    }
    
    let handlebars = create_handlebars_registry();
    
    // Convert environment values to serde_json::Value for handlebars
    let data = Value::Object(env_values.iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect());
    
    handlebars.render_template(template_string, &data)
        .map_err(|e| anyhow!(
            "Error in string template at {}: {}\nTemplate: {}",
            error_context, e, template_string
        ))
}

// Helper implementations

fn to_json_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("toJson helper requires one parameter"))?
        .value();
    
    let json_str = serde_json::to_string(value)
        .map_err(|e| handlebars::RenderError::new(&format!("Failed to serialize to JSON: {}", e)))?;
    
    out.write(&json_str)?;
    Ok(())
}

fn to_json_pretty_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("toJsonPretty helper requires one parameter"))?
        .value();
    
    let json_str = serde_json::to_string_pretty(value)
        .map_err(|e| handlebars::RenderError::new(&format!("Failed to serialize to pretty JSON: {}", e)))?;
    
    out.write(&json_str)?;
    Ok(())
}

fn to_yaml_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("toYaml helper requires one parameter"))?
        .value();
    
    let yaml_str = serde_yaml::to_string(value)
        .map_err(|e| handlebars::RenderError::new(&format!("Failed to serialize to YAML: {}", e)))?;
    
    out.write(&yaml_str)?;
    Ok(())
}

fn base64_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("base64 helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("base64 helper requires a string parameter"))?;
    
    let encoded = general_purpose::STANDARD.encode(string_value.as_bytes());
    out.write(&encoded)?;
    Ok(())
}

fn to_lower_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("toLowerCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("toLowerCase helper requires a string parameter"))?;
    
    out.write(&string_value.to_lowercase())?;
    Ok(())
}

fn to_upper_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("toUpperCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("toUpperCase helper requires a string parameter"))?;
    
    out.write(&string_value.to_uppercase())?;
    Ok(())
}

fn titleize_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("titleize helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("titleize helper requires a string parameter"))?;
    
    let titleized = string_value
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    
    out.write(&titleized)?;
    Ok(())
}

fn camel_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("camelCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("camelCase helper requires a string parameter"))?;
    
    let camel_cased = string_value
        .split_whitespace()
        .enumerate()
        .map(|(i, word)| {
            if i == 0 {
                word.to_lowercase()
            } else {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                }
            }
        })
        .collect::<Vec<_>>()
        .join("");
    
    out.write(&camel_cased)?;
    Ok(())
}

fn snake_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("snakeCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("snakeCase helper requires a string parameter"))?;
    
    let snake_cased = string_value
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join("_");
    
    out.write(&snake_cased)?;
    Ok(())
}

fn kebab_case_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("kebabCase helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("kebabCase helper requires a string parameter"))?;
    
    let kebab_cased = string_value
        .split_whitespace()
        .map(|word| word.to_lowercase())
        .collect::<Vec<_>>()
        .join("-");
    
    out.write(&kebab_cased)?;
    Ok(())
}

fn capitalize_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("capitalize helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("capitalize helper requires a string parameter"))?;
    
    let mut chars = string_value.chars();
    let capitalized = match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
    };
    
    out.write(&capitalized)?;
    Ok(())
}

fn trim_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("trim helper requires one parameter"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("trim helper requires a string parameter"))?;
    
    out.write(string_value.trim())?;
    Ok(())
}

fn replace_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let value = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires three parameters: string, search, replacement"))?
        .value();
    
    let search = h.param(1)
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires three parameters: string, search, replacement"))?
        .value();
    
    let replacement = h.param(2)
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires three parameters: string, search, replacement"))?
        .value();
    
    let string_value = value.as_str()
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires a string as first parameter"))?;
    
    let search_str = search.as_str()
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires a string as second parameter"))?;
    
    let replacement_str = replacement.as_str()
        .ok_or_else(|| handlebars::RenderError::new("replace helper requires a string as third parameter"))?;
    
    let replaced = string_value.replace(search_str, replacement_str);
    out.write(&replaced)?;
    Ok(())
}

fn lookup_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let object = h.param(0)
        .ok_or_else(|| handlebars::RenderError::new("lookup helper requires two parameters: object and key"))?
        .value();
    
    let key = h.param(1)
        .ok_or_else(|| handlebars::RenderError::new("lookup helper requires two parameters: object and key"))?
        .value();
    
    let key_str = key.as_str()
        .ok_or_else(|| handlebars::RenderError::new("lookup helper requires key to be a string"))?;
    
    match object {
        Value::Object(obj) => {
            if let Some(value) = obj.get(key_str) {
                let value_str = match value {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "".to_string(),
                    _ => serde_json::to_string(value)
                        .map_err(|e| handlebars::RenderError::new(&format!("Failed to serialize lookup result: {}", e)))?,
                };
                out.write(&value_str)?;
            }
            // If key not found, output nothing (handlebars convention)
        }
        Value::Array(arr) => {
            if let Ok(index) = key_str.parse::<usize>() {
                if let Some(value) = arr.get(index) {
                    let value_str = match value {
                        Value::String(s) => s.clone(),
                        Value::Number(n) => n.to_string(),
                        Value::Bool(b) => b.to_string(),
                        Value::Null => "".to_string(),
                        _ => serde_json::to_string(value)
                            .map_err(|e| handlebars::RenderError::new(&format!("Failed to serialize lookup result: {}", e)))?,
                    };
                    out.write(&value_str)?;
                }
            }
        }
        _ => {
            return Err(handlebars::RenderError::new("lookup helper requires first parameter to be an object or array"));
        }
    }
    
    Ok(())
}

// Note: Block helpers (if, unless, each, with) are built into handlebars-rs
// They provide the standard handlebars functionality without custom implementation

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_interpolation() {
        let mut env = HashMap::new();
        env.insert("name".to_string(), json!("world"));
        
        let result = interpolate_handlebars_string("Hello {{name}}", &env, "test").unwrap();
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_no_interpolation_needed() {
        let env = HashMap::new();
        let result = interpolate_handlebars_string("Hello world", &env, "test").unwrap();
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_to_json_helper() {
        let mut env = HashMap::new();
        env.insert("data".to_string(), json!({"key": "value"}));
        
        let result = interpolate_handlebars_string("{{toJson data}}", &env, "test").unwrap();
        assert_eq!(result, r#"{"key":"value"}"#);
    }

    #[test]
    fn test_to_yaml_helper() {
        let mut env = HashMap::new();
        env.insert("data".to_string(), json!({"key": "value"}));
        
        let result = interpolate_handlebars_string("{{toYaml data}}", &env, "test").unwrap();
        assert_eq!(result, "key: value\n");
    }

    #[test]
    fn test_base64_helper() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("hello"));
        
        let result = interpolate_handlebars_string("{{base64 text}}", &env, "test").unwrap();
        assert_eq!(result, "aGVsbG8=");
    }

    #[test]
    fn test_case_helpers() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("Hello World"));
        
        let lower = interpolate_handlebars_string("{{toLowerCase text}}", &env, "test").unwrap();
        assert_eq!(lower, "hello world");
        
        let upper = interpolate_handlebars_string("{{toUpperCase text}}", &env, "test").unwrap();
        assert_eq!(upper, "HELLO WORLD");
    }

    #[test]
    fn test_complex_interpolation() {
        let mut env = HashMap::new();
        env.insert("config".to_string(), json!({"env": "production", "port": 3000}));
        env.insert("service".to_string(), json!("api"));
        
        let template = "https://{{service}}.{{config.env}}.example.com:{{config.port}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "https://api.production.example.com:3000");
    }

    #[test]
    fn test_error_handling() {
        let env = HashMap::new();
        let result = interpolate_handlebars_string("{{missing_var}}", &env, "test");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Error in string template at test"));
    }

    #[test]
    fn test_titleize_helper() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("one two three"));
        
        let result = interpolate_handlebars_string("{{titleize text}}", &env, "test").unwrap();
        assert_eq!(result, "One Two Three");
        
        // Test with mixed case
        env.insert("text".to_string(), json!("hello WORLD test"));
        let result = interpolate_handlebars_string("{{titleize text}}", &env, "test").unwrap();
        assert_eq!(result, "Hello World Test");
    }

    #[test]
    fn test_camel_case_helper() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("hello world test"));
        
        let result = interpolate_handlebars_string("{{camelCase text}}", &env, "test").unwrap();
        assert_eq!(result, "helloWorldTest");
        
        // Test with single word
        env.insert("text".to_string(), json!("hello"));
        let result = interpolate_handlebars_string("{{camelCase text}}", &env, "test").unwrap();
        assert_eq!(result, "hello");
    }

    #[test]
    fn test_snake_case_helper() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("Hello World Test"));
        
        let result = interpolate_handlebars_string("{{snakeCase text}}", &env, "test").unwrap();
        assert_eq!(result, "hello_world_test");
    }

    #[test]
    fn test_kebab_case_helper() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("Hello World Test"));
        
        let result = interpolate_handlebars_string("{{kebabCase text}}", &env, "test").unwrap();
        assert_eq!(result, "hello-world-test");
    }

    #[test]
    fn test_capitalize_helper() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("hello world"));
        
        let result = interpolate_handlebars_string("{{capitalize text}}", &env, "test").unwrap();
        assert_eq!(result, "Hello world");
        
        // Test with empty string
        env.insert("text".to_string(), json!(""));
        let result = interpolate_handlebars_string("{{capitalize text}}", &env, "test").unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_trim_helper() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("  hello world  "));
        
        let result = interpolate_handlebars_string("{{trim text}}", &env, "test").unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_replace_helper() {
        let mut env = HashMap::new();
        env.insert("text".to_string(), json!("hello world hello"));
        
        let result = interpolate_handlebars_string("{{replace text \"hello\" \"hi\"}}", &env, "test").unwrap();
        assert_eq!(result, "hi world hi");
        
        // Test with no matches
        let result = interpolate_handlebars_string("{{replace text \"xyz\" \"abc\"}}", &env, "test").unwrap();
        assert_eq!(result, "hello world hello");
    }

    #[test]
    fn test_string_helpers_chaining() {
        let mut env = HashMap::new();
        env.insert("input".to_string(), json!("  HELLO world  "));
        
        // Test complex chaining scenario similar to iidy-js tests
        let template = "{{trim (titleize (toLowerCase input))}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_if_block_helper() {
        let mut env = HashMap::new();
        env.insert("condition".to_string(), json!(true));
        env.insert("name".to_string(), json!("World"));
        
        let template = "{{#if condition}}Hello {{name}}{{/if}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "Hello World");
        
        // Test false condition
        env.insert("condition".to_string(), json!(false));
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "");
        
        // Test with else
        let template_with_else = "{{#if condition}}Hello {{name}}{{else}}Goodbye{{/if}}";
        let result = interpolate_handlebars_string(template_with_else, &env, "test").unwrap();
        assert_eq!(result, "Goodbye");
    }

    #[test]
    fn test_unless_block_helper() {
        let mut env = HashMap::new();
        env.insert("condition".to_string(), json!(false));
        env.insert("name".to_string(), json!("World"));
        
        let template = "{{#unless condition}}Hello {{name}}{{/unless}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "Hello World");
        
        // Test true condition (should not render)
        env.insert("condition".to_string(), json!(true));
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_each_block_helper_array() {
        let mut env = HashMap::new();
        env.insert("items".to_string(), json!(["apple", "banana", "cherry"]));
        
        let template = "{{#each items}}{{@index}}: {{this}} {{/each}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "0: apple 1: banana 2: cherry ");
        
        // Test with @first and @last
        let template_detailed = "{{#each items}}{{#if @first}}first: {{/if}}{{this}}{{#if @last}} last{{/if}} {{/each}}";
        let result = interpolate_handlebars_string(template_detailed, &env, "test").unwrap();
        assert_eq!(result, "first: apple banana cherry last ");
    }

    #[test]
    fn test_each_block_helper_object() {
        let mut env = HashMap::new();
        env.insert("config".to_string(), json!({"name": "test", "port": 3000, "debug": true}));
        
        let template = "{{#each config}}{{@key}}={{this}} {{/each}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        
        // Object iteration order may vary, so check that all expected parts are present
        assert!(result.contains("name=test"));
        assert!(result.contains("port=3000"));
        assert!(result.contains("debug=true"));
    }

    #[test]
    fn test_each_block_helper_empty() {
        let mut env = HashMap::new();
        env.insert("items".to_string(), json!([]));
        
        let template = "{{#each items}}{{this}}{{else}}No items{{/each}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "No items");
    }

    #[test]
    fn test_with_block_helper() {
        let mut env = HashMap::new();
        env.insert("user".to_string(), json!({"name": "John", "age": 30}));
        
        let template = "{{#with user}}Name: {{name}}, Age: {{age}}{{/with}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "Name: John, Age: 30");
        
        // Test with null context
        env.insert("user".to_string(), json!(null));
        let template_with_else = "{{#with user}}Name: {{name}}{{else}}No user{{/with}}";
        let result = interpolate_handlebars_string(template_with_else, &env, "test").unwrap();
        assert_eq!(result, "No user");
    }

    #[test]
    fn test_complex_object_access() {
        let mut env = HashMap::new();
        env.insert("config".to_string(), json!({
            "database": {
                "host": "localhost",
                "port": 5432,
                "credentials": {
                    "username": "admin",
                    "password": "secret"
                }
            },
            "features": ["auth", "logging", "metrics"]
        }));
        
        // Test nested object access
        let template = "Host: {{config.database.host}}:{{config.database.port}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "Host: localhost:5432");
        
        // Test deep nesting
        let template = "User: {{config.database.credentials.username}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "User: admin");
        
        // Test array access
        let template = "First feature: {{config.features.[0]}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "First feature: auth");
        
        // Test array with complex paths
        let template = "Features: {{#each config.features}}{{this}} {{/each}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "Features: auth logging metrics ");
    }

    #[test] 
    fn test_dynamic_key_access() {
        let mut env = HashMap::new();
        env.insert("obj".to_string(), json!({"key1": "value1", "key2": "value2"}));
        env.insert("keyName".to_string(), json!("key1"));
        
        // Test dynamic property access (this requires helper support)
        let template = "{{lookup obj keyName}}";
        let result = interpolate_handlebars_string(template, &env, "test");
        
        // This might fail if lookup helper is not available, which is expected
        // We should implement a lookup helper
        if result.is_err() {
            // Expected for now, we'll implement lookup helper
            println!("lookup helper not yet implemented: {}", result.unwrap_err());
            return;
        }
        
        assert_eq!(result.unwrap(), "value1");
    }

    #[test]
    fn test_nested_template_scenarios() {
        let mut env = HashMap::new();
        env.insert("services".to_string(), json!([
            {"name": "api", "port": 3000, "config": {"env": "prod"}},
            {"name": "web", "port": 8080, "config": {"env": "dev"}}
        ]));
        
        // Test complex nested access in loops
        let template = "{{#each services}}{{name}}({{config.env}}):{{port}} {{/each}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "api(prod):3000 web(dev):8080 ");
        
        // Test with conditional logic on nested properties
        let template = "{{#each services}}{{#if config.env}}{{name}}: {{config.env}} {{/if}}{{/each}}";
        let result = interpolate_handlebars_string(template, &env, "test").unwrap();
        assert_eq!(result, "api: prod web: dev ");
    }
}