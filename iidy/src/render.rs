//! Template rendering functionality
//!
//! This module handles the rendering of YAML templates with preprocessing,
//! query selectors, and output formatting.

use anyhow::Result;
use std::fs;
use std::path::Path;
use std::io::{self, Read};
use serde_yaml::Value;

use crate::{cli::RenderArgs, yaml::preprocess_yaml};

/// Handle the render command - process YAML template and output in specified format
pub async fn handle_render_command(args: &RenderArgs) -> Result<()> {
    // Read the template content from file or stdin
    let (template_content, base_location) = if args.template == "-" {
        // Read from stdin
        let mut buffer = String::new();
        io::stdin().read_to_string(&mut buffer)?;
        (buffer, "<stdin>".to_string())
    } else {
        // Read from file
        let content = fs::read_to_string(&args.template)?;
        (content, args.template.clone())
    };
    
    // Process the YAML with the new preprocessing system using specified YAML spec
    let processed_value = preprocess_yaml(&template_content, &base_location, &args.yaml_spec).await?;
    
    // Apply query selector if provided
    let output_value = if let Some(query) = &args.query {
        apply_query_to_value(processed_value, query)?
    } else {
        processed_value
    };
    
    // Format output based on requested format
    let formatted_output = match args.format.as_str() {
        "json" => serde_json::to_string_pretty(&output_value)?,
        "yaml" | "yml" => serialize_yaml_iidy_js_compatible(&output_value)?,
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

/// Serialize YAML in a way that's compatible with iidy-js output formatting
/// 
/// This function mimics the behavior of iidy-js's dump function which uses js-yaml
/// with specific options and post-processing to ensure consistent output formatting.
fn serialize_yaml_iidy_js_compatible(value: &Value) -> Result<String> {
    // Use serde_yaml with default settings first
    let mut yaml_output = serde_yaml::to_string(value)?;
    
    // Apply post-processing to match iidy-js formatting:
    // 1. Convert CloudFormation mapping format to proper YAML tags
    yaml_output = convert_cf_mappings_to_tags(yaml_output)?;
    
    // 2. Quote numeric-looking values that should be strings (like version numbers)
    yaml_output = quote_numeric_looking_strings(yaml_output);
    
    Ok(yaml_output)
}

/// Post-process YAML output to quote numeric-looking strings like version numbers
/// 
/// This matches the behavior of iidy-js which quotes values like "2010-09-09" 
/// to ensure they're treated as strings rather than numbers or dates.
fn quote_numeric_looking_strings(yaml: String) -> String {
    use regex::Regex;
    
    // Quote version-like patterns (e.g., "2010-09-09", "1.2.3")
    // This regex looks for values that look like dates or version numbers
    // Matches both top-level keys (no indent) and nested keys (with indent)
    let numeric_pattern = Regex::new(r"^(\s*)([A-Za-z][A-Za-z0-9]*): (\d{4}-\d{2}-\d{2}|\d+[-\.]\d+[-\.]\d+(?:[-\.]\d+)*)$").unwrap();
    
    yaml.lines()
        .map(|line| {
            if let Some(captures) = numeric_pattern.captures(line) {
                let indent = &captures[1];
                let key = &captures[2];
                let value = &captures[3];
                format!("{}{}: '{}'", indent, key, value)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<String>>()
        .join("\n")
}

/// Convert CloudFormation mapping format to proper YAML tags
/// 
/// Transforms output from `'!Ref': value` to `!Ref value` for all CloudFormation intrinsic functions.
/// This post-processing step works around serde_yaml's inability to serialize `Value::Tagged` by
/// converting the mapping format (which serde_yaml can handle) to proper CloudFormation YAML syntax.
/// 
/// ## Alternative Approach Evaluated
/// 
/// A custom YAML serializer was explored as an alternative to this post-processing approach.
/// The custom serializer provided direct CloudFormation tag serialization without string manipulation,
/// but after evaluation, this simpler post-processing approach was chosen because:
/// 
/// - **Simplicity**: ~20 lines of regex vs ~500+ lines of custom serializer
/// - **Maintenance**: Leverages battle-tested serde_yaml for edge cases  
/// - **Performance**: String post-processing is fast enough for typical templates
/// - **Robustness**: Minimal code surface area reduces bug risk
/// 
/// See commit 34f4a56 for the full custom serializer implementation and detailed analysis.
fn convert_cf_mappings_to_tags(yaml: String) -> Result<String> {
    use regex::Regex;
    
    // First handle single-line patterns like '!Ref': value -> !Ref value
    let cf_single_line_pattern = Regex::new(r"(\s*)'!(Ref|Sub|GetAtt|Base64|Select|Split|Join|ImportValue|FindInMap|Cidr|Length|ToJsonString|Transform|ForEach|If|Equals|And|Or|Not|GetAZs)': (.+)")?;
    
    let mut converted = cf_single_line_pattern.replace_all(&yaml, |caps: &regex::Captures| {
        let indent = &caps[1];
        let function = &caps[2];
        let value = &caps[3];
        format!("{}!{} {}", indent, function, value)
    }).to_string();
    
    // Then handle multi-line patterns like:
    // '!Select':    ->    !Select
    // - 0                 - 0  
    // - !GetAZs ''        - !GetAZs ''
    let cf_multi_line_pattern = Regex::new(r"(\s*)'!(Ref|Sub|GetAtt|Base64|Select|Split|Join|ImportValue|FindInMap|Cidr|Length|ToJsonString|Transform|ForEach|If|Equals|And|Or|Not|GetAZs)':")?;
    
    converted = cf_multi_line_pattern.replace_all(&converted, |caps: &regex::Captures| {
        let indent = &caps[1];
        let function = &caps[2];
        format!("{}!{}", indent, function)
    }).to_string();
    
    Ok(converted)
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

    #[test]
    fn test_yaml_version_quoting() {
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            Value::String("AWSTemplateFormatVersion".to_string()),
            Value::String("2010-09-09".to_string())
        );
        map.insert(
            Value::String("Description".to_string()),
            Value::String("Test template".to_string())
        );
        map.insert(
            Value::String("Version".to_string()),
            Value::String("1.2.3".to_string())
        );
        map.insert(
            Value::String("RegularNumber".to_string()),
            Value::Number(serde_yaml::Number::from(123))
        );
        map.insert(
            Value::String("RegularFloat".to_string()),
            Value::Number(serde_yaml::Number::from(1.5))
        );
        map.insert(
            Value::String("DateString".to_string()),
            Value::String("2023-12-25".to_string())
        );
        map.insert(
            Value::String("DashVersion".to_string()),
            Value::String("1-2-3".to_string())
        );
        let value = Value::Mapping(map);
        
        let result = serialize_yaml_iidy_js_compatible(&value).unwrap();
        
        // Should quote version numbers, dates, and dash versions
        assert!(result.contains("AWSTemplateFormatVersion: '2010-09-09'"));
        assert!(result.contains("Version: '1.2.3'"));
        assert!(result.contains("DateString: '2023-12-25'"));
        assert!(result.contains("DashVersion: '1-2-3'"));
        
        // Should not quote regular text, regular numbers, or regular floats
        assert!(result.contains("Description: Test template"));
        assert!(!result.contains("Description: 'Test template'"));
        assert!(result.contains("RegularNumber: 123"));
        assert!(!result.contains("RegularNumber: '123'"));
        assert!(result.contains("RegularFloat: 1.5"));
        assert!(!result.contains("RegularFloat: '1.5'"));
    }

}