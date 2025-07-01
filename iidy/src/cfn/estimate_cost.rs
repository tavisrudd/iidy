use anyhow::Result;
use std::path::Path;

use crate::{
    cfn::{create_context, CfnOperation},
    cli::{NormalizedAwsOpts, StackFileArgs, GlobalOpts},
    output::{
        DynamicOutputManager, manager::OutputOptions,
        OutputData, StatusUpdate, StatusLevel
    },
    stack_args::load_stack_args,
    aws::AwsSettings,
};

/// Estimate stack cost using CloudFormation's estimateTemplateCost API.
///
/// Loads the template and parameters from stack-args.yaml, calls AWS
/// CloudFormation's cost estimation API, and displays the cost estimator URL.
pub async fn estimate_cost(
    opts: &NormalizedAwsOpts, 
    args: &StackFileArgs,
    global_opts: &GlobalOpts
) -> Result<()> {
    let output_options = OutputOptions::minimal();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    let cli_aws_settings = AwsSettings::from_normalized_opts(opts);
    let operation = CfnOperation::EstimateCost;
    let stack_args = load_stack_args(
        &args.argsfile,
        Some(&global_opts.environment),
        &operation,
        &cli_aws_settings,
    ).await?;
    let context = create_context(opts, false).await?; // Read-only operation, no NTP needed

    // Determine stack name from args or stack-args.yaml (not needed for estimate cost, but validate it exists)
    let _stack_name = args.stack_name.as_ref()
        .or(stack_args.stack_name.as_ref())
        .ok_or_else(|| anyhow::anyhow!("Stack name must be provided either via --stack-name or in stack-args.yaml"))?;

    let template_body = match &stack_args.template {
        Some(template_path) => {
            let template_path = Path::new(template_path);
            if !template_path.exists() {
                return Err(anyhow::anyhow!("Template file not found: {}", template_path.display()));
            }
            Some(std::fs::read_to_string(template_path)?)
        }
        None => return Err(anyhow::anyhow!("Template must be specified in stack-args.yaml")),
    };

    // Convert parameters to CloudFormation format
    let mut cfn_parameters = Vec::new();
    if let Some(params) = &stack_args.parameters {
        for (key, value) in params {
            cfn_parameters.push(
                aws_sdk_cloudformation::types::Parameter::builder()
                    .parameter_key(key)
                    .parameter_value(value.to_string())
                    .build()
            );
        }
    }

    let mut estimate_request = context.client
        .estimate_template_cost();

    if let Some(body) = template_body {
        estimate_request = estimate_request.template_body(body);
    }

    if !cfn_parameters.is_empty() {
        estimate_request = estimate_request.set_parameters(Some(cfn_parameters));
    }

    let estimate_response = estimate_request.send().await?;

    if let Some(url) = estimate_response.url {
        let status_update = StatusUpdate {
            message: format!("Stack cost estimator: {}", url),
            timestamp: chrono::Utc::now(),
            level: StatusLevel::Info,
        };
        output_manager.render(OutputData::StatusUpdate(status_update)).await?;
    } else {
        return Err(anyhow::anyhow!("AWS did not return a cost estimation URL"));
    }

    Ok(())
}
