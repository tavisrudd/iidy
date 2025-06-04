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
}