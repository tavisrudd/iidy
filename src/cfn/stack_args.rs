use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_yaml::{Mapping, Value};

use crate::aws::config_from_merged_settings;
use crate::{aws::AwsSettings, cfn::CfnOperation, cli::YamlSpec, yaml::preprocess_yaml};

#[allow(dead_code)]
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
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

/// Load stack args with full iidy-js compatibility including AWS credential merging and $envValues injection
///
/// Returns both the parsed StackArgs and the merged AWS config (from CLI + stack-args.yaml).
/// The returned AWS config should be used to create the CfnContext to ensure consistency
/// between preprocessing and CloudFormation operations.
pub async fn load_stack_args(
    argsfile: &str,
    environment: &str,
    operation: &CfnOperation,
    cli_aws_settings: &AwsSettings,
) -> Result<(
    StackArgs,
    aws_config::SdkConfig,
    crate::aws::CredentialSourceStack,
)> {
    let path = Path::new(argsfile);
    let contents = tokio::fs::read_to_string(path).await?;

    // Use YAML v1.1 spec for CloudFormation compatibility
    let yaml_spec = YamlSpec::V11;
    let base_location = path.to_string_lossy();

    // Initial YAML preprocessing
    let mut value = preprocess_yaml(&contents, &base_location, &yaml_spec).await?;

    // Resolve environment maps for AWS credential fields BEFORE AWS config
    if let Value::Mapping(map) = &mut value {
        for key in ["Profile", "AssumeRoleARN", "Region"] {
            let map_key = Value::String(key.to_string());
            if let Some(v) = map.get_mut(&map_key) {
                let new_v = resolve_env_map(v, environment, key)?;
                *v = new_v;
            }
        }
        ensure_environment_tag(map, environment);
    }

    // Extract AWS settings from argsfile
    let argsfile_aws_settings = AwsSettings {
        profile: value
            .get("Profile")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        region: value
            .get("Region")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        assume_role_arn: value
            .get("AssumeRoleARN")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };

    // Merge AWS settings (CLI overrides argsfile)
    let merged_aws_settings = AwsSettings {
        profile: cli_aws_settings
            .profile
            .clone()
            .or_else(|| argsfile_aws_settings.profile.clone()),
        region: cli_aws_settings
            .region
            .clone()
            .or_else(|| argsfile_aws_settings.region.clone()),
        assume_role_arn: cli_aws_settings
            .assume_role_arn
            .clone()
            .or_else(|| argsfile_aws_settings.assume_role_arn.clone()),
    };

    // Create detection context with unmerged settings for provenance tracking
    let detection_ctx = crate::aws::CredentialDetectionContext {
        cli_profile: cli_aws_settings.profile.clone(),
        stack_args_profile: argsfile_aws_settings.profile.clone(),
        cli_assume_role_arn: cli_aws_settings.assume_role_arn.clone(),
        stack_args_assume_role_arn: argsfile_aws_settings.assume_role_arn.clone(),
    };

    // Configure AWS BEFORE preprocessing (enables $imports with AWS calls)
    let (aws_config, credential_sources) =
        config_from_merged_settings(&merged_aws_settings, &detection_ctx).await?;

    // Validate that a region is configured (needed for AWS API calls in $imports and CommandsBefore)
    let current_region = aws_config.region().map(|r| r.as_ref()).ok_or_else(|| {
        anyhow::anyhow!(
            "No AWS region configured. Please specify a region via:\n\
             - CLI flag: --region us-east-1\n\
             - Stack args: Region: us-east-1\n\
             - Environment variable: AWS_REGION or AWS_DEFAULT_REGION\n\
             - AWS config file: ~/.aws/config"
        )
    })?;

    // Create and inject $envValues
    let env_values = create_env_values(
        Some(environment),
        operation,
        current_region,
        merged_aws_settings.profile.as_deref(),
    );
    inject_env_values(&mut value, env_values);

    // Handle CommandsBefore if present and command supports it
    if let Some(commands_before) = value.get("CommandsBefore").cloned() {
        if should_process_commands_before(operation) {
            // Two-pass processing for CommandsBefore
            // Pass 1: Process without CommandsBefore to get full context for handlebars
            let mut value_pass1 = value.clone();
            if let Value::Mapping(map) = &mut value_pass1 {
                map.remove(Value::String("CommandsBefore".to_string()));
            }

            // Process pass 1 to get complete context
            let pass1_yaml = serde_yaml::to_string(&value_pass1)?;
            let pass1_value = preprocess_yaml(&pass1_yaml, &base_location, &yaml_spec).await?;
            let stack_args_pass1: StackArgs = serde_yaml::from_value(pass1_value)?;

            // Execute CommandsBefore with full context
            let processed_commands = process_commands_before(
                commands_before,
                &stack_args_pass1,
                Some(environment),
                operation,
                current_region,
                merged_aws_settings.profile.as_deref(),
                path,
            )?;

            // Update value with processed commands
            if let Value::Mapping(map) = &mut value {
                map.insert(
                    Value::String("CommandsBefore".to_string()),
                    Value::Sequence(processed_commands.into_iter().map(Value::String).collect()),
                );
            }
        } else {
            // Remove CommandsBefore for operations that don't support it
            if let Value::Mapping(map) = &mut value {
                map.remove(Value::String("CommandsBefore".to_string()));
            }
        }
    }

    // Final preprocessing pass with AWS config available
    let final_value =
        preprocess_yaml(&serde_yaml::to_string(&value)?, &base_location, &yaml_spec).await?;

    // Deserialize to StackArgs
    let mut stack_args: StackArgs = serde_yaml::from_value(final_value)?;

    // Apply global configuration from SSM parameter store
    // TODO disable behind feature flag
    apply_global_configuration(&mut stack_args, &aws_config).await?;

    Ok((stack_args, aws_config, credential_sources))
}

/// Create $envValues object matching iidy-js structure for template compatibility
fn create_env_values(
    environment: Option<&str>,
    operation: &CfnOperation,
    current_aws_region: &str,
    current_aws_profile: Option<&str>,
) -> Value {
    use std::collections::BTreeMap;

    let mut env_values = BTreeMap::new();

    // Legacy bare values (TODO: deprecate in iidy-js compatibility)
    env_values.insert(
        "region".to_string(),
        Value::String(current_aws_region.to_string()),
    );
    if let Some(env) = environment {
        env_values.insert("environment".to_string(), Value::String(env.to_string()));
    }

    // New namespaced structure (iidy.*)
    let mut iidy_values = BTreeMap::new();
    iidy_values.insert(
        "command".to_string(),
        Value::String(operation.as_str().to_string()),
    );
    if let Some(env) = environment {
        iidy_values.insert("environment".to_string(), Value::String(env.to_string()));
    }
    iidy_values.insert(
        "region".to_string(),
        Value::String(current_aws_region.to_string()),
    );
    if let Some(profile) = current_aws_profile {
        iidy_values.insert("profile".to_string(), Value::String(profile.to_string()));
    }

    env_values.insert(
        "iidy".to_string(),
        Value::Mapping(
            iidy_values
                .into_iter()
                .map(|(k, v)| (Value::String(k), v))
                .collect(),
        ),
    );

    Value::Mapping(
        env_values
            .into_iter()
            .map(|(k, v)| (Value::String(k), v))
            .collect(),
    )
}

/// Inject $envValues into YAML data before preprocessing
fn inject_env_values(argsdata: &mut Value, env_values: Value) {
    if let Value::Mapping(map) = argsdata {
        // Merge with existing $envValues if present, new values take precedence
        let env_values_key = Value::String("$envValues".to_string());
        match map.get(&env_values_key) {
            Some(Value::Mapping(existing_map)) => {
                if let Value::Mapping(new_map) = &env_values {
                    let mut merged_map = existing_map.clone();
                    for (k, v) in new_map {
                        merged_map.insert(k.clone(), v.clone());
                    }
                    map.insert(env_values_key, Value::Mapping(merged_map));
                } else {
                    map.insert(env_values_key, env_values);
                }
            }
            _ => {
                map.insert(env_values_key, env_values);
            }
        }
    }
}

/// Apply global configuration from SSM parameter store, matching iidy-js applyGlobalConfiguration
async fn apply_global_configuration(
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
                                apply_sns_notification_global_configuration(
                                    args, &value, aws_config,
                                )
                                .await?;
                            }
                            "/iidy/disable-template-approval" => {
                                if value.to_lowercase() == "true"
                                    && args.approved_template_location.is_some()
                                {
                                    eprintln!(
                                        "Disabling template approval based on global {name} parameter store configuration"
                                    );
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

    match sns.get_topic_attributes().topic_arn(topic_arn).send().await {
        Ok(_) => {
            // Topic exists, add it to notification ARNs (matching iidy-js concat behavior)
            let notification_arns = args.notification_arns.get_or_insert_with(Vec::new);
            notification_arns.push(topic_arn.to_string());
        }
        Err(_) => {
            eprintln!("iidy's default NotificationARN set in this region is invalid: {topic_arn}");
        }
    }

    Ok(())
}

/// Check if CommandsBefore should be processed for the given operation
fn should_process_commands_before(operation: &CfnOperation) -> bool {
    matches!(
        operation,
        CfnOperation::CreateStack
            | CfnOperation::UpdateStack
            | CfnOperation::CreateChangeset
            | CfnOperation::CreateOrUpdate
    )
}

/// Process CommandsBefore with handlebars templating and command execution
fn process_commands_before(
    commands: Value,
    stack_args: &StackArgs,
    environment: Option<&str>,
    operation: &CfnOperation,
    region: &str,
    profile: Option<&str>,
    argsfile_path: &Path,
) -> Result<Vec<String>> {
    use crate::yaml::handlebars::interpolate_handlebars_string;
    use std::process::Command;

    // Extract commands as strings
    let commands = match commands {
        Value::Sequence(seq) => seq
            .into_iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect::<Vec<_>>(),
        _ => bail!("CommandsBefore must be an array of strings"),
    };

    // Build handlebars context with full iidy namespace
    let mut handlebars_env = BTreeMap::new();

    // Add iidy namespace
    let mut iidy_values = BTreeMap::new();
    iidy_values.insert("stackArgs".to_string(), serde_yaml::to_value(stack_args)?);
    // Use stack name from stack args, but this could be overridden by CLI later
    if let Some(stack_name) = &stack_args.stack_name {
        iidy_values.insert("stackName".to_string(), Value::String(stack_name.clone()));
    }
    iidy_values.insert(
        "command".to_string(),
        Value::String(operation.as_str().to_string()),
    );
    if let Some(env) = environment {
        iidy_values.insert("environment".to_string(), Value::String(env.to_string()));
    }
    iidy_values.insert("region".to_string(), Value::String(region.to_string()));
    if let Some(p) = profile {
        iidy_values.insert("profile".to_string(), Value::String(p.to_string()));
    }

    handlebars_env.insert(
        "iidy".to_string(),
        Value::Mapping(
            iidy_values
                .into_iter()
                .map(|(k, v)| (Value::String(k), v))
                .collect(),
        ),
    );

    // Add legacy values
    handlebars_env.insert("region".to_string(), Value::String(region.to_string()));
    if let Some(env) = environment {
        handlebars_env.insert("environment".to_string(), Value::String(env.to_string()));
    }

    let handlebars_value = Value::Mapping(
        handlebars_env
            .into_iter()
            .map(|(k, v)| (Value::String(k), v))
            .collect(),
    );

    // Get working directory from argsfile path
    let cwd = argsfile_path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Cannot determine working directory from argsfile path"))?;

    println!(
        "== Executing CommandsBefore from argsfile {}",
        "=".repeat(28)
    );

    let mut expanded_commands = Vec::new();

    // Convert serde_yaml::Value to HashMap<String, serde_json::Value> for handlebars
    let handlebars_map = if let Value::Mapping(map) = &handlebars_value {
        let mut json_map = std::collections::HashMap::new();
        for (k, v) in map {
            if let Value::String(key) = k {
                // Convert serde_yaml::Value to serde_json::Value
                let json_value = serde_json::to_value(v).with_context(|| {
                    format!("Failed to convert YAML value to JSON for key: {key}")
                })?;
                json_map.insert(key.clone(), json_value);
            }
        }
        json_map
    } else {
        bail!("Handlebars environment must be a mapping");
    };

    for (index, cmd) in commands.iter().enumerate() {
        // Process handlebars templates in command
        let expanded_command =
            interpolate_handlebars_string(cmd, &handlebars_map, "CommandsBefore")?;
        expanded_commands.push(expanded_command.clone());

        println!("\n-- Command {} {}", index + 1, "-".repeat(50));
        if expanded_command != *cmd {
            println!("# raw command before processing handlebars variables:");
            println!("{cmd}");
            println!("# command after processing handlebars variables:");
            println!("{expanded_command}");
        } else {
            println!("{cmd}");
        }

        println!("-- Command {} Output {}", index + 1, "-".repeat(25));

        // Execute command with environment variables
        let mut command = Command::new("/bin/bash");
        command.arg("-c").arg(&expanded_command);
        command.current_dir(cwd);

        // Set environment variables matching iidy-js
        command.env("iidy_profile", profile.unwrap_or(""));
        command.env("iidy_region", region);
        command.env("iidy_environment", environment.unwrap_or(""));
        command.env("PKG_SKIP_EXECPATH_PATCH", "yes");

        // Add bash functions matching iidy-js
        command.env(
            "BASH_FUNC_iidy_filehash%%",
            "() {   shasum -p -a 256 \"$1\" | cut -f 1 -d ' '; }",
        );
        command.env(
            "BASH_FUNC_iidy_filehash_base64%%",
            "() { shasum -p -a 256 \"$1\" | cut -f 1 -d ' ' | xxd -r -p | base64; }",
        );
        command.env("BASH_FUNC_iidy_s3_upload%%", r#"() {
  echo '>> NOTE: iidy_s3_upload is an experimental addition to iidy. It might be removed in future versions.'
  FILE=$1
  BUCKET=$2
  S3_KEY=$3
  aws --profile "$iidy_profile" --region "$iidy_region" s3api head-object --bucket "$BUCKET" --key "$S3_KEY" 2>&1 >/dev/null || \
        aws --profile "$iidy_profile" --region "$iidy_region" s3 cp "$FILE" "s3://$BUCKET/$S3_KEY";

 }"#);

        // Execute command
        let output = command
            .output()
            .with_context(|| format!("Failed to execute command: {expanded_command}"))?;

        // Print output
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }

        // Check exit status
        if !output.status.success() {
            bail!(
                "Error running command (exit code {}):\n{}",
                output.status.code().unwrap_or(-1),
                cmd
            );
        }
    }

    println!();
    println!("== End CommandsBefore {}", "=".repeat(48));
    println!();

    Ok(expanded_commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn parse_basic_args() {
        use crate::aws::AwsSettings;
        use std::io::Write;

        let yaml = "StackName: test\nTemplate: foo.yaml\nRegion: us-east-1\n";

        // Create temporary file
        let mut temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(yaml.as_bytes())
            .expect("Failed to write to temp file");
        let temp_path = temp_file.path().to_str().expect("Invalid path");

        // Create mock AWS settings
        let aws_settings = AwsSettings::default();

        let (result, _aws_config, _credential_sources) =
            load_stack_args(temp_path, "dev", &CfnOperation::CreateStack, &aws_settings)
                .await
                .expect("failed to load");

        assert_eq!(result.stack_name.as_deref(), Some("test"));
        assert_eq!(result.template.as_deref(), Some("foo.yaml"));
        assert_eq!(
            result.tags.unwrap().get("environment").map(String::as_str),
            Some("dev")
        );
    }

    #[tokio::test]
    async fn resolve_environment_map() {
        use crate::aws::AwsSettings;
        use std::io::Write;

        let yaml = r#"
Profile: default
Region:
  dev: us-east-1
  prod: us-west-2
StackName: s
Template: t
"#;

        // Create temporary file
        let mut temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(yaml.as_bytes())
            .expect("Failed to write to temp file");
        let temp_path = temp_file.path().to_str().expect("Invalid path");

        // Create mock AWS settings
        let aws_settings = AwsSettings::default();

        let (result, _aws_config, _credential_sources) =
            load_stack_args(temp_path, "prod", &CfnOperation::UpdateStack, &aws_settings)
                .await
                .unwrap();

        assert_eq!(result.region.as_deref(), Some("us-west-2"));
        assert_eq!(result.profile.as_deref(), Some("default"));
    }

    #[tokio::test]
    async fn test_yaml_preprocessing_integration() {
        use crate::aws::AwsSettings;
        use std::io::Write;

        // Test that our YAML preprocessing system is being used in stack args parsing
        let yaml = r#"
StackName: !$join ["-", ["my-app", "production"]]
Template: template.yaml
Region: us-west-2
"#;

        // Create temporary file
        let mut temp_file = tempfile::NamedTempFile::new().expect("Failed to create temp file");
        temp_file
            .write_all(yaml.as_bytes())
            .expect("Failed to write to temp file");
        let temp_path = temp_file.path().to_str().expect("Invalid path");

        // Create mock AWS settings
        let aws_settings = AwsSettings::default();

        let result =
            load_stack_args(temp_path, "prod", &CfnOperation::CreateStack, &aws_settings).await;

        // Should succeed even with custom tags (currently they get converted to null)
        assert!(
            result.is_ok(),
            "Stack args with custom tags should parse successfully"
        );

        let (stack_args, _aws_config, _credential_sources) = result.unwrap();
        // Currently custom tags become null since AST resolution isn't implemented yet
        // But parsing should succeed
        assert_eq!(stack_args.template.as_deref(), Some("template.yaml"));
        assert_eq!(stack_args.region.as_deref(), Some("us-west-2"));
    }

    #[test]
    fn test_create_env_values() {
        let env_values = create_env_values(
            Some("production"),
            &CfnOperation::CreateStack,
            "us-west-2",
            Some("prod-profile"),
        );

        // Verify structure matches iidy-js
        if let Value::Mapping(map) = env_values {
            // Legacy values
            assert_eq!(
                map.get(Value::String("region".to_string())),
                Some(&Value::String("us-west-2".to_string()))
            );
            assert_eq!(
                map.get(Value::String("environment".to_string())),
                Some(&Value::String("production".to_string()))
            );

            // Namespaced iidy values
            if let Some(Value::Mapping(iidy_map)) = map.get(Value::String("iidy".to_string())) {
                assert_eq!(
                    iidy_map.get(Value::String("command".to_string())),
                    Some(&Value::String("create-stack".to_string()))
                );
                assert_eq!(
                    iidy_map.get(Value::String("environment".to_string())),
                    Some(&Value::String("production".to_string()))
                );
                assert_eq!(
                    iidy_map.get(Value::String("region".to_string())),
                    Some(&Value::String("us-west-2".to_string()))
                );
                assert_eq!(
                    iidy_map.get(Value::String("profile".to_string())),
                    Some(&Value::String("prod-profile".to_string()))
                );
            } else {
                panic!("Expected iidy namespace in $envValues");
            }
        } else {
            panic!("Expected $envValues to be a mapping");
        }
    }

    #[test]
    fn test_inject_env_values() {
        let mut argsdata = serde_yaml::from_str::<Value>(
            r#"
StackName: test-stack
Template: template.yaml
"#,
        )
        .unwrap();

        let env_values =
            create_env_values(Some("dev"), &CfnOperation::UpdateStack, "us-east-1", None);

        inject_env_values(&mut argsdata, env_values);

        // Verify $envValues was injected
        if let Value::Mapping(map) = &argsdata {
            assert!(map.contains_key(Value::String("$envValues".to_string())));

            if let Some(Value::Mapping(env_map)) = map.get(Value::String("$envValues".to_string()))
            {
                assert_eq!(
                    env_map.get(Value::String("environment".to_string())),
                    Some(&Value::String("dev".to_string()))
                );
                assert_eq!(
                    env_map.get(Value::String("region".to_string())),
                    Some(&Value::String("us-east-1".to_string()))
                );
            }
        } else {
            panic!("Expected argsdata to be a mapping");
        }
    }
}
