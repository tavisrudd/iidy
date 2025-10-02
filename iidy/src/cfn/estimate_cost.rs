use anyhow::Result;

use crate::cfn::{CfnContext, apply_stack_name_override_and_validate, template_loader::{load_cfn_template, TEMPLATE_MAX_BYTES}, StackArgs};
use crate::cli::{Cli, StackFileArgs};
use crate::output::{
    DynamicOutputManager,
    OutputData, data::{CostEstimate, CostEstimateInfo}
};
use crate::run_command_handler_with_stack_args;

async fn estimate_cost_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &StackFileArgs,
    _opts: &crate::cli::NormalizedAwsOpts,
    stack_args: &StackArgs,
) -> Result<i32> {
    let global_opts = &cli.global_opts;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args.clone(), args.stack_name.as_ref())?;
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

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

    let estimate_response = estimate_request.send().await.map_err(anyhow::Error::from)?;

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

pub async fn estimate_cost(cli: &Cli, args: &StackFileArgs) -> Result<i32> {
    run_command_handler_with_stack_args!(estimate_cost_impl, cli, args, &args.argsfile)
}
