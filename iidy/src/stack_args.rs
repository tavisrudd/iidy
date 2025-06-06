use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_yaml::{Mapping, Value};

use crate::preprocess;

#[allow(dead_code)]
#[derive(Debug, Deserialize, Clone, Default)]
pub struct StackArgs {
    #[serde(rename = "StackName")]
    pub stack_name: Option<String>,
    #[serde(rename = "Template")]
    pub template: Option<String>,
    #[serde(rename = "ApprovedTemplateLocation", default)]
    pub approved_template_location: Option<String>,
    #[serde(rename = "Region", default)]
    pub region: Option<String>,
    #[serde(rename = "Profile", default)]
    pub profile: Option<String>,
    #[serde(rename = "Capabilities", default)]
    pub capabilities: Option<Vec<String>>, // TODO enum type
    #[serde(rename = "Tags", default)]
    pub tags: Option<BTreeMap<String, String>>,
    #[serde(rename = "Parameters", default)]
    pub parameters: Option<BTreeMap<String, String>>,
    #[serde(rename = "NotificationARNs", default)]
    pub notification_arns: Option<Vec<String>>,
    #[serde(rename = "AssumeRoleARN", default)]
    pub assume_role_arn: Option<String>,
    #[serde(rename = "ServiceRoleARN", default)]
    pub service_role_arn: Option<String>,
    #[serde(rename = "RoleARN", default)]
    pub role_arn: Option<String>,
    #[serde(rename = "TimeoutInMinutes", default)]
    pub timeout_in_minutes: Option<u32>,
    #[serde(rename = "OnFailure", default)]
    pub on_failure: Option<String>,
    #[serde(rename = "DisableRollback", default)]
    pub disable_rollback: Option<bool>,
    #[serde(rename = "EnableTerminationProtection", default)]
    pub enable_termination_protection: Option<bool>,
    #[serde(rename = "StackPolicy", default)]
    pub stack_policy: Option<Value>,
    #[serde(rename = "ResourceTypes", default)]
    pub resource_types: Option<Vec<String>>,
    #[serde(rename = "UsePreviousTemplate", default)]
    pub use_previous_template: Option<bool>,
    #[serde(rename = "UsePreviousParameterValues", default)]
    pub use_previous_parameter_values: Option<Vec<String>>,
    #[serde(rename = "CommandsBefore", default)]
    pub commands_before: Option<Vec<String>>,

    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

fn resolve_env_map(value: &Value, env: &str, key: &str) -> Result<Value> {
    match value {
        Value::Mapping(m) => {
            let k = Value::String(env.to_string());
            match m.get(&k) {
                Some(Value::String(s)) => Ok(Value::String(s.clone())),
                Some(_) => {
                    bail!("The {key} setting in stack-args.yaml must map environments to strings")
                }
                None => bail!("environment '{env}' not found in {key} map"),
            }
        }
        Value::String(s) => Ok(Value::String(s.clone())),
        Value::Null => Ok(Value::Null),
        _ => bail!("The {key} setting in stack-args.yaml must be a string or an environment map"),
    }
}

fn ensure_environment_tag(root: &mut Mapping, env: &str) {
    let tags_key = Value::String("Tags".to_string());
    let env_key = Value::String("environment".to_string());

    let tags = root
        .entry(tags_key.clone())
        .or_insert_with(|| Value::Mapping(Mapping::new()));
    if let Value::Mapping(map) = tags {
        map.entry(env_key).or_insert(Value::String(env.to_string()));
    }
}

pub fn load_stack_args_file(path: &Path, environment: Option<&str>) -> Result<StackArgs> {
    let contents = fs::read_to_string(path)?;
    load_stack_args_str(&contents, path, environment)
}

pub fn load_stack_args_str(
    content: &str,
    _path: &Path,
    environment: Option<&str>,
) -> Result<StackArgs> {
    // stack-args.yaml is always YAML
    let mut value: Value = serde_yaml::from_str(content)?;

    if let (Some(env), Value::Mapping(map)) = (environment, &mut value) {
        for key in ["Profile", "AssumeRoleARN", "Region"] {
            let map_key = Value::String(key.to_string());
            if let Some(v) = map.get_mut(&map_key) {
                let new_v = resolve_env_map(v, env, key)?;
                *v = new_v;
            }
        }
        ensure_environment_tag(map, env);
    }

    let processed: StackArgs = preprocess::preprocess_sync(value)?;
    Ok(processed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_basic_args() {
        let yaml = "StackName: test\nTemplate: foo.yaml\n";
        let result =
            load_stack_args_str(yaml, Path::new("test.yaml"), Some("dev")).expect("failed to load");
        assert_eq!(result.stack_name.as_deref(), Some("test"));
        assert_eq!(result.template.as_deref(), Some("foo.yaml"));
        assert_eq!(
            result.tags.unwrap().get("environment").map(String::as_str),
            Some("dev")
        );
    }

    #[test]
    fn resolve_environment_map() {
        let yaml = r#"
Profile: default
Region:
  dev: us-east-1
  prod: us-west-2
StackName: s
Template: t
"#;
        let result = load_stack_args_str(yaml, Path::new("test.yaml"), Some("prod")).unwrap();
        assert_eq!(result.region.as_deref(), Some("us-west-2"));
        assert_eq!(result.profile.as_deref(), Some("default"));
    }

    #[test]
    fn test_yaml_preprocessing_integration() {
        // Test that our YAML preprocessing system is being used in stack args parsing
        let yaml = r#"
StackName: !$join ["-", ["my-app", "production"]]
Template: template.yaml
Region: us-west-2
"#;
        let result = load_stack_args_str(yaml, Path::new("test.yaml"), Some("prod"));
        
        // Should succeed even with custom tags (currently they get converted to null)
        assert!(result.is_ok(), "Stack args with custom tags should parse successfully");
        
        let stack_args = result.unwrap();
        // Currently custom tags become null since AST resolution isn't implemented yet
        // But parsing should succeed
        assert_eq!(stack_args.template.as_deref(), Some("template.yaml"));
        assert_eq!(stack_args.region.as_deref(), Some("us-west-2"));
    }
}
