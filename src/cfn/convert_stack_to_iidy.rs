use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Result, anyhow, bail};
use aws_sdk_cloudformation::types::TemplateStage;
use serde_json::Value as JsonValue;
use serde_yaml::Value as YamlValue;

use crate::cfn::CfnContext;
use crate::cli::{Cli, ConvertArgs, NormalizedAwsOpts};
use crate::output::DynamicOutputManager;

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

fn template_body_to_yaml(template_body: &str) -> Result<String> {
    let trimmed = template_body.trim_start();
    let yaml_value: YamlValue = if trimmed.starts_with('{') {
        let json: JsonValue = serde_json::from_str(trimmed)?;
        serde_yaml::to_value(&json)?
    } else {
        serde_yaml::from_str(trimmed)?
    };
    Ok(serde_yaml::to_string(&yaml_value)?)
}

fn build_stack_args_yaml(
    stack: &aws_sdk_cloudformation::types::Stack,
    stack_name: &str,
    project: &str,
) -> Result<String> {
    let mut doc: BTreeMap<String, YamlValue> = BTreeMap::new();

    // $defs.project
    let mut defs = BTreeMap::new();
    defs.insert(
        "project".to_string(),
        YamlValue::String(project.to_string()),
    );
    doc.insert("$defs".to_string(), serde_yaml::to_value(&defs)?);

    // $imports.build_number
    let mut imports = BTreeMap::new();
    imports.insert(
        "build_number".to_string(),
        YamlValue::String("env:build_number:0".to_string()),
    );
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

    // Parameters (parameterized)
    let params: BTreeMap<String, YamlValue> = stack
        .parameters()
        .iter()
        .filter_map(|p| {
            let key = p.parameter_key()?;
            let value = p.parameter_value()?;
            let val = if key == "Environment" || key == "environment" {
                "{{environment}}".to_string()
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

    serde_yaml::to_string(&doc).map_err(Into::into)
}

async fn convert_stack_to_iidy_impl(
    _output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    _cli: &Cli,
    args: &ConvertArgs,
    _opts: &NormalizedAwsOpts,
) -> Result<i32> {
    if args.move_params_to_ssm {
        bail!("--move-params-to-ssm is not yet implemented");
    }

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
    let yaml_template = template_body_to_yaml(template_body)?;
    let cfn_template_path = output_dir.join("cfn-template.yaml");
    std::fs::write(&cfn_template_path, &yaml_template)?;
    eprintln!("Wrote {}", cfn_template_path.display());

    // 9. Determine project name
    let project = args.project.clone().unwrap_or_else(|| {
        stack
            .tags()
            .iter()
            .find(|t| t.key() == Some("project"))
            .and_then(|t| t.value().map(String::from))
            .unwrap_or_default()
    });

    // 10. Build and write stack-args.yaml
    let stack_args_content = build_stack_args_yaml(stack, stack_name, &project)?;
    let stack_args_path = output_dir.join("stack-args.yaml");
    std::fs::write(&stack_args_path, &stack_args_content)?;
    eprintln!("Wrote {}", stack_args_path.display());

    Ok(0)
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
        let yaml = template_body_to_yaml(json).unwrap();
        assert!(yaml.contains("AWSTemplateFormatVersion"));
        assert!(yaml.contains("Resources"));
        // Should not start with '{'
        assert!(!yaml.trim().starts_with('{'));
    }

    #[test]
    fn template_body_yaml_passthrough() {
        let input = "AWSTemplateFormatVersion: '2010-09-09'\nResources: {}\n";
        let yaml = template_body_to_yaml(input).unwrap();
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
            build_stack_args_yaml(&stack, "myproject-production-api-42", "myproject").unwrap();

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
}
