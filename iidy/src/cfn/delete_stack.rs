use anyhow::Result;
use aws_sdk_cloudformation::error::SdkError;
use aws_sdk_cloudformation::error::ProvideErrorMetadata;

use crate::cfn::{
    create_context_for_operation, stack_operations::{StackInfoService, StackEventsService, collect_stack_contents}, 
    CfnOperation, determine_operation_success, DELETE_SUCCESS_STATES
};
use crate::cli::{DeleteArgs, Cli};
use crate::output::{
    DynamicOutputManager, OutputData,
    aws_conversion::{
        create_command_metadata, convert_token_info, 
        convert_stack_to_definition, convert_stack_events_to_display_with_max,
        create_final_command_summary, convert_aws_error_to_error_info
    },
    manager::OutputOptions,
    data::StackAbsentInfo
};
use crate::stack_args::StackArgs;
use crate::cfn::watch_stack::{SenderOutput, watch_stack_live_events_with_seen_events, DEFAULT_POLL_INTERVAL_SECS};

// Remove old error handling import - now using data-driven error system
use super::CfnContext;

/// Get AWS account and auth principal information
async fn get_aws_identity_info(aws_config: &aws_config::SdkConfig) -> (String, String) {
    let sts_client = aws_sdk_sts::Client::new(aws_config);
    
    match sts_client.get_caller_identity().send().await {
        Ok(response) => {
            let account = response.account().unwrap_or("unknown").to_string();
            let auth_arn = response.arn().unwrap_or("arn:aws:iam::unknown:user/current").to_string();
            (account, auth_arn)
        }
        Err(_) => {
            // Fall back to placeholder if STS call fails
            ("unknown".to_string(), "arn:aws:iam::unknown:user/current".to_string())
        }
    }
}

/// Check if a stack exists for deletion, properly handling different error types
async fn check_stack_exists_for_delete(context: &CfnContext, stack_name: &str) -> Result<Option<aws_sdk_cloudformation::types::Stack>> {
    let describe_request = context.client.describe_stacks().stack_name(stack_name);

    match describe_request.send().await {
        Ok(response) => {
            // Stack exists, return the first stack from the response
            let stack = response
                .stacks
                .and_then(|mut stacks| stacks.pop())
                .ok_or_else(|| anyhow::anyhow!("stack '{}' not found in response", stack_name))?;
            Ok(Some(stack))
        }
        Err(SdkError::ServiceError(e)) => {
            let service_err = e.err();
            if service_err.code() == Some("ValidationError") &&
               service_err.message().unwrap_or("").contains("does not exist") {
                // Stack doesn't exist - this is expected for delete operations
                Ok(None)
            } else {
                // Real error (expired token, access denied, etc.) - propagate it directly
                Err(SdkError::ServiceError(e).into())
            }
        }
        Err(e) => {
            // Other errors (network, timeout, etc.) - propagate them directly
            Err(e.into())
        }
    }
}

/// Perform the actual stack deletion operation and return the start time and stack_id for watching
async fn perform_stack_deletion(
    context: &CfnContext,
    stack_name: &str,
    args: &DeleteArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    // Derive a token for the delete operation
    let token = context.derive_token_for_step(&CfnOperation::DeleteStack);
    
    // Show token info
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    // Get stack ID before deletion
    let stack_id = StackInfoService::get_stack_id(&context.client, stack_name).await?;

    // Build the delete request
    let mut request = context
        .client
        .delete_stack()
        .stack_name(stack_name)
        .client_request_token(&token.value);

    if let Some(role) = &args.role_arn {
        request = request.role_arn(role);
    }

    if !args.retain_resources.is_empty() {
        request = request.set_retain_resources(Some(args.retain_resources.clone()));
    }

    // Execute the delete operation
    match request.send().await {
        Ok(_) => {
            Ok(stack_id)
        }
        Err(e) => {
            let anyhow_error = anyhow::Error::from(e);
            let error_info = convert_aws_error_to_error_info(&anyhow_error);
            output_manager.render(OutputData::Error(error_info)).await?;
            Err(anyhow_error)
        }
    }
}

/// Delete a CloudFormation stack following exact iidy-js pattern.
///
/// Implements the complete delete-stack flow:
/// 1. Command metadata
/// 2. Stack information and confirmation 
/// 3. Delete operation
/// 4. Watch deletion progress with live events
/// Uses the data-driven output architecture for consistent rendering across output modes.
pub async fn delete_stack(cli: &Cli, args: &DeleteArgs) -> Result<i32> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    let stack_name = &args.stackname;
    
    // Setup AWS context for delete operation
    let context = create_context_for_operation(&opts, CfnOperation::DeleteStack).await?;

    // Setup data-driven output manager with full CLI context
    let output_options = OutputOptions::new(cli.clone());
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Check if stack exists and get its information
    let (stack, stack_id) = match check_stack_exists_for_delete(&context, stack_name).await {
        Ok(Some(stack)) => {
            let stack_id = StackInfoService::get_stack_id(&context.client, stack_name).await?;
            (stack, stack_id)
        }
        Ok(None) => {
            // Stack doesn't exist - create iidy-js style info data
            let (account, auth_arn) = get_aws_identity_info(&context.aws_config).await;
            let stack_absent_info = StackAbsentInfo {
                stack_name: stack_name.clone(),
                environment: global_opts.environment.clone(),
                region: opts.region.clone().unwrap_or_else(|| "us-east-1".to_string()),
                account,
                auth_arn,
            };
            output_manager.render(OutputData::StackAbsentInfo(stack_absent_info)).await?;
            let elapsed_seconds = context.elapsed_seconds().await?;
            let final_command_summary = create_final_command_summary(
                true, // Mark as success since stack is already deleted (matches iidy-js behavior)
                elapsed_seconds
            );
            output_manager.render(final_command_summary).await?;
            return Ok(0); // Return exit code 0 for success
        }
        Err(e) => {
            // Real error (not stack-not-found) - show user-friendly error via renderer
            let error_info = convert_aws_error_to_error_info(&e);
            output_manager.render(OutputData::Error(error_info)).await?;
            return Ok(1); // Return exit code 1 for failure
        }
    };

    // 2. Prepare command metadata synchronously (doesn't require AWS API calls)
    let minimal_stack_args = StackArgs {
        stack_name: Some(stack_name.clone()),
        template: None,
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        ..Default::default()
    };
    let command_metadata = create_command_metadata(&context, &opts, &minimal_stack_args, &global_opts.environment).await?;

    // 3. Start parallel data collection and rendering (following predefined sections pattern)
    let sender = output_manager.start();
    
    // Send command metadata immediately (prepared synchronously but sent via channel for ordering)
    let _ = sender.send(OutputData::CommandMetadata(command_metadata));
    
    // Start stack definition task
    let stack_definition_task = {
        let tx = sender.clone();
        let stack_clone = stack.clone();
        tokio::spawn(async move {
            let stack_definition = convert_stack_to_definition(&stack_clone, true);
            let _ = tx.send(stack_definition);
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Start previous events task (max 10 events like watch-stack)
    let previous_events_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        tokio::spawn(async move {
            let events = StackEventsService::fetch_events(&client, &stack_id).await?;
            let events_display = convert_stack_events_to_display_with_max(
                events,
                "Previous Stack Events (max 10):",
                Some(10),
            );
            let _ = tx.send(events_display);
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Collect stack contents directly before starting async tasks (since we can't clone context)
    let stack_contents = collect_stack_contents(&context, &stack_id).await?;
    
    // Send stack contents to the output manager
    let stack_contents_task = {
        let tx = sender.clone();
        let stack_contents_clone = stack_contents.clone();
        tokio::spawn(async move {
            let _ = tx.send(OutputData::StackContents(stack_contents_clone));
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Drop the original sender so the receiver knows when all tasks are done
    drop(sender);
    
    // Process and render all data from parallel operations
    output_manager.stop().await?;
    
    // Wait for all initial tasks to complete before proceeding with deletion
    let (definition_result, events_result, contents_result) = tokio::join!(
        stack_definition_task,
        previous_events_task,
        stack_contents_task
    );
    
    // Propagate any errors from the spawned tasks
    definition_result??;
    events_result??;
    contents_result??;
    
    // 4. Request confirmation before deletion
    let confirmed = if args.yes {
        true
    } else {
        let message = format!("Are you sure you want to DELETE the stack {}?", stack_name);
        output_manager.request_confirmation(message).await?
    };
    
    if !confirmed {
        let elapsed_seconds = context.elapsed_seconds().await?;
        let final_summary = create_final_command_summary(true, elapsed_seconds);
        output_manager.render(final_summary).await?;
        return Ok(130); // 130 = interrupted by user (Ctrl-C equivalent)
    }
    
    // 5. Perform deletion and start live events monitoring  
    let _ = perform_stack_deletion(&context, stack_name, args, &mut output_manager).await?;
    
    // 6. Start live events monitoring in parallel pattern
    let sender = output_manager.start();
    
    let live_events_task: tokio::task::JoinHandle<Result<Option<String>, anyhow::Error>> = {
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
                std::time::Duration::from_secs(3600), // 1 hour timeout for delete operations
                vec![] // No previous events for delete operation (they were already shown)
            ).await?;
            Ok(final_status)
        })
    };
    
    // Drop sender and process live events
    drop(sender);
    output_manager.stop().await?;
    
    // Wait for live events to complete
    let final_status = live_events_task.await??;
    
    // 7. Determine success based on final stack status and show final command summary
    let elapsed_seconds = context.elapsed_seconds().await?;
    
    // Determine success using centralized helper
    let success = determine_operation_success(&final_status, DELETE_SUCCESS_STATES);
    
    let final_command_summary = create_final_command_summary(
        success,
        elapsed_seconds
    );
    output_manager.render(final_command_summary).await?;
    
    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}
