use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;
use std::sync::Arc;

use crate::{
    aws,
    cli::{NormalizedAwsOpts, StackFileArgs},
    stack_args::load_stack_args_file,
    timing::{ReliableTimeProvider, TimeProvider},
    cfn::{CfnContext, CfnRequestBuilder, ConsoleReporter},
};

/// Create a CloudFormation stack using the request builder pattern.
/// 
/// This function loads stack arguments, creates the necessary context and builders,
/// and executes the CreateStack operation with proper token management.
pub async fn create_stack(opts: &NormalizedAwsOpts, args: &StackFileArgs) -> Result<()> {
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
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }
    
    // Setup AWS client and context
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;
    
    // Setup console reporter and request builder
    let reporter = ConsoleReporter::new("create-stack");
    let builder = CfnRequestBuilder::new(&context, &final_stack_args);
    
    // Show primary token
    reporter.show_primary_token(&context.primary_token());
    
    // Build and execute the CreateStack request
    let (create_request, token) = builder.build_create_stack("create-stack");
    reporter.show_step_token("create-stack", &token);
    
    reporter.show_progress(&format!("Creating stack: {}", final_stack_args.stack_name.as_ref().unwrap()));
    
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
