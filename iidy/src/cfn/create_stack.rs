use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::{
    cfn::{CfnRequestBuilder, create_context_for_operation},
    cli::{NormalizedAwsOpts, StackFileArgs, GlobalOpts},
    stack_args::load_stack_args_with_context,
    aws::AwsSettings,
    output::{
        DynamicOutputManager, OutputData, CfnOperation,
        aws_conversion::{create_command_metadata, progress_message, success_message, create_command_result, convert_token_info}
    },
};

/// Create a CloudFormation stack following exact iidy-js pattern.
///
/// Implements the complete create-stack flow:
/// 1. Command metadata
/// 2. Stack creation operation  
/// 3. Watch and summarize with stack definition, previous events, live events, and final contents
/// Uses the data-driven output architecture for consistent rendering across output modes.
pub async fn create_stack(
    opts: &NormalizedAwsOpts, 
    args: &StackFileArgs, 
    global_opts: &GlobalOpts
) -> Result<()> {
    // Load stack configuration with full context (AWS credential merging + $envValues injection)
    let cli_aws_settings = AwsSettings::from_normalized_opts(opts);
    let command = vec!["create-stack".to_string()];
    let stack_args = load_stack_args_with_context(
        &args.argsfile,
        Some(&global_opts.environment),
        &command,
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

    let stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    // Setup AWS context for create operation
    let context = create_context_for_operation(opts, CfnOperation::CreateStack).await?;

    // Setup data-driven output manager with full CLI context
    let output_options = crate::output::manager::OutputOptions::minimal();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Show command metadata (exact iidy-js pattern)
    let command_metadata = create_command_metadata(&context, opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // 2. Stack operation - create the stack
    let start_time = perform_stack_creation(&context, &final_stack_args, args, global_opts, &mut output_manager).await?;
    
    // 3. Watch and summarize (exact iidy-js pattern)
    watch_and_summarize_stack(stack_name, start_time, &context, &mut output_manager).await
}

/// Perform the actual stack creation operation and return the start time for watching
async fn perform_stack_creation(
    context: &crate::cfn::CfnContext,
    stack_args: &crate::stack_args::StackArgs,
    args: &StackFileArgs,
    global_opts: &GlobalOpts,
    output_manager: &mut DynamicOutputManager,
) -> Result<DateTime<Utc>> {
    // Setup request builder
    let builder = CfnRequestBuilder::new(context, stack_args);

    // Build and execute the CreateStack request
    let (create_request, token) = builder.build_create_stack(
        "create-stack",
        &args.argsfile,
        Some(&global_opts.environment),
    ).await?;
    
    // Show token info
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    
    // Record start time before the operation
    let start_time = context.start_time.unwrap_or_else(Utc::now);
    
    output_manager.render(progress_message(&format!("Creating stack: {}", stack_name))).await?;

    let response = create_request.send().await?;

    let success_msg = if let Some(stack_id) = response.stack_id() {
        format!("Stack creation initiated: {}", stack_id)
    } else {
        "Stack creation initiated".to_string()
    };
    
    output_manager.render(success_message(&success_msg)).await?;

    Ok(start_time)
}

/// Watch and summarize stack progress following exact iidy-js pattern
async fn watch_and_summarize_stack(
    stack_name: &str,
    start_time: DateTime<Utc>,
    context: &crate::cfn::CfnContext,
    output_manager: &mut DynamicOutputManager,
) -> Result<()> {
    // Use parallel data collection and rendering like watch-stack
    let sender = output_manager.start();
    
    // Start stack definition task
    let stack_task = {
        let client = context.client.clone();
        let stack_name = stack_name.to_string();
        let tx = sender.clone();
        tokio::spawn(async move {
            let stack_resp = client
                .describe_stacks()
                .stack_name(&stack_name)
                .send()
                .await
                .map_err(anyhow::Error::from)?;
                
            let stack = stack_resp
                .stacks
                .and_then(|mut s| s.pop())
                .ok_or_else(|| anyhow::anyhow!("stack not found"))?;
                
            let output_data = crate::output::aws_conversion::convert_stack_to_definition(&stack, true);
            let _ = tx.send(output_data);
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Sequential execution: previous events MUST complete before live events start
    let events_and_live_task = {
        let client = context.client.clone();
        let stack_name = stack_name.to_string();
        let tx = sender.clone();
        let live_start_time = Some(start_time);
        
        tokio::spawn(async move {
            // Step 1: Fetch and display previous events (max 10 for create-stack)
            let first_events_resp = client
                .describe_stack_events()
                .stack_name(&stack_name)
                .send()
                .await
                .map_err(anyhow::Error::from)?;
        
            let mut all_events = first_events_resp.stack_events.unwrap_or_default();
            let mut next_token = first_events_resp.next_token;
            
            // Fetch additional pages if needed (limit to reasonable amount)
            while next_token.is_some() && all_events.len() < 20 {
                let events_resp = client
                    .describe_stack_events()
                    .stack_name(&stack_name)
                    .set_next_token(next_token)
                    .send()
                    .await?;
                    
                let mut page_events = events_resp.stack_events.unwrap_or_default();
                all_events.append(&mut page_events);
                next_token = events_resp.next_token;
            }
            
            // Show previous events (max 10)
            let output_data = crate::output::aws_conversion::convert_stack_events_to_display_with_max(
                all_events.clone(),
                "Previous Stack Events (max 10):",
                Some(10),
            );
            
            let _ = tx.send(output_data);
            
            // Step 2: Watch live events with timeout
            let sender_output = crate::cfn::watch_stack::SenderOutput { sender: tx };
            crate::cfn::watch_stack::watch_stack_live_events_with_seen_events(
                &client, 
                live_start_time, 
                &stack_name, 
                sender_output, 
                std::time::Duration::from_secs(crate::cfn::watch_stack::DEFAULT_POLL_INTERVAL_SECS), 
                std::time::Duration::from_secs(3600), // 1 hour timeout for create operations
                all_events
            ).await
        })
    };
    
    // Drop the original sender so the receiver knows when all tasks are done
    drop(sender);
    
    // Process and render all data from parallel operations
    output_manager.stop().await?;
    
    // Wait for all tasks to complete and handle any errors
    let (stack_result, events_and_live_result) = tokio::join!(
        stack_task,
        events_and_live_task
    );
    
    // Propagate any errors from the spawned tasks
    stack_result??;
    events_and_live_result??;
    
    // Final step: Show stack contents
    let stack_contents = crate::cfn::watch_stack::collect_stack_contents(context, stack_name).await?;
    let sender = output_manager.start();
    let _ = sender.send(OutputData::StackContents(stack_contents));
    drop(sender);
    output_manager.stop().await?;
    
    // Show final command result
    let elapsed_seconds = (Utc::now() - start_time).num_seconds();
    let command_result = create_command_result(
        true, 
        elapsed_seconds, 
        Some("Stack creation completed successfully".to_string())
    );
    output_manager.render(command_result).await?;
    
    Ok(())
}
