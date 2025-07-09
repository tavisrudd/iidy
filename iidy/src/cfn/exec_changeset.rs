use anyhow::Result;

use crate::cfn::{CfnRequestBuilder, apply_stack_name_override_and_validate, CfnOperation, stack_operations::{StackInfoService, collect_stack_contents, StackEventsService}, determine_operation_success, constants::{DEFAULT_POLL_INTERVAL_SECS, DEFAULT_PREVIOUS_EVENTS_COUNT}};
use crate::cli::{Cli, ExecChangeSetArgs};
use crate::output::{
    DynamicOutputManager,
    aws_conversion::{convert_token_info, create_command_metadata, create_final_command_summary, convert_stack_to_definition},
    data::{OutputData, StackEventsDisplay}
};
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;
use crate::run_command_handler;

async fn exec_changeset_impl(
    output_manager: &mut DynamicOutputManager,
    context: &crate::cfn::CfnContext,
    cli: &Cli,
    args: &ExecChangeSetArgs,
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

    let _stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    let command_metadata = create_command_metadata(context, opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    let stack_id = perform_changeset_execution(context, &final_stack_args, args, output_manager).await?;

    let stack_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let stack = StackInfoService::get_stack(&client, &stack_id).await?;
            let output_data = convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };
    
    let previous_events_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let events = StackEventsService::fetch_events(&client, &stack_id).await?;
            let events_with_timing: Vec<crate::output::data::StackEventWithTiming> = events.into_iter()
                .map(|event| crate::output::data::StackEventWithTiming {
                    event: crate::output::data::StackEvent {
                        event_id: event.event_id().unwrap_or("unknown").to_string(),
                        stack_id: event.stack_id().unwrap_or("unknown").to_string(),
                        stack_name: event.stack_name().unwrap_or("unknown").to_string(),
                        timestamp: event.timestamp().and_then(StackEventsService::aws_timestamp_to_chrono),
                        resource_status: event.resource_status().map(|s| s.as_str()).unwrap_or("UNKNOWN").to_string(),
                        resource_type: event.resource_type().unwrap_or("Unknown").to_string(),
                        logical_resource_id: event.logical_resource_id().unwrap_or("Unknown").to_string(),
                        physical_resource_id: event.physical_resource_id().map(|s| s.to_string()),
                        resource_status_reason: event.resource_status_reason().map(|s| s.to_string()),
                        resource_properties: event.resource_properties().map(|s| s.to_string()),
                        client_request_token: event.client_request_token().map(|s| s.to_string()),
                    },
                    duration_seconds: None,
                })
                .collect();
            
            let events_display = StackEventsDisplay {
                title: format!("Previous Stack Events (max {}):", DEFAULT_PREVIOUS_EVENTS_COUNT),
                events: events_with_timing,
                max_events: Some(DEFAULT_PREVIOUS_EVENTS_COUNT),
                truncated: None,
            };
            Ok::<OutputData, anyhow::Error>(OutputData::StackEvents(events_display))
        })
    };
    
    output_manager.render(stack_task.await??).await?;
    output_manager.render(previous_events_task.await??).await?;
    
    use super::watch_stack::watch_stack_with_data_output;
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
    
    const CHANGESET_EXECUTE_SUCCESS_STATES: &[&str] = &["UPDATE_COMPLETE", "CREATE_COMPLETE"];
    let success = determine_operation_success(&final_status, CHANGESET_EXECUTE_SUCCESS_STATES);
    
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            let final_command_summary = create_final_command_summary(success, elapsed_seconds);
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

pub async fn exec_changeset(cli: &Cli, args: &ExecChangeSetArgs) -> Result<i32> {
    run_command_handler!(exec_changeset_impl, cli, args)
}

async fn perform_changeset_execution(
    context: &crate::cfn::CfnContext,
    stack_args: &crate::stack_args::StackArgs,
    args: &ExecChangeSetArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    let builder = CfnRequestBuilder::new(context, stack_args);

    let (execute_request, token) = builder.build_execute_changeset(&args.changeset_name, true, &CfnOperation::ExecuteChangeset);
    
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _response = execute_request.send().await?;

    let stack_id = StackInfoService::get_stack_id(&context.client, 
        stack_args.stack_name.as_ref().unwrap()).await?;

    Ok(stack_id)
}
