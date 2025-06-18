use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use crate::{
    aws,
    cfn::{CfnContext, CfnRequestBuilder},
    cli::{ExecChangeSetArgs, NormalizedAwsOpts, GlobalOpts},
    output::{
        DynamicOutputManager, manager::OutputOptions,
        aws_conversion::{progress_message, success_message, warning_message, create_command_result},
    },
    stack_args::load_stack_args_file,
    timing::{ReliableTimeProvider, TimeProvider},
};

/// Execute a CloudFormation changeset with data-driven output.
pub async fn exec_changeset(
    opts: &NormalizedAwsOpts, 
    args: &ExecChangeSetArgs,
    global_opts: &GlobalOpts
) -> Result<()> {
    let start_time = Instant::now();
    let output_options = OutputOptions::default();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;
    // Load stack configuration
    let stack_args = load_stack_args_file(Path::new(&args.argsfile), Some(&global_opts.environment))?;

    // Override stack name if provided via CLI
    let mut final_stack_args = stack_args;
    if let Some(ref stack_name) = args.stack_name {
        final_stack_args.stack_name = Some(stack_name.clone());
    }

    // Validate required fields
    if final_stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }

    // Setup AWS client and context
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;

    // Setup request builder
    let builder = CfnRequestBuilder::new(&context, &final_stack_args);

    // Show primary token
    output_manager.render(progress_message(&format!("Primary operation token: {}", context.primary_token().value))).await?;

    // Build and execute the ExecuteChangeSet request
    let (execute_request, token) =
        builder.build_execute_changeset(&args.changeset_name, "execute-changeset");
    output_manager.render(progress_message(&format!("Execute changeset token: {}", token.value))).await?;

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
    let elapsed = start_time.elapsed().as_secs() as i64;
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
