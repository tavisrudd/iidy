use anyhow::{Result, anyhow};
use jmespath::Variable;

use crate::{
    aws::{config_from_normalized_opts, format_aws_error},
    cli::{Cli, GetImportArgs},
    yaml::imports::{ImportLoader, loaders::ProductionImportLoader},
};

/// Retrieve and display data from any import location supported by the iidy import system.
///
/// This is a data extraction command (output piped to stdout), so it uses
/// direct stderr output rather than the output manager.
pub async fn get_import(cli: &Cli, args: &GetImportArgs) -> Result<i32> {
    let normalized_opts = cli.aws_opts.clone().normalize();

    let aws_config = match config_from_normalized_opts(&normalized_opts).await {
        Ok((config, _credential_sources)) => Some(config),
        Err(e) => {
            eprintln!("{}", format_aws_error(&e));
            return Ok(1);
        }
    };

    let import_loader = match aws_config {
        Some(config) => ProductionImportLoader::new().with_aws_config(config),
        None => ProductionImportLoader::new(),
    };

    let base_location = ".";
    let import_data = match import_loader.load(&args.import, base_location).await {
        Ok(data) => data,
        Err(e) => {
            let error_msg = if e.to_string().contains("AWS") {
                format_aws_error(&e)
            } else {
                format!("Import error: {}", e)
            };
            eprintln!("{}", error_msg);
            return Ok(1);
        }
    };

    let mut output_doc = import_data.doc;
    if let Some(query_str) = &args.query {
        let json_value = yaml_to_json_value(&output_doc)?;

        match jmespath::compile(query_str) {
            Ok(expression) => match expression.search(&json_value) {
                Ok(result) => {
                    let json_result = jmespath_to_json_value(result)?;
                    output_doc = json_to_yaml_value(&json_result)?;
                }
                Err(e) => {
                    eprintln!("JMESPath query execution error: {}", e);
                    return Ok(1);
                }
            },
            Err(e) => {
                eprintln!("Invalid JMESPath query '{}': {}", query_str, e);
                return Ok(1);
            }
        }
    }

    match args.format.as_str() {
        "yaml" => match serde_yaml::to_string(&output_doc) {
            Ok(yaml_str) => print!("{}", yaml_str),
            Err(e) => {
                eprintln!("YAML serialization error: {}", e);
                return Ok(1);
            }
        },
        "json" => match serde_json::to_string_pretty(&yaml_to_json_value(&output_doc)?) {
            Ok(json_str) => println!("{}", json_str),
            Err(e) => {
                eprintln!("JSON serialization error: {}", e);
                return Ok(1);
            }
        },
        _ => {
            eprintln!(
                "Unsupported format: '{}'. Use 'yaml' or 'json'.",
                args.format
            );
            return Ok(1);
        }
    }

    Ok(0)
}

fn yaml_to_json_value(yaml_value: &serde_yaml::Value) -> Result<serde_json::Value> {
    let yaml_str = serde_yaml::to_string(yaml_value)
        .map_err(|e| anyhow!("Failed to serialize YAML value: {}", e))?;

    let json_value: serde_json::Value = serde_yaml::from_str(&yaml_str)
        .map_err(|e| anyhow!("Failed to convert YAML to JSON: {}", e))?;

    Ok(json_value)
}

fn json_to_yaml_value(json_value: &serde_json::Value) -> Result<serde_yaml::Value> {
    let json_str = serde_json::to_string(json_value)
        .map_err(|e| anyhow!("Failed to serialize JSON value: {}", e))?;

    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to convert JSON to YAML: {}", e))?;

    Ok(yaml_value)
}

fn jmespath_to_json_value(variable: std::rc::Rc<Variable>) -> Result<serde_json::Value> {
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
        Variable::Expref(_) => serde_json::Value::Null,
    };

    Ok(json_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jmespath::Variable;
    use serde_json::json;
    use serde_yaml::Value as YamlValue;
    use std::rc::Rc;

    #[test]
    fn test_yaml_to_json_conversion() {
        let yaml_value = serde_yaml::from_str(
            r#"
            name: "test"
            values: [1, 2, 3]
            nested:
              key: "value"
        "#,
        )
        .unwrap();

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
        let original_yaml = serde_yaml::from_str(
            r#"
            database:
              host: "localhost"
              port: 5432
              enabled: true
            features: ["auth", "logging"]
        "#,
        )
        .unwrap();

        let json_value = yaml_to_json_value(&original_yaml).unwrap();
        let final_yaml = json_to_yaml_value(&json_value).unwrap();

        let original_str = serde_yaml::to_string(&original_yaml).unwrap();
        let final_str = serde_yaml::to_string(&final_yaml).unwrap();

        let original_reparsed: serde_yaml::Value = serde_yaml::from_str(&original_str).unwrap();
        let final_reparsed: serde_yaml::Value = serde_yaml::from_str(&final_str).unwrap();

        assert_eq!(original_reparsed, final_reparsed);
    }

    #[test]
    fn test_jmespath_variable_conversion() {
        let string_var = Rc::new(Variable::String("test".to_string()));
        let json_value = jmespath_to_json_value(string_var).unwrap();
        assert_eq!(json_value, json!("test"));

        let num_var = Rc::new(Variable::Number(serde_json::Number::from(42)));
        let json_value = jmespath_to_json_value(num_var).unwrap();
        assert_eq!(json_value, json!(42));

        let bool_var = Rc::new(Variable::Bool(true));
        let json_value = jmespath_to_json_value(bool_var).unwrap();
        assert_eq!(json_value, json!(true));

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
        obj.insert(
            "name".to_string(),
            Rc::new(Variable::String("test".to_string())),
        );
        obj.insert(
            "count".to_string(),
            Rc::new(Variable::Number(serde_json::Number::from(42))),
        );
        obj.insert("enabled".to_string(), Rc::new(Variable::Bool(true)));

        let object_var = Rc::new(Variable::Object(obj));
        let json_value = jmespath_to_json_value(object_var).unwrap();

        assert_eq!(json_value["name"], "test");
        assert_eq!(json_value["count"], 42);
        assert_eq!(json_value["enabled"], true);
    }
}
