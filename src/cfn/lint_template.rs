use anyhow::Result;

use crate::cfn::{
    CfnContext, StackArgs,
    template_loader::{TEMPLATE_MAX_BYTES, load_cfn_template},
    template_validation::validate_template,
};
use crate::cli::{Cli, LintTemplateArgs, NormalizedAwsOpts};
use crate::output::{DynamicOutputManager, OutputData};

async fn lint_template_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &LintTemplateArgs,
    _opts: &NormalizedAwsOpts,
    stack_args: &StackArgs,
) -> Result<i32> {
    let template_result = load_cfn_template(
        stack_args.template.as_deref(),
        &args.argsfile,
        Some(&cli.global_opts.environment),
        TEMPLATE_MAX_BYTES,
        Some(&context.create_s3_client()),
    )
    .await?;

    let template_body = template_result
        .template_body
        .ok_or_else(|| anyhow::anyhow!("Failed to load template body"))?;

    let validation = validate_template(context, &template_body).await?;
    let has_errors = !validation.errors.is_empty();

    output_manager
        .render(OutputData::TemplateValidation(validation))
        .await?;

    Ok(if has_errors { 1 } else { 0 })
}

pub async fn lint_template(cli: &Cli, args: &LintTemplateArgs) -> Result<i32> {
    crate::run_command_handler_with_stack_args!(lint_template_impl, cli, args, &args.argsfile)
}
