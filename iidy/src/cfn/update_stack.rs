use anyhow::Result;
use std::path::Path;
use std::time::Instant;

use crate::{
    cfn::{CfnRequestBuilder, create_context},
    cli::{NormalizedAwsOpts, UpdateStackArgs, GlobalOpts},
    stack_args::load_stack_args_file,
    output::{
        DynamicOutputManager, OutputData, create_command_metadata, 
        progress_message, success_message, warning_message, create_command_result
    },
};

/// Update a CloudFormation stack using the request builder pattern.
///
/// This function supports both direct updates and changeset-based updates
/// depending on the --changeset flag in UpdateStackArgs.
/// Uses the data-driven output architecture for consistent rendering across output modes.
pub async fn update_stack(
    opts: &NormalizedAwsOpts, 
    args: &UpdateStackArgs, 
    global_opts: &GlobalOpts
) -> Result<()> {
    // Load stack configuration
    let stack_args = load_stack_args_file(Path::new(&args.base.argsfile), None)?;

    // Override stack name if provided via CLI
    let mut final_stack_args = stack_args;
    if let Some(ref stack_name) = args.base.stack_name {
        final_stack_args.stack_name = Some(stack_name.clone());
    }

    // Validate required fields
    if final_stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }

    // Check if changeset mode is requested
    if args.changeset {
        return update_stack_with_changeset(opts, args, &final_stack_args, global_opts).await;
    }

    // Direct update mode
    update_stack_direct(opts, args, &final_stack_args, global_opts).await
}

/// Perform a direct stack update without using changesets.
async fn update_stack_direct(
    opts: &NormalizedAwsOpts,
    _args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    global_opts: &GlobalOpts,
) -> Result<()> {
    let start_time = Instant::now();

    // Setup AWS client and context
    let context = create_context(opts).await?;

    // Setup data-driven output manager
    let output_options = crate::output::manager::OutputOptions {
        color_choice: global_opts.color,
        theme: global_opts.theme,
        terminal_width: None, // Will auto-detect
        buffer_limit: 100,
    };
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // Show command metadata
    let command_metadata = create_command_metadata(&context, opts, stack_args, "update-stack", &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // Setup request builder
    let builder = CfnRequestBuilder::new(&context, stack_args);

    // Build and execute the UpdateStack request
    let (update_request, _token) = builder.build_update_stack("update-stack");

    let stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    
    output_manager.render(progress_message(&format!("Updating stack: {}", stack_name))).await?;

    let response = update_request.send().await?;

    let success_msg = if let Some(stack_id) = response.stack_id() {
        format!("Stack update initiated: {}", stack_id)
    } else {
        "Stack update initiated".to_string()
    };
    
    output_manager.render(success_message(&success_msg)).await?;

    // Show final operation result
    let elapsed = start_time.elapsed().as_secs() as i64;
    let command_result = create_command_result(true, elapsed, Some("Stack update completed".to_string()));
    output_manager.render(command_result).await?;

    Ok(())
}

/// Perform a stack update using changesets for preview and safer deployment.
///
/// This demonstrates the full multi-step operation with proper token derivation:
/// 1. Create changeset with derived token
/// 2. Execute changeset with another derived token
/// 3. Watch stack progress
async fn update_stack_with_changeset(
    opts: &NormalizedAwsOpts,
    args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
    global_opts: &GlobalOpts,
) -> Result<()> {
    let start_time = Instant::now();

    // Setup AWS client and context
    let context = create_context(opts).await?;

    // Setup data-driven output manager
    let output_options = crate::output::manager::OutputOptions {
        color_choice: global_opts.color,
        theme: global_opts.theme,
        terminal_width: None, // Will auto-detect
        buffer_limit: 100,
    };
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // Show command metadata
    let command_metadata = create_command_metadata(&context, opts, stack_args, "update-stack --changeset", &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // Setup request builder
    let builder = CfnRequestBuilder::new(&context, stack_args);

    // Step 1: Create changeset
    let changeset_name = format!("iidy-update-{}", &context.primary_token().value[..8]);
    let (create_request, _create_token) =
        builder.build_create_changeset(&changeset_name, "create-changeset");

    let stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    
    output_manager.render(progress_message(&format!(
        "Creating changeset '{}' for stack: {}",
        changeset_name, stack_name
    ))).await?;

    let create_response = create_request.send().await?;

    let changeset_success_msg = if let Some(changeset_id) = create_response.id() {
        format!("Changeset created: {}", changeset_id)
    } else {
        "Changeset created".to_string()
    };
    output_manager.render(success_message(&changeset_success_msg)).await?;

    // Ask for confirmation unless --yes is specified
    if !args.yes {
        println!();
        println!("Review the changeset in the AWS Console if needed.");
        println!("Do you want to execute this changeset? (y/N)");

        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();

        if input != "y" && input != "yes" {
            output_manager.render(warning_message("Changeset execution cancelled by user")).await?;
            println!(
                "Changeset '{}' has been created but not executed.",
                changeset_name
            );
            println!("You can execute it later with:");
            println!("  iidy exec-changeset stack-args.yaml {}", changeset_name);
            
            let elapsed = start_time.elapsed().as_secs() as i64;
            let command_result = create_command_result(true, elapsed, Some("Changeset created (not executed)".to_string()));
            output_manager.render(command_result).await?;
            return Ok(());
        }
    }

    // Step 2: Execute changeset
    let (execute_request, _execute_token) =
        builder.build_execute_changeset(&changeset_name, "execute-changeset");

    output_manager.render(progress_message("Executing changeset...")).await?;

    let _execute_response = execute_request.send().await?;

    output_manager.render(success_message("Changeset execution initiated")).await?;

    // Step 3: Watch stack progress
    use super::watch_stack::watch_stack_with_context;
    output_manager.render(progress_message("Watching stack operation progress...")).await?;

    // Use the stack_name we already validated
    let watch_result = watch_stack_with_context(&context, stack_name, std::time::Duration::from_secs(5)).await;
    
    let (success, final_message) = match watch_result {
        Ok(_) => {
            output_manager.render(success_message("Stack update completed successfully")).await?;
            (true, "Stack update completed successfully".to_string())
        }
        Err(e) => {
            output_manager.render(warning_message(&format!("Error watching stack progress: {}", e))).await?;
            println!(
                "The changeset execution was initiated, but there was an error watching progress."
            );
            println!("You can check the stack status manually in the AWS Console.");
            (true, "Changeset executed (watch failed)".to_string()) // Still successful - just watching failed
        }
    };

    // Show final operation result
    let elapsed = start_time.elapsed().as_secs() as i64;
    let command_result = create_command_result(success, elapsed, Some(final_message));
    output_manager.render(command_result).await?;

    Ok(())
}
