use anyhow::Result;
use std::time::Instant;

use crate::{
    cfn::{CfnRequestBuilder, create_context_for_operation, CfnOperation},
    cli::{CreateChangeSetArgs, NormalizedAwsOpts, GlobalOpts},
    output::{
        DynamicOutputManager, manager::OutputOptions,
        aws_conversion::{progress_message, success_message, create_command_result},
    },
    stack_args::load_stack_args,
    aws::AwsSettings,
};

/// Create a CloudFormation changeset with data-driven output.
pub async fn create_changeset(
    opts: &NormalizedAwsOpts, 
    args: &CreateChangeSetArgs,
    global_opts: &GlobalOpts
) -> Result<()> {
    let start_time = Instant::now();
    let output_options = OutputOptions::minimal();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;
    // Load stack configuration with full context (AWS credential merging + $envValues injection)
    let cli_aws_settings = AwsSettings::from_normalized_opts(opts);
    let operation = CfnOperation::CreateChangeset;
    let stack_args = load_stack_args(
        &args.argsfile,
        Some(&global_opts.environment),
        &operation,
        &cli_aws_settings,
    ).await?;

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

    // Setup AWS context for changeset creation
    let context = create_context_for_operation(opts, CfnOperation::CreateChangeset).await?;

    // Setup request builder
    let builder = CfnRequestBuilder::new(&context, &final_stack_args);

    // Pass primary token to output manager for conditional display
    let primary_token = crate::output::aws_conversion::convert_token_info(&context.primary_token());
    output_manager.render(crate::output::data::OutputData::TokenInfo(primary_token)).await?;

    // Determine changeset name
    let default_changeset_name = format!("iidy-{}", &context.primary_token().value[..8]);
    let changeset_name = args
        .changeset_name
        .as_deref()
        .unwrap_or(&default_changeset_name);

    // Build and execute the CreateChangeSet request
    let (create_request, token) =
        builder.build_create_changeset(changeset_name, &CfnOperation::CreateChangeset);
    
    // Pass token to output manager for conditional display
    let output_token = crate::output::aws_conversion::convert_token_info(&token);
    output_manager.render(crate::output::data::OutputData::TokenInfo(output_token)).await?;

    output_manager.render(progress_message(&format!(
        "Creating changeset '{}' for stack: {}",
        changeset_name,
        final_stack_args.stack_name.as_ref().unwrap()
    ))).await?;

    let result = match create_request.send().await {
        Ok(response) => {
            if let Some(changeset_id) = response.id() {
                output_manager.render(success_message(&format!("Changeset created: {}", changeset_id))).await?;

                if let Some(stack_id) = response.stack_id() {
                    output_manager.render(success_message(&format!("Stack ID: {}", stack_id))).await?;
                }
            } else {
                output_manager.render(success_message("Changeset created")).await?;
            }

            // Show execution instructions
            output_manager.render(success_message(&format!(
                "To execute this changeset, run: iidy exec-changeset {} {}",
                args.argsfile, changeset_name
            ))).await?;

            Ok(())
        }
        Err(e) => Err(e.into())
    };

    // Show final result
    let elapsed = start_time.elapsed().as_secs() as i64;
    match result {
        Ok(_) => {
            output_manager.render(create_command_result(true, elapsed, Some("Changeset creation completed".to_string()))).await?;
        }
        Err(ref e) => {
            output_manager.render(create_command_result(false, elapsed, Some(format!("Changeset creation failed: {}", e)))).await?;
        }
    }

    result
}
