use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::sync::Arc;

use crate::{
    aws,
    cli::{AwsOpts, DeleteArgs},
    timing::{ReliableTimeProvider, TimeProvider},
};

use super::{watch_stack::watch_stack_with_context, CfnContext};

/// Delete a CloudFormation stack with timing context.
///
/// Uses the timing abstraction for reliable event filtering and elapsed time tracking.
pub async fn delete_stack_with_context(
    ctx: &CfnContext,
    stack_name: &str,
    role_arn: Option<&str>,
    retain_resources: Option<Vec<String>>,
    client_request_token: Option<&str>,
) -> Result<()> {
    println!("🗑️  Deleting stack: {}", stack_name);
    
    // Start the delete operation
    let mut request = ctx.client.delete_stack().stack_name(stack_name);
    
    if let Some(role) = role_arn {
        request = request.role_arn(role);
    }
    
    if let Some(resources) = retain_resources {
        request = request.set_retain_resources(Some(resources));
    }
    
    if let Some(token) = client_request_token {
        request = request.client_request_token(token);
    }
    
    request.send().await?;
    
    println!("✅ Delete operation initiated, watching for completion...");
    
    // Watch the stack deletion
    watch_stack_with_context(ctx, stack_name, std::time::Duration::from_secs(5)).await?;
    
    Ok(())
}

/// Delete a CloudFormation stack.
///
/// This is the main entry point that creates its own timing context.
pub async fn delete_stack(opts: &AwsOpts, args: &DeleteArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let client = Client::new(&config);
    
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let ctx = CfnContext::new(client, time_provider).await?;
    
    delete_stack_with_context(
        &ctx,
        &args.stackname,
        args.role_arn.as_deref(),
        if args.retain_resources.is_empty() { None } else { Some(args.retain_resources.clone()) },
        None, // DeleteArgs doesn't have client_request_token
    ).await
}
