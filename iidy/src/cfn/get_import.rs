//! Get-import command implementation
//!
//! Retrieves and displays data from any import location supported by the iidy import system.
//! This command enables users to directly access and inspect data from various sources 
//! (files, S3, HTTP, AWS services, etc.) without needing to create a template.

use anyhow::{Result, anyhow};
use jmespath;
use serde_json;
use serde_yaml;

use crate::{
    cli::{Cli, GetImportArgs},
    aws::{config_from_normalized_opts, format_aws_error},
    yaml::imports::{loaders::ProductionImportLoader, ImportLoader},
};

/// Get import data and display it in the requested format
///
/// This function implements the same behavior as iidy-js getImport:
/// 1. Configure AWS credentials if needed for AWS-based imports
/// 2. Load data from the specified import location
/// 3. Apply JMESPath query if specified  
/// 4. Output result in requested format (YAML or JSON)
///
/// # Arguments
/// * `cli` - CLI context with global and AWS options
/// * `args` - GetImport command arguments
///
/// # Returns
/// Result with exit code (0 for success, 1 for error)
pub async fn get_import(cli: &Cli, args: &GetImportArgs) -> Result<i32> {
    // Normalize AWS options from CLI
    let normalized_opts = cli.aws_opts.clone().normalize();
    
    // Configure AWS SDK for AWS-based imports (cfn, ssm, s3)
    let aws_config = match config_from_normalized_opts(&normalized_opts).await {
        Ok(config) => Some(config),
        Err(e) => {
            // Handle AWS configuration errors gracefully
            eprintln!("{}", format_aws_error(&e));
            return Ok(1);
        }
    };

    // Create import loader with AWS configuration
    let import_loader = match aws_config {
        Some(config) => ProductionImportLoader::new().with_aws_config(config),
        None => ProductionImportLoader::new(),
    };

    // Load data from import location
    // Use "." as base location (local context) to allow all import types like iidy-js
    let base_location = ".";
    let import_data = match import_loader.load(&args.import, base_location).await {
        Ok(data) => data,
        Err(e) => {
            // Check if this is an AWS error and format appropriately
            let error_msg = if e.to_string().contains("AWS") {
                format_aws_error(&e)
            } else {
                format!("Import error: {}", e)
            };
            eprintln!("{}", error_msg);
            return Ok(1);
        }
    };

    // Apply JMESPath query if specified
    let mut output_doc = import_data.doc;
    if let Some(query_str) = &args.query {
        // Convert serde_yaml::Value to serde_json::Value for JMESPath
        let json_value = yaml_to_json_value(&output_doc)?;
        
        // Compile and execute JMESPath query
        match jmespath::compile(query_str) {
            Ok(expression) => {
                match expression.search(&json_value) {
                    Ok(result) => {
                        // Convert JMESPath result back to serde_json::Value then to serde_yaml::Value
                        let json_result = jmespath_to_json_value(result)?;
                        output_doc = json_to_yaml_value(&json_result)?;
                    }
                    Err(e) => {
                        eprintln!("JMESPath query execution error: {}", e);
                        return Ok(1);
                    }
                }
            }
            Err(e) => {
                eprintln!("Invalid JMESPath query '{}': {}", query_str, e);
                return Ok(1);
            }
        }
    }

    // Output result in requested format
    match args.format.as_str() {
        "yaml" => {
            // Output as YAML (default)
            match serde_yaml::to_string(&output_doc) {
                Ok(yaml_str) => print!("{}", yaml_str),
                Err(e) => {
                    eprintln!("YAML serialization error: {}", e);
                    return Ok(1);
                }
            }
        }
        "json" => {
            // Output as pretty-printed JSON
            match serde_json::to_string_pretty(&yaml_to_json_value(&output_doc)?) {
                Ok(json_str) => println!("{}", json_str),
                Err(e) => {
                    eprintln!("JSON serialization error: {}", e);
                    return Ok(1);
                }
            }
        }
        _ => {
            eprintln!("Unsupported format: '{}'. Use 'yaml' or 'json'.", args.format);
            return Ok(1);
        }
    }

    Ok(0)
}

/// Convert serde_yaml::Value to serde_json::Value for JMESPath processing
fn yaml_to_json_value(yaml_value: &serde_yaml::Value) -> Result<serde_json::Value> {
    // Serialize YAML to string, then deserialize as JSON
    // This handles the conversion between the two value types
    let yaml_str = serde_yaml::to_string(yaml_value)
        .map_err(|e| anyhow!("Failed to serialize YAML value: {}", e))?;
    
    let json_value: serde_json::Value = serde_yaml::from_str(&yaml_str)
        .map_err(|e| anyhow!("Failed to convert YAML to JSON: {}", e))?;
    
    Ok(json_value)
}

/// Convert serde_json::Value back to serde_yaml::Value
fn json_to_yaml_value(json_value: &serde_json::Value) -> Result<serde_yaml::Value> {
    // Serialize JSON to string, then deserialize as YAML
    let json_str = serde_json::to_string(json_value)
        .map_err(|e| anyhow!("Failed to serialize JSON value: {}", e))?;
    
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to convert JSON to YAML: {}", e))?;
    
    Ok(yaml_value)
}

/// Convert JMESPath Variable to serde_json::Value
fn jmespath_to_json_value(variable: std::rc::Rc<jmespath::Variable>) -> Result<serde_json::Value> {
    use jmespath::Variable;
    
    let json_value = match variable.as_ref() {
        Variable::Null => serde_json::Value::Null,
        Variable::Bool(b) => serde_json::Value::Bool(*b),
        Variable::Number(n) => {
            if let Some(i) = n.as_i64() {
                serde_json::Value::Number(serde_json::Number::from(i))
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f)
                    .map(serde_json::Value::Number)
                    .unwrap_or(serde_json::Value::Null)
            } else {
                serde_json::Value::Null
            }
        }
        Variable::String(s) => serde_json::Value::String(s.clone()),
        Variable::Array(arr) => {
            let mut json_arr = Vec::new();
            for item in arr {
                json_arr.push(jmespath_to_json_value(item.clone())?);
            }
            serde_json::Value::Array(json_arr)
        }
        Variable::Object(obj) => {
            let mut json_obj = serde_json::Map::new();
            for (key, value) in obj {
                json_obj.insert(key.clone(), jmespath_to_json_value(value.clone())?);
            }
            serde_json::Value::Object(json_obj)
        }
        Variable::Expref(_) => {
            // Expression references are not directly convertible to JSON
            // Return null for now - this could be enhanced to handle specific use cases
            serde_json::Value::Null
        }
    };
    
    Ok(json_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use serde_yaml::Value as YamlValue;
    use jmespath::Variable;
    use std::rc::Rc;

    #[test]
    fn test_yaml_to_json_conversion() {
        let yaml_value = serde_yaml::from_str(r#"
            name: "test"
            values: [1, 2, 3]
            nested:
              key: "value"
        "#).unwrap();

        let json_value = yaml_to_json_value(&yaml_value).unwrap();
        
        assert_eq!(json_value["name"], "test");
        assert_eq!(json_value["values"], json!([1, 2, 3]));
        assert_eq!(json_value["nested"]["key"], "value");
    }

    #[test]
    fn test_json_to_yaml_conversion() {
        let json_value = json!({
            "name": "test",
            "values": [1, 2, 3],
            "nested": {
                "key": "value"
            }
        });

        let yaml_value = json_to_yaml_value(&json_value).unwrap();
        
        // Convert back to verify round-trip
        if let YamlValue::Mapping(map) = yaml_value {
            assert!(map.contains_key(&YamlValue::String("name".to_string())));
            assert!(map.contains_key(&YamlValue::String("values".to_string())));
            assert!(map.contains_key(&YamlValue::String("nested".to_string())));
        } else {
            panic!("Expected YAML mapping");
        }
    }

    #[test]
    fn test_round_trip_conversion() {
        let original_yaml = serde_yaml::from_str(r#"
            database:
              host: "localhost"
              port: 5432
              enabled: true
            features: ["auth", "logging"]
        "#).unwrap();

        // Round trip: YAML -> JSON -> YAML
        let json_value = yaml_to_json_value(&original_yaml).unwrap();
        let final_yaml = json_to_yaml_value(&json_value).unwrap();
        
        // Verify structure is preserved
        let original_str = serde_yaml::to_string(&original_yaml).unwrap();
        let final_str = serde_yaml::to_string(&final_yaml).unwrap();
        
        // Parse both back to ensure they represent the same data
        let original_reparsed: serde_yaml::Value = serde_yaml::from_str(&original_str).unwrap();
        let final_reparsed: serde_yaml::Value = serde_yaml::from_str(&final_str).unwrap();
        
        assert_eq!(original_reparsed, final_reparsed);
    }

    #[test]
    fn test_jmespath_variable_conversion() {
        // Test string conversion
        let string_var = Rc::new(Variable::String("test".to_string()));
        let json_value = jmespath_to_json_value(string_var).unwrap();
        assert_eq!(json_value, json!("test"));

        // Test number conversion
        let num_var = Rc::new(Variable::Number(serde_json::Number::from(42)));
        let json_value = jmespath_to_json_value(num_var).unwrap();
        assert_eq!(json_value, json!(42));

        // Test boolean conversion
        let bool_var = Rc::new(Variable::Bool(true));
        let json_value = jmespath_to_json_value(bool_var).unwrap();
        assert_eq!(json_value, json!(true));

        // Test null conversion
        let null_var = Rc::new(Variable::Null);
        let json_value = jmespath_to_json_value(null_var).unwrap();
        assert_eq!(json_value, json!(null));
    }

    #[test]
    fn test_jmespath_array_conversion() {
        let items = vec![
            Rc::new(Variable::String("item1".to_string())),
            Rc::new(Variable::Number(serde_json::Number::from(2))),
            Rc::new(Variable::Bool(true)),
        ];
        let array_var = Rc::new(Variable::Array(items));
        let json_value = jmespath_to_json_value(array_var).unwrap();
        
        assert_eq!(json_value, json!(["item1", 2, true]));
    }

    #[test]
    fn test_jmespath_object_conversion() {
        let mut obj = std::collections::BTreeMap::new();
        obj.insert("name".to_string(), Rc::new(Variable::String("test".to_string())));
        obj.insert("count".to_string(), Rc::new(Variable::Number(serde_json::Number::from(42))));
        obj.insert("enabled".to_string(), Rc::new(Variable::Bool(true)));
        
        let object_var = Rc::new(Variable::Object(obj));
        let json_value = jmespath_to_json_value(object_var).unwrap();
        
        assert_eq!(json_value["name"], "test");
        assert_eq!(json_value["count"], 42);
        assert_eq!(json_value["enabled"], true);
    }
}