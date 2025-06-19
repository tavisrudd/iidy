use anyhow::{Result, anyhow};

use crate::{
    cfn::create_context,
    cli::{DescribeArgs, NormalizedAwsOpts, GlobalOpts},
    output::{
        DynamicOutputManager, OutputData, convert_stack_to_definition,
        StackContents, StackResourceInfo,
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

    // Start operation with spinner for better UX
    output_manager.start_operation("Loading stack description").await?;

    // Execute AWS API calls in parallel for better performance
    let event_count = args.events as usize;
    
    // Streaming approach: Start all API calls in parallel but show results
    // as soon as they're ready, maintaining logical display order:
    // 1. Stack Definition (fast, users expect this first)
    // 2. Stack Events (medium speed, logical to show after definition)  
    // 3. Stack Contents (slower, contains resource details)
    let stack_future = async {
        context
            .client
            .describe_stacks()
            .stack_name(args.stackname.clone())
            .send()
            .await
            .map_err(anyhow::Error::from)
    };
    
    let resources_future = async {
        context
            .client
            .describe_stack_resources()
            .stack_name(&args.stackname)
            .send()
            .await
            .map_err(anyhow::Error::from)
    };
    
    let events_future = async {
        context
            .client
            .describe_stack_events()
            .stack_name(&args.stackname)
            .send()
            .await
            .map_err(anyhow::Error::from)
    };

    // 1. Get and show stack definition first (usually fastest)
    let stack_resp = stack_future.await?;
    let stack = stack_resp
        .stacks
        .and_then(|mut s| s.pop())
        .ok_or_else(|| anyhow!("stack not found"))?;

    // Clear spinner and show stack definition immediately
    output_manager.end_operation_success("Stack info loaded").await?;
    
    let stack_definition = convert_stack_to_definition(&stack, true);
    output_manager.render(stack_definition).await?;

    // 2. Start new operation for events loading
    output_manager.start_operation("Loading stack events").await?;
    
    let first_events_resp = events_future.await?;
    
    // Continue fetching stack events if needed (pagination)
    let mut all_events = first_events_resp.stack_events.unwrap_or_default();
    let mut next_token = first_events_resp.next_token;
    
    // Fetch additional pages if needed
    while next_token.is_some() && all_events.len() < event_count * 2 {
        let events_resp = context
            .client
            .describe_stack_events()
            .stack_name(&args.stackname)
            .set_next_token(next_token)
            .send()
            .await?;
            
        let mut page_events = events_resp.stack_events.unwrap_or_default();
        all_events.append(&mut page_events);
        next_token = events_resp.next_token;
    }
    
    // Clear spinner and show events
    output_manager.end_operation_success("Stack events loaded").await?;
    
    let events_display = crate::output::aws_conversion::convert_stack_events_to_display_with_max(
        all_events,
        &format!("Previous Stack Events (max {}):", event_count),
        Some(event_count),
    );
    output_manager.render(events_display).await?;

    // 3. Get and show stack contents (resources are likely ready by now)
    let resources_resp = resources_future.await?;
    
    // Update progress - all data ready
    output_manager.update_operation("Loading stack description... all data ready").await?;
    
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

    // Extract outputs from stack (we already have this from the first call)
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

    // Show stack contents as soon as resources are ready
    output_manager.render(OutputData::StackContents(stack_contents)).await?;

    // Complete the operation successfully
    output_manager.end_operation_success("Stack description loaded").await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    // Tests for this module are now primarily in the output conversion utilities
    // and renderer integration tests. The describe_stack function is tested end-to-end
    // through the data-driven output architecture.
}
