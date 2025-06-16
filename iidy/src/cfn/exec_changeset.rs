use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;
use std::sync::Arc;

use crate::{
    aws,
    cfn::{CfnContext, CfnRequestBuilder, ConsoleReporter},
    cli::{ExecChangeSetArgs, NormalizedAwsOpts},
    stack_args::load_stack_args_file,
    timing::{ReliableTimeProvider, TimeProvider},
};

/// Execute a CloudFormation changeset using the request builder pattern.
///
/// This function executes a previously created changeset with proper token derivation.
/// After execution, it watches the stack operation progress.
pub async fn exec_changeset(opts: &NormalizedAwsOpts, args: &ExecChangeSetArgs) -> Result<()> {
    // Load stack configuration
    let stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;

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

    // Setup console reporter and request builder
    let reporter = ConsoleReporter::new("exec-changeset");
    let builder = CfnRequestBuilder::new(&context, &final_stack_args);

    // Show primary token
    reporter.show_primary_token(&context.primary_token());

    // Build and execute the ExecuteChangeSet request
    let (execute_request, token) =
        builder.build_execute_changeset(&args.changeset_name, "execute-changeset");
    reporter.show_step_token("execute-changeset", &token);

    reporter.show_progress(&format!(
        "Executing changeset '{}' for stack: {}",
        args.changeset_name,
        final_stack_args.stack_name.as_ref().unwrap()
    ));

    let _response = execute_request.send().await?;

    reporter.show_success("Changeset execution initiated");

    // Watch the stack operation progress
    use super::watch_stack::watch_stack_with_context;
    reporter.show_progress("Watching stack operation progress...");

    if let Err(e) = watch_stack_with_context(
        &context,
        final_stack_args.stack_name.as_ref().unwrap(),
        std::time::Duration::from_secs(5),
    )
    .await
    {
        reporter.show_warning(&format!("Error watching stack progress: {}", e));
        println!(
            "The changeset execution was initiated, but there was an error watching progress."
        );
        println!("You can check the stack status manually in the AWS Console.");
    } else {
        reporter.show_success("Stack operation completed successfully");
    }

    // Show operation summary
    reporter.show_operation_summary(&context);

    Ok(())
}
