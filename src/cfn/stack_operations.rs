//! Shared stack operations for CloudFormation stack management
//!
//! This module provides common operations that are used across multiple CloudFormation handlers,
//! such as collecting stack contents, managing stack information, and handling events.

use anyhow::Result;
use aws_sdk_cloudformation::{
    Client,
    types::{Stack, StackEvent},
};
use chrono::{DateTime, Utc};
use std::collections::HashSet;

use super::{
    CfnContext, constants::DEFAULT_POLL_INTERVAL_SECS, determine_operation_success,
    is_terminal_status::is_terminal_resource_status,
};
use crate::output::{
    ChangeDetail, ChangeInfo, ChangeSetInfo, DynamicOutputManager, StackContents, StackStatusInfo,
    aws_conversion::{
        convert_outputs_to_exports, convert_stack_outputs, convert_stack_resources,
        convert_stack_to_definition, create_final_command_summary,
    },
    data::OutputData,
};

/// Collect stack contents data (controller pattern - no display logic)
///
/// This function fetches all the necessary data to display stack contents:
/// - Stack resources
/// - Stack outputs  
/// - Stack exports
/// - Current status
/// - Pending changesets (placeholder for now)
///
/// Used by multiple operations like watch-stack, describe-stack, and create-stack.
pub async fn collect_stack_contents(ctx: &CfnContext, stack_name: &str) -> Result<StackContents> {
    // Start both API calls in parallel - we'll await them as needed
    let resources_future = async {
        ctx.client
            .describe_stack_resources()
            .stack_name(stack_name)
            .send()
            .await
            .map_err(anyhow::Error::from)
    };

    let stack_future = async {
        ctx.client
            .describe_stacks()
            .stack_name(stack_name)
            .send()
            .await
            .map_err(anyhow::Error::from)
    };

    // We need the stack info for outputs, so get that first
    let stack_resp = stack_future.await?;
    let stack = stack_resp
        .stacks
        .and_then(|mut s| s.pop())
        .ok_or_else(|| anyhow::anyhow!("stack not found"))?;

    // Get resources (this might still be loading)
    let resources_resp = resources_future.await?;
    let resources = convert_stack_resources(resources_resp.stack_resources.unwrap_or_default());

    // Extract outputs from stack
    let outputs = convert_stack_outputs(stack.outputs.unwrap_or_default());

    // Get exports if any outputs have export names
    let stack_id = stack.stack_id.clone().unwrap_or_default();
    let exports = convert_outputs_to_exports(&outputs, &stack_id);

    // Current status
    let current_status = StackStatusInfo {
        status: stack
            .stack_status
            .map(|s| s.as_str().to_string())
            .unwrap_or_default(),
        status_reason: stack.stack_status_reason,
        timestamp: stack
            .last_updated_time
            .or(stack.creation_time)
            .and_then(|ts| chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())),
    };

    // Get pending changesets
    let pending_changesets = collect_pending_changesets(ctx, stack_name)
        .await
        .unwrap_or_default();

    Ok(StackContents {
        resources,
        outputs,
        exports,
        current_status,
        pending_changesets,
    })
}

/// Collect pending changesets for a stack
async fn collect_pending_changesets(
    ctx: &CfnContext,
    stack_name: &str,
) -> Result<Vec<ChangeSetInfo>> {
    // List all changesets for the stack
    let list_resp = ctx
        .client
        .list_change_sets()
        .stack_name(stack_name)
        .send()
        .await?;

    let changeset_summaries = list_resp.summaries.unwrap_or_default();
    let mut changesets = Vec::new();

    for summary in changeset_summaries {
        // Get detailed information about each changeset
        let changeset_name = summary.change_set_name().unwrap_or("").to_string();
        if changeset_name.is_empty() {
            continue;
        }

        let describe_resp = ctx
            .client
            .describe_change_set()
            .stack_name(stack_name)
            .change_set_name(&changeset_name)
            .send()
            .await?;

        // Convert AWS changeset to our format
        let mut changes = Vec::new();
        if let Some(ref changeset_changes) = describe_resp.changes {
            for change in changeset_changes {
                if let Some(ref resource_change) = change.resource_change {
                    changes.push(ChangeInfo {
                        action: resource_change
                            .action()
                            .map(|a| a.as_str())
                            .unwrap_or("Unknown")
                            .to_string(),
                        logical_resource_id: resource_change
                            .logical_resource_id()
                            .unwrap_or("")
                            .to_string(),
                        physical_resource_id: resource_change
                            .physical_resource_id()
                            .map(|s| s.to_string()),
                        resource_type: resource_change.resource_type().unwrap_or("").to_string(),
                        replacement: resource_change
                            .replacement()
                            .map(|r| r.as_str().to_string()),
                        scope: Some(
                            resource_change
                                .scope()
                                .iter()
                                .map(|s| s.as_str().to_string())
                                .collect(),
                        ),
                        details: resource_change
                            .details()
                            .iter()
                            .map(|detail| ChangeDetail {
                                target: detail
                                    .target()
                                    .and_then(|t| t.name())
                                    .unwrap_or("")
                                    .to_string(),
                                evaluation: detail.evaluation().map(|e| e.as_str().to_string()),
                                change_source: detail
                                    .change_source()
                                    .map(|cs| cs.as_str().to_string()),
                                causing_entity: detail.causing_entity().map(|ce| ce.to_string()),
                            })
                            .collect(),
                    });
                }
            }
        }

        let changeset_info = ChangeSetInfo {
            change_set_name: changeset_name,
            change_set_id: describe_resp.change_set_id().unwrap_or("").to_string(),
            stack_id: describe_resp.stack_id().unwrap_or("").to_string(),
            stack_name: describe_resp.stack_name().unwrap_or("").to_string(),
            description: describe_resp.description().map(|s| s.to_string()),
            status: describe_resp
                .status()
                .map(|s| s.as_str().to_string())
                .unwrap_or("UNKNOWN".to_string()),
            status_reason: describe_resp.status_reason().map(|s| s.to_string()),
            creation_time: describe_resp.creation_time().and_then(|ts| {
                chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
                    .map(|dt| dt.with_timezone(&chrono::Utc))
            }),
            execution_status: describe_resp
                .execution_status()
                .map(|es| es.as_str().to_string()),
            changes,
        };

        changesets.push(changeset_info);
    }

    Ok(changesets)
}

/// Stack Events Service - provides common event fetching and processing patterns
pub struct StackEventsService;

impl StackEventsService {
    /// Retrieve and sort all events for a stack
    pub async fn fetch_events(client: &Client, stack_name: &str) -> Result<Vec<StackEvent>> {
        let resp = client
            .describe_stack_events()
            .stack_name(stack_name)
            .send()
            .await?;

        let mut events = resp.stack_events.unwrap_or_default();
        events.sort_by_key(|e| e.timestamp().map(|t| t.as_nanos()).unwrap_or(0));
        Ok(events)
    }

    /// Convert AWS timestamp to chrono DateTime
    pub fn aws_timestamp_to_chrono(aws_time: &aws_smithy_types::DateTime) -> Option<DateTime<Utc>> {
        DateTime::from_timestamp(aws_time.secs(), aws_time.subsec_nanos())
            .map(|dt| dt.with_timezone(&Utc))
    }

    /// Filter events to only include those after the given start time
    pub fn filter_events_after_start_time(
        events: Vec<StackEvent>,
        start_time: DateTime<Utc>,
    ) -> Vec<StackEvent> {
        events
            .into_iter()
            .filter(|event| {
                event
                    .timestamp()
                    .and_then(|ts| Self::aws_timestamp_to_chrono(ts))
                    .map(|event_time| event_time > start_time)
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Determine if an event indicates the stack has reached a terminal state
    /// Only considers terminal states for the main stack resource, not nested stacks.
    pub fn check_terminal_event(event: &StackEvent, stack_identifier: &str) -> Option<String> {
        // Only check CloudFormation Stack resources
        if event.resource_type() != Some("AWS::CloudFormation::Stack") {
            return None;
        }

        // Extract stack name from stack identifier (could be name or ARN)
        let stack_name = Self::extract_stack_name_from_identifier(stack_identifier);

        // The logical resource ID must match the stack name (this is the main stack, not a nested stack)
        if event.logical_resource_id() != Some(&stack_name) {
            return None;
        }

        // Check if the resource status is terminal
        if let Some(status) = event.resource_status() {
            if is_terminal_resource_status(status) {
                return Some(status.as_str().to_string());
            }
        }

        None
    }

    /// Extract stack name from stack identifier (handles both name and ARN)
    pub fn extract_stack_name_from_identifier(identifier: &str) -> String {
        if identifier.starts_with("arn:aws:cloudformation:") {
            // ARN format: arn:aws:cloudformation:region:account:stack/stack-name/stack-id
            if let Some(stack_part) = identifier.split('/').nth(1) {
                stack_part.to_string()
            } else {
                identifier.to_string() // Fallback to full identifier
            }
        } else {
            identifier.to_string() // Already a stack name
        }
    }

    /// Filter new events and check for terminal state
    /// Returns (new_events, terminal_detected, final_status)
    pub fn process_new_events(
        events: Vec<StackEvent>,
        seen: &mut HashSet<String>,
        stack_identifier: &str,
        start_time: DateTime<Utc>,
    ) -> (Vec<StackEvent>, bool, Option<String>) {
        // Filter events by start time
        let filtered_events = Self::filter_events_after_start_time(events, start_time);

        let mut new_events = Vec::new();
        let mut done = false;
        let mut final_status = None;

        for ev in filtered_events {
            if let Some(id) = ev.event_id() {
                if seen.insert(id.to_string()) {
                    if let Some(status) = Self::check_terminal_event(&ev, stack_identifier) {
                        done = true;
                        final_status = Some(status);
                    }
                    new_events.push(ev);
                }
            }
        }

        (new_events, done, final_status)
    }
}

/// Stack Info Service - provides common stack information retrieval patterns
pub struct StackInfoService;

impl StackInfoService {
    /// Retrieve a single stack by name (handles the common pattern of describe_stacks + error handling)
    pub async fn get_stack(client: &Client, stack_name: &str) -> Result<Stack> {
        let stack_resp = client
            .describe_stacks()
            .stack_name(stack_name)
            .send()
            .await?;

        let stack = stack_resp
            .stacks
            .and_then(|mut s| s.pop())
            .ok_or_else(|| anyhow::anyhow!("stack '{}' not found", stack_name))?;

        Ok(stack)
    }

    /// Retrieve a single stack by name and extract its ID (common pattern for operations that need ARN)
    pub async fn get_stack_id(client: &Client, stack_name: &str) -> Result<String> {
        let stack = Self::get_stack(client, stack_name).await?;

        let stack_id = stack
            .stack_id
            .ok_or_else(|| anyhow::anyhow!("stack '{}' does not have an ID", stack_name))?;

        Ok(stack_id)
    }

    /// Check if a stack exists (returns Ok(true/false) instead of error for missing stacks)
    pub async fn stack_exists(client: &Client, stack_name: &str) -> Result<bool> {
        match Self::get_stack(client, stack_name).await {
            Ok(_) => Ok(true),
            Err(e) => {
                // Check if this is a "stack not found" error vs a real AWS error
                let error_string = e.to_string().to_lowercase();
                if error_string.contains("not found") || error_string.contains("does not exist") {
                    Ok(false)
                } else {
                    Err(e) // Re-throw non-existence errors
                }
            }
        }
    }
}

/// Watch stack operation and summarize results - shared pattern across multiple commands
///
/// This function implements the common pattern of:
/// 1. Fetching and displaying stack definition
/// 2. Watching stack operation progress with live events
/// 3. Handling DELETE_COMPLETE early exit
/// 4. Collecting and displaying final stack contents
/// 5. Creating final command summary
///
/// Used by create_or_update, exec_changeset, and update_stack commands.
pub async fn watch_stack_operation_and_summarize(
    context: &CfnContext,
    stack_id: &str,
    output_manager: &mut DynamicOutputManager,
    success_states: &[&str],
) -> Result<i32> {
    // Start stack definition task
    let stack_task = {
        let client = context.client.clone();
        let stack_id = stack_id.to_string();
        tokio::spawn(async move {
            let stack = StackInfoService::get_stack(&client, &stack_id).await?;
            let output_data = convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };

    // Await and render stack definition first
    crate::await_and_render!(stack_task, output_manager);

    // Then handle live events watching using the existing helper function
    use super::watch_stack::watch_stack_with_data_output;
    let final_status = match watch_stack_with_data_output(
        context,
        stack_id,
        output_manager,
        std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
    )
    .await
    {
        Ok(status) => status,
        Err(error) => {
            let error_info =
                crate::output::aws_conversion::convert_aws_error_to_error_info(&error, None).await;
            output_manager
                .render(crate::output::OutputData::Error(error_info))
                .await?;
            return Ok(1);
        }
    };

    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = determine_operation_success(&final_status, success_states);

    // Skip stack contents if the stack was deleted (can happen with failed operations)
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            let final_command_summary = create_final_command_summary(
                false, // Mark as failed since stack was deleted
                elapsed_seconds,
            );
            output_manager.render(final_command_summary).await?;
            return Ok(1); // Return exit code 1 for failure
        }
    }

    let stack_contents = collect_stack_contents(&context, &stack_id).await?;
    output_manager
        .render(OutputData::StackContents(stack_contents))
        .await?;

    let final_command_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_command_summary).await?;

    Ok(if success { 0 } else { 1 })
}
