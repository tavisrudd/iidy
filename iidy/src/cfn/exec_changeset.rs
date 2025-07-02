use anyhow::Result;

use crate::cfn::{CfnRequestBuilder, create_context_for_operation, apply_stack_name_override_and_validate};
use crate::cli::{Cli, ExecChangeSetArgs};
use crate::output::{
    DynamicOutputManager, manager::OutputOptions,
    aws_conversion::{progress_message, success_message, warning_message, create_command_result, convert_token_info},
    data::OutputData
};
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;

/// Execute a CloudFormation changeset with data-driven output.
pub async fn exec_changeset(cli: &Cli, args: &ExecChangeSetArgs) -> Result<()> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    let output_options = OutputOptions::minimal();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;
    // Load stack configuration
    // Load stack configuration with full context (AWS credential merging + $envValues injection)
    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = cli.command.to_cfn_operation();
    let stack_args = load_stack_args(
        &args.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.stack_name.as_ref())?;

    // Setup AWS context for changeset execution
    let context = create_context_for_operation(&opts, operation.clone()).await?;

    // Setup request builder
    let builder = CfnRequestBuilder::new(&context, &final_stack_args);

    // Pass primary token to output manager for conditional display
    let primary_token = convert_token_info(&context.primary_token());
    output_manager.render(OutputData::TokenInfo(primary_token)).await?;

    // Build and execute the ExecuteChangeSet request
    let (execute_request, token) =
        builder.build_execute_changeset(&args.changeset_name, &operation);
    
    // Pass token to output manager for conditional display
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    output_manager.render(progress_message(&format!(
        "Executing changeset '{}' for stack: {}",
        args.changeset_name,
        final_stack_args.stack_name.as_ref().unwrap()
    ))).await?;

    let result = match execute_request.send().await {
        Ok(_response) => {
            output_manager.render(success_message("Changeset execution initiated")).await?;

            // Watch the stack operation progress
            use super::watch_stack::watch_stack_with_data_output;
            output_manager.render(progress_message("Watching stack operation progress...")).await?;

            if let Err(e) = watch_stack_with_data_output(
                &context,
                final_stack_args.stack_name.as_ref().unwrap(),
                &mut output_manager,
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
        Err(e) => Err(e.into())
    };

    // Show final result
    let elapsed = context.elapsed_seconds().await?;
    match result {
        Ok(_) => {
            output_manager.render(create_command_result(true, elapsed, Some("Changeset execution completed".to_string()))).await?;
        }
        Err(ref e) => {
            output_manager.render(create_command_result(false, elapsed, Some(format!("Changeset execution failed: {}", e)))).await?;
        }
    }

    result
}
