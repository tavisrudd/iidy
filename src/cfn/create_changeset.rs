use anyhow::Result;

use crate::cfn::{
    CfnContext, StackArgs, apply_stack_name_override_and_validate, changeset_operations,
};
use crate::cli::{Cli, CreateChangeSetArgs};
use crate::output::{
    DynamicOutputManager,
    aws_conversion::{create_command_metadata, create_final_command_summary},
    data::OutputData,
};
use crate::run_command_handler_with_stack_args;

async fn create_changeset_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &CreateChangeSetArgs,
    opts: &crate::cli::NormalizedAwsOpts,
    stack_args: &StackArgs,
) -> Result<i32> {
    let global_opts = &cli.global_opts;

    let final_stack_args =
        apply_stack_name_override_and_validate(stack_args.clone(), args.stack_name.as_ref())?;
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    let command_metadata =
        create_command_metadata(context, opts, &final_stack_args, &global_opts.environment).await?;
    output_manager
        .render(OutputData::CommandMetadata(command_metadata))
        .await?;

    let changeset_result = changeset_operations::create_changeset_comprehensive(
        context,
        &final_stack_args,
        args.changeset_name.as_deref(),
        &args.argsfile,
        true,
        output_manager,
        args.description.as_deref(),
        Some(&global_opts.environment),
    )
    .await?;

    output_manager
        .render(OutputData::ChangeSetResult(changeset_result))
        .await?;

    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = true;

    let final_command_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_command_summary).await?;

    Ok(if success { 0 } else { 1 })
}

pub async fn create_changeset(cli: &Cli, args: &CreateChangeSetArgs) -> Result<i32> {
    run_command_handler_with_stack_args!(create_changeset_impl, cli, args, &args.argsfile)
}
