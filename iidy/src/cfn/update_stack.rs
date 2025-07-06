use anyhow::Result;

use crate::cfn::{
    CfnRequestBuilder, create_context_for_operation, stack_operations::StackInfoService, 
    CfnOperation, determine_operation_success, UPDATE_SUCCESS_STATES, apply_stack_name_override_and_validate
};
use crate::cli::{UpdateStackArgs, Cli};
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;
use crate::output::{
    DynamicOutputManager, OutputData,
    aws_conversion::{create_command_metadata, create_final_command_summary, convert_token_info, convert_stack_to_definition},
    manager::OutputOptions
};
use crate::cfn::stack_operations::collect_stack_contents;
use crate::cfn::watch_stack::{SenderOutput, watch_stack_live_events_with_seen_events, DEFAULT_POLL_INTERVAL_SECS, watch_stack_with_data_output};
use crate::cli::{AwsOpts, Commands};

/// Update a CloudFormation stack following exact iidy-js pattern.
///
/// Implements the complete update-stack flow:
/// 1. Command metadata
/// 2. Stack update operation (direct or changeset-based)
/// 3. Watch and summarize with stack definition, live events, and final contents
/// Uses the data-driven output architecture for consistent rendering across output modes.
/// Returns exit code: 0 for success, 1 for failure, 130 for interrupt.
pub async fn update_stack(cli: &Cli, args: &UpdateStackArgs) -> Result<i32> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
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

    // Setup AWS context for update operation
    let context = create_context_for_operation(&opts, operation).await?;

    // Setup data-driven output manager with full CLI context
    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: Commands::UpdateStack(args.clone()),
    };
    let output_options = OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Show command metadata (exact iidy-js pattern)
    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // Check if changeset mode is requested
    if args.changeset {
        return update_stack_with_changeset(&context, args, &final_stack_args, stack_name, &mut output_manager).await;
    }

    // 2. Direct update mode - perform stack update operation
    let stack_id = perform_stack_update(&context, &final_stack_args, args, &global_opts.environment, &mut output_manager).await?;
    
    // 3. Start parallel data collection and rendering (like create-stack pattern)
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
    
    // Start live events task (no previous events for update-stack, similar to create-stack)
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
                std::time::Duration::from_secs(3600), // 1 hour timeout for update operations
                vec![] // No previous events for update operations
            ).await?;
            Ok(final_status)
        })
    };
    
    // Drop the original sender so the receiver knows when all tasks are done
    drop(sender);
    
    // Process and render all data from parallel operations
    output_manager.stop().await?;
    
    // Wait for all tasks to complete and handle any errors
    let (stack_result, events_result) = tokio::join!(
        stack_task,
        events_task
    );
    
    // Propagate any errors from the spawned tasks
    stack_result??;
    let final_status = events_result??;
    
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = determine_operation_success(&final_status, UPDATE_SUCCESS_STATES);
    
    // Skip stack contents if the stack was deleted (can happen with failed updates)
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            let final_command_summary = create_final_command_summary(
                false, // Mark as failed since stack was deleted
                elapsed_seconds
            );
            output_manager.render(final_command_summary).await?;
            return Ok(1); // Return exit code 1 for failure
        }
    }
    
    let stack_contents = collect_stack_contents(&context, &stack_id).await?;
    output_manager.render(OutputData::StackContents(stack_contents)).await?;
    
    let final_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_summary).await?;
    
    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}

/// Perform the actual stack update operation
async fn perform_stack_update(
    context: &crate::cfn::CfnContext,
    stack_args: &crate::stack_args::StackArgs,
    args: &UpdateStackArgs,
    environment: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    // Setup request builder
    let builder = CfnRequestBuilder::new(context, stack_args);

    let (update_request, token) = builder.build_update_stack(
        true,
        &CfnOperation::UpdateStack,
        &args.base.argsfile,
        Some(environment),
    ).await?;
    
    // Pass token to output manager for conditional display
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    
    // Execute the update request
    let response = update_request.send().await?;

    let stack_id = response.stack_id()
        .ok_or_else(|| anyhow::anyhow!("AWS did not return a stack ID"))?
        .to_string();

    // Return stack ID for monitoring
    Ok(stack_id)
}

/// Perform a stack update using changesets for preview and safer deployment.
///
/// This demonstrates the full multi-step operation with proper token derivation:
/// 1. Create changeset with derived token
/// 2. Execute changeset with another derived token
/// 3. Watch stack progress
async fn update_stack_with_changeset(
    context: &crate::cfn::CfnContext,
    args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    stack_name: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<i32> {
    // Setup request builder
    let builder = CfnRequestBuilder::new(context, stack_args);

    // Step 1: Create changeset
    let changeset_name = format!("iidy-update-{}", &context.primary_token().value[..8]);
    let (create_request, create_token) =
        builder.build_create_changeset(&changeset_name, false, &CfnOperation::CreateChangeset);
    
    // Pass create token to output manager for conditional display
    let output_token = convert_token_info(&create_token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _create_response = create_request.send().await?;

    // Ask for confirmation unless --yes is specified (exact iidy-js message)
    let confirmed = if args.yes {
        true
    } else {
        output_manager.request_confirmation("Do you want to execute this changeset now?".to_string()).await?
    };
    
    if !confirmed {
        let elapsed = context.elapsed_seconds().await?;
        let final_summary = create_final_command_summary(true, elapsed);
        output_manager.render(final_summary).await?;
        return Ok(130); // 130 = interrupted by user (Ctrl-C equivalent)
    }

    let (execute_request, execute_token) =
        builder.build_execute_changeset(&changeset_name, false, &CfnOperation::ExecuteChangeset);
    
    // Pass execute token to output manager for conditional display
    let output_token = convert_token_info(&execute_token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _execute_response = execute_request.send().await?;

    // Step 3: Watch stack progress using data-driven output
    let watch_result = watch_stack_with_data_output(
        context, 
        stack_name, 
        output_manager, 
        std::time::Duration::from_secs(5)
    ).await;
    
    let success = match watch_result {
        Ok(_) => true,
        Err(_) => false, // Watching failed - unknown operation status, treat as failure
    };

    // Show final command summary (exact iidy-js showFinalComandSummary pattern)
    let elapsed = context.elapsed_seconds().await?;
    let final_summary = create_final_command_summary(success, elapsed);
    output_manager.render(final_summary).await?;

    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}
