use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, ListArgs, GlobalOpts, NormalizedAwsOpts},
    output::{
        DynamicOutputManager, convert_stacks_to_list_display
    },
};

// Note: The complex color formatting and lifecycle icon logic has been moved 
// to the output renderers where it can be applied consistently across all modes.

/// Retrieve all stacks for the configured AWS region and display them.
///
/// Uses the data-driven output architecture for consistent rendering across output modes.
/// The stack list can be displayed in Interactive (with colors and icons), Plain (CI-friendly),
/// or JSON (machine-readable) formats.
pub async fn list_stacks(
    opts: &NormalizedAwsOpts, 
    args: &ListArgs, 
    global_opts: &GlobalOpts
) -> Result<()> {

    // Setup data-driven output manager
    let output_options = crate::output::manager::OutputOptions {
        color_choice: global_opts.color,
        theme: global_opts.theme,
        terminal_width: None, // Will auto-detect
        buffer_limit: 100,
    };
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // Don't show command metadata or progress messages for list operations

    // Setup AWS client and retrieve stacks
    let config = aws::config_from_opts(&AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: None, // Not needed for list operation
    }).await?;
    let client = aws_sdk_cloudformation::Client::new(&config);

    // Use the paginator to retrieve all stacks in the region.
    let stacks: Vec<aws_sdk_cloudformation::types::Stack> = client
        .describe_stacks()
        .into_paginator()
        .items()
        .send()
        .try_collect()
        .await?;

    // Convert to structured data and render
    let stack_list_display = convert_stacks_to_list_display(stacks, args.tags);
    output_manager.render(stack_list_display).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests for this module are now primarily in the output conversion utilities
    // and renderer integration tests. The list_stacks function is tested end-to-end
    // through the data-driven output architecture.
}
