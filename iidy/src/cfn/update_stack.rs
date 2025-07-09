use anyhow::Result;

use crate::cfn::{
    CfnRequestBuilder, stack_operations::StackInfoService, 
    CfnOperation, determine_operation_success, UPDATE_SUCCESS_STATES, apply_stack_name_override_and_validate
};
use crate::cli::{UpdateStackArgs, Cli};
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;
use crate::output::{
    DynamicOutputManager, OutputData,
    aws_conversion::{create_command_metadata, create_final_command_summary, convert_token_info, convert_stack_to_definition}
};
use crate::cfn::stack_operations::collect_stack_contents;
use crate::cfn::watch_stack::{watch_stack_with_data_output};
use crate::cfn::constants::DEFAULT_POLL_INTERVAL_SECS;
use crate::run_command_handler;

async fn update_stack_impl(
    output_manager: &mut DynamicOutputManager,
    context: &crate::cfn::CfnContext,
    cli: &Cli,
    args: &UpdateStackArgs,
    opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let global_opts = &cli.global_opts;
    let cli_aws_settings = AwsSettings::from_normalized_opts(opts);
    let operation = cli.command.to_cfn_operation();
    let stack_args = load_stack_args(
        &args.base.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.base.stack_name.as_ref())?;
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    let stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    let command_metadata = create_command_metadata(context, opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    if args.changeset {
        return update_stack_with_changeset(context, args, &final_stack_args, stack_name, output_manager, &global_opts.environment, cli).await;
    }

    let stack_id = perform_stack_update(context, &final_stack_args, args, &global_opts.environment, output_manager).await?;
    
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
    
    let final_status = match watch_stack_with_data_output(
        context,
        &stack_id,
        output_manager,
        std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
    ).await {
        Ok(status) => status,
        Err(error) => {
            let error_info = crate::output::aws_conversion::convert_aws_error_to_error_info(&error);
            output_manager.render(crate::output::OutputData::Error(error_info)).await?;
            return Ok(1);
        }
    };
    
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = determine_operation_success(&final_status, UPDATE_SUCCESS_STATES);
    
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
    
    let final_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_summary).await?;
    
    Ok(if success { 0 } else { 1 })
}

pub async fn update_stack(cli: &Cli, args: &UpdateStackArgs) -> Result<i32> {
    run_command_handler!(update_stack_impl, cli, args)
}

async fn perform_stack_update(
    context: &crate::cfn::CfnContext,
    stack_args: &crate::stack_args::StackArgs,
    args: &UpdateStackArgs,
    environment: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    let builder = CfnRequestBuilder::new(context, stack_args);

    let (update_request, token) = builder.build_update_stack(
        true,
        &CfnOperation::UpdateStack,
        &args.base.argsfile,
        Some(environment),
    ).await?;
    
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    
    let response = update_request.send().await?;

    let stack_id = response.stack_id()
        .ok_or_else(|| anyhow::anyhow!("AWS did not return a stack ID"))?
        .to_string();

    Ok(stack_id)
}

async fn update_stack_with_changeset(
    context: &crate::cfn::CfnContext,
    args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    _stack_name: &str,
    output_manager: &mut DynamicOutputManager,
    environment: &str,
    cli: &Cli,
) -> Result<i32> {
    let stack_name = stack_args.stack_name.as_ref().unwrap();
    let stack_task = {
        let client = context.client.clone();
        let stack_name = stack_name.clone();
        tokio::spawn(async move {
            let stack = crate::cfn::stack_operations::StackInfoService::get_stack(&client, &stack_name).await?;
            let output_data = crate::output::aws_conversion::convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };
    
    output_manager.render(stack_task.await??).await?;
    
    let changeset_name = format!("iidy-update-{}", &context.primary_token().value[..8]);
    
    let changeset_result = crate::cfn::changeset_operations::create_changeset_comprehensive(
        context,
        stack_args,
        Some(&changeset_name),
        &args.base.argsfile,
        false,
        output_manager,
        None,
        Some(environment),
    ).await?;

    output_manager.render(OutputData::ChangeSetResult(changeset_result.clone())).await?;

    let confirmed = if args.yes {
        true
    } else {
        output_manager.request_confirmation("Do you want to execute this changeset now?".to_string()).await?
    };
    
    if !confirmed {
        let elapsed = context.elapsed_seconds().await?;
        let final_summary = create_final_command_summary(true, elapsed);
        output_manager.render(final_summary).await?;
        return Ok(130);
    }

    use super::exec_changeset;
    
    let exec_args = crate::cli::ExecChangeSetArgs {
        changeset_name: changeset_result.changeset_name,
        argsfile: args.base.argsfile.clone(),
        stack_name: Some(changeset_result.stack_name),
    };
    
    let exec_cli = crate::cli::Cli {
        global_opts: cli.global_opts.clone(),
        aws_opts: cli.aws_opts.clone(),
        command: crate::cli::Commands::ExecChangeset(exec_args.clone()),
    };
    
    exec_changeset::exec_changeset(&exec_cli, &exec_args).await
}
