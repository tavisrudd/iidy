use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::{
    cfn::{CfnRequestBuilder, create_context_for_operation, stack_operations::StackInfoService, CfnOperation},
    cli::{StackFileArgs, GlobalOpts, Cli, Commands},
    stack_args::load_stack_args,
    aws::AwsSettings,
    output::{
        DynamicOutputManager, OutputData,
        aws_conversion::{create_command_metadata, convert_token_info}
    },
};

/// Create a CloudFormation stack following exact iidy-js pattern.
///
/// Implements the complete create-stack flow:
/// 1. Command metadata
/// 2. Stack creation operation  
/// 3. Watch and summarize with stack definition, previous events, live events, and final contents
/// Uses the data-driven output architecture for consistent rendering across output modes.
pub async fn create_stack(cli: &Cli) -> Result<i32> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;
    let args = match &cli.command {
        Commands::CreateStack(args) => args,
        _ => anyhow::bail!("Invalid command type for create_stack"),
    };

    // Load stack configuration with full context (AWS credential merging + $envValues injection)
    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = CfnOperation::CreateStack;
    let stack_args = load_stack_args(
        &args.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    // Override stack name if provided via CLI
    let mut final_stack_args = stack_args;
    if let Some(ref stack_name) = args.stack_name {
        final_stack_args.stack_name = Some(stack_name.clone());
    }

    // Validate required fields
    if final_stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }
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
    let aws_opts = crate::cli::AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = crate::cli::Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: crate::cli::Commands::CreateStack(args.clone()),
    };
    let output_options = crate::output::manager::OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Show command metadata (exact iidy-js pattern)
    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // 2. Stack operation - create the stack
    let (start_time, stack_id) = perform_stack_creation(&context, &final_stack_args, args, global_opts, &mut output_manager).await?;
    
    // 3. Start parallel data collection and rendering (like watch-stack pattern)
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
    
    // Start live events task (no previous events for create-stack)
    let events_task: tokio::task::JoinHandle<Result<Option<String>, anyhow::Error>> = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        let live_start_time = context.start_time;
        
        tokio::spawn(async move {
            let sender_output = crate::cfn::watch_stack::SenderOutput { sender: tx };
            let final_status = crate::cfn::watch_stack::watch_stack_live_events_with_seen_events(
                &client, 
                live_start_time, 
                &stack_id, 
                sender_output, 
                std::time::Duration::from_secs(crate::cfn::watch_stack::DEFAULT_POLL_INTERVAL_SECS), 
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
    let elapsed_seconds = (Utc::now() - start_time).num_seconds();
    
    // Expected successful terminal states for create-stack (based on iidy-js spec)
    let expected_success_states = ["CREATE_COMPLETE"];
    let success = final_status.as_ref()
        .map(|status| expected_success_states.contains(&status.as_str()))
        .unwrap_or(false);
    
    // Skip stack contents if the stack was deleted (unlikely for create-stack, but handle gracefully)
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            // Stack was deleted during creation (e.g., OnFailure=DELETE), skip stack contents
            let final_command_summary = crate::output::aws_conversion::create_final_command_summary(
                false, // Mark as failed since stack was deleted
                elapsed_seconds
            );
            output_manager.render(final_command_summary).await?;
            return Ok(1); // Return exit code 1 for failure
        }
    }
    
    // Final step: Show stack contents (for successful or failed stacks that still exist)
    let stack_contents = crate::cfn::stack_operations::collect_stack_contents(&context, &stack_id).await?;
    output_manager.render(crate::output::OutputData::StackContents(stack_contents)).await?;
    
    // Show final command summary
    let final_command_summary = crate::output::aws_conversion::create_final_command_summary(
        success,
        elapsed_seconds
    );
    output_manager.render(final_command_summary).await?;
    
    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}

/// Perform the actual stack creation operation and return the start time and stack_id for watching
async fn perform_stack_creation(
    context: &crate::cfn::CfnContext,
    stack_args: &crate::stack_args::StackArgs,
    args: &StackFileArgs,
    global_opts: &GlobalOpts,
    output_manager: &mut DynamicOutputManager,
) -> Result<(DateTime<Utc>, String)> {
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
    
    // Record start time before the operation
    let start_time = context.start_time.unwrap_or_else(Utc::now);

    let response = create_request.send().await?;

    let stack_id = response.stack_id()
        .ok_or_else(|| anyhow::anyhow!("Stack creation response did not include stack ID"))?
        .to_string();

    Ok((start_time, stack_id))
}

