use anyhow::Result;

use crate::{
    cfn::{CfnRequestBuilder, CfnContext, stack_operations::{StackInfoService, collect_stack_contents}, CfnOperation, determine_operation_success, CREATE_SUCCESS_STATES, apply_stack_name_override_and_validate, constants::{DEFAULT_POLL_INTERVAL_SECS, DEFAULT_POLL_TIMEOUT_SECS}, stack_args::{load_stack_args, StackArgs}},
    cli::{CreateStackArgs, GlobalOpts, Cli},
    aws::AwsSettings,
    output::{
        DynamicOutputManager, OutputData, convert_stack_to_definition,
        aws_conversion::{create_command_metadata, convert_token_info, create_final_command_summary}
    },
    run_command_handler,
};

async fn create_stack_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &CreateStackArgs,
    opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let global_opts = &cli.global_opts;
    let cli_aws_settings = AwsSettings::from_normalized_opts(opts);
    let operation = cli.command.to_cfn_operation();
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

    let _stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    let command_metadata = create_command_metadata(context, opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    let stack_id = perform_stack_creation(context, &final_stack_args, args, global_opts, output_manager).await?;
    
    let stack_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let stack = StackInfoService::get_stack(&client, &stack_id).await?;
            let output_data = convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };
    
    output_manager.render(stack_task.await??).await?;
    
    let final_status = {
        use crate::cfn::watch_stack::{ManagerOutput, watch_stack_live_events_with_seen_events};
        
        let manager_output = ManagerOutput { manager: output_manager };
        match watch_stack_live_events_with_seen_events(
            &context.client, 
            context, 
            &stack_id, 
            manager_output,
            std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS), 
            std::time::Duration::from_secs(DEFAULT_POLL_TIMEOUT_SECS),
            vec![]
        ).await {
            Ok(status) => status,
            Err(error) => {
                let error_info = crate::output::aws_conversion::convert_aws_error_to_error_info(&error);
                output_manager.render(OutputData::Error(error_info)).await?;
                return Ok(1);
            }
        }
    };
    
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = determine_operation_success(&final_status, CREATE_SUCCESS_STATES);
    
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            let final_command_summary = create_final_command_summary(
                false,
                elapsed_seconds
            );
            output_manager.render(final_command_summary).await?;
            return Ok(1);
        }
    }
    
    let stack_contents = collect_stack_contents(context, &stack_id).await?;
    output_manager.render(OutputData::StackContents(stack_contents)).await?;
    
    let final_command_summary = create_final_command_summary(
        success,
        elapsed_seconds
    );
    output_manager.render(final_command_summary).await?;
    
    Ok(if success { 0 } else { 1 })
}

pub async fn create_stack(cli: &Cli, args: &CreateStackArgs) -> Result<i32> {
    run_command_handler!(create_stack_impl, cli, args)
}

async fn perform_stack_creation(
    context: &CfnContext,
    stack_args: &StackArgs,
    args: &CreateStackArgs,
    global_opts: &GlobalOpts,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    let builder = CfnRequestBuilder::new(context, stack_args);

    let (create_request, token) = builder.build_create_stack(
        true,
        &CfnOperation::CreateStack,
        &args.argsfile,
        Some(&global_opts.environment),
    ).await?;
    
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    
    let response = create_request.send().await?;

    let stack_id = response.stack_id()
        .ok_or_else(|| anyhow::anyhow!("Stack creation response did not include stack ID"))?
        .to_string();

    Ok(stack_id)
}

