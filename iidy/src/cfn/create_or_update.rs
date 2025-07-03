use anyhow::Result;

use crate::cfn::{CfnContext, CfnRequestBuilder, CfnOperation, apply_stack_name_override_and_validate, create_context_for_operation, stack_operations::{StackInfoService, collect_stack_contents}, determine_operation_success, CREATE_SUCCESS_STATES, UPDATE_SUCCESS_STATES, watch_stack::{SenderOutput, watch_stack_live_events_with_seen_events, DEFAULT_POLL_INTERVAL_SECS}};
use crate::cli::{UpdateStackArgs, Cli, AwsOpts, Commands};
use crate::output::{
    DynamicOutputManager, manager::OutputOptions,
    aws_conversion::{progress_message, success_message, warning_message, create_command_result, convert_token_info, create_command_metadata, create_final_command_summary, convert_stack_to_definition},
    data::OutputData
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

    output_manager.render(progress_message(&format!("Checking if stack '{}' exists...", stack_name))).await?;

    let stack_exists = check_stack_exists(&context, stack_name).await?;

    if stack_exists {
        output_manager.render(progress_message(&format!("Stack '{}' exists, performing update", stack_name))).await?;

        if args.changeset {
            return update_stack_with_changeset_data(&context, args, &final_stack_args, &mut output_manager).await;
        } else {
            let stack_id = update_stack_direct_data(&context, args, &final_stack_args, &mut output_manager).await?;
            return watch_and_summarize_stack_operation(&context, &stack_id, &mut output_manager, UPDATE_SUCCESS_STATES).await;
        }
    } else {
        output_manager.render(progress_message(&format!(
            "Stack '{}' does not exist, performing create",
            stack_name
        ))).await?;

        if args.changeset {
            output_manager.render(warning_message("Changeset mode not recommended for new stacks, creating directly")).await?;
        }

        let stack_id = create_stack_direct_data(&context, &final_stack_args, &args.base.argsfile, &global_opts.environment, &mut output_manager).await?;
        return watch_and_summarize_stack_operation(&context, &stack_id, &mut output_manager, CREATE_SUCCESS_STATES).await;
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
        Err(err) => {
            // Check if it's a "stack does not exist" error
            let error_message = format!("{}", err);
            if error_message.contains("does not exist") || error_message.contains("ValidationError")
            {
                Ok(false)
            } else {
                // Some other error occurred
                Err(anyhow::anyhow!("Error checking stack existence: {}", err))
            }
        }
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

    // Build and execute the CreateStack request
    let (create_request, token) = builder.build_create_stack(
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

/// Update stack directly with data-driven output.
async fn update_stack_direct_data(
    context: &CfnContext,
    _args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    let builder = CfnRequestBuilder::new(&context, stack_args);

    // Build and execute the UpdateStack request  
    let (update_request, token) = builder.build_update_stack(&CfnOperation::CreateOrUpdate);
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    let response = update_request.send().await?;

    let stack_id = response.stack_id()
        .ok_or_else(|| anyhow::anyhow!("AWS did not return a stack ID"))?
        .to_string();

    Ok(stack_id)
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
        builder.build_create_changeset(&changeset_name, &CfnOperation::CreateChangeset);
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
        output_manager.request_confirmation("Do you want to execute this changeset now?".to_string()).await?
    };
    
    if !confirmed {
        let elapsed = context.elapsed_seconds().await?;
        output_manager.render(create_command_result(true, elapsed, Some("Changeset execution declined".to_string()))).await?;
        return Ok(130); // 130 = interrupted by user (Ctrl-C equivalent)
    }

    // Step 2: Execute changeset
    let (execute_request, execute_token) =
        builder.build_execute_changeset(&changeset_name, &CfnOperation::ExecuteChangeset);
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
