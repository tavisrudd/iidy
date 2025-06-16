use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;
use std::sync::Arc;

use crate::{
    aws,
    cfn::{CfnContext, CfnRequestBuilder, ConsoleReporter},
    cli::{CreateChangeSetArgs, NormalizedAwsOpts},
    stack_args::load_stack_args_file,
    timing::{ReliableTimeProvider, TimeProvider},
};

/// Create a CloudFormation changeset using the request builder pattern.
///
/// This function creates a changeset with proper token derivation and console feedback.
/// The changeset can then be reviewed and executed separately.
pub async fn create_changeset(opts: &NormalizedAwsOpts, args: &CreateChangeSetArgs) -> Result<()> {
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
    let reporter = ConsoleReporter::new("create-changeset");
    let builder = CfnRequestBuilder::new(&context, &final_stack_args);

    // Show primary token
    reporter.show_primary_token(&context.primary_token());

    // Determine changeset name
    let default_changeset_name = format!("iidy-{}", &context.primary_token().value[..8]);
    let changeset_name = args
        .changeset_name
        .as_deref()
        .unwrap_or(&default_changeset_name);

    // Build and execute the CreateChangeSet request
    let (create_request, token) =
        builder.build_create_changeset(changeset_name, "create-changeset");
    reporter.show_step_token("create-changeset", &token);

    reporter.show_progress(&format!(
        "Creating changeset '{}' for stack: {}",
        changeset_name,
        final_stack_args.stack_name.as_ref().unwrap()
    ));

    let response = create_request.send().await?;

    if let Some(changeset_id) = response.id() {
        reporter.show_success(&format!("Changeset created: {}", changeset_id));
        println!("Changeset ID: {}", changeset_id);

        if let Some(stack_id) = response.stack_id() {
            println!("Stack ID: {}", stack_id);
        }
    } else {
        reporter.show_success("Changeset created");
    }

    // Show execution instructions
    println!();
    println!("To execute this changeset, run:");
    println!("  iidy exec-changeset {} {}", args.argsfile, changeset_name);

    // Show operation summary
    reporter.show_operation_summary(&context);

    Ok(())
}
