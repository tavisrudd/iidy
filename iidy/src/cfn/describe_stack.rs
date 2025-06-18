use anyhow::{Result, anyhow};

use crate::{
    cfn::create_context,
    cli::{DescribeArgs, NormalizedAwsOpts, GlobalOpts},
    output::{
        DynamicOutputManager, OutputData, convert_stack_to_definition,
        convert_stack_events_to_display, StackContents, StackResourceInfo,
        StackOutputInfo, StackExportInfo, StackStatusInfo
    },
};

// Note: Stack formatting logic has been moved to the output renderers
// where it can be applied consistently across all modes (Interactive, Plain, JSON).

/// Retrieve a stack description from AWS and display it.
///
/// Uses the data-driven output architecture for consistent rendering across output modes.
/// The stack details can be displayed in Interactive (with colors and formatting), 
/// Plain (CI-friendly), or JSON (machine-readable) formats.
pub async fn describe_stack(
    opts: &NormalizedAwsOpts, 
    args: &DescribeArgs, 
    global_opts: &GlobalOpts
) -> Result<()> {

    // Setup AWS context
    let context = create_context(opts).await?;

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

    // Don't show command metadata or progress messages for describe operations

    // Fetch stack information
    let resp = context
        .client
        .describe_stacks()
        .stack_name(args.stackname.clone())
        .send()
        .await?;

    let stack = resp
        .stacks
        .and_then(|mut s| s.pop())
        .ok_or_else(|| anyhow!("stack not found"))?;

    // 1. Show stack definition (show times by default)
    let stack_definition = convert_stack_to_definition(&stack, true);
    output_manager.render(stack_definition).await?;

    // 2. Show stack events (previous stack events, max 50 by default)
    let event_count = args.events as usize;
    let events_resp = context
        .client
        .describe_stack_events()
        .stack_name(&args.stackname)
        .send()
        .await?;
    
    let events = events_resp.stack_events.unwrap_or_default();
    let events_display = convert_stack_events_to_display(
        events.into_iter().take(event_count).collect(),
        &format!("Previous Stack Events (max {}):", event_count),
    );
    output_manager.render(events_display).await?;

    // 3. Show stack contents (resources, outputs, exports)
    // Get stack resources
    let resources_resp = context
        .client
        .describe_stack_resources()
        .stack_name(&args.stackname)
        .send()
        .await?;
    
    let resources: Vec<StackResourceInfo> = resources_resp
        .stack_resources
        .unwrap_or_default()
        .into_iter()
        .map(|r| StackResourceInfo {
            logical_resource_id: r.logical_resource_id.unwrap_or_default(),
            physical_resource_id: r.physical_resource_id,
            resource_type: r.resource_type.unwrap_or_default(),
            resource_status: r.resource_status.map(|s| s.as_str().to_string()).unwrap_or_default(),
            resource_status_reason: r.resource_status_reason,
            last_updated_timestamp: r.timestamp.and_then(|ts| {
                chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
            }),
        })
        .collect();

    // Extract outputs from stack
    let outputs: Vec<StackOutputInfo> = stack
        .outputs
        .unwrap_or_default()
        .into_iter()
        .map(|o| StackOutputInfo {
            output_key: o.output_key.unwrap_or_default(),
            output_value: o.output_value.unwrap_or_default(),
            description: o.description,
            export_name: o.export_name,
        })
        .collect();

    // Get exports if any outputs have export names
    let mut exports: Vec<StackExportInfo> = vec![];
    for output in &outputs {
        if let Some(export_name) = &output.export_name {
            // For now, we'll create basic export info
            // In a full implementation, we'd query for imports
            exports.push(StackExportInfo {
                name: export_name.clone(),
                value: output.output_value.clone(),
                exporting_stack_id: stack.stack_id.clone().unwrap_or_default(),
                importing_stacks: vec![], // Would need separate query to find importers
            });
        }
    }

    // Current status
    let current_status = StackStatusInfo {
        status: stack.stack_status.map(|s| s.as_str().to_string()).unwrap_or_default(),
        status_reason: stack.stack_status_reason,
        timestamp: stack.last_updated_time.or(stack.creation_time).and_then(|ts| {
            chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
        }),
    };

    let stack_contents = StackContents {
        resources,
        outputs,
        exports,
        current_status,
        pending_changesets: vec![], // Would need separate query
    };

    output_manager.render(OutputData::StackContents(stack_contents)).await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests for this module are now primarily in the output conversion utilities
    // and renderer integration tests. The describe_stack function is tested end-to-end
    // through the data-driven output architecture.
}
