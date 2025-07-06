use anyhow::Result;
use aws_sdk_cloudformation::error::SdkError;
use aws_sdk_cloudformation::error::ProvideErrorMetadata;

use crate::cfn::{CfnContext, CfnRequestBuilder, CfnOperation, apply_stack_name_override_and_validate, create_context_for_operation, stack_operations::{StackInfoService, collect_stack_contents}, determine_operation_success, CREATE_SUCCESS_STATES, UPDATE_SUCCESS_STATES, watch_stack::{SenderOutput, watch_stack_live_events_with_seen_events, DEFAULT_POLL_INTERVAL_SECS}, StackChangeType, UpdateResult, changeset_operations};
use crate::cli::{UpdateStackArgs, Cli, AwsOpts, Commands};
use crate::output::{
    DynamicOutputManager, manager::OutputOptions,
    aws_conversion::{progress_message, success_message, warning_message, create_command_result, convert_token_info, create_command_metadata, create_final_command_summary, convert_stack_to_definition},
    data::{OutputData, StackChangeDetails}
};
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;

/// Create or update a CloudFormation stack using intelligent detection with data-driven output.
pub async fn create_or_update(cli: &Cli, args: &UpdateStackArgs) -> Result<i32> {
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

    let context = create_context_for_operation(&opts, operation).await?;

    let stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: Commands::CreateOrUpdate(args.clone()),
    };
    let output_options = OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    let stack_exists = check_stack_exists(&context, stack_name).await?;

    // Check stack existence and determine change type
    let stack_change_details = if stack_exists {
        if args.changeset {
            return update_stack_with_changeset_data(&context, args, &final_stack_args, &mut output_manager).await;
        } else {
            // Try update and check for no-changes case
            match try_update_stack(&context, args, &final_stack_args, &global_opts.environment).await {
                Ok(UpdateResult::NoChanges) => StackChangeDetails {
                    change_type: StackChangeType::UpdateNoChanges,
                    stack_name: stack_name.clone(),
                },
                Ok(UpdateResult::StackId(stack_id)) => StackChangeDetails {
                    change_type: StackChangeType::UpdateWithChanges { stack_id },
                    stack_name: stack_name.clone(),
                },
                Err(e) => return Err(e),
            }
        }
    } else {
        if args.changeset {
            // Use CREATE changeset for new stacks when --changeset is specified
            return create_stack_with_changeset_data(&context, args, &final_stack_args, &mut output_manager, &global_opts, &opts).await;
        }
        StackChangeDetails {
            change_type: StackChangeType::Create,
            stack_name: stack_name.clone(),
        }
    };

    // Send change details to renderer
    output_manager.render(OutputData::StackChangeDetails(stack_change_details.clone())).await?;

    // Continue based on change type  
    match stack_change_details.change_type {
        StackChangeType::UpdateNoChanges => {
            // Early exit is handled in render_stack_change_details
            // Just return success - the renderer will handle cleanup and final summary
            Ok(0)
        },
        StackChangeType::Create => {
            let stack_id = create_stack_direct_data(&context, &final_stack_args, &args.base.argsfile, &global_opts.environment, &mut output_manager).await?;
            watch_and_summarize_stack_operation(&context, &stack_id, &mut output_manager, CREATE_SUCCESS_STATES).await
        },
        StackChangeType::UpdateWithChanges { stack_id } => {
            let token = context.derive_token_for_step(&CfnOperation::CreateOrUpdate);
            let output_token = convert_token_info(&token);
            output_manager.render(OutputData::TokenInfo(output_token)).await?;
            watch_and_summarize_stack_operation(&context, &stack_id, &mut output_manager, UPDATE_SUCCESS_STATES).await
        },
    }
}

/// Watch stack operation and summarize results following create_stack.rs/update_stack.rs pattern
async fn watch_and_summarize_stack_operation(
    context: &CfnContext,
    stack_id: &str,
    output_manager: &mut DynamicOutputManager,
    success_states: &[&str],
) -> Result<i32> {
    let sender = output_manager.start();
    
    let stack_task = {
        let client = context.client.clone();
        let stack_id = stack_id.to_string();
        let tx = sender.clone();
        tokio::spawn(async move {
            let stack = StackInfoService::get_stack(&client, &stack_id).await?;
            let output_data = convert_stack_to_definition(&stack, true);
            let _ = tx.send(output_data);
            Ok::<(), anyhow::Error>(())
        })
    };
    
    let events_task: tokio::task::JoinHandle<Result<Option<String>, anyhow::Error>> = {
        let client = context.client.clone();
        let stack_id = stack_id.to_string();
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
                std::time::Duration::from_secs(3600),
                vec![]
            ).await?;
            Ok(final_status)
        })
    };
    
    drop(sender);
    output_manager.stop().await?;
    
    let (stack_result, events_result) = tokio::join!(
        stack_task,
        events_task
    );
    
    stack_result??;
    let final_status = events_result??;
    
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = determine_operation_success(&final_status, success_states);
    
    // Skip stack contents if the stack was deleted (can happen with failed operations)
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
    
    let final_command_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_command_summary).await?;
    
    Ok(if success { 0 } else { 1 })
}

/// Check if a CloudFormation stack exists.
async fn check_stack_exists(context: &CfnContext, stack_name: &str) -> Result<bool> {
    let describe_request = context.client.describe_stacks().stack_name(stack_name);

    match describe_request.send().await {
        Ok(_) => Ok(true),
        Err(SdkError::ServiceError(e)) => {
            let service_err = e.err();
            if service_err.code() == Some("ValidationError") &&
               service_err.message().unwrap_or("").contains("does not exist") {
                Ok(false)
            } else {
                Err(SdkError::ServiceError(e).into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Create a new stack directly with data-driven output.
async fn create_stack_direct_data(
    context: &CfnContext,
    stack_args: &crate::stack_args::StackArgs,
    argsfile: &str,
    environment: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    let builder = CfnRequestBuilder::new(&context, stack_args);

    let (create_request, token) = builder.build_create_stack(
        true,
        &CfnOperation::CreateOrUpdate,
        argsfile,
        Some(environment),
    ).await?;
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

/// Try to update a stack and return the result without any output.
/// This allows us to detect no-changes case early before showing stack details.
async fn try_update_stack(
    context: &CfnContext,
    args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    environment: &str,
) -> Result<UpdateResult> {
    let builder = CfnRequestBuilder::new(&context, stack_args);

    let (update_request, _token) = builder.build_update_stack(
        true,
        &CfnOperation::CreateOrUpdate,
        &args.base.argsfile,
        Some(environment),
    ).await?;

    match update_request.send().await {
        Ok(response) => {
            let stack_id = response.stack_id()
                .ok_or_else(|| anyhow::anyhow!("AWS did not return a stack ID"))?
                .to_string();
            Ok(UpdateResult::StackId(stack_id))
        }
        Err(SdkError::ServiceError(e)) => {
            let service_err = e.err();
            if service_err.code() == Some("ValidationError") &&
               (service_err.message().unwrap_or("").contains("No updates are to be performed") ||
                service_err.message().unwrap_or("").contains("No changes detected")) {
                // No changes detected - this is a success case, not an error
                Ok(UpdateResult::NoChanges)
            } else {
                Err(anyhow::anyhow!("Update failed: {}", SdkError::ServiceError(e)))
            }
        }
        Err(e) => Err(anyhow::anyhow!("Update failed: {}", e)),
    }
}

/// Update stack with changeset using data-driven output.
async fn update_stack_with_changeset_data(
    context: &CfnContext,
    args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<i32> {
    let builder = CfnRequestBuilder::new(&context, stack_args);

    // Step 1: Create changeset
    let changeset_name = format!(
        "iidy-create-or-update-{}",
        &context.primary_token().value[..8]
    );
    let (create_request, create_token) =
        builder.build_create_changeset(&changeset_name, false, &CfnOperation::CreateChangeset);
    // Pass create_token to output manager for conditional display
    let output_token = convert_token_info(&create_token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    output_manager.render(progress_message(&format!(
        "Creating changeset '{}' for stack: {}",
        changeset_name,
        stack_args.stack_name.as_ref().unwrap()
    ))).await?;

    let create_response = create_request.send().await?;

    if let Some(changeset_id) = create_response.id() {
        output_manager.render(success_message(&format!("Changeset created: {}", changeset_id))).await?;
    } else {
        output_manager.render(success_message("Changeset created")).await?;
    }

    // Ask for confirmation unless --yes is specified
    let confirmed = if args.yes {
        true
    } else {
        output_manager.request_confirmation_with_key(
            "Do you want to execute this changeset now?".to_string(),
            "execute_changeset".to_string()
        ).await?
    };
    
    if !confirmed {
        let elapsed = context.elapsed_seconds().await?;
        output_manager.render(create_command_result(true, elapsed, Some("Changeset execution declined".to_string()))).await?;
        return Ok(130); // 130 = interrupted by user (Ctrl-C equivalent)
    }

    let (execute_request, execute_token) =
        builder.build_execute_changeset(&changeset_name, false, &CfnOperation::ExecuteChangeset);
    // Pass execute_token to output manager for conditional display
    let output_token = convert_token_info(&execute_token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    output_manager.render(progress_message("Executing changeset...")).await?;

    let _execute_response = execute_request.send().await?;

    output_manager.render(success_message("Changeset execution initiated")).await?;

    // Step 3: Watch stack progress
    use super::watch_stack::watch_stack_with_data_output;
    output_manager.render(progress_message("Watching stack operation progress...")).await?;

    let success = if let Err(e) = watch_stack_with_data_output(
        &context,
        stack_args.stack_name.as_ref().unwrap(),
        output_manager,
        std::time::Duration::from_secs(5),
    )
    .await
    {
        output_manager.render(warning_message(&format!("Error watching stack progress: {}", e))).await?;
        output_manager.render(warning_message("The changeset execution was initiated, but there was an error watching progress.")).await?;
        output_manager.render(warning_message("You can check the stack status manually in the AWS Console.")).await?;
        false // Watching failed - unknown operation status, treat as failure
    } else {
        output_manager.render(success_message("Stack operation completed successfully")).await?;
        true
    };

    Ok(if success { 0 } else { 1 })
}

/// Create new stack with changeset using shared changeset functionality.
async fn create_stack_with_changeset_data(
    context: &CfnContext,
    args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    output_manager: &mut DynamicOutputManager,
    global_opts: &crate::cli::GlobalOpts,
    opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let changeset_result = changeset_operations::create_changeset_comprehensive(
        context,
        stack_args,
        None,
        &args.base.argsfile,
        false,
        output_manager,
    ).await?;

    // Render changeset result
    output_manager.render(OutputData::ChangeSetResult(changeset_result.clone())).await?;

    // Ask for confirmation unless --yes is specified
    let confirmed = if args.yes {
        true
    } else {
        output_manager.request_confirmation_with_key(
            "Do you want to execute this changeset now?".to_string(),
            "execute_changeset".to_string()
        ).await?
    };
    
    if !confirmed {
        let elapsed = context.elapsed_seconds().await?;
        output_manager.render(create_command_result(true, elapsed, Some("Changeset execution declined".to_string()))).await?;
        return Ok(130); // 130 = interrupted by user (Ctrl-C equivalent)
    }

    // Execute changeset - use exec_changeset functionality
    use super::exec_changeset;
    
    // Create exec_changeset args from the changeset result
    let exec_args = crate::cli::ExecChangeSetArgs {
        changeset_name: changeset_result.changeset_name,
        argsfile: args.base.argsfile.clone(),
        stack_name: Some(changeset_result.stack_name),
    };
    
    // Reconstruct CLI from the passed options
    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let exec_cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: Commands::ExecChangeset(exec_args.clone()),
    };
    
    // Execute the changeset using the exec_changeset handler
    exec_changeset::exec_changeset(&exec_cli, &exec_args).await
}
