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
/// This demonstrates the full multi-step operation with proper token derivation:
/// 1. Create changeset with derived token
/// 2. Execute changeset with another derived token
/// 3. Watch stack progress
async fn update_stack_with_changeset(
    opts: &NormalizedAwsOpts,
    args: &UpdateStackArgs,
    stack_args: &crate::stack_args::StackArgs,
) -> Result<()> {
    // Setup AWS client and context
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;
    
    // Setup console reporter and request builder
    let reporter = ConsoleReporter::new("update-stack --changeset");
    let builder = CfnRequestBuilder::new(&context, stack_args);
    
    // Show primary token
    reporter.show_primary_token(&context.primary_token());
    
    // Step 1: Create changeset
    let changeset_name = format!("iidy-update-{}", &context.primary_token().value[..8]);
    let (create_request, create_token) = builder.build_create_changeset(&changeset_name, "create-changeset");
    reporter.show_step_token("create-changeset", &create_token);
    
    reporter.show_progress(&format!("Creating changeset '{}' for stack: {}", 
        changeset_name, stack_args.stack_name.as_ref().unwrap()));
    
    let create_response = create_request.send().await?;
    
    if let Some(changeset_id) = create_response.id() {
        reporter.show_success(&format!("Changeset created: {}", changeset_id));
    } else {
        reporter.show_success("Changeset created");
    }
    
    // Ask for confirmation unless --yes is specified
    if !args.yes {
        println!();
        println!("Review the changeset in the AWS Console if needed.");
        println!("Do you want to execute this changeset? (y/N)");
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let input = input.trim().to_lowercase();
        
        if input != "y" && input != "yes" {
            reporter.show_warning("Changeset execution cancelled by user");
            println!("Changeset '{}' has been created but not executed.", changeset_name);
            println!("You can execute it later with:");
            println!("  iidy exec-changeset stack-args.yaml {}", changeset_name);
            reporter.show_operation_summary(&context);
            return Ok(());
        }
    }
    
    // Step 2: Execute changeset
    let (execute_request, execute_token) = builder.build_execute_changeset(&changeset_name, "execute-changeset");
    reporter.show_step_token("execute-changeset", &execute_token);
    
    reporter.show_progress("Executing changeset...");
    
    let _execute_response = execute_request.send().await?;
    
    reporter.show_success("Changeset execution initiated");
    
    // Step 3: Watch stack progress
    use super::watch_stack::watch_stack_with_context;
    reporter.show_progress("Watching stack operation progress...");
    
    if let Err(e) = watch_stack_with_context(&context, stack_args.stack_name.as_ref().unwrap(), 
                                           std::time::Duration::from_secs(5)).await {
        reporter.show_warning(&format!("Error watching stack progress: {}", e));
        println!("The changeset execution was initiated, but there was an error watching progress.");
        println!("You can check the stack status manually in the AWS Console.");
    } else {
        reporter.show_success("Stack update completed successfully");
    }
    
    // Show operation summary
    reporter.show_operation_summary(&context);
    
    Ok(())
}
