use anyhow::Result;

use crate::cfn::{CfnRequestBuilder, create_context_for_operation, apply_stack_name_override_and_validate, CfnOperation, stack_operations::{StackInfoService, collect_stack_contents, StackEventsService}, determine_operation_success, watch_stack::{SenderOutput, watch_stack_live_events_with_seen_events, DEFAULT_POLL_INTERVAL_SECS}};
use crate::cli::{Cli, ExecChangeSetArgs, AwsOpts, Commands};
use crate::output::{
    DynamicOutputManager, manager::OutputOptions,
    aws_conversion::{convert_token_info, create_command_metadata, create_final_command_summary, convert_stack_to_definition},
    data::{OutputData, StackEventsDisplay}
};
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;

/// Execute a CloudFormation changeset with data-driven output.
pub async fn exec_changeset(cli: &Cli, args: &ExecChangeSetArgs) -> Result<i32> {
    // Extract components from CLI (identical to update-stack.rs)
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
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

    // Setup AWS context
    let context = create_context_for_operation(&opts, operation).await?;

    // Setup data-driven output manager with full CLI context (identical to update-stack.rs)
    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: Commands::ExecChangeset(args.clone()),
    };
    let output_options = OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Show command metadata (exact iidy-js pattern)
    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // 2. Execute changeset operation
    let stack_id = perform_changeset_execution(&context, &final_stack_args, args, &mut output_manager).await?;

    // 3. Start parallel data collection and rendering (identical to update-stack.rs pattern)
    let sender = output_manager.start();
    
    // Start stack definition task
    let stack_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        tokio::spawn(async move {
            let stack = StackInfoService::get_stack(&client, &stack_id).await?;
            let output_data = convert_stack_to_definition(&stack, true);
            let _ = tx.send(output_data);
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Start previous events task (identical to update-stack.rs)
    let previous_events_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        tokio::spawn(async move {
            let events = StackEventsService::fetch_events(&client, &stack_id).await?;
            // Convert AWS events to our format with timing
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
                    duration_seconds: None, // Duration is calculated later
                })
                .collect();
            
            let events_display = StackEventsDisplay {
                title: "Previous Stack Events (max 10):".to_string(),
                events: events_with_timing,
                max_events: Some(10),
                truncated: None,
            };
            let _ = tx.send(OutputData::StackEvents(events_display));
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Start live events task
    let events_task: tokio::task::JoinHandle<Result<Option<String>, anyhow::Error>> = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        let context_clone = context.clone();
        
        tokio::spawn(async move {
            let sender_output = SenderOutput { sender: tx };
            let final_status = watch_stack_live_events_with_seen_events(
                &client, 
                &context_clone, 
                &stack_id, 
                sender_output, 
                std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS), 
                std::time::Duration::from_secs(3600), // 1 hour timeout
                vec![] // No previous events for changeset execution
            ).await?;
            Ok(final_status)
        })
    };
    
    // Drop the original sender so the receiver knows when all tasks are done
    drop(sender);
    
    // Process and render all data from parallel operations
    output_manager.stop().await?;
    
    // Wait for all tasks to complete and handle any errors
    let (stack_result, previous_events_result, events_result) = tokio::join!(
        stack_task,
        previous_events_task,
        events_task
    );
    
    // Propagate any errors from the spawned tasks
    stack_result??;
    previous_events_result??;
    let final_status = events_result??;
    
    let elapsed_seconds = context.elapsed_seconds().await?;
    
    // Define success states for changeset execution
    const CHANGESET_EXECUTE_SUCCESS_STATES: &[&str] = &["UPDATE_COMPLETE", "CREATE_COMPLETE"];
    let success = determine_operation_success(&final_status, CHANGESET_EXECUTE_SUCCESS_STATES);
    
    // Skip stack contents if the stack was deleted
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            let final_command_summary = create_final_command_summary(success, elapsed_seconds);
            output_manager.render(final_command_summary).await?;
            return Ok(1); // Return exit code 1 for failure
        }
    }
    
    // Show stack contents (identical to update-stack.rs)
    let stack_contents = collect_stack_contents(&context, &stack_id).await?;
    output_manager.render(OutputData::StackContents(stack_contents)).await?;
    
    let final_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_summary).await?;
    
    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}

async fn perform_changeset_execution(
    context: &crate::cfn::CfnContext,
    stack_args: &crate::stack_args::StackArgs,
    args: &ExecChangeSetArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    // Setup request builder
    let builder = CfnRequestBuilder::new(context, stack_args);

    let (execute_request, token) = builder.build_execute_changeset(&args.changeset_name, true, &CfnOperation::ExecuteChangeset);
    
    // Show token info
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _response = execute_request.send().await?;

    // For ExecuteChangeset, we need to get the stack ID from the stack name
    let stack_id = StackInfoService::get_stack_id(&context.client, 
        stack_args.stack_name.as_ref().unwrap()).await?;

    Ok(stack_id)
}
