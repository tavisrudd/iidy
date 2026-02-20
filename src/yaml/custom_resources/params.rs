use std::collections::HashMap;

use anyhow::{Result, anyhow};
use serde_json::Value as JsonValue;
use serde_yaml::Value;

#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub default: Option<Value>,
    pub param_type: Option<String>,
    pub allowed_values: Option<Vec<Value>>,
    pub allowed_pattern: Option<String>,
    pub schema: Option<Value>,
    pub is_global: bool,
}

/// Parse `$params` from a `Value::Sequence` of mappings.
pub fn parse_params(value: &Value) -> Result<Vec<ParamDef>> {
    let seq = value
        .as_sequence()
        .ok_or_else(|| anyhow!("$params must be a sequence"))?;

    let mut params = Vec::with_capacity(seq.len());
    for entry in seq {
        let map = entry
            .as_mapping()
            .ok_or_else(|| anyhow!("Each $params entry must be a mapping"))?;

        let name = map
            .get(Value::String("Name".into()))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Each $params entry must have a 'Name' string field"))?
            .to_string();

        let default = map.get(Value::String("Default".into())).cloned();

        let param_type = map
            .get(Value::String("Type".into()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let allowed_values = map
            .get(Value::String("AllowedValues".into()))
            .and_then(|v| v.as_sequence())
            .cloned();

        let allowed_pattern = map
            .get(Value::String("AllowedPattern".into()))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let schema = map.get(Value::String("Schema".into())).cloned();

        let is_global = map
            .get(Value::String("$global".into()))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        params.push(ParamDef {
            name,
            default,
            param_type,
            allowed_values,
            allowed_pattern,
            schema,
            is_global,
        });
    }

    Ok(params)
}

/// Merge param defaults with provided values.
/// For each ParamDef: use provided value if present, else default if present.
pub fn merge_params(
    defs: &[ParamDef],
    provided: &HashMap<String, Value>,
) -> HashMap<String, Value> {
    let mut merged = HashMap::new();
    for def in defs {
        if let Some(val) = provided.get(&def.name) {
            merged.insert(def.name.clone(), val.clone());
        } else if let Some(ref default) = def.default {
            merged.insert(def.name.clone(), default.clone());
        }
    }
    merged
}

/// Validate merged params against their definitions.
pub fn validate_params(
    defs: &[ParamDef],
    merged: &HashMap<String, Value>,
    resource_name: &str,
) -> Result<()> {
    for def in defs {
        let Some(value) = merged.get(&def.name) else {
            return Err(anyhow!(
                "Missing required parameter '{}' in {}",
                def.name,
                resource_name
            ));
        };

        if let Some(ref allowed) = def.allowed_values {
            if !allowed.contains(value) {
                return Err(anyhow!(
                    "Parameter validation error for '{}' in {}: value not in AllowedValues",
                    def.name,
                    resource_name
                ));
            }
        }

        if let Some(ref pattern) = def.allowed_pattern {
            let re = regex::Regex::new(pattern).map_err(|e| {
                anyhow!(
                    "Invalid AllowedPattern regex for '{}' in {}: {}",
                    def.name,
                    resource_name,
                    e
                )
            })?;
            match value.as_str() {
                Some(s) if re.is_match(s) => {}
                _ => {
                    return Err(anyhow!(
                        "Invalid value for '{}' in {}. AllowedPattern: {}",
                        def.name,
                        resource_name,
                        pattern
                    ));
                }
            }
        }

        if let Some(ref type_name) = def.param_type {
            validate_type(type_name, value, &def.name, resource_name)?;
        }

        if let Some(ref schema) = def.schema {
            validate_schema(schema, value, &def.name, resource_name)?;
        }
    }

    Ok(())
}

fn validate_type(
    type_name: &str,
    value: &Value,
    param_name: &str,
    resource_name: &str,
) -> Result<()> {
    match type_name {
        "String" | "string" => {
            if !value.is_string() {
                return Err(anyhow!(
                    "Invalid parameter value for '{}' in {}. Expected a String",
                    param_name,
                    resource_name
                ));
            }
        }
        "Number" | "number" => {
            if !value.is_number() {
                return Err(anyhow!(
                    "Invalid parameter value for '{}' in {}. Expected a Number",
                    param_name,
                    resource_name
                ));
            }
        }
        "Object" | "object" => {
            if !value.is_mapping() {
                return Err(anyhow!(
                    "Invalid parameter value for '{}' in {}. Expected an Object",
                    param_name,
                    resource_name
                ));
            }
        }
        t if t.starts_with("AWS:") || t.starts_with("List<") || t == "CommaDelimitedList" => {
            // AWS-specific types -- skip validation (CFN handles these)
        }
        _ => {
            return Err(anyhow!("Unknown parameter type: {}", type_name));
        }
    }
    Ok(())
}

fn yaml_to_json(value: &Value) -> Option<JsonValue> {
    if contains_tagged_value(value) {
        return None;
    }
    let yaml_str = serde_yaml::to_string(value).ok()?;
    serde_yaml::from_str::<JsonValue>(&yaml_str).ok()
}

fn contains_tagged_value(value: &Value) -> bool {
    match value {
        Value::Tagged(_) => true,
        Value::Sequence(seq) => seq.iter().any(contains_tagged_value),
        Value::Mapping(map) => map
            .iter()
            .any(|(k, v)| contains_tagged_value(k) || contains_tagged_value(v)),
        _ => false,
    }
}

fn validate_schema(
    schema: &Value,
    value: &Value,
    param_name: &str,
    resource_name: &str,
) -> Result<()> {
    let Some(json_schema) = yaml_to_json(schema) else {
        return Err(anyhow!(
            "Schema definition for '{}' in {} contains CloudFormation tags and cannot be parsed",
            param_name,
            resource_name
        ));
    };

    // Skip validation if the value contains CFN intrinsic functions (tagged values).
    // Schema validation only applies to plain data values.
    let Some(json_value) = yaml_to_json(value) else {
        return Ok(());
    };

    let validator = jsonschema::validator_for(&json_schema).map_err(|e| {
        anyhow!(
            "Invalid JSON Schema for parameter '{}' in {}: {}",
            param_name,
            resource_name,
            e
        )
    })?;

    if let Err(error) = validator.validate(&json_value) {
        return Err(anyhow!(
            "Schema validation failed for parameter '{}' in {}: {}",
            param_name,
            resource_name,
            error
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value;

    fn make_param_yaml(yaml_str: &str) -> Value {
        serde_yaml::from_str(yaml_str).unwrap()
    }

    #[test]
    fn parse_params_all_fields() {
        let yaml = make_param_yaml(
            r#"
- Name: QueueLabel
  Type: String
- Name: AlarmPeriod
  Type: Number
  Default: 60
- Name: Priority
  AllowedValues: [P1, P2, P3]
  Default: P3
"#,
        );

        let params = parse_params(&yaml).unwrap();
        assert_eq!(params.len(), 3);

        assert_eq!(params[0].name, "QueueLabel");
        assert_eq!(params[0].param_type.as_deref(), Some("String"));
        assert!(params[0].default.is_none());

        assert_eq!(params[1].name, "AlarmPeriod");
        assert_eq!(params[1].param_type.as_deref(), Some("Number"));
        assert_eq!(params[1].default, Some(Value::Number(60.into())));

        assert_eq!(params[2].name, "Priority");
        assert!(params[2].allowed_values.is_some());
        assert_eq!(params[2].allowed_values.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn parse_params_minimal() {
        let yaml = make_param_yaml(
            r#"
- Name: Foo
"#,
        );

        let params = parse_params(&yaml).unwrap();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].name, "Foo");
        assert!(params[0].default.is_none());
        assert!(params[0].param_type.is_none());
        assert!(!params[0].is_global);
    }

    #[test]
    fn parse_params_global_flag() {
        let yaml = make_param_yaml(
            r#"
- Name: RoleName
  Default: my-role
  $global: true
"#,
        );

        let params = parse_params(&yaml).unwrap();
        assert!(params[0].is_global);
    }

    #[test]
    fn parse_params_missing_name_errors() {
        let yaml = make_param_yaml(
            r#"
- Type: String
"#,
        );

        let err = parse_params(&yaml).unwrap_err();
        assert!(err.to_string().contains("Name"));
    }

    #[test]
    fn parse_params_not_sequence_errors() {
        let yaml = make_param_yaml("Name: Foo");
        let err = parse_params(&yaml).unwrap_err();
        assert!(err.to_string().contains("sequence"));
    }

    #[test]
    fn merge_params_with_defaults_and_overrides() {
        let defs = vec![
            ParamDef {
                name: "A".into(),
                default: Some(Value::String("default-a".into())),
                param_type: None,
                allowed_values: None,
                allowed_pattern: None,
                schema: None,
                is_global: false,
            },
            ParamDef {
                name: "B".into(),
                default: None,
                param_type: None,
                allowed_values: None,
                allowed_pattern: None,
                schema: None,
                is_global: false,
            },
            ParamDef {
                name: "C".into(),
                default: Some(Value::Number(42.into())),
                param_type: None,
                allowed_values: None,
                allowed_pattern: None,
                schema: None,
                is_global: false,
            },
        ];

        let mut provided = HashMap::new();
        provided.insert("A".into(), Value::String("override-a".into()));

        let merged = merge_params(&defs, &provided);

        // A: provided overrides default
        assert_eq!(merged["A"], Value::String("override-a".into()));
        // B: no default, no provided -> absent
        assert!(!merged.contains_key("B"));
        // C: default used
        assert_eq!(merged["C"], Value::Number(42.into()));
    }

    #[test]
    fn validate_missing_required_param() {
        let defs = vec![ParamDef {
            name: "Required".into(),
            default: None,
            param_type: None,
            allowed_values: None,
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let merged = HashMap::new();
        let err = validate_params(&defs, &merged, "MyResource").unwrap_err();
        assert!(err.to_string().contains("Missing required parameter"));
        assert!(err.to_string().contains("Required"));
        assert!(err.to_string().contains("MyResource"));
    }

    #[test]
    fn validate_type_string_pass() {
        let defs = vec![ParamDef {
            name: "Name".into(),
            default: None,
            param_type: Some("String".into()),
            allowed_values: None,
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Name".into(), Value::String("hello".into()));

        assert!(validate_params(&defs, &merged, "Res").is_ok());
    }

    #[test]
    fn validate_type_string_fail() {
        let defs = vec![ParamDef {
            name: "Name".into(),
            default: None,
            param_type: Some("String".into()),
            allowed_values: None,
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Name".into(), Value::Number(42.into()));

        let err = validate_params(&defs, &merged, "Res").unwrap_err();
        assert!(err.to_string().contains("Expected a String"));
    }

    #[test]
    fn validate_type_number_pass() {
        let defs = vec![ParamDef {
            name: "Count".into(),
            default: None,
            param_type: Some("Number".into()),
            allowed_values: None,
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Count".into(), Value::Number(5.into()));

        assert!(validate_params(&defs, &merged, "Res").is_ok());
    }

    #[test]
    fn validate_type_number_fail() {
        let defs = vec![ParamDef {
            name: "Count".into(),
            default: None,
            param_type: Some("Number".into()),
            allowed_values: None,
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Count".into(), Value::String("five".into()));

        let err = validate_params(&defs, &merged, "Res").unwrap_err();
        assert!(err.to_string().contains("Expected a Number"));
    }

    #[test]
    fn validate_allowed_values_pass() {
        let defs = vec![ParamDef {
            name: "Env".into(),
            default: None,
            param_type: None,
            allowed_values: Some(vec![
                Value::String("dev".into()),
                Value::String("prod".into()),
            ]),
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Env".into(), Value::String("dev".into()));

        assert!(validate_params(&defs, &merged, "Res").is_ok());
    }

    #[test]
    fn validate_allowed_values_fail() {
        let defs = vec![ParamDef {
            name: "Env".into(),
            default: None,
            param_type: None,
            allowed_values: Some(vec![
                Value::String("dev".into()),
                Value::String("prod".into()),
            ]),
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Env".into(), Value::String("staging".into()));

        let err = validate_params(&defs, &merged, "Res").unwrap_err();
        assert!(err.to_string().contains("AllowedValues"));
    }

    #[test]
    fn validate_allowed_pattern_pass() {
        let defs = vec![ParamDef {
            name: "Id".into(),
            default: None,
            param_type: None,
            allowed_values: None,
            allowed_pattern: Some("^[a-z]+$".into()),
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Id".into(), Value::String("abc".into()));

        assert!(validate_params(&defs, &merged, "Res").is_ok());
    }

    #[test]
    fn validate_allowed_pattern_fail() {
        let defs = vec![ParamDef {
            name: "Id".into(),
            default: None,
            param_type: None,
            allowed_values: None,
            allowed_pattern: Some("^[a-z]+$".into()),
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Id".into(), Value::String("ABC123".into()));

        let err = validate_params(&defs, &merged, "Res").unwrap_err();
        assert!(err.to_string().contains("AllowedPattern"));
    }

    #[test]
    fn validate_aws_type_skipped() {
        let defs = vec![ParamDef {
            name: "Vpc".into(),
            default: None,
            param_type: Some("AWS::EC2::VPC::Id".into()),
            allowed_values: None,
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("Vpc".into(), Value::String("vpc-123".into()));

        assert!(validate_params(&defs, &merged, "Res").is_ok());
    }

    #[test]
    fn validate_unknown_type_errors() {
        let defs = vec![ParamDef {
            name: "X".into(),
            default: None,
            param_type: Some("FooBar".into()),
            allowed_values: None,
            allowed_pattern: None,
            schema: None,
            is_global: false,
        }];

        let mut merged = HashMap::new();
        merged.insert("X".into(), Value::String("val".into()));

        let err = validate_params(&defs, &merged, "Res").unwrap_err();
        assert!(err.to_string().contains("Unknown parameter type"));
    }

    #[test]
    fn validate_schema_object_pass() {
        let schema: Value = serde_yaml::from_str(
            r#"
type: object
required: [host, port]
properties:
  host:
    type: string
  port:
    type: integer
    minimum: 1
    maximum: 65535
"#,
        )
        .unwrap();

        let defs = vec![ParamDef {
            name: "Config".into(),
            default: None,
            param_type: None,
            allowed_values: None,
            allowed_pattern: None,
            schema: Some(schema),
            is_global: false,
        }];

        let value: Value = serde_yaml::from_str(
            r#"
host: db.example.com
port: 5432
"#,
        )
        .unwrap();

        let mut merged = HashMap::new();
        merged.insert("Config".into(), value);

        assert!(validate_params(&defs, &merged, "Res").is_ok());
    }

    #[test]
    fn validate_schema_object_fail_missing_required() {
        let schema: Value = serde_yaml::from_str(
            r#"
type: object
required: [host, port]
properties:
  host:
    type: string
  port:
    type: integer
"#,
        )
        .unwrap();

        let defs = vec![ParamDef {
            name: "Config".into(),
            default: None,
            param_type: None,
            allowed_values: None,
            allowed_pattern: None,
            schema: Some(schema),
            is_global: false,
        }];

        let value: Value = serde_yaml::from_str("host: db.example.com").unwrap();

        let mut merged = HashMap::new();
        merged.insert("Config".into(), value);

        let err = validate_params(&defs, &merged, "Res").unwrap_err();
        assert!(err.to_string().contains("Schema validation failed"));
    }

    #[test]
    fn validate_schema_array_with_items() {
        let schema: Value = serde_yaml::from_str(
            r#"
type: array
items:
  type: string
  pattern: "^arn:aws:"
minItems: 1
"#,
        )
        .unwrap();

        let defs = vec![ParamDef {
            name: "Arns".into(),
            default: None,
            param_type: None,
            allowed_values: None,
            allowed_pattern: None,
            schema: Some(schema),
            is_global: false,
        }];

        let value: Value = serde_yaml::from_str(
            r#"
- arn:aws:s3:::my-bucket
- arn:aws:sqs:us-east-1:123456:my-queue
"#,
        )
        .unwrap();

        let mut merged = HashMap::new();
        merged.insert("Arns".into(), value);

        assert!(validate_params(&defs, &merged, "Res").is_ok());
    }

    #[test]
    fn validate_schema_array_fail_wrong_pattern() {
        let schema: Value = serde_yaml::from_str(
            r#"
type: array
items:
  type: string
  pattern: "^arn:aws:"
"#,
        )
        .unwrap();

        let defs = vec![ParamDef {
            name: "Arns".into(),
            default: None,
            param_type: None,
            allowed_values: None,
            allowed_pattern: None,
            schema: Some(schema),
            is_global: false,
        }];

        let value: Value = serde_yaml::from_str(
            r#"
- not-an-arn
"#,
        )
        .unwrap();

        let mut merged = HashMap::new();
        merged.insert("Arns".into(), value);

        let err = validate_params(&defs, &merged, "Res").unwrap_err();
        assert!(err.to_string().contains("Schema validation failed"));
    }
}
