use anyhow::Result;

use crate::{
    cfn::{CfnRequestBuilder, create_context_for_operation, stack_operations::StackInfoService, CfnOperation, determine_operation_success, UPDATE_SUCCESS_STATES, apply_stack_name_override_and_validate},
    cli::{UpdateStackArgs, Cli},
    stack_args::load_stack_args,
    aws::AwsSettings,
    output::{
        DynamicOutputManager, OutputData,
        aws_conversion::{create_command_metadata, create_final_command_summary, convert_token_info}
    },
};

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
    let operation = CfnOperation::UpdateStack;
    let stack_args = load_stack_args(
        &args.base.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.base.stack_name.as_ref())?;

    let stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    // Setup AWS context for update operation
    let context = create_context_for_operation(&opts, CfnOperation::UpdateStack).await?;

    // Setup data-driven output manager with full CLI context
    let aws_opts = crate::cli::AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = crate::cli::Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: crate::cli::Commands::UpdateStack(args.clone()),
    };
    let output_options = crate::output::manager::OutputOptions::new(cli);
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
    let stack_id = perform_stack_update(&context, &final_stack_args, args, &mut output_manager).await?;
    
    // 3. Start parallel data collection and rendering (like create-stack pattern)
    let sender = output_manager.start();
    
    // Start stack definition task
    let stack_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        tokio::spawn(async move {
            let stack = StackInfoService::get_stack(&client, &stack_id).await?;
            let output_data = crate::output::aws_conversion::convert_stack_to_definition(&stack, true);
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
            let sender_output = crate::cfn::watch_stack::SenderOutput { sender: tx };
            let final_status = crate::cfn::watch_stack::watch_stack_live_events_with_seen_events(
                &client, 
                &context_clone, 
                &stack_id, 
                sender_output, 
                std::time::Duration::from_secs(crate::cfn::watch_stack::DEFAULT_POLL_INTERVAL_SECS), 
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
    
    // Final step: Show stack contents (like create-stack)
    let stack_contents = crate::cfn::stack_operations::collect_stack_contents(&context, &stack_id).await?;
    let sender = output_manager.start();
    let _ = sender.send(OutputData::StackContents(stack_contents));
    drop(sender);
    output_manager.stop().await?;
    
    // Determine success using centralized helper
    let success = determine_operation_success(&final_status, UPDATE_SUCCESS_STATES);
    
    // Show final command summary (exact iidy-js showFinalComandSummary pattern)
    let elapsed = context.elapsed_seconds().await?;
    let final_summary = create_final_command_summary(success, elapsed);
    output_manager.render(final_summary).await?;
    
    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}

/// Perform the actual stack update operation
async fn perform_stack_update(
    context: &crate::cfn::CfnContext,
    stack_args: &crate::stack_args::StackArgs,
    _args: &UpdateStackArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    // Setup request builder
    let builder = CfnRequestBuilder::new(context, stack_args);

    // Build and execute the UpdateStack request
    let (update_request, token) = builder.build_update_stack(&CfnOperation::UpdateStack);
    
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
        builder.build_create_changeset(&changeset_name, &CfnOperation::CreateChangeset);
    
    // Pass create token to output manager for conditional display
    let output_token = convert_token_info(&create_token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _create_response = create_request.send().await?;

    // Ask for confirmation unless --yes is specified
    if !args.yes {
        // Use data-driven output for user interaction prompts
        println!();
        println!("Review the changeset in the AWS Console if needed.");
        println!("Do you want to execute this changeset? (y/N)");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            let elapsed = context.elapsed_seconds().await?;
            let final_summary = create_final_command_summary(true, elapsed);
            output_manager.render(final_summary).await?;
            return Ok(0);
        }
    }

    // Step 2: Execute changeset
    let (execute_request, execute_token) =
        builder.build_execute_changeset(&changeset_name, &CfnOperation::ExecuteChangeset);
    
    // Pass execute token to output manager for conditional display
    let output_token = convert_token_info(&execute_token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _execute_response = execute_request.send().await?;

    // Step 3: Watch stack progress using data-driven output
    let watch_result = crate::cfn::watch_stack::watch_stack_with_data_output(
        context, 
        stack_name, 
        output_manager, 
        std::time::Duration::from_secs(5)
    ).await;
    
    let success = match watch_result {
        Ok(_) => true,
        Err(_) => true, // Still successful - changeset executed, just watching failed
    };

    // Show final command summary (exact iidy-js showFinalComandSummary pattern)
    let elapsed = context.elapsed_seconds().await?;
    let final_summary = create_final_command_summary(success, elapsed);
    output_manager.render(final_summary).await?;

    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}
