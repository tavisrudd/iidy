use anyhow::Result;

use crate::cfn::{create_context_for_operation, apply_stack_name_override_and_validate, changeset_operations};
use crate::cli::{Cli, CreateChangeSetArgs, AwsOpts, Commands};
use crate::output::{
    DynamicOutputManager, manager::OutputOptions,
    aws_conversion::{create_command_metadata, create_final_command_summary},
    data::OutputData
};
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;

/// Create a CloudFormation changeset following exact iidy-js pattern.
///
/// Implements the complete create-changeset flow:
/// 1. Command metadata
/// 2. Changeset creation operation (CREATE or UPDATE based on stack existence)
/// 3. Comprehensive changeset result with console URL, pending changesets, and next steps
/// Uses the data-driven output architecture for consistent rendering across output modes.
pub async fn create_changeset(cli: &Cli, args: &CreateChangeSetArgs) -> Result<i32> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    // Load stack configuration with full context (AWS credential merging + $envValues injection)
    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = cli.command.to_cfn_operation();
    let stack_args = load_stack_args(
        &args.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.stack_name.as_ref())?;
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    // Setup AWS context for changeset creation
    let context = create_context_for_operation(&opts, operation).await?;

    // Setup data-driven output manager with full CLI context
    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: Commands::CreateChangeset(args.clone()),
    };
    let output_options = OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Show command metadata (exact iidy-js pattern)
    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    let changeset_result = changeset_operations::create_changeset_comprehensive(
        &context,
        &final_stack_args,
        args.changeset_name.as_deref(),
        &args.argsfile,
        true,
        &mut output_manager,
        args.description.as_deref(),
        Some(&global_opts.environment),
    ).await?;

    // 3. Render changeset result
    output_manager.render(OutputData::ChangeSetResult(changeset_result)).await?;

    // 4. Calculate elapsed time and determine success
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = true; // Changeset creation success is determined by API call success

    // 5. Show final command summary
    let final_command_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_command_summary).await?;

    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}