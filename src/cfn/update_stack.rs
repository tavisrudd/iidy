use anyhow::Result;

use crate::cfn::{
    CfnContext, CfnOperation, CfnRequestBuilder, StackArgs, UPDATE_SUCCESS_STATES,
    apply_stack_name_override_and_validate, changeset_operations::confirm_changeset_execution,
    exec_changeset::call_exec_changeset_with_reconstruction,
    stack_operations::watch_stack_operation_and_summarize,
};
use crate::cli::{Cli, UpdateStackArgs};
use crate::output::{
    DynamicOutputManager, OutputData,
    aws_conversion::{convert_token_info, create_command_metadata},
};

async fn update_stack_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &UpdateStackArgs,
    opts: &crate::cli::NormalizedAwsOpts,
    stack_args: &StackArgs,
) -> Result<i32> {
    let global_opts = &cli.global_opts;

    let final_stack_args =
        apply_stack_name_override_and_validate(stack_args.clone(), args.base.stack_name.as_ref())?;
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    let command_metadata =
        create_command_metadata(context, opts, &final_stack_args, &global_opts.environment).await?;
    output_manager
        .render(OutputData::CommandMetadata(command_metadata))
        .await?;

    if args.changeset {
        return update_stack_with_changeset(
            context,
            args,
            &final_stack_args,
            output_manager,
            &global_opts.environment,
            cli,
        )
        .await;
    }

    let stack_id = perform_stack_update(
        context,
        &final_stack_args,
        args,
        &global_opts.environment,
        output_manager,
    )
    .await?;

    watch_stack_operation_and_summarize(context, &stack_id, output_manager, UPDATE_SUCCESS_STATES)
        .await
}

pub async fn update_stack(cli: &Cli, args: &UpdateStackArgs) -> Result<i32> {
    run_command_handler_with_stack_args!(update_stack_impl, cli, args, &args.base.argsfile)
}

async fn perform_stack_update(
    context: &CfnContext,
    stack_args: &StackArgs,
    args: &UpdateStackArgs,
    environment: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    let builder = CfnRequestBuilder::new(context, stack_args);

    let (update_request, token) = builder
        .build_update_stack(
            true,
            &CfnOperation::UpdateStack,
            &args.base.argsfile,
            Some(environment),
        )
        .await?;

    let output_token = convert_token_info(&token);
    output_manager
        .render(OutputData::TokenInfo(output_token))
        .await?;

    let response = update_request.send().await?;

    let stack_id = response
        .stack_id()
        .ok_or_else(|| anyhow::anyhow!("AWS did not return a stack ID"))?
        .to_string();

    Ok(stack_id)
}

async fn update_stack_with_changeset(
    context: &CfnContext,
    args: &UpdateStackArgs,
    stack_args: &StackArgs,
    output_manager: &mut DynamicOutputManager,
    environment: &str,
    cli: &Cli,
) -> Result<i32> {
    let stack_name = stack_args.require_stack_name()?;
    let stack_task = {
        let client = context.client.clone();
        let stack_name = stack_name.to_owned();
        tokio::spawn(async move {
            let stack =
                crate::cfn::stack_operations::StackInfoService::get_stack(&client, &stack_name)
                    .await?;
            let output_data =
                crate::output::aws_conversion::convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };

    output_manager.render(stack_task.await??).await?;

    let changeset_name = format!("iidy-update-{}", &context.primary_token().value[..8]);

    let changeset_result = crate::cfn::changeset_operations::create_changeset_comprehensive(
        context,
        stack_args,
        Some(&changeset_name),
        &args.base.argsfile,
        false,
        output_manager,
        None,
        Some(environment),
    )
    .await?;

    output_manager
        .render(OutputData::ChangeSetResult(changeset_result.clone()))
        .await?;

    let confirmed = confirm_changeset_execution(output_manager, context, args.yes, false).await?;

    if !confirmed {
        return Ok(130);
    }

    call_exec_changeset_with_reconstruction(
        changeset_result.changeset_name,
        changeset_result.stack_name,
        Some(args.base.argsfile.clone()),
        &cli.global_opts,
        &cli.aws_opts,
    )
    .await
}
