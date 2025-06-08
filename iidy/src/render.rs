//! Template rendering functionality
//!
//! This module handles the rendering of YAML templates with preprocessing,
//! query selectors, and output formatting.

use anyhow::Result;
use std::fs;
use std::path::Path;
use serde_yaml::Value;

use crate::{cli::RenderArgs, yaml::preprocess_yaml_with_spec};

/// Handle the render command - process YAML template and output in specified format
pub async fn handle_render_command(args: &RenderArgs) -> Result<()> {
    // Read the template file
    let template_content = fs::read_to_string(&args.template)?;
    
    // Get the base location from the template file path for relative imports
    let base_location = &args.template;
    
    // Process the YAML with the new preprocessing system using specified YAML spec
    let processed_value = preprocess_yaml_with_spec(&template_content, base_location, &args.yaml_spec).await?;
    
    // Apply query selector if provided
    let output_value = if let Some(query) = &args.query {
        apply_query_to_value(processed_value, query)?
    } else {
        processed_value
    };
    
    // Format output based on requested format
    let formatted_output = match args.format.as_str() {
        "json" => serde_json::to_string_pretty(&output_value)?,
        "yaml" | "yml" => serde_yaml::to_string(&output_value)?,
        _ => return Err(anyhow::anyhow!("Unsupported format: {}. Use 'yaml' or 'json'", args.format)),
    };
    
    // Output to file or stdout
    if args.outfile == "stdout" || args.outfile == "-" {
        println!("{}", formatted_output);
    } else {
        // Check if file exists and handle overwrite logic
        if Path::new(&args.outfile).exists() && !args.overwrite {
            return Err(anyhow::anyhow!(
                "Output file '{}' exists. Use --overwrite to overwrite it.", 
                args.outfile
            ));
        }
        
        fs::write(&args.outfile, formatted_output)?;
        eprintln!("Template rendered to: {}", args.outfile);
    }
    
    Ok(())
}

/// Apply a query selector to extract a subset of the processed YAML value
/// 
/// Supports dot notation like "Resources.MyBucket" to navigate through nested mappings.
/// 
/// # Arguments
/// * `value` - The processed YAML value to query
/// * `query` - Dot-separated path to the desired subset (e.g., "Resources.MyBucket.Properties")
/// 
/// # Returns
/// The value at the specified query path, or an error if the path is not found
pub fn apply_query_to_value(value: Value, query: &str) -> Result<Value> {
    // Simple query support - handles dot notation like "Resources.MyBucket"
    let parts: Vec<&str> = query.split('.').collect();
    let mut current = value;
    
    for part in parts {
        if part.is_empty() {
            continue;
        }
        
        match current {
            Value::Mapping(ref map) => {
                let key = Value::String(part.to_string());
                if let Some(next_value) = map.get(&key) {
                    current = next_value.clone();
                } else {
                    return Err(anyhow::anyhow!("Query path '{}' not found at key '{}'", query, part));
                }
            }
            _ => {
                return Err(anyhow::anyhow!("Cannot query '{}' on non-mapping value", part));
            }
        }
    }
    
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Mapping;

    #[test]
    fn test_apply_query_to_value_simple() {
        let mut map = Mapping::new();
        map.insert(
            Value::String("Resources".to_string()),
            Value::String("test-value".to_string())
        );
        let value = Value::Mapping(map);
        
        let result = apply_query_to_value(value, "Resources").unwrap();
        assert_eq!(result, Value::String("test-value".to_string()));
    }

    #[test]
    fn test_apply_query_to_value_nested() {
        let mut inner_map = Mapping::new();
        inner_map.insert(
            Value::String("BucketName".to_string()),
            Value::String("my-bucket".to_string())
        );
        
        let mut outer_map = Mapping::new();
        outer_map.insert(
            Value::String("MyBucket".to_string()),
            Value::Mapping(inner_map)
        );
        
        let mut root_map = Mapping::new();
        root_map.insert(
            Value::String("Resources".to_string()),
            Value::Mapping(outer_map)
        );
        
        let value = Value::Mapping(root_map);
        
        let result = apply_query_to_value(value, "Resources.MyBucket.BucketName").unwrap();
        assert_eq!(result, Value::String("my-bucket".to_string()));
    }

    #[test]
    fn test_apply_query_to_value_not_found() {
        let mut map = Mapping::new();
        map.insert(
            Value::String("Resources".to_string()),
            Value::String("test-value".to_string())
        );
        let value = Value::Mapping(map);
        
        let result = apply_query_to_value(value, "NotFound");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Query path 'NotFound' not found"));
    }

    #[test]
    fn test_apply_query_to_value_invalid_path() {
        let value = Value::String("not-a-mapping".to_string());
        
        let result = apply_query_to_value(value, "some.path");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Cannot query 'some' on non-mapping value"));
    }
}