//! Shared stack operations for CloudFormation stack management
//!
//! This module provides common operations that are used across multiple CloudFormation handlers,
//! such as collecting stack contents, managing stack information, and handling events.

use anyhow::Result;
use aws_sdk_cloudformation::{Client, types::{StackEvent, Stack}};
use chrono::{DateTime, Utc};
use std::collections::HashSet;

use super::{CfnContext, is_terminal_status::is_terminal_resource_status};

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
pub async fn collect_stack_contents(
    ctx: &CfnContext,
    stack_name: &str,
) -> Result<crate::output::StackContents> {
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
    let resources = crate::output::aws_conversion::convert_stack_resources(
        resources_resp.stack_resources.unwrap_or_default()
    );

    // Extract outputs from stack
    let outputs = crate::output::aws_conversion::convert_stack_outputs(
        stack.outputs.unwrap_or_default()
    );

    // Get exports if any outputs have export names
    let stack_id = stack.stack_id.clone().unwrap_or_default();
    let exports = crate::output::aws_conversion::convert_outputs_to_exports(&outputs, &stack_id);

    // Current status
    let current_status = crate::output::StackStatusInfo {
        status: stack.stack_status.map(|s| s.as_str().to_string()).unwrap_or_default(),
        status_reason: stack.stack_status_reason,
        timestamp: stack.last_updated_time.or(stack.creation_time).and_then(|ts| {
            chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
        }),
    };

    Ok(crate::output::StackContents {
        resources,
        outputs,
        exports,
        current_status,
        pending_changesets: vec![], // Would need separate query for changeset operations
    })
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
        start_time: Option<DateTime<Utc>>,
    ) -> (Vec<StackEvent>, bool, Option<String>) {
        // Filter events by start time if provided
        let filtered_events = match start_time {
            Some(start) => Self::filter_events_after_start_time(events, start),
            None => events,
        };

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
        
        let stack_id = stack.stack_id
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