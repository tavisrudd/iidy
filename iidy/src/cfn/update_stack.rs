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
use crate::cfn::watch_stack::{DEFAULT_POLL_INTERVAL_SECS, watch_stack_with_data_output};
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
        return update_stack_with_changeset(&context, args, &final_stack_args, stack_name, &mut output_manager, &global_opts.environment).await;
    }

    // 2. Direct update mode - perform stack update operation
    let stack_id = perform_stack_update(&context, &final_stack_args, args, &global_opts.environment, &mut output_manager).await?;
    
    // 3. Start parallel data collection and rendering using sequential await pattern
    
    // Start stack definition task
    let stack_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let stack = StackInfoService::get_stack(&client, &stack_id).await?;
            let output_data = convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };
    
    // Await and render stack definition first
    crate::await_and_render!(stack_task, output_manager);
    
    // Then handle live events watching using the existing helper function
    let final_status = match watch_stack_with_data_output(
        &context,
        &stack_id,
        &mut output_manager,
        std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
    ).await {
        Ok(status) => status,
        Err(error) => {
            let error_info = crate::output::aws_conversion::convert_aws_error_to_error_info(&error);
            output_manager.render(crate::output::OutputData::Error(error_info)).await?;
            return Ok(1);
        }
    };
    
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
    _stack_name: &str,
    output_manager: &mut DynamicOutputManager,
    environment: &str,
) -> Result<i32> {
    // Step 1: Fetch and render stack definition first
    let stack_name = stack_args.stack_name.as_ref().unwrap();
    let stack_task = {
        let client = context.client.clone();
        let stack_name = stack_name.clone();
        tokio::spawn(async move {
            let stack = crate::cfn::stack_operations::StackInfoService::get_stack(&client, &stack_name).await?;
            let output_data = crate::output::aws_conversion::convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };
    
    // Await and render stack definition
    crate::await_and_render!(stack_task, output_manager);
    
    // Step 2: Create changeset using shared changeset operations
    let changeset_name = format!("iidy-update-{}", &context.primary_token().value[..8]);
    
    let changeset_result = crate::cfn::changeset_operations::create_changeset_comprehensive(
        context,
        stack_args,
        Some(&changeset_name),
        &args.base.argsfile,
        false, // Don't use primary token for changeset creation
        output_manager,
        None, // No description for update-stack changeset
        Some(environment),
    ).await?;

    // Render changeset result
    output_manager.render(OutputData::ChangeSetResult(changeset_result.clone())).await?;

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

    // Step 2: Execute changeset using shared exec_changeset functionality
    use super::exec_changeset;
    
    // Create exec_changeset args from the changeset result
    let exec_args = crate::cli::ExecChangeSetArgs {
        changeset_name: changeset_result.changeset_name,
        argsfile: args.base.argsfile.clone(),
        stack_name: Some(changeset_result.stack_name),
    };
    
    // We need to reconstruct the CLI context for exec_changeset
    let aws_opts = crate::cli::AwsOpts {
        region: context.client.config().region().map(|r| r.to_string()),
        profile: None, // Profile info is not easily accessible from context
        assume_role_arn: None,
        client_request_token: Some(context.primary_token().value.clone()),
    };
    let exec_cli = crate::cli::Cli {
        global_opts: crate::cli::GlobalOpts {
            environment: environment.to_string(),
            output_mode: None, 
            color: crate::cli::ColorChoice::Auto,
            theme: crate::cli::Theme::Auto,
            debug: false,
            log_full_error: false,
        },
        aws_opts,
        command: crate::cli::Commands::ExecChangeset(exec_args.clone()),
    };
    
    // Execute the changeset using the exec_changeset handler
    exec_changeset::exec_changeset(&exec_cli, &exec_args).await
}
