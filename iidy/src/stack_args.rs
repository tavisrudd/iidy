use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Result, bail};
use serde::Deserialize;
use serde_yaml::{Mapping, Value};

use crate::{cli::YamlSpec, yaml::preprocess_yaml, cli_context::CliContext, aws::AwsSettings};

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

/// Load stack args with full CLI context (new iidy-js compatible interface)
pub async fn load_stack_args_from_context(
    cli_context: &CliContext,
    _filter_keys: Vec<String>,
) -> Result<StackArgs> {
    let path = Path::new(&cli_context.argsfile);
    let environment = Some(cli_context.environment());
    
    // For now, delegate to the existing implementation
    // TODO: Implement full iidy-js compatibility with:
    // - AWS credential configuration
    // - $envValues injection  
    // - Global configuration via SSM
    // - CommandsBefore processing
    // - Multi-pass preprocessing
    load_stack_args_file(path, environment)
}

/// Load stack args with full iidy-js compatibility including AWS credential merging and $envValues injection
pub async fn load_stack_args_with_context(
    argsfile: &str,
    environment: Option<&str>,
    command: &[String],
    cli_aws_settings: &AwsSettings,
) -> Result<StackArgs> {
    use crate::aws::config_from_merged_settings;
    
    let path = Path::new(argsfile);
    let contents = fs::read_to_string(path)?;
    
    // Use YAML v1.1 spec for CloudFormation compatibility
    let yaml_spec = YamlSpec::V11;
    let base_location = path.to_string_lossy();
    
    // Initial YAML preprocessing
    let mut value = preprocess_yaml(&contents, &base_location, &yaml_spec).await?;
    
    // Resolve environment maps for AWS credential fields BEFORE AWS config
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
    
    // Extract AWS settings from argsfile
    let argsfile_aws_settings = AwsSettings {
        profile: value.get("Profile").and_then(|v| v.as_str()).map(|s| s.to_string()),
        region: value.get("Region").and_then(|v| v.as_str()).map(|s| s.to_string()),
        assume_role_arn: value.get("AssumeRoleARN").and_then(|v| v.as_str()).map(|s| s.to_string()),
    };
    
    // Merge AWS settings (CLI overrides argsfile)
    let merged_aws_settings = argsfile_aws_settings.merge_with(cli_aws_settings);
    
    // Configure AWS BEFORE preprocessing (enables $imports with AWS calls)
    let aws_config = config_from_merged_settings(&merged_aws_settings).await?;
    let current_region = aws_config.region()
        .map(|r| r.as_ref())
        .unwrap_or("us-east-1"); // Default fallback
    
    // Create and inject $envValues
    let env_values = create_env_values(
        environment,
        command,
        current_region,
        merged_aws_settings.profile.as_deref(),
    );
    inject_env_values(&mut value, env_values);
    
    // Final preprocessing pass with AWS config available
    let final_value = preprocess_yaml(
        &serde_yaml::to_string(&value)?,
        &base_location,
        &yaml_spec
    ).await?;
    
    // Deserialize to StackArgs
    let mut stack_args: StackArgs = serde_yaml::from_value(final_value)?;
    
    // Apply global configuration from SSM parameter store
    apply_global_configuration(&mut stack_args, &aws_config).await?;
    
    Ok(stack_args)
}

pub fn load_stack_args_str(
    content: &str,
    path: &Path,
    environment: Option<&str>,
) -> Result<StackArgs> {
    // Create a tokio runtime for async preprocessing
    let rt = tokio::runtime::Runtime::new()?;

    // Use YAML v1.1 spec for CloudFormation compatibility
    let yaml_spec = YamlSpec::V11;

    // Get base location from path for relative imports
    let base_location = path.to_string_lossy();

    // Process the YAML with full preprocessing pipeline
    let mut value = rt.block_on(preprocess_yaml(content, &base_location, &yaml_spec))?;

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

    // Deserialize to StackArgs
    let processed: StackArgs = serde_yaml::from_value(value)?;
    Ok(processed)
}

/// Create $envValues object matching iidy-js structure for template compatibility
fn create_env_values(
    environment: Option<&str>,
    command: &[String],
    current_region: &str,
    current_profile: Option<&str>,
) -> Value {
    use std::collections::BTreeMap;
    
    let mut env_values = BTreeMap::new();
    
    // Legacy bare values (TODO: deprecate in iidy-js compatibility)
    env_values.insert("region".to_string(), Value::String(current_region.to_string()));
    if let Some(env) = environment {
        env_values.insert("environment".to_string(), Value::String(env.to_string()));
    }
    
    // New namespaced structure (iidy.*)
    let mut iidy_values = BTreeMap::new();
    iidy_values.insert("command".to_string(), Value::String(command.join(" ")));
    if let Some(env) = environment {
        iidy_values.insert("environment".to_string(), Value::String(env.to_string()));
    }
    iidy_values.insert("region".to_string(), Value::String(current_region.to_string()));
    if let Some(profile) = current_profile {
        iidy_values.insert("profile".to_string(), Value::String(profile.to_string()));
    }
    
    env_values.insert("iidy".to_string(), Value::Mapping(
        iidy_values.into_iter()
            .map(|(k, v)| (Value::String(k), v))
            .collect()
    ));
    
    Value::Mapping(
        env_values.into_iter()
            .map(|(k, v)| (Value::String(k), v))
            .collect()
    )
}

/// Inject $envValues into YAML data before preprocessing
fn inject_env_values(argsdata: &mut Value, env_values: Value) {
    if let Value::Mapping(map) = argsdata {
        // Merge with existing $envValues if present, new values take precedence
        let env_values_key = Value::String("$envValues".to_string());
        match map.get(&env_values_key) {
            Some(existing) => {
                if let Value::Mapping(existing_map) = existing {
                    if let Value::Mapping(new_map) = &env_values {
                        let mut merged_map = existing_map.clone();
                        for (k, v) in new_map {
                            merged_map.insert(k.clone(), v.clone());
                        }
                        map.insert(env_values_key, Value::Mapping(merged_map));
                    } else {
                        map.insert(env_values_key, env_values);
                    }
                } else {
                    map.insert(env_values_key, env_values);
                }
            }
            None => {
                map.insert(env_values_key, env_values);
            }
        }
    }
}

/// Apply global configuration from SSM parameter store, matching iidy-js applyGlobalConfiguration
pub async fn apply_global_configuration(
    args: &mut StackArgs,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    let ssm = aws_sdk_ssm::Client::new(aws_config);
    
    match ssm
        .get_parameters_by_path()
        .path("/iidy/")
        .with_decryption(true)
        .send()
        .await
    {
        Ok(response) => {
            if let Some(parameters) = response.parameters {
                for parameter in parameters {
                    if let (Some(name), Some(value)) = (parameter.name, parameter.value) {
                        match name.as_str() {
                            "/iidy/default-notification-arn" => {
                                apply_sns_notification_global_configuration(args, &value, aws_config).await?;
                            }
                            "/iidy/disable-template-approval" => {
                                if value.to_lowercase() == "true" && args.approved_template_location.is_some() {
                                    eprintln!("Disabling template approval based on global {} parameter store configuration", name);
                                    args.approved_template_location = None;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        Err(_) => {
            // Debug message would go here - failed to fetch global configuration
            // We silently continue if SSM is not accessible
        }
    }
    
    Ok(())
}

/// Apply SNS notification global configuration
async fn apply_sns_notification_global_configuration(
    args: &mut StackArgs,
    topic_arn: &str,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    let sns = aws_sdk_sns::Client::new(aws_config);
    
    match sns
        .get_topic_attributes()
        .topic_arn(topic_arn)
        .send()
        .await
    {
        Ok(_) => {
            // Topic exists, add it to notification ARNs
            let notification_arns = args.notification_arns.get_or_insert_with(Vec::new);
            if !notification_arns.contains(&topic_arn.to_string()) {
                notification_arns.push(topic_arn.to_string());
            }
        }
        Err(_) => {
            eprintln!("iidy's default NotificationARN set in this region is invalid: {}", topic_arn);
        }
    }
    
    Ok(())
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
        assert!(
            result.is_ok(),
            "Stack args with custom tags should parse successfully"
        );

        let stack_args = result.unwrap();
        // Currently custom tags become null since AST resolution isn't implemented yet
        // But parsing should succeed
        assert_eq!(stack_args.template.as_deref(), Some("template.yaml"));
        assert_eq!(stack_args.region.as_deref(), Some("us-west-2"));
    }

    #[test]
    fn test_create_env_values() {
        let env_values = create_env_values(
            Some("production"),
            &["create-stack".to_string()],
            "us-west-2",
            Some("prod-profile"),
        );

        // Verify structure matches iidy-js
        if let Value::Mapping(map) = env_values {
            // Legacy values
            assert_eq!(map.get(&Value::String("region".to_string())), 
                      Some(&Value::String("us-west-2".to_string())));
            assert_eq!(map.get(&Value::String("environment".to_string())), 
                      Some(&Value::String("production".to_string())));

            // Namespaced iidy values
            if let Some(Value::Mapping(iidy_map)) = map.get(&Value::String("iidy".to_string())) {
                assert_eq!(iidy_map.get(&Value::String("command".to_string())), 
                          Some(&Value::String("create-stack".to_string())));
                assert_eq!(iidy_map.get(&Value::String("environment".to_string())), 
                          Some(&Value::String("production".to_string())));
                assert_eq!(iidy_map.get(&Value::String("region".to_string())), 
                          Some(&Value::String("us-west-2".to_string())));
                assert_eq!(iidy_map.get(&Value::String("profile".to_string())), 
                          Some(&Value::String("prod-profile".to_string())));
            } else {
                panic!("Expected iidy namespace in $envValues");
            }
        } else {
            panic!("Expected $envValues to be a mapping");
        }
    }

    #[test]
    fn test_inject_env_values() {
        let mut argsdata = serde_yaml::from_str::<Value>(r#"
StackName: test-stack
Template: template.yaml
"#).unwrap();

        let env_values = create_env_values(
            Some("dev"),
            &["update-stack".to_string()],
            "us-east-1", 
            None,
        );

        inject_env_values(&mut argsdata, env_values);

        // Verify $envValues was injected
        if let Value::Mapping(map) = &argsdata {
            assert!(map.contains_key(&Value::String("$envValues".to_string())));
            
            if let Some(Value::Mapping(env_map)) = map.get(&Value::String("$envValues".to_string())) {
                assert_eq!(env_map.get(&Value::String("environment".to_string())), 
                          Some(&Value::String("dev".to_string())));
                assert_eq!(env_map.get(&Value::String("region".to_string())), 
                          Some(&Value::String("us-east-1".to_string())));
            }
        } else {
            panic!("Expected argsdata to be a mapping");
        }
    }
}
