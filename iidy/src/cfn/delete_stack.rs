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
        create_command_metadata, 
        convert_stack_to_definition, convert_stack_events_to_display_with_max,
        create_final_command_summary, convert_aws_error_to_error_info
    },
    manager::OutputOptions,
    data::StackAbsentInfo
};
use crate::stack_args::StackArgs;
use crate::cfn::watch_stack::{ManagerOutput, watch_stack_live_events_with_seen_events, DEFAULT_POLL_INTERVAL_SECS};

use super::CfnContext;

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


async fn perform_stack_deletion_without_output(
    context: &CfnContext,
    stack_name: &str,
    stack_id: &str,
    args: &DeleteArgs,
) -> Result<String> {
    let token = context.primary_token();

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
    request.send().await?;
    Ok(stack_id.to_string())
}

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
            let stack_absent_info = StackAbsentInfo {
                stack_name: stack_name.clone(),
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

    // 3. Render command metadata immediately
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;
    
    // 4. Render stack definition immediately (synchronous conversion)
    let stack_definition = convert_stack_to_definition(&stack, true);
    output_manager.render(stack_definition).await?;
    
    // Start parallel data collection tasks for async operations  
    let previous_events_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let events = StackEventsService::fetch_events(&client, &stack_id).await?;
            let events_display = convert_stack_events_to_display_with_max(
                events,
                "Previous Stack Events (max 10):",
                Some(10),
            );
            Ok::<OutputData, anyhow::Error>(events_display)
        })
    };
    
    // Start stack contents task
    let stack_contents_task = {
        let context_clone = context.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let stack_contents = collect_stack_contents(&context_clone, &stack_id).await?;
            Ok::<OutputData, anyhow::Error>(OutputData::StackContents(stack_contents))
        })
    };
    
    // Await and render remaining async tasks in correct section order
    crate::await_and_render!(previous_events_task, output_manager);
    crate::await_and_render!(stack_contents_task, output_manager);
    
    // 5. Request confirmation before deletion
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
            
    // 6. Perform deletion (similar to create_stack pattern)
    let stack_id_for_deletion = perform_stack_deletion_without_output(&context, stack_name, &stack_id, args).await?;

    // 7. Handle live events directly with the output manager (sequential await pattern)
    let final_status = {
        let live_events_context = context.clone();
        let manager_output = ManagerOutput { manager: &mut output_manager };
        match watch_stack_live_events_with_seen_events(
            &live_events_context.client, 
            &live_events_context, 
            &stack_id_for_deletion, 
            manager_output,
            std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS), 
            std::time::Duration::from_secs(3600), // 1 hour timeout for delete operations
            vec![] // No previous events for delete operation (they were already shown)
        ).await {
            Ok(status) => status,
            Err(error) => {
                let error_info = convert_aws_error_to_error_info(&error);
                output_manager.render(OutputData::Error(error_info)).await?;
                return Ok(1);
            }
        }
    };
    
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = determine_operation_success(&final_status, DELETE_SUCCESS_STATES);
    let final_command_summary = create_final_command_summary(
        success,
        elapsed_seconds
    );
    output_manager.render(final_command_summary).await?;
    
    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}
