use anyhow::Result;

use crate::cfn::{create_context_for_operation, CfnOperation, apply_stack_name_override_and_validate, template_loader::{load_cfn_template, TEMPLATE_MAX_BYTES}};
use crate::cli::{Cli, StackFileArgs, Commands, AwsOpts};
use crate::output::{
    DynamicOutputManager, manager::OutputOptions,
    OutputData, data::{CostEstimate, CostEstimateInfo},
    aws_conversion::create_command_metadata
};
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;

/// Estimate stack cost using CloudFormation's estimateTemplateCost API.
///
/// Loads the template and parameters from stack-args.yaml, calls AWS
/// CloudFormation's cost estimation API, and displays the cost estimator URL.
/// Uses data-driven output architecture with command metadata and cost estimate sections.
pub async fn estimate_cost(cli: &Cli, args: &StackFileArgs) -> Result<i32> {
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = CfnOperation::EstimateCost;
    let stack_args = load_stack_args(
        &args.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.stack_name.as_ref())?;
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    let context = create_context_for_operation(&opts, operation).await?;

    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: Commands::EstimateCost(args.clone()),
    };
    let output_options = OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    let template_result = if let Some(ref template_location) = final_stack_args.template {
        load_cfn_template(
            Some(template_location),
            &args.argsfile,
            Some(&global_opts.environment),
            TEMPLATE_MAX_BYTES,
            Some(&context.create_s3_client()),
        ).await?
    } else {
        return Err(anyhow::anyhow!("Template must be specified in stack-args.yaml"));
    };

    let mut cfn_parameters = Vec::new();
    if let Some(params) = &final_stack_args.parameters {
        for (key, value) in params {
            cfn_parameters.push(
                aws_sdk_cloudformation::types::Parameter::builder()
                    .parameter_key(key)
                    .parameter_value(value.to_string())
                    .build()
            );
        }
    }

    let mut estimate_request = context.client.estimate_template_cost();

    if let Some(template_body) = template_result.template_body {
        estimate_request = estimate_request.template_body(template_body);
    } else if let Some(template_url) = template_result.template_url {
        estimate_request = estimate_request.template_url(template_url);
    }

    if !cfn_parameters.is_empty() {
        estimate_request = estimate_request.set_parameters(Some(cfn_parameters));
    }

    let estimate_response = estimate_request.send().await?;

    let url = estimate_response.url
        .ok_or_else(|| anyhow::anyhow!("AWS did not return a cost estimation URL"))?;

    let cost_info = CostEstimateInfo {
        url,
        stack_name: final_stack_args.stack_name.clone(),
        template_file: final_stack_args.template.clone(),
    };

    output_manager.render(OutputData::CostEstimate(CostEstimate {
        info: cost_info,
    })).await?;

    Ok(0)
}
