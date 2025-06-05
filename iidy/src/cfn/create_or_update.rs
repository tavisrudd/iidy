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

/// Create or update a CloudFormation stack using intelligent detection.
/// 
/// This function checks if the stack exists and automatically chooses between
/// create or update operations. It supports both direct operations and 
/// changeset-based workflows with proper token management.
pub async fn create_or_update(opts: &NormalizedAwsOpts, args: &UpdateStackArgs) -> Result<()> {
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
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }
    
    // Setup AWS client and context
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;
    
    // Setup console reporter
    let reporter = ConsoleReporter::new("create-or-update");
    reporter.show_primary_token(&context.primary_token());
    
    let stack_name = final_stack_args.stack_name.as_ref().unwrap();
    
    // Check if stack exists
    reporter.show_progress(&format!("Checking if stack '{}' exists...", stack_name));
    
    let stack_exists = check_stack_exists(&context, stack_name).await?;
    
    if stack_exists {
        reporter.show_progress(&format!("Stack '{}' exists, performing update", stack_name));
        
        // Use the existing update_stack logic
        if args.changeset {
            update_stack_with_changeset(opts, args, &final_stack_args).await
        } else {
            update_stack_direct(opts, args, &final_stack_args).await
        }
    } else {
        reporter.show_progress(&format!("Stack '{}' does not exist, performing create", stack_name));
        
        // Create stack (changesets not typically used for new stacks)
        if args.changeset {
            reporter.show_warning("Changeset mode not recommended for new stacks, creating directly");
        }
        
        create_stack_direct(opts, &final_stack_args).await
    }
}

/// Check if a CloudFormation stack exists.
async fn check_stack_exists(context: &CfnContext, stack_name: &str) -> Result<bool> {
    let describe_request = context.client.describe_stacks()
        .stack_name(stack_name);
    
    match describe_request.send().await {
        Ok(_) => Ok(true),
        Err(err) => {
            // Check if it's a "stack does not exist" error
            let error_message = format!("{}", err);
            if error_message.contains("does not exist") || error_message.contains("ValidationError") {
                Ok(false)
            } else {
                // Some other error occurred
                Err(anyhow::anyhow!("Error checking stack existence: {}", err))
            }
        }
    }
}

/// Create a new stack directly (reusing create_stack logic).
async fn create_stack_direct(opts: &NormalizedAwsOpts, stack_args: &crate::stack_args::StackArgs) -> Result<()> {
    // Setup context and builder  
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;
    
    let reporter = ConsoleReporter::new("create-stack");
    let builder = CfnRequestBuilder::new(&context, stack_args);
    
    // Build and execute the CreateStack request
    let (create_request, token) = builder.build_create_stack("create-stack");
    reporter.show_step_token("create-stack", &token);
    
    reporter.show_progress(&format!("Creating stack: {}", stack_args.stack_name.as_ref().unwrap()));
    
    let response = create_request.send().await?;
    
    if let Some(stack_id) = response.stack_id() {
        reporter.show_success(&format!("Stack creation initiated: {}", stack_id));
        println!("Stack ID: {}", stack_id);
    } else {
        reporter.show_success("Stack creation initiated");
    }
    
    // Show operation summary
    reporter.show_operation_summary(&context);
    
    Ok(())
}

/// Update stack directly (reusing update_stack logic).
async fn update_stack_direct(opts: &NormalizedAwsOpts, _args: &UpdateStackArgs, stack_args: &crate::stack_args::StackArgs) -> Result<()> {
    // Setup context and builder
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;
    
    let reporter = ConsoleReporter::new("update-stack");
    let builder = CfnRequestBuilder::new(&context, stack_args);
    
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

/// Update stack with changeset (reusing update_stack logic).
async fn update_stack_with_changeset(opts: &NormalizedAwsOpts, args: &UpdateStackArgs, stack_args: &crate::stack_args::StackArgs) -> Result<()> {
    // Reuse the existing changeset workflow from update_stack
    // This demonstrates code reuse while maintaining token correlation
    
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;
    
    let reporter = ConsoleReporter::new("create-or-update --changeset");
    let builder = CfnRequestBuilder::new(&context, stack_args);
    
    // Step 1: Create changeset
    let changeset_name = format!("iidy-create-or-update-{}", &context.primary_token().value[..8]);
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
            println!("  iidy exec-changeset {} {}", args.base.argsfile, changeset_name);
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
        reporter.show_success("Stack operation completed successfully");
    }
    
    // Show operation summary
    reporter.show_operation_summary(&context);
    
    Ok(())
}
