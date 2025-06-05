use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;
use std::sync::Arc;

use crate::{
    aws,
    cli::{NormalizedAwsOpts, UpdateStackArgs},
    stack_args::load_stack_args_file,
    timing::{ReliableTimeProvider, TimeProvider},
    cfn::{CfnContext, CfnRequestBuilder, ConsoleReporter},
};

/// Update a CloudFormation stack using the request builder pattern.
/// 
/// This function supports both direct updates and changeset-based updates
/// depending on the --changeset flag in UpdateStackArgs.
pub async fn update_stack(opts: &NormalizedAwsOpts, args: &UpdateStackArgs) -> Result<()> {
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
        return update_stack_with_changeset(opts, args, &final_stack_args).await;
    }
    
    // Direct update mode
    update_stack_direct(opts, args, &final_stack_args).await
}

/// Perform a direct stack update without using changesets.
async fn update_stack_direct(
    opts: &NormalizedAwsOpts, 
    _args: &UpdateStackArgs, 
    stack_args: &crate::stack_args::StackArgs
) -> Result<()> {
    // Setup AWS client and context
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;
    
    // Setup console reporter and request builder
    let reporter = ConsoleReporter::new("update-stack");
    let builder = CfnRequestBuilder::new(&context, stack_args);
    
    // Show primary token
    reporter.show_primary_token(&context.primary_token());
    
    // Build and execute the UpdateStack request
    let (update_request, token) = builder.build_update_stack("update-stack");
    reporter.show_step_token("update-stack", &token);
    
    reporter.show_progress(&format!("Updating stack: {}", stack_args.stack_name.as_ref().unwrap()));
    
    let response = update_request.send().await?;
    
    if let Some(stack_id) = response.stack_id() {
        reporter.show_success(&format!("Stack update initiated: {}", stack_id));
        println!("Stack ID: {}", stack_id);
    } else {
        reporter.show_success("Stack update initiated");
    }
    
    // Show operation summary
    reporter.show_operation_summary(&context);
    
    Ok(())
}

/// Perform a stack update using changesets for preview and safer deployment.
/// 
/// This is a placeholder implementation. The full changeset workflow will be
/// implemented in Phase 3 with proper multi-step token management.
async fn update_stack_with_changeset(
    _opts: &NormalizedAwsOpts,
    _args: &UpdateStackArgs,
    _stack_args: &crate::stack_args::StackArgs,
) -> Result<()> {
    // TODO: Implement changeset workflow in Phase 3
    // This will involve:
    // 1. Create changeset with derived token
    // 2. Display changeset preview
    // 3. Execute changeset with another derived token
    // 4. Watch stack progress
    anyhow::bail!("Changeset-based updates not yet implemented. Use without --changeset flag for direct updates.")
}
