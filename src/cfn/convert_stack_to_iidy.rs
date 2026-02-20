use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use aws_sdk_cloudformation::types::TemplateStage;
use aws_sdk_kms::Client as KmsClient;
use aws_sdk_ssm::Client as SsmClient;
use serde_json::Value as JsonValue;
use serde_yaml::Mapping;
use serde_yaml::Value as YamlValue;

use crate::cfn::CfnContext;
use crate::cli::{Cli, ConvertArgs, NormalizedAwsOpts};
use crate::output::DynamicOutputManager;
use crate::params::get_kms_alias_for_parameter;

const ENVIRONMENTS: &[&str] = &[
    "production",
    "staging",
    "development",
    "integration",
    "testing",
];

const DEFAULT_STACK_POLICY: &str = r#"{
 "Statement": [
  {
   "Effect": "Allow",
   "Action": "Update:*",
   "Principal": "*",
   "Resource": "*"
  }
 ]
}"#;

fn parameterize_env(s: &str) -> String {
    let mut result = s.to_string();
    for env in ENVIRONMENTS {
        result = result.replace(env, "{{environment}}");
    }
    result
}

fn parameterize_stack_name(name: &str, project: &str) -> String {
    let mut result = parameterize_env(name);
    // Replace trailing -digits with -{{build_number}}
    if let Some(pos) = result.rfind('-') {
        let suffix = &result[pos + 1..];
        if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit()) {
            result = format!("{}-{{{{build_number}}}}", &result[..pos]);
        }
    }
    result = result.replace(project, "{{project}}");
    result
}

const DEFAULT_SORT_WEIGHT: i32 = 9999;

fn cfn_document_weight(key: &str) -> i32 {
    match key {
        "AWSTemplateFormatVersion" => 0,
        "Description" => 1,
        "Metadata" => 2,
        "Parameters" => 3,
        "Mappings" => 4,
        "Conditions" => 5,
        "Transform" => 6,
        "Resources" => 7,
        "Outputs" => 8,
        _ => DEFAULT_SORT_WEIGHT,
    }
}

fn cfn_parameter_weight(key: &str) -> i32 {
    match key {
        "Description" => 0,
        "Type" => 1,
        "MinValue" => 2,
        "MaxValue" => 3,
        "MinLength" => 4,
        "MaxLength" => 5,
        _ => DEFAULT_SORT_WEIGHT,
    }
}

fn cfn_resource_weight(key: &str) -> i32 {
    match key {
        "Type" => 0,
        "Properties" => DEFAULT_SORT_WEIGHT + 1,
        _ => DEFAULT_SORT_WEIGHT,
    }
}

fn cfn_output_weight(key: &str) -> i32 {
    match key {
        "Description" => 0,
        "Value" => 1,
        "Export" => 2,
        _ => DEFAULT_SORT_WEIGHT,
    }
}

fn cfn_tag_weight(key: &str) -> i32 {
    match key {
        "Key" => 0,
        "Value" => 1,
        _ => DEFAULT_SORT_WEIGHT,
    }
}

fn cfn_iam_statement_weight(key: &str) -> i32 {
    match key {
        "Sid" => 0,
        "Effect" => 1,
        "Action" => 2,
        "Resource" => 3,
        "Condition" => 4,
        _ => DEFAULT_SORT_WEIGHT,
    }
}

fn cfn_policy_doc_weight(key: &str) -> i32 {
    match key {
        "Version" => 0,
        "Statement" => 1,
        _ => DEFAULT_SORT_WEIGHT,
    }
}

fn cfn_policy_weight(key: &str) -> i32 {
    match key {
        "PolicyName" => 0,
        "PolicyDocument" => 1,
        _ => DEFAULT_SORT_WEIGHT,
    }
}

fn sort_mapping(mapping: &Mapping, weight_fn: fn(&str) -> i32) -> Mapping {
    let mut pairs: Vec<(YamlValue, YamlValue)> = mapping
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    pairs.sort_by(|(a, _), (b, _)| {
        let a_key = a.as_str().unwrap_or("");
        let b_key = b.as_str().unwrap_or("");
        let a_weight = weight_fn(a_key);
        let b_weight = weight_fn(b_key);
        a_weight.cmp(&b_weight).then_with(|| a_key.cmp(b_key))
    });

    let mut result = Mapping::new();
    for (k, v) in pairs {
        result.insert(k, v);
    }
    result
}

fn sort_cfn_value(value: &YamlValue, parent_key: &str, current_key: &str) -> YamlValue {
    match value {
        YamlValue::Mapping(mapping) => {
            let weight_fn: fn(&str) -> i32 = if parent_key.is_empty() && current_key.is_empty() {
                cfn_document_weight
            } else if parent_key == "Parameters" {
                cfn_parameter_weight
            } else if parent_key == "Resources" {
                cfn_resource_weight
            } else if parent_key == "Tags" {
                cfn_tag_weight
            } else if parent_key == "Outputs" {
                cfn_output_weight
            } else if parent_key == "Statement" {
                cfn_iam_statement_weight
            } else if current_key == "PolicyDocument" || current_key == "AssumeRolePolicyDocument" {
                cfn_policy_doc_weight
            } else if parent_key == "Policies" {
                cfn_policy_weight
            } else {
                |_| DEFAULT_SORT_WEIGHT
            };

            let sorted = sort_mapping(mapping, weight_fn);
            let mut result = Mapping::new();
            for (k, v) in &sorted {
                let key_str = k.as_str().unwrap_or("");
                let new_parent = if parent_key.is_empty() && current_key.is_empty() {
                    key_str.to_string()
                } else {
                    current_key.to_string()
                };
                result.insert(k.clone(), sort_cfn_value(v, &new_parent, key_str));
            }
            YamlValue::Mapping(result)
        }
        YamlValue::Sequence(seq) => {
            let sorted_seq: Vec<YamlValue> = seq
                .iter()
                .enumerate()
                .map(|(i, v)| sort_cfn_value(v, current_key, &i.to_string()))
                .collect();
            YamlValue::Sequence(sorted_seq)
        }
        _ => value.clone(),
    }
}

fn sort_cfn_keys(value: &YamlValue) -> YamlValue {
    sort_cfn_value(value, "", "")
}

fn template_body_to_yaml(template_body: &str, sortkeys: bool) -> Result<String> {
    let trimmed = template_body.trim_start();
    let mut yaml_value: YamlValue = if trimmed.starts_with('{') {
        let json: JsonValue = serde_json::from_str(trimmed)?;
        serde_yaml::to_value(&json)?
    } else {
        serde_yaml::from_str(trimmed)?
    };
    if sortkeys {
        yaml_value = sort_cfn_keys(&yaml_value);
    }
    Ok(serde_yaml::to_string(&yaml_value)?)
}

fn build_stack_args_yaml(
    stack: &aws_sdk_cloudformation::types::Stack,
    stack_name: &str,
    project: &str,
    ssm_param_keys: &[String],
) -> Result<String> {
    let mut doc: BTreeMap<String, YamlValue> = BTreeMap::new();

    // $defs.project
    let mut defs = BTreeMap::new();
    defs.insert(
        "project".to_string(),
        YamlValue::String(project.to_string()),
    );
    doc.insert("$defs".to_string(), serde_yaml::to_value(&defs)?);

    // $imports.build_number (and ssmParams if moving to SSM)
    let mut imports = BTreeMap::new();
    imports.insert(
        "build_number".to_string(),
        YamlValue::String("env:build_number:0".to_string()),
    );
    if !ssm_param_keys.is_empty() {
        imports.insert(
            "ssmParams".to_string(),
            YamlValue::String("ssm-path:/{{environment}}/{{project}}/".to_string()),
        );
    }
    doc.insert("$imports".to_string(), serde_yaml::to_value(&imports)?);

    // Template
    doc.insert(
        "Template".to_string(),
        YamlValue::String("./cfn-template.yaml".to_string()),
    );

    // StackName (parameterized)
    let parameterized_name = parameterize_stack_name(stack_name, project);
    doc.insert(
        "StackName".to_string(),
        YamlValue::String(parameterized_name),
    );

    // StackPolicy
    doc.insert(
        "StackPolicy".to_string(),
        YamlValue::String("./stack-policy.json".to_string()),
    );

    // Parameters (parameterized; SSM params use !$ tag via placeholder)
    let params: BTreeMap<String, YamlValue> = stack
        .parameters()
        .iter()
        .filter_map(|p| {
            let key = p.parameter_key()?;
            let value = p.parameter_value()?;
            let val = if key == "Environment" || key == "environment" {
                "{{environment}}".to_string()
            } else if ssm_param_keys.contains(&key.to_string()) {
                format!("__SSM_REF__{key}")
            } else {
                value.to_string()
            };
            Some((key.to_string(), YamlValue::String(val)))
        })
        .collect();
    if !params.is_empty() {
        doc.insert("Parameters".to_string(), serde_yaml::to_value(&params)?);
    }

    // Tags (parameterized)
    let tags: BTreeMap<String, YamlValue> = stack
        .tags()
        .iter()
        .filter_map(|t| {
            let key = t.key()?;
            let value = t.value()?;
            let val = match key {
                "project" => "{{project}}".to_string(),
                "environment" | "Environment" => "{{environment}}".to_string(),
                _ => value.to_string(),
            };
            Some((key.to_string(), YamlValue::String(val)))
        })
        .collect();
    if !tags.is_empty() {
        doc.insert("Tags".to_string(), serde_yaml::to_value(&tags)?);
    }

    // Capabilities
    let capabilities: Vec<String> = stack
        .capabilities()
        .iter()
        .map(|c| c.as_str().to_string())
        .collect();
    if !capabilities.is_empty() {
        doc.insert(
            "Capabilities".to_string(),
            serde_yaml::to_value(&capabilities)?,
        );
    }

    // TimeoutInMinutes
    if let Some(timeout) = stack.timeout_in_minutes() {
        doc.insert(
            "TimeoutInMinutes".to_string(),
            serde_yaml::to_value(timeout)?,
        );
    }

    // EnableTerminationProtection
    if stack.enable_termination_protection() == Some(true) {
        doc.insert(
            "EnableTerminationProtection".to_string(),
            YamlValue::Bool(true),
        );
    }

    // NotificationARNs
    let notification_arns: Vec<&str> = stack
        .notification_arns()
        .iter()
        .map(|s| s.as_str())
        .collect();
    if !notification_arns.is_empty() {
        doc.insert(
            "NotificationARNs".to_string(),
            serde_yaml::to_value(&notification_arns)?,
        );
    }

    // RoleARN
    if let Some(role_arn) = stack.role_arn() {
        doc.insert(
            "RoleARN".to_string(),
            YamlValue::String(role_arn.to_string()),
        );
    }

    // DisableRollback
    if stack.disable_rollback() == Some(true) {
        doc.insert("DisableRollback".to_string(), YamlValue::Bool(true));
    }

    let mut yaml = serde_yaml::to_string(&doc)?;

    // Post-process: replace SSM-migrated parameter values with !$ tags.
    // serde_yaml cannot emit custom YAML tags, so we do string replacement
    // on the serialized output. The serde output for these keys will be
    // `  KeyName: __SSM_REF__KeyName` which we replace with `  KeyName: !$ ssmParams.KeyName`.
    for key in ssm_param_keys {
        let placeholder = format!("__SSM_REF__{key}");
        let tag_ref = format!("!$ ssmParams.{key}");
        yaml = yaml.replace(&placeholder, &tag_ref);
    }

    Ok(yaml)
}

async fn convert_stack_to_iidy_impl(
    _output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    _cli: &Cli,
    args: &ConvertArgs,
    _opts: &NormalizedAwsOpts,
) -> Result<i32> {
    let stack_name = &args.stackname;
    let output_dir = Path::new(&args.output_dir);

    // 1. Fetch original template
    let template_resp = context
        .client
        .get_template()
        .stack_name(stack_name)
        .template_stage(TemplateStage::Original)
        .send()
        .await?;
    let template_body = template_resp
        .template_body()
        .ok_or_else(|| anyhow!("No template body returned for stack {stack_name}"))?;

    // 2. Detect format
    let is_json = template_body.trim_start().starts_with('{');
    let original_ext = if is_json { "json" } else { "yaml" };

    // 3. Describe stack
    let stack_resp = context
        .client
        .describe_stacks()
        .stack_name(stack_name)
        .send()
        .await?;
    let stack = stack_resp
        .stacks()
        .first()
        .ok_or_else(|| anyhow!("Stack {stack_name} not found"))?;

    // 4. Get stack policy
    let policy_body = context
        .client
        .get_stack_policy()
        .stack_name(stack_name)
        .send()
        .await
        .ok()
        .and_then(|r| r.stack_policy_body().map(String::from));

    let policy_content = match &policy_body {
        Some(body) => {
            // Pretty-print the policy JSON
            let parsed: JsonValue = serde_json::from_str(body)?;
            serde_json::to_string_pretty(&parsed)?
        }
        None => DEFAULT_STACK_POLICY.to_string(),
    };

    // 5. Create output directory
    std::fs::create_dir_all(output_dir)?;

    // 6. Write stack-policy.json
    let policy_path = output_dir.join("stack-policy.json");
    std::fs::write(&policy_path, &policy_content)?;
    eprintln!("Wrote {}", policy_path.display());

    // 7. Write _original-template.{json|yaml}
    let original_filename = format!("_original-template.{original_ext}");
    let original_path = output_dir.join(&original_filename);
    std::fs::write(&original_path, template_body)?;
    eprintln!("Wrote {}", original_path.display());

    // 8. Write cfn-template.yaml (convert to YAML if needed)
    let yaml_template = template_body_to_yaml(template_body, args.sortkeys)?;
    let cfn_template_path = output_dir.join("cfn-template.yaml");
    std::fs::write(&cfn_template_path, &yaml_template)?;
    eprintln!("Wrote {}", cfn_template_path.display());

    // 9. Determine project name and current environment
    let project = args.project.clone().unwrap_or_else(|| {
        stack
            .tags()
            .iter()
            .find(|t| t.key() == Some("project"))
            .and_then(|t| t.value().map(String::from))
            .unwrap_or_default()
    });

    let current_environment = stack
        .tags()
        .iter()
        .find(|t| matches!(t.key(), Some("environment" | "Environment")))
        .and_then(|t| t.value().map(String::from))
        .unwrap_or_else(|| "development".to_string());

    // 10. Optionally migrate parameters to SSM
    let ssm_param_keys = if args.move_params_to_ssm {
        if project.is_empty() {
            bail!(
                "--move-params-to-ssm requires a project name (use --project or add a 'project' tag to the stack)"
            );
        }
        move_params_to_ssm(context, stack, &current_environment, &project).await?
    } else {
        Vec::new()
    };

    // 11. Build and write stack-args.yaml
    let stack_args_content = build_stack_args_yaml(stack, stack_name, &project, &ssm_param_keys)?;
    let stack_args_path = output_dir.join("stack-args.yaml");
    std::fs::write(&stack_args_path, &stack_args_content)?;
    eprintln!("Wrote {}", stack_args_path.display());

    Ok(0)
}

/// Write each non-environment parameter to SSM as SecureString.
/// Returns the list of parameter keys that were migrated.
async fn move_params_to_ssm(
    context: &CfnContext,
    stack: &aws_sdk_cloudformation::types::Stack,
    current_environment: &str,
    project: &str,
) -> Result<Vec<String>> {
    let ssm = SsmClient::new(&context.aws_config);
    let kms = KmsClient::new(&context.aws_config);

    let ssm_prefix = format!("/{current_environment}/{project}/");
    let key_id = get_kms_alias_for_parameter(&kms, &ssm_prefix).await?;

    let mut migrated_keys = Vec::new();

    for param in stack.parameters() {
        let Some(key) = param.parameter_key() else {
            continue;
        };
        if key == "Environment" || key == "environment" {
            continue;
        }
        let Some(value) = param.parameter_value() else {
            continue;
        };

        let name = format!("{ssm_prefix}{key}");
        eprintln!("Writing SSM parameter: {name}");

        let mut req = ssm
            .put_parameter()
            .name(&name)
            .value(value)
            .r#type(aws_sdk_ssm::types::ParameterType::SecureString)
            .overwrite(true);
        if let Some(alias) = &key_id {
            req = req.key_id(alias);
        }
        req.send().await?;

        migrated_keys.push(key.to_string());
    }

    Ok(migrated_keys)
}

pub async fn convert_stack_to_iidy(cli: &Cli, args: &ConvertArgs) -> Result<i32> {
    crate::run_command_handler!(convert_stack_to_iidy_impl, cli, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameterize_env_replaces_known_environments() {
        assert_eq!(
            parameterize_env("my-app-production-cluster"),
            "my-app-{{environment}}-cluster"
        );
        assert_eq!(parameterize_env("my-app-staging"), "my-app-{{environment}}");
        assert_eq!(
            parameterize_env("my-app-development"),
            "my-app-{{environment}}"
        );
        assert_eq!(
            parameterize_env("my-app-integration"),
            "my-app-{{environment}}"
        );
        assert_eq!(parameterize_env("my-app-testing"), "my-app-{{environment}}");
    }

    #[test]
    fn parameterize_env_leaves_unknown_strings() {
        assert_eq!(parameterize_env("my-app-custom"), "my-app-custom");
    }

    #[test]
    fn parameterize_stack_name_replaces_trailing_digits() {
        assert_eq!(
            parameterize_stack_name("myproject-production-api-42", "myproject"),
            "{{project}}-{{environment}}-api-{{build_number}}"
        );
    }

    #[test]
    fn parameterize_stack_name_no_trailing_digits() {
        assert_eq!(
            parameterize_stack_name("myproject-production-api", "myproject"),
            "{{project}}-{{environment}}-api"
        );
    }

    #[test]
    fn parameterize_stack_name_only_project() {
        assert_eq!(
            parameterize_stack_name("myproject-custom-stack", "myproject"),
            "{{project}}-custom-stack"
        );
    }

    #[test]
    fn template_body_json_to_yaml() {
        let json = r#"{"AWSTemplateFormatVersion": "2010-09-09", "Resources": {}}"#;
        let yaml = template_body_to_yaml(json, false).unwrap();
        assert!(yaml.contains("AWSTemplateFormatVersion"));
        assert!(yaml.contains("Resources"));
        assert!(!yaml.trim().starts_with('{'));
    }

    #[test]
    fn template_body_yaml_passthrough() {
        let input = "AWSTemplateFormatVersion: '2010-09-09'\nResources: {}\n";
        let yaml = template_body_to_yaml(input, false).unwrap();
        assert!(yaml.contains("AWSTemplateFormatVersion"));
    }

    #[test]
    fn default_stack_policy_is_valid_json() {
        let parsed: JsonValue = serde_json::from_str(DEFAULT_STACK_POLICY).unwrap();
        assert!(parsed["Statement"].is_array());
        assert_eq!(parsed["Statement"][0]["Effect"], "Allow");
    }

    #[test]
    fn build_stack_args_yaml_basic() {
        use aws_sdk_cloudformation::types::{Capability, Parameter, Stack, Tag};

        let stack = Stack::builder()
            .stack_name("myproject-production-api-42")
            .stack_status(aws_sdk_cloudformation::types::StackStatus::CreateComplete)
            .set_parameters(Some(vec![
                Parameter::builder()
                    .parameter_key("Environment")
                    .parameter_value("production")
                    .build(),
                Parameter::builder()
                    .parameter_key("InstanceType")
                    .parameter_value("t3.medium")
                    .build(),
            ]))
            .set_tags(Some(vec![
                Tag::builder().key("project").value("myproject").build(),
                Tag::builder()
                    .key("environment")
                    .value("production")
                    .build(),
                Tag::builder().key("team").value("platform").build(),
            ]))
            .set_capabilities(Some(vec![Capability::CapabilityIam]))
            .enable_termination_protection(true)
            .creation_time(aws_sdk_cloudformation::primitives::DateTime::from_secs(0))
            .build();

        let yaml_str =
            build_stack_args_yaml(&stack, "myproject-production-api-42", "myproject", &[]).unwrap();

        // Parse back and verify structure
        let parsed: BTreeMap<String, YamlValue> = serde_yaml::from_str(&yaml_str).unwrap();

        fn ykey(s: &str) -> YamlValue {
            YamlValue::String(s.to_string())
        }

        // $defs.project
        let defs = parsed["$defs"].as_mapping().unwrap();
        assert_eq!(
            defs.get(ykey("project")).unwrap().as_str().unwrap(),
            "myproject"
        );

        // $imports.build_number
        let imports = parsed["$imports"].as_mapping().unwrap();
        assert_eq!(
            imports.get(ykey("build_number")).unwrap().as_str().unwrap(),
            "env:build_number:0"
        );

        // Template and StackPolicy
        assert_eq!(parsed["Template"].as_str().unwrap(), "./cfn-template.yaml");
        assert_eq!(
            parsed["StackPolicy"].as_str().unwrap(),
            "./stack-policy.json"
        );

        // EnableTerminationProtection
        assert!(parsed["EnableTerminationProtection"].as_bool().unwrap());

        // Parameters should be parameterized
        let params = parsed["Parameters"].as_mapping().unwrap();
        assert_eq!(
            params.get(ykey("Environment")).unwrap().as_str().unwrap(),
            "{{environment}}"
        );
        assert_eq!(
            params.get(ykey("InstanceType")).unwrap().as_str().unwrap(),
            "t3.medium"
        );

        // Tags should be parameterized
        let tags = parsed["Tags"].as_mapping().unwrap();
        assert_eq!(
            tags.get(ykey("project")).unwrap().as_str().unwrap(),
            "{{project}}"
        );
        assert_eq!(
            tags.get(ykey("environment")).unwrap().as_str().unwrap(),
            "{{environment}}"
        );
        assert_eq!(
            tags.get(ykey("team")).unwrap().as_str().unwrap(),
            "platform"
        );

        // Capabilities
        let caps = parsed["Capabilities"].as_sequence().unwrap();
        assert_eq!(caps[0].as_str().unwrap(), "CAPABILITY_IAM");
    }

    #[test]
    fn build_stack_args_yaml_with_ssm_params() {
        use aws_sdk_cloudformation::types::{Parameter, Stack, Tag};

        let stack = Stack::builder()
            .stack_name("myproject-production-api-42")
            .stack_status(aws_sdk_cloudformation::types::StackStatus::CreateComplete)
            .set_parameters(Some(vec![
                Parameter::builder()
                    .parameter_key("Environment")
                    .parameter_value("production")
                    .build(),
                Parameter::builder()
                    .parameter_key("DatabasePassword")
                    .parameter_value("secret123")
                    .build(),
                Parameter::builder()
                    .parameter_key("ApiKey")
                    .parameter_value("key456")
                    .build(),
                Parameter::builder()
                    .parameter_key("InstanceType")
                    .parameter_value("t3.medium")
                    .build(),
            ]))
            .set_tags(Some(vec![
                Tag::builder().key("project").value("myproject").build(),
            ]))
            .creation_time(aws_sdk_cloudformation::primitives::DateTime::from_secs(0))
            .build();

        let ssm_keys = vec!["DatabasePassword".to_string(), "ApiKey".to_string()];
        let yaml_str = build_stack_args_yaml(
            &stack,
            "myproject-production-api-42",
            "myproject",
            &ssm_keys,
        )
        .unwrap();

        // SSM-migrated params should have !$ tags
        assert!(yaml_str.contains("DatabasePassword: !$ ssmParams.DatabasePassword"));
        assert!(yaml_str.contains("ApiKey: !$ ssmParams.ApiKey"));
        // Non-SSM param should retain its value
        assert!(yaml_str.contains("InstanceType: t3.medium"));
        // Environment should still be handlebars
        assert!(yaml_str.contains("Environment: '{{environment}}'"));
        // ssmParams import should be present
        assert!(yaml_str.contains("ssmParams: ssm-path:/{{environment}}/{{project}}/"));
    }

    #[test]
    fn sort_cfn_keys_reorders_top_level() {
        let input = "Resources: {}\nDescription: hello\nAWSTemplateFormatVersion: '2010-09-09'\nOutputs: {}\nParameters: {}\n";
        let yaml = template_body_to_yaml(input, true).unwrap();
        let version_pos = yaml.find("AWSTemplateFormatVersion").unwrap();
        let desc_pos = yaml.find("Description").unwrap();
        let params_pos = yaml.find("Parameters").unwrap();
        let resources_pos = yaml.find("Resources").unwrap();
        let outputs_pos = yaml.find("Outputs").unwrap();
        assert!(version_pos < desc_pos);
        assert!(desc_pos < params_pos);
        assert!(params_pos < resources_pos);
        assert!(resources_pos < outputs_pos);
    }

    #[test]
    fn sort_cfn_keys_preserves_unknown_keys_after_known() {
        let input =
            "ZCustom: foo\nResources: {}\nAWSTemplateFormatVersion: '2010-09-09'\nACustom: bar\n";
        let yaml = template_body_to_yaml(input, true).unwrap();
        let version_pos = yaml.find("AWSTemplateFormatVersion").unwrap();
        let resources_pos = yaml.find("Resources").unwrap();
        let acustom_pos = yaml.find("ACustom").unwrap();
        let zcustom_pos = yaml.find("ZCustom").unwrap();
        assert!(version_pos < resources_pos);
        assert!(resources_pos < acustom_pos);
        assert!(acustom_pos < zcustom_pos);
    }

    #[test]
    fn sort_cfn_keys_sorts_resource_entries() {
        let input = "Resources:\n  MyBucket:\n    Properties:\n      BucketName: test\n    Type: AWS::S3::Bucket\n";
        let yaml = template_body_to_yaml(input, true).unwrap();
        let type_pos = yaml.find("Type:").unwrap();
        let props_pos = yaml.find("Properties:").unwrap();
        assert!(type_pos < props_pos);
    }

    #[test]
    fn sort_cfn_keys_sorts_parameter_entries() {
        let input = "Parameters:\n  MyParam:\n    MaxLength: '10'\n    Description: a param\n    Type: String\n";
        let yaml = template_body_to_yaml(input, true).unwrap();
        let desc_pos = yaml.find("Description:").unwrap();
        let type_pos = yaml.find("Type:").unwrap();
        let max_pos = yaml.find("MaxLength:").unwrap();
        assert!(desc_pos < type_pos);
        assert!(type_pos < max_pos);
    }

    #[test]
    fn sort_cfn_keys_no_sort_when_disabled() {
        let input = "Resources: {}\nAWSTemplateFormatVersion: '2010-09-09'\n";
        let yaml = template_body_to_yaml(input, false).unwrap();
        let resources_pos = yaml.find("Resources").unwrap();
        let version_pos = yaml.find("AWSTemplateFormatVersion").unwrap();
        assert!(resources_pos < version_pos);
    }
}
