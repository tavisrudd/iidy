use anyhow::Result;
use std::time::Instant;

use crate::{
    cfn::{CfnContext, CfnRequestBuilder, create_context, CfnOperation},
    cli::{UpdateStackArgs, Cli, Commands},
    output::{
        DynamicOutputManager, manager::OutputOptions,
        aws_conversion::{progress_message, success_message, warning_message, create_command_result},
    },
    stack_args::load_stack_args,
    aws::AwsSettings,
};

/// Create or update a CloudFormation stack using intelligent detection with data-driven output.
pub async fn create_or_update(cli: &Cli) -> Result<()> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;
    let args = match &cli.command {
        Commands::CreateOrUpdate(args) => args,
        _ => anyhow::bail!("Invalid command type for create_or_update"),
    };

    let start_time = Instant::now();
    let output_options = OutputOptions::minimal();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;
    // Load stack configuration with full context (AWS credential merging + $envValues injection)
    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = CfnOperation::CreateOrUpdate;
    let stack_args = load_stack_args(
        &args.base.argsfile,
        Some(&global_opts.environment),
        &operation,
        &cli_aws_settings,
    ).await?;

    // Override stack name if provided via CLI
    let mut final_stack_args = stack_args;
    if let Some(ref stack_name) = args.base.stack_name {
        final_stack_args.stack_name = Some(stack_name.clone());
    }

    // Validate required fields
    if final_stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    // Setup AWS client and context
    let context = create_context(&opts, true).await?; // Write operation, needs NTP for precise timing

    let stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    // Check if stack exists
    output_manager.render(progress_message(&format!("Checking if stack '{}' exists...", stack_name))).await?;

    let stack_exists = check_stack_exists(&context, stack_name).await?;

    let result = if stack_exists {
        output_manager.render(progress_message(&format!("Stack '{}' exists, performing update", stack_name))).await?;

        // Use the existing update_stack logic
        if args.changeset {
            update_stack_with_changeset_data(&context, args, &final_stack_args, &mut output_manager).await
        } else {
            update_stack_direct_data(&context, args, &final_stack_args, &mut output_manager).await
        }
    } else {
        output_manager.render(progress_message(&format!(
            "Stack '{}' does not exist, performing create",
            stack_name
        ))).await?;

        // Create stack (changesets not typically used for new stacks)
        if args.changeset {
            output_manager.render(warning_message("Changeset mode not recommended for new stacks, creating directly")).await?;
        }

        create_stack_direct_data(&context, &final_stack_args, &args.base.argsfile, &global_opts.environment, &mut output_manager).await
    };

    // Show final result
    let elapsed = start_time.elapsed().as_secs() as i64;
    match result {
        Ok(_) => {
            output_manager.render(create_command_result(true, elapsed, Some("Create or update operation completed".to_string()))).await?;
        }
        Err(ref e) => {
            output_manager.render(create_command_result(false, elapsed, Some(format!("Create or update operation failed: {}", e)))).await?;
        }
    }

    result
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
) -> Result<()> {
    // Use provided context
    let builder = CfnRequestBuilder::new(&context, stack_args);

    // Build and execute the CreateStack request
    let (create_request, token) = builder.build_create_stack(
        &CfnOperation::CreateOrUpdate,
        argsfile,
        Some(environment),
    ).await?;
    // Pass token to output manager for conditional display
    let output_token = crate::output::aws_conversion::convert_token_info(&token);
    output_manager.render(crate::output::data::OutputData::TokenInfo(output_token)).await?;

    let stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    output_manager.render(progress_message(&format!("Creating stack: {}", stack_name))).await?;

    let response = create_request.send().await?;

    if let Some(stack_id) = response.stack_id() {
        output_manager.render(success_message(&format!("Stack creation initiated: {}", stack_id))).await?;
    } else {
        output_manager.render(success_message("Stack creation initiated")).await?;
    }

    Ok(())
}

/// Update stack directly with data-driven output.
async fn update_stack_direct_data(
    context: &CfnContext,
    _args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<()> {
    // Use provided context
    let builder = CfnRequestBuilder::new(&context, stack_args);

    // Build and execute the UpdateStack request  
    let (update_request, token) = builder.build_update_stack(&CfnOperation::CreateOrUpdate);
    // Pass token to output manager for conditional display
    let output_token = crate::output::aws_conversion::convert_token_info(&token);
    output_manager.render(crate::output::data::OutputData::TokenInfo(output_token)).await?;

    let stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    output_manager.render(progress_message(&format!("Updating stack: {}", stack_name))).await?;

    let response = update_request.send().await?;

    if let Some(stack_id) = response.stack_id() {
        output_manager.render(success_message(&format!("Stack update initiated: {}", stack_id))).await?;
    } else {
        output_manager.render(success_message("Stack update initiated")).await?;
    }

    Ok(())
}

/// Update stack with changeset using data-driven output.
async fn update_stack_with_changeset_data(
    context: &CfnContext,
    args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<()> {
    // Use provided context
    let builder = CfnRequestBuilder::new(&context, stack_args);

    // Step 1: Create changeset
    let changeset_name = format!(
        "iidy-create-or-update-{}",
        &context.primary_token().value[..8]
    );
    let (create_request, create_token) =
        builder.build_create_changeset(&changeset_name, &CfnOperation::CreateChangeset);
    // Pass create_token to output manager for conditional display
    let output_token = crate::output::aws_conversion::convert_token_info(&create_token);
    output_manager.render(crate::output::data::OutputData::TokenInfo(output_token)).await?;

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
    // TODO: Implement interactive prompts in data-driven output system
    if !args.yes {
        output_manager.render(warning_message("Interactive confirmation not yet implemented in data-driven output. Use --yes to proceed automatically.")).await?;
        return Ok(());
    }

    // Step 2: Execute changeset
    let (execute_request, execute_token) =
        builder.build_execute_changeset(&changeset_name, &CfnOperation::ExecuteChangeset);
    // Pass execute_token to output manager for conditional display
    let output_token = crate::output::aws_conversion::convert_token_info(&execute_token);
    output_manager.render(crate::output::data::OutputData::TokenInfo(output_token)).await?;

    output_manager.render(progress_message("Executing changeset...")).await?;

    let _execute_response = execute_request.send().await?;

    output_manager.render(success_message("Changeset execution initiated")).await?;

    // Step 3: Watch stack progress
    use super::watch_stack::watch_stack_with_data_output;
    output_manager.render(progress_message("Watching stack operation progress...")).await?;

    if let Err(e) = watch_stack_with_data_output(
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
    } else {
        output_manager.render(success_message("Stack operation completed successfully")).await?;
    }

    Ok(())
}
