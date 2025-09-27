use anyhow::Result;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::ObjectCannedAcl;

use crate::cli::{Cli, ApprovalRequestArgs};
use crate::output::{DynamicOutputManager, OutputData};
use crate::cfn::{
    CfnOperation,
    template_loader::{load_cfn_template, TEMPLATE_MAX_BYTES},
    template_hash::generate_versioned_location,
    s3_utils::check_template_exists,
};
use crate::output::aws_conversion::create_command_metadata;
use crate::stack_args::load_stack_args;
use crate::aws::AwsSettings;
use crate::run_command_handler;

async fn template_approval_request_impl(
    output_manager: &mut DynamicOutputManager,
    context: &crate::cfn::CfnContext,
    cli: &Cli,
    args: &ApprovalRequestArgs,
    opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let global_opts = &cli.global_opts;
    let cli_aws_settings = AwsSettings::from_normalized_opts(opts);
    let operation = CfnOperation::TemplateApprovalRequest;
    
    // Load stack args
    let stack_args = load_stack_args(
        &args.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;
    
    // Validate required fields
    if stack_args.approved_template_location.is_none() {
        anyhow::bail!("ApprovedTemplateLocation is required in stack-args.yaml");
    }
    if stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }
    
    // Render command metadata
    let command_metadata = create_command_metadata(context, opts, &stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;
    
    // Load template using standard loader (includes all preprocessing)
    let template_result = load_cfn_template(
        stack_args.template.as_ref().map(|x| x.as_str()),
        &args.argsfile,
        Some(&global_opts.environment),
        TEMPLATE_MAX_BYTES,
        Some(&context.create_s3_client()),
    ).await?;
    
    let template_body = template_result.template_body
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
            template_location: format!("s3://{}/{}", bucket, key),
            pending_location: format!("s3://{}/{}.pending", bucket, key),
            already_approved: true,
            next_steps: vec!["Template has already been approved".to_string()],
        };
        output_manager.render(OutputData::ApprovalRequestResult(result)).await?;
        return Ok(0);
    }
    
    // Template validation (if enabled)
    if args.lint_template {
        let validation_result = validate_template(context, &template_body, &stack_args, args.lint_using_parameters).await?;
        output_manager.render(OutputData::TemplateValidation(validation_result)).await?;
    }
    
    // Upload pending template
    let pending_key = format!("{}.pending", key);
    upload_template_to_s3(&s3_client, &bucket, &pending_key, &template_body).await?;
    
    // Render approval request result
    let result = crate::output::data::ApprovalRequestResult {
        template_location: format!("s3://{}/{}", bucket, key),
        pending_location: format!("s3://{}/{}", bucket, pending_key),
        already_approved: false,
        next_steps: vec![
            format!("Review template with: iidy template-approval review s3://{}/{}", bucket, pending_key),
        ],
    };
    output_manager.render(OutputData::ApprovalRequestResult(result)).await?;
    
    Ok(0)
}


/// Upload template to S3 with appropriate ACL
async fn upload_template_to_s3(s3_client: &aws_sdk_s3::Client, bucket: &str, key: &str, content: &str) -> Result<()> {
    s3_client.put_object()
        .bucket(bucket)
        .key(key)
        .body(ByteStream::from(content.as_bytes().to_vec()))
        .acl(ObjectCannedAcl::BucketOwnerFullControl)
        .send()
        .await?;
    Ok(())
}

/// Validate template using CloudFormation
async fn validate_template(context: &crate::cfn::CfnContext, template_body: &str, stack_args: &crate::stack_args::StackArgs, using_parameters: bool) -> Result<crate::output::data::TemplateValidation> {
    let mut validation_request = context.client.validate_template();
    
    if template_body.len() > 51200 {
        // Use URL for large templates (not implemented yet)
        validation_request = validation_request.template_url("template_url");
    } else {
        validation_request = validation_request.template_body(template_body);
    }
    
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    
    match validation_request.send().await {
        Ok(_response) => {
            // Template is valid
        }
        Err(e) => {
            errors.push(format!("Template validation failed: {}", e));
        }
    }
    
    // Add parameter validation if requested
    if using_parameters && stack_args.parameters.is_some() {
        // This would require additional validation logic
        // For now, just note that parameters were considered
        warnings.push("Parameter validation not fully implemented yet".to_string());
    }
    
    Ok(crate::output::data::TemplateValidation {
        enabled: true,
        using_parameters,
        errors,
        warnings,
    })
}

pub async fn template_approval_request(cli: &Cli, args: &ApprovalRequestArgs) -> Result<i32> {
    run_command_handler!(template_approval_request_impl, cli, args)
}