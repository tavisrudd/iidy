use anyhow::Result;
use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::types::ObjectCannedAcl;
use owo_colors::OwoColorize;
use similar::{ChangeTag, TextDiff};

use crate::cfn::{
    CfnContext, StackArgs, s3_utils::check_template_exists, template_hash::parse_s3_url,
};
use crate::cli::{ApprovalReviewArgs, Cli};
use crate::output::aws_conversion::create_command_metadata;
use crate::output::{DynamicOutputManager, OutputData};

async fn template_approval_review_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &ApprovalReviewArgs,
    opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let global_opts = &cli.global_opts;

    // Parse S3 URL
    let (bucket, pending_key) = parse_s3_url(&args.url)?;

    // Derive the approved key (remove .pending suffix)
    let approved_key = if pending_key.ends_with(".pending") {
        pending_key[..pending_key.len() - 8].to_string()
    } else {
        anyhow::bail!("URL must end with .pending suffix");
    };

    // This command doesn't use stack-args.yaml (it operates on S3 URLs directly).
    // We use StackArgs::default() for command metadata because:
    // - create_command_metadata() needs stack_args.role_arn for IAM service role display
    // - Default has role_arn: None, accurately representing "no stack args for this command"
    // - This is cleaner than attempting to load_stack_args() from a non-existent file
    let stack_args = StackArgs::default();

    // Render command metadata
    let command_metadata =
        create_command_metadata(context, opts, &stack_args, &global_opts.environment).await?;
    output_manager
        .render(OutputData::CommandMetadata(command_metadata))
        .await?;

    // Derive latest key from pending key's parent directory (matches JS behavior)
    let bucket_dir = std::path::Path::new(&pending_key)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let latest_key = if bucket_dir.is_empty() {
        "latest".to_string()
    } else {
        format!("{bucket_dir}/latest")
    };

    let s3_client = context.create_s3_client();

    // Check if pending template exists before proceeding
    let pending_exists = check_template_exists(&s3_client, &bucket, &pending_key).await?;
    if !pending_exists {
        anyhow::bail!("Pending template not found at {}", args.url);
    }

    // Check if template already approved
    let already_approved = check_template_exists(&s3_client, &bucket, &approved_key).await?;

    let approval_status = crate::output::data::ApprovalStatus {
        pending_exists,
        already_approved,
        pending_location: args.url.clone(),
        approved_location: if already_approved {
            Some(format!("s3://{bucket}/{approved_key}"))
        } else {
            None
        },
    };
    output_manager
        .render(OutputData::ApprovalStatus(approval_status))
        .await?;

    if already_approved {
        return Ok(0);
    }

    // Download templates
    let pending_template = download_template(&s3_client, &bucket, &pending_key).await?;
    let latest_template = download_template(&s3_client, &bucket, &latest_key)
        .await
        .unwrap_or_else(|_| String::new());

    // Generate and display diff
    let diff_output = generate_template_diff(&latest_template, &pending_template, args.context)?;
    let has_changes = !diff_output.is_empty();

    let template_diff = crate::output::data::TemplateDiff {
        diff_output,
        context_lines: args.context,
        has_changes,
    };
    output_manager
        .render(OutputData::TemplateDiff(template_diff))
        .await?;

    if !has_changes {
        let result = crate::output::data::ApprovalResult {
            approved: true,
            approved_location: Some(format!("s3://{bucket}/{approved_key}")),
            latest_location: Some(format!("s3://{bucket}/{latest_key}")),
            cleanup_completed: true,
        };
        output_manager
            .render(OutputData::ApprovalResult(result))
            .await?;
        return Ok(0);
    }

    // Request user confirmation
    let user_confirmed = output_manager
        .request_confirmation("Would you like to approve these changes?".to_string())
        .await?;

    if user_confirmed {
        approve_template(
            &s3_client,
            &bucket,
            &pending_key,
            &approved_key,
            &latest_key,
            &pending_template,
        )
        .await?;

        let result = crate::output::data::ApprovalResult {
            approved: true,
            approved_location: Some(format!("s3://{bucket}/{approved_key}")),
            latest_location: Some(format!("s3://{bucket}/{latest_key}")),
            cleanup_completed: true,
        };
        output_manager
            .render(OutputData::ApprovalResult(result))
            .await?;
        Ok(0)
    } else {
        let result = crate::output::data::ApprovalResult {
            approved: false,
            approved_location: None,
            latest_location: None,
            cleanup_completed: false,
        };
        output_manager
            .render(OutputData::ApprovalResult(result))
            .await?;
        Ok(1)
    }
}

/// Download template content from S3
async fn download_template(
    s3_client: &aws_sdk_s3::Client,
    bucket: &str,
    key: &str,
) -> Result<String> {
    let response = s3_client
        .get_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await?;

    let body = response.body.collect().await?.into_bytes();
    let content = String::from_utf8(body.to_vec())?;
    Ok(content)
}

/// Generate colored diff between two templates
fn generate_template_diff(old: &str, new: &str, context: u32) -> Result<String> {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();

    for (idx, group) in diff.grouped_ops(context as usize).iter().enumerate() {
        if idx > 0 {
            output.push_str(&"---\n".dimmed().to_string());
        }

        for op in group {
            for change in diff.iter_changes(op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };

                let line = format!("{}{}", sign, change.value());
                output.push_str(&match change.tag() {
                    ChangeTag::Delete => line.red().to_string(),
                    ChangeTag::Insert => line.green().to_string(),
                    ChangeTag::Equal => line,
                });
            }
        }
    }

    Ok(output)
}

/// Approve template by copying to final location and updating latest
async fn approve_template(
    s3_client: &aws_sdk_s3::Client,
    bucket: &str,
    pending_key: &str,
    approved_key: &str,
    latest_key: &str,
    content: &str,
) -> Result<()> {
    // Copy to approved location
    s3_client
        .put_object()
        .bucket(bucket)
        .key(approved_key)
        .body(ByteStream::from(content.as_bytes().to_vec()))
        .acl(ObjectCannedAcl::BucketOwnerFullControl)
        .send()
        .await?;

    // Update latest copy
    s3_client
        .put_object()
        .bucket(bucket)
        .key(latest_key)
        .body(ByteStream::from(content.as_bytes().to_vec()))
        .acl(ObjectCannedAcl::BucketOwnerFullControl)
        .send()
        .await?;

    // Delete pending template
    s3_client
        .delete_object()
        .bucket(bucket)
        .key(pending_key)
        .send()
        .await?;

    Ok(())
}

pub async fn template_approval_review(cli: &Cli, args: &ApprovalReviewArgs) -> Result<i32> {
    // JS forces us-east-1 when no region is specified for review
    let mut cli = cli.clone();
    if cli.aws_opts.region.is_none() {
        cli.aws_opts.region = Some("us-east-1".to_string());
    }
    run_command_handler!(template_approval_review_impl, &cli, args)
}
