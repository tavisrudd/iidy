use anyhow::Result;
use std::time::Instant;

use crate::{
    cfn::create_context,
    cli::{DeleteArgs, NormalizedAwsOpts, GlobalOpts},
    output::{
        DynamicOutputManager, manager::OutputOptions,
        aws_conversion::{progress_message, success_message, warning_message, error_message, create_command_result, convert_stack_to_definition},
    },
};

use super::{CfnContext, watch_stack::watch_stack_with_data_output};

/// Delete a CloudFormation stack with DynamicOutputManager.
async fn delete_stack_with_data_output(
    ctx: &CfnContext,
    stack_name: &str,
    role_arn: Option<&str>,
    retain_resources: Option<Vec<String>>,
    output_manager: &mut DynamicOutputManager,
) -> Result<()> {
    // Derive a token for the delete operation
    let token = ctx.derive_token_for_step("delete-stack");
    
    // Pass token to output manager for conditional display
    let output_token = crate::output::aws_conversion::convert_token_info(&token);
    output_manager.render(crate::output::data::OutputData::TokenInfo(output_token)).await?;

    output_manager.render(progress_message(&format!("Deleting stack: {}", stack_name))).await?;

    // Check if stack exists first
    match ctx.client.describe_stacks().stack_name(stack_name).send().await {
        Ok(resp) => {
            if let Some(stacks) = resp.stacks {
                if let Some(stack) = stacks.first() {
                    // Show stack definition before deletion
                    let stack_def = convert_stack_to_definition(stack, true);
                    output_manager.render(stack_def).await?;
                }
            }
        }
        Err(_) => {
            output_manager.render(warning_message(&format!("Stack {} does not exist or is not accessible", stack_name))).await?;
            return Ok(());
        }
    }

    // Start the delete operation
    let mut request = ctx
        .client
        .delete_stack()
        .stack_name(stack_name)
        .client_request_token(&token.value);

    if let Some(role) = role_arn {
        request = request.role_arn(role);
        output_manager.render(progress_message(&format!("Using IAM role: {}", role))).await?;
    }

    if let Some(resources) = retain_resources {
        request = request.set_retain_resources(Some(resources.clone()));
        output_manager.render(progress_message(&format!("Retaining {} resources", resources.len()))).await?;
    }

    match request.send().await {
        Ok(_) => {
            output_manager.render(success_message("Delete operation initiated, watching for completion...")).await?;

            // Watch the stack deletion with data-driven output
            watch_stack_with_data_output(ctx, stack_name, output_manager, std::time::Duration::from_secs(5)).await?;
        }
        Err(e) => {
            output_manager.render(error_message(&format!("Failed to initiate stack deletion: {}", e))).await?;
            return Err(e.into());
        }
    }

    Ok(())
}

/// Delete a CloudFormation stack with data-driven output.
pub async fn delete_stack(
    opts: &NormalizedAwsOpts, 
    args: &DeleteArgs, 
    global_opts: &GlobalOpts
) -> Result<()> {
    let start_time = Instant::now();
    let output_options = OutputOptions::default();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // Create CloudFormation context
    let ctx = create_context(opts).await?;

    // Pass primary token to output manager for conditional display
    let primary_token = crate::output::aws_conversion::convert_token_info(&ctx.primary_token());
    output_manager.render(crate::output::data::OutputData::TokenInfo(primary_token)).await?;

    // Check if need to load stack args for confirmation
    let stack_name = &args.stackname;
    
    // TODO: Add confirmation prompt support in data-driven output
    // For now, proceed directly (in iidy-js this would show confirmation dialog)

    let result = delete_stack_with_data_output(
        &ctx,
        stack_name,
        args.role_arn.as_deref(),
        if args.retain_resources.is_empty() {
            None
        } else {
            Some(args.retain_resources.clone())
        },
        &mut output_manager,
    )
    .await;

    // Show final result
    let elapsed = start_time.elapsed().as_secs() as i64;
    match result {
        Ok(_) => {
            output_manager.render(create_command_result(true, elapsed, Some("Stack deletion completed".to_string()))).await?;
        }
        Err(ref e) => {
            output_manager.render(create_command_result(false, elapsed, Some(format!("Stack deletion failed: {}", e)))).await?;
        }
    }

    result
}
