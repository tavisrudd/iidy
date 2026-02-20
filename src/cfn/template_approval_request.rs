use anyhow::Result;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::ObjectCannedAcl;

use crate::cfn::{
    CfnContext, StackArgs,
    s3_utils::check_template_exists,
    template_hash::generate_versioned_location,
    template_loader::{TEMPLATE_MAX_BYTES, load_cfn_template},
    template_validation::validate_template,
};
use crate::cli::{ApprovalRequestArgs, Cli};
use crate::output::aws_conversion::create_command_metadata;
use crate::output::{DynamicOutputManager, OutputData};

async fn template_approval_request_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &ApprovalRequestArgs,
    opts: &crate::cli::NormalizedAwsOpts,
    stack_args: &StackArgs,
) -> Result<i32> {
    let global_opts = &cli.global_opts;

    // Validate required fields
    if stack_args.approved_template_location.is_none() {
        anyhow::bail!("ApprovedTemplateLocation is required in stack-args.yaml");
    }
    if stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    // Render command metadata
    let command_metadata =
        create_command_metadata(context, opts, stack_args, &global_opts.environment).await?;
    output_manager
        .render(OutputData::CommandMetadata(command_metadata))
        .await?;

    // Load template using standard loader (includes all preprocessing)
    let template_result = load_cfn_template(
        stack_args.template.as_deref(),
        &args.argsfile,
        Some(&global_opts.environment),
        TEMPLATE_MAX_BYTES,
        Some(&context.create_s3_client()),
    )
    .await?;

    let template_body = template_result
        .template_body
        .ok_or_else(|| anyhow::anyhow!("Failed to load template body"))?;

    // Generate versioned location
    let (bucket, key) = generate_versioned_location(
        stack_args.approved_template_location.as_ref().unwrap(),
        &template_body,
        stack_args.template.as_ref().unwrap(),
    )?;

    // Check if template already approved
    let s3_client = context.create_s3_client();
    let already_approved = check_template_exists(&s3_client, &bucket, &key).await?;

    if already_approved {
        let result = crate::output::data::ApprovalRequestResult {
            template_location: format!("s3://{bucket}/{key}"),
            pending_location: format!("s3://{bucket}/{key}.pending"),
            already_approved: true,
            next_steps: vec!["Template has already been approved".to_string()],
        };
        output_manager
            .render(OutputData::ApprovalRequestResult(result))
            .await?;
        return Ok(0);
    }

    // Template validation (if enabled)
    if args.lint_template {
        let validation_result = validate_template(context, &template_body).await?;
        let has_errors = !validation_result.errors.is_empty();
        output_manager
            .render(OutputData::TemplateValidation(validation_result))
            .await?;
        if has_errors {
            return Ok(1);
        }
    }

    // Upload pending template
    let pending_key = format!("{key}.pending");
    upload_template_to_s3(&s3_client, &bucket, &pending_key, &template_body).await?;

    // Render approval request result
    let result = crate::output::data::ApprovalRequestResult {
        template_location: format!("s3://{bucket}/{key}"),
        pending_location: format!("s3://{bucket}/{pending_key}"),
        already_approved: false,
        next_steps: vec![format!(
            "Review template with: iidy template-approval review s3://{}/{}",
            bucket, pending_key
        )],
    };
    output_manager
        .render(OutputData::ApprovalRequestResult(result))
        .await?;

    Ok(0)
}

/// Upload template to S3 with appropriate ACL
async fn upload_template_to_s3(
    s3_client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
    content: &str,
) -> Result<()> {
    s3_client
        .put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from(content.as_bytes().to_vec()))
        .acl(ObjectCannedAcl::BucketOwnerFullControl)
        .send()
        .await?;
    Ok(())
}

pub async fn template_approval_request(cli: &Cli, args: &ApprovalRequestArgs) -> Result<i32> {
    run_command_handler_with_stack_args!(template_approval_request_impl, cli, args, &args.argsfile)
}
