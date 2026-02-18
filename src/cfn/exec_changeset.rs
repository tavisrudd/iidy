use anyhow::Result;

use crate::cfn::{
    CfnContext, CfnOperation, CfnRequestBuilder, StackArgs, apply_stack_name_override_and_validate,
    constants::DEFAULT_PREVIOUS_EVENTS_COUNT,
    stack_operations::{StackEventsService, StackInfoService, watch_stack_operation_and_summarize},
};
use crate::cli::{Cli, ExecChangeSetArgs};
use crate::output::{
    DynamicOutputManager,
    aws_conversion::{convert_token_info, create_command_metadata},
    data::{OutputData, StackEventsDisplay},
};
use crate::run_command_handler_with_stack_args;

pub async fn exec_changeset_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &ExecChangeSetArgs,
    opts: &crate::cli::NormalizedAwsOpts,
    stack_args: &StackArgs,
) -> Result<i32> {
    let global_opts = &cli.global_opts;

    let final_stack_args =
        apply_stack_name_override_and_validate(stack_args.clone(), args.stack_name.as_ref())?;

    let _stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    let command_metadata =
        create_command_metadata(context, opts, &final_stack_args, &global_opts.environment).await?;
    output_manager
        .render(OutputData::CommandMetadata(command_metadata))
        .await?;

    let stack_id =
        perform_changeset_execution(context, &final_stack_args, args, output_manager).await?;

    // Display previous events first (unique to exec_changeset)
    let previous_events_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let events = StackEventsService::fetch_events(&client, &stack_id).await?;
            let events_with_timing: Vec<crate::output::data::StackEventWithTiming> = events
                .into_iter()
                .map(|event| crate::output::data::StackEventWithTiming {
                    event: crate::output::data::StackEvent {
                        event_id: event.event_id().unwrap_or("unknown").to_string(),
                        stack_id: event.stack_id().unwrap_or("unknown").to_string(),
                        stack_name: event.stack_name().unwrap_or("unknown").to_string(),
                        timestamp: event
                            .timestamp()
                            .and_then(StackEventsService::aws_timestamp_to_chrono),
                        resource_status: event
                            .resource_status()
                            .map(|s| s.as_str())
                            .unwrap_or("UNKNOWN")
                            .to_string(),
                        resource_type: event.resource_type().unwrap_or("Unknown").to_string(),
                        logical_resource_id: event
                            .logical_resource_id()
                            .unwrap_or("Unknown")
                            .to_string(),
                        physical_resource_id: event.physical_resource_id().map(|s| s.to_string()),
                        resource_status_reason: event
                            .resource_status_reason()
                            .map(|s| s.to_string()),
                        resource_properties: event.resource_properties().map(|s| s.to_string()),
                        client_request_token: event.client_request_token().map(|s| s.to_string()),
                    },
                    duration_seconds: None,
                })
                .collect();

            let events_display = StackEventsDisplay {
                title: format!(
                    "Previous Stack Events (max {}):",
                    DEFAULT_PREVIOUS_EVENTS_COUNT
                ),
                events: events_with_timing,
                max_events: Some(DEFAULT_PREVIOUS_EVENTS_COUNT),
                truncated: None,
            };
            Ok::<OutputData, anyhow::Error>(OutputData::StackEvents(events_display))
        })
    };

    output_manager.render(previous_events_task.await??).await?;

    // Use shared pattern for stack definition, watching, and summary
    const CHANGESET_EXECUTE_SUCCESS_STATES: &[&str] = &["UPDATE_COMPLETE", "CREATE_COMPLETE"];
    watch_stack_operation_and_summarize(
        context,
        &stack_id,
        output_manager,
        CHANGESET_EXECUTE_SUCCESS_STATES,
    )
    .await
}

pub async fn exec_changeset(cli: &Cli, args: &ExecChangeSetArgs) -> Result<i32> {
    run_command_handler_with_stack_args!(exec_changeset_impl, cli, args, &args.argsfile)
}

async fn perform_changeset_execution(
    context: &CfnContext,
    stack_args: &StackArgs,
    args: &ExecChangeSetArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    let builder = CfnRequestBuilder::new(context, stack_args);

    let (execute_request, token) = builder.build_execute_changeset(
        &args.changeset_name,
        true,
        &CfnOperation::ExecuteChangeset,
    );

    let output_token = convert_token_info(&token);
    output_manager
        .render(OutputData::TokenInfo(output_token))
        .await?;

    let _response = execute_request.send().await?;

    let stack_id =
        StackInfoService::get_stack_id(&context.client, stack_args.stack_name.as_ref().unwrap())
            .await?;

    Ok(stack_id)
}

/// Helper to create CLI and call exec_changeset - avoids duplicating CLI reconstruction
pub async fn call_exec_changeset_with_reconstruction(
    changeset_name: String,
    stack_name: String,
    argsfile: Option<String>,
    global_opts: &crate::cli::GlobalOpts,
    aws_opts: &crate::cli::AwsOpts,
) -> Result<i32> {
    use super::exec_changeset;

    // Create exec_changeset args from the changeset result
    let exec_args = ExecChangeSetArgs {
        changeset_name,
        argsfile: argsfile.unwrap_or_default(),
        stack_name: Some(stack_name),
    };

    // Reconstruct CLI - this is the pattern we're extracting
    let exec_cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts: aws_opts.clone(),
        command: crate::cli::Commands::ExecChangeset(exec_args.clone()),
    };

    exec_changeset::exec_changeset(&exec_cli, &exec_args).await
}
