use std::rc::Rc;

use anyhow::{Result, anyhow};
use jmespath::Variable;

/// Apply a JMESPath expression to a YAML value.
///
/// Converts the value to JSON, compiles and executes the JMESPath expression,
/// then converts the result back to YAML.
pub fn apply_jmespath_query(
    value: &serde_yaml::Value,
    expression: &str,
) -> Result<serde_yaml::Value> {
    let json_value = yaml_to_json_value(value)?;

    let compiled = jmespath::compile(expression).map_err(|e| {
        // The JMESPath crate's error display includes multi-line caret output;
        // collapse to a single line for embedding in our error format.
        let msg = e.to_string().lines().next().unwrap_or_default().to_string();
        anyhow!("Invalid JMESPath expression '{}': {}", expression, msg)
    })?;

    let result = compiled.search(&json_value).map_err(|e| {
        let msg = e.to_string().lines().next().unwrap_or_default().to_string();
        anyhow!("JMESPath query '{}' failed: {}", expression, msg)
    })?;

    let json_result = jmespath_to_json_value(result)?;
    json_to_yaml_value(&json_result)
}

pub fn yaml_to_json_value(yaml_value: &serde_yaml::Value) -> Result<serde_json::Value> {
    let yaml_str = serde_yaml::to_string(yaml_value)
        .map_err(|e| anyhow!("Failed to serialize YAML value: {}", e))?;

    let json_value: serde_json::Value = serde_yaml::from_str(&yaml_str)
        .map_err(|e| anyhow!("Failed to convert YAML to JSON: {}", e))?;

    Ok(json_value)
}

pub fn json_to_yaml_value(json_value: &serde_json::Value) -> Result<serde_yaml::Value> {
    let json_str = serde_json::to_string(json_value)
        .map_err(|e| anyhow!("Failed to serialize JSON value: {}", e))?;

    let yaml_value: serde_yaml::Value = serde_yaml::from_str(&json_str)
        .map_err(|e| anyhow!("Failed to convert JSON to YAML: {}", e))?;

    Ok(yaml_value)
}

fn jmespath_to_json_value(variable: Rc<Variable>) -> Result<serde_json::Value> {
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
