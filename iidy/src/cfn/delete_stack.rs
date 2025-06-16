use anyhow::Result;

use crate::{
    cfn::{ConsoleReporter, create_context},
    cli::{DeleteArgs, NormalizedAwsOpts},
};

use super::{CfnContext, watch_stack::watch_stack_with_context};

/// Delete a CloudFormation stack with timing context and console reporting.
///
/// Uses the timing abstraction for reliable event filtering and elapsed time tracking.
pub async fn delete_stack_with_context(
    ctx: &CfnContext,
    stack_name: &str,
    role_arn: Option<&str>,
    retain_resources: Option<Vec<String>>,
    reporter: &ConsoleReporter,
) -> Result<()> {
    // Derive a token for the delete operation
    let token = ctx.derive_token_for_step("delete-stack");
    reporter.show_step_token("delete-stack", &token);

    reporter.show_progress(&format!("Deleting stack: {}", stack_name));

    // Start the delete operation
    let mut request = ctx
        .client
        .delete_stack()
        .stack_name(stack_name)
        .client_request_token(&token.value);

    if let Some(role) = role_arn {
        request = request.role_arn(role);
    }

    if let Some(resources) = retain_resources {
        request = request.set_retain_resources(Some(resources));
    }

    request.send().await?;

    reporter.show_success("Delete operation initiated, watching for completion...");

    // Watch the stack deletion
    watch_stack_with_context(ctx, stack_name, std::time::Duration::from_secs(5)).await?;

    Ok(())
}

/// Delete a CloudFormation stack.
///
/// This is the main entry point that creates its own timing context.
pub async fn delete_stack(opts: &NormalizedAwsOpts, args: &DeleteArgs) -> Result<()> {
    let ctx = create_context(opts).await?;

    // Setup console reporter
    let reporter = ConsoleReporter::new("delete-stack");

    // Show primary token
    reporter.show_primary_token(&ctx.primary_token());

    let result = delete_stack_with_context(
        &ctx,
        &args.stackname,
        args.role_arn.as_deref(),
        if args.retain_resources.is_empty() {
            None
        } else {
            Some(args.retain_resources.clone())
        },
        &reporter,
    )
    .await;

    // Show operation summary
    reporter.show_operation_summary(&ctx);

    result
}
