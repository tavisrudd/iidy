use anyhow::Result;

use crate::{
    cfn::{CfnRequestBuilder, CfnContext, create_context_for_operation, stack_operations::{StackInfoService, collect_stack_contents}, CfnOperation, determine_operation_success, CREATE_SUCCESS_STATES, apply_stack_name_override_and_validate, watch_stack::{SenderOutput, watch_stack_live_events_with_seen_events, DEFAULT_POLL_INTERVAL_SECS}},
    cli::{CreateStackArgs, GlobalOpts, Cli, AwsOpts, Commands},
    stack_args::{load_stack_args, StackArgs},
    aws::AwsSettings,
    output::{
        DynamicOutputManager, OutputData, manager::OutputOptions, convert_stack_to_definition,
        aws_conversion::{create_command_metadata, convert_token_info, create_final_command_summary}
    },
};

/// Create a CloudFormation stack following exact iidy-js pattern.
///
/// Implements the complete create-stack flow:
/// 1. Command metadata
/// 2. Stack creation operation  
/// 3. Watch and summarize with stack definition, previous events, live events, and final contents
/// Uses the data-driven output architecture for consistent rendering across output modes.
pub async fn create_stack(cli: &Cli, args: &CreateStackArgs) -> Result<i32> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    // Load stack configuration with full context (AWS credential merging + $envValues injection)
    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = CfnOperation::CreateStack;
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

    // Setup AWS context for create operation
    let context = create_context_for_operation(&opts, CfnOperation::CreateStack).await?;

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
        command: Commands::CreateStack(args.clone()),
    };
    let output_options = OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Show command metadata (exact iidy-js pattern)
    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // 2. Stack operation - create the stack
    let stack_id = perform_stack_creation(&context, &final_stack_args, args, global_opts, &mut output_manager).await?;
    
    // 3. Start parallel data collection and rendering (like watch-stack pattern)
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
    
    // Start live events task (no previous events for create-stack)
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
                std::time::Duration::from_secs(3600), // 1 hour timeout for create operations
                vec![] // No previous events for brand new stack
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
    
    // Calculate elapsed time and determine success based on final stack status
    let elapsed_seconds = context.elapsed_seconds().await?;
    
    // Determine success using centralized helper
    let success = determine_operation_success(&final_status, CREATE_SUCCESS_STATES);
    
    // Skip stack contents if the stack was deleted (unlikely for create-stack, but handle gracefully)
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            // Stack was deleted during creation (e.g., OnFailure=DELETE), skip stack contents
            let final_command_summary = create_final_command_summary(
                false, // Mark as failed since stack was deleted
                elapsed_seconds
            );
            output_manager.render(final_command_summary).await?;
            return Ok(1); // Return exit code 1 for failure
        }
    }
    
    // Final step: Show stack contents (for successful or failed stacks that still exist)
    let stack_contents = collect_stack_contents(&context, &stack_id).await?;
    output_manager.render(OutputData::StackContents(stack_contents)).await?;
    
    // Show final command summary
    let final_command_summary = create_final_command_summary(
        success,
        elapsed_seconds
    );
    output_manager.render(final_command_summary).await?;
    
    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}

/// Perform the actual stack creation operation and return the start time and stack_id for watching
async fn perform_stack_creation(
    context: &CfnContext,
    stack_args: &StackArgs,
    args: &CreateStackArgs,
    global_opts: &GlobalOpts,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    // Setup request builder
    let builder = CfnRequestBuilder::new(context, stack_args);

    // Build and execute the CreateStack request
    let (create_request, token) = builder.build_create_stack(
        &CfnOperation::CreateStack,
        &args.argsfile,
        Some(&global_opts.environment),
    ).await?;
    
    // Show token info
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

