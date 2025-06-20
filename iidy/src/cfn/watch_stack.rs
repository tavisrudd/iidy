use anyhow::Result;
use aws_sdk_cloudformation::{Client, types::StackEvent};
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::time::Duration;

// Default number of previous events to show when watching a stack
const DEFAULT_PREVIOUS_EVENTS_COUNT: usize = 10;

// Default poll interval for watch operations (seconds)
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 2;

// Very long timeout for effectively infinite waiting (1 year in seconds)
const INFINITE_TIMEOUT_SECS: u64 = 86400 * 365;

use crate::{
    cli::Commands,
    output::{
        DynamicOutputManager, OutputData,
        StackEventWithTiming,
        OperationCompleteInfo, InactivityTimeoutInfo,
    },
};

use super::{CfnContext, is_terminal_status::is_terminal_resource_status};

// Removed format_event function - using data-driven output architecture instead

/// Determine if an event indicates the stack has reached a terminal state.
fn event_indicates_terminal(event: &StackEvent, stack_name: &str) -> bool {
    if event.resource_type() == Some("AWS::CloudFormation::Stack")
        && event.logical_resource_id() == Some(stack_name)
    {
        if let Some(status) = event.resource_status() {
            return is_terminal_resource_status(status);
        }
    }
    false
}

/// Retrieve and sort all events for a stack.
async fn fetch_events(client: &Client, stack_name: &str) -> Result<Vec<StackEvent>> {
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
fn aws_timestamp_to_chrono(aws_time: &aws_smithy_types::DateTime) -> Option<DateTime<Utc>> {
    DateTime::from_timestamp(aws_time.secs(), aws_time.subsec_nanos())
        .map(|dt| dt.with_timezone(&Utc))
}

/// Filter events to only include those after the given start time
fn filter_events_after_start_time(
    events: Vec<StackEvent>,
    start_time: DateTime<Utc>,
) -> Vec<StackEvent> {
    events
        .into_iter()
        .filter(|event| {
            event
                .timestamp()
                .and_then(|ts| aws_timestamp_to_chrono(ts))
                .map(|event_time| event_time > start_time)
                .unwrap_or(false)
        })
        .collect()
}

/// Filter new events and check for terminal state, using data-driven architecture
fn process_new_events(
    events: Vec<StackEvent>,
    seen: &mut HashSet<String>,
    stack_name: &str,
    start_time: Option<DateTime<Utc>>,
) -> (Vec<StackEvent>, bool) {
    // Filter events by start time if provided
    let filtered_events = match start_time {
        Some(start) => filter_events_after_start_time(events, start),
        None => events,
    };

    let mut new_events = Vec::new();
    let mut done = false;
    
    for ev in filtered_events {
        if let Some(id) = ev.event_id() {
            if seen.insert(id.to_string()) {
                if event_indicates_terminal(&ev, stack_name) {
                    done = true;
                }
                new_events.push(ev);
            }
        }
    }
    
    (new_events, done)
}

// Removed manual Spinner struct - using data-driven output architecture instead

// Removed watch_stack_with_context - replaced with data-driven architecture in watch_stack function

/// Watch a CloudFormation stack for changes with DynamicOutputManager.
/// 
/// Follows the exact iidy-js watchStackMain pattern:
/// 1. Show stack definition
/// 2. Show previous stack events (max 10)  
/// 3. Show live stack events with polling and spinner
/// 4. Show stack contents at the end
pub async fn watch_stack(
    cli: &crate::cli::Cli
) -> Result<()> {
    let Commands::WatchStack(args) = &cli.command else {
        return Err(anyhow::anyhow!("Invalid command for watch_stack"));
    };
    
    // Normalize AWS options 
    let opts = cli.aws_opts.clone().normalize();
    
    // Setup data-driven output manager with full CLI context (like describe-stack)
    let output_options = crate::output::manager::OutputOptions::new(cli.clone());
    let mut output_manager = DynamicOutputManager::new(
        cli.global_opts.effective_output_mode(),
        output_options
    ).await?;

    let event_count = DEFAULT_PREVIOUS_EVENTS_COUNT; // Fixed at 10 for watch-stack per iidy-js
    
    // Start parallel rendering
    let sender = output_manager.start();
    
    // Setup AWS context (no need for command metadata for read-only operation)
    let context = crate::cfn::create_context_for_operation(&opts, crate::output::CfnOperation::WatchStack).await?;

    // Clone values needed for the async tasks
    let client = context.client.clone();
    let stack_name = args.stackname.clone();
    
    // Start stack definition task
    let stack_task = {
        let client = client.clone();
        let stack_name = stack_name.clone();
        let tx = sender.clone();
        tokio::spawn(async move {
            let stack_resp = client
                .describe_stacks()
                .stack_name(&stack_name)
                .send()
                .await
                .map_err(anyhow::Error::from)?;
                
            let stack = stack_resp
                .stacks
                .and_then(|mut s| s.pop())
                .ok_or_else(|| anyhow::anyhow!("stack not found"))?;
                
            let output_data = crate::output::convert_stack_to_definition(&stack, true);
            let _ = tx.send(output_data);
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Sequential execution: previous events MUST complete before live events start
    let events_and_live_task = {
        let client = client.clone();
        let stack_name = stack_name.clone();
        let tx = sender.clone();
        let live_start_time = context.start_time;
        let inactivity_timeout = args.inactivity_timeout;
        
        tokio::spawn(async move {
            // Step 1: Fetch and display previous events
            let first_events_resp = client
                .describe_stack_events()
                .stack_name(&stack_name)
                .send()
                .await
                .map_err(anyhow::Error::from)?;
        
            // Continue fetching stack events if needed (pagination)
            let mut all_events = first_events_resp.stack_events.unwrap_or_default();
            let mut next_token = first_events_resp.next_token;
            
            // Fetch additional pages if needed
            while next_token.is_some() && all_events.len() < event_count * 2 {
                let events_resp = client
                    .describe_stack_events()
                    .stack_name(&stack_name)
                    .set_next_token(next_token)
                    .send()
                    .await?;
                    
                let mut page_events = events_resp.stack_events.unwrap_or_default();
                all_events.append(&mut page_events);
                next_token = events_resp.next_token;
            }
            
            // Create events display for PREVIOUS events (separate from live events)
            let output_data = crate::output::aws_conversion::convert_stack_events_to_display_with_max(
                all_events.clone(), // Clone for live events task to use
                &format!("Previous Stack Events (max {}):", event_count),
                Some(event_count),
            );
            
            let _ = tx.send(output_data);
            
            // Step 2: Now start live events polling with all existing events pre-marked as seen
            let sender_output = SenderOutput { sender: tx };
            watch_stack_live_events_with_seen_events(&client, live_start_time, &stack_name, sender_output, Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS), Duration::from_secs(inactivity_timeout as u64), all_events).await
        })
    };
    
    // Drop the original sender so the receiver knows when all tasks are done
    drop(sender);
    
    // Process and render all data from parallel operations (but keep renderer alive)
    output_manager.stop().await?;
    
    // Wait for all tasks to complete and handle any errors
    let (stack_result, events_and_live_result) = tokio::join!(
        stack_task,
        events_and_live_task
    );
    
    // Propagate any errors from the spawned tasks
    stack_result??;
    events_and_live_result??;
    
    // Final step: Show stack contents like iidy-js (restart the output manager)
    let stack_contents = collect_stack_contents(&context, &stack_name).await?;
    let sender = output_manager.start();
    let _ = sender.send(OutputData::StackContents(stack_contents));
    drop(sender);
    output_manager.stop().await?;
    
    Ok(())
}

/// Output trait for live events - allows using either DynamicOutputManager or sender (public for use by other operations)
pub trait LiveEventsOutput {
    fn send_new_events(&mut self, events: Vec<StackEventWithTiming>) -> impl std::future::Future<Output = Result<()>> + Send;
    fn send_operation_complete(&mut self, info: OperationCompleteInfo) -> impl std::future::Future<Output = Result<()>> + Send;
    fn send_inactivity_timeout(&mut self, info: InactivityTimeoutInfo) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Implementation for DynamicOutputManager
struct ManagerOutput<'a> {
    manager: &'a mut DynamicOutputManager,
}

impl<'a> LiveEventsOutput for ManagerOutput<'a> {
    fn send_new_events(&mut self, events: Vec<StackEventWithTiming>) -> impl std::future::Future<Output = Result<()>> + Send {
        self.manager.render(OutputData::NewStackEvents(events))
    }
    
    fn send_operation_complete(&mut self, info: OperationCompleteInfo) -> impl std::future::Future<Output = Result<()>> + Send {
        self.manager.render(OutputData::OperationComplete(info))
    }
    
    fn send_inactivity_timeout(&mut self, info: InactivityTimeoutInfo) -> impl std::future::Future<Output = Result<()>> + Send {
        self.manager.render(OutputData::InactivityTimeout(info))
    }
}

/// Implementation for direct sender (public for use by other operations)
pub struct SenderOutput {
    pub sender: tokio::sync::mpsc::UnboundedSender<OutputData>,
}

impl LiveEventsOutput for SenderOutput {
    fn send_new_events(&mut self, events: Vec<StackEventWithTiming>) -> impl std::future::Future<Output = Result<()>> + Send {
        let _ = self.sender.send(OutputData::NewStackEvents(events));
        async { Ok(()) }
    }
    
    fn send_operation_complete(&mut self, info: OperationCompleteInfo) -> impl std::future::Future<Output = Result<()>> + Send {
        let _ = self.sender.send(OutputData::OperationComplete(info));
        async { Ok(()) }
    }
    
    fn send_inactivity_timeout(&mut self, info: InactivityTimeoutInfo) -> impl std::future::Future<Output = Result<()>> + Send {
        let _ = self.sender.send(OutputData::InactivityTimeout(info));
        async { Ok(()) }
    }
}

/// Live events polling function with pre-fetched events marked as seen (public for use by other operations)
pub async fn watch_stack_live_events_with_seen_events(
    client: &Client,
    start_time: Option<DateTime<Utc>>,
    stack_name: &str,
    mut output: impl LiveEventsOutput,
    poll_interval: Duration,
    inactivity_timeout: Duration,
    previous_events: Vec<StackEvent>,
) -> Result<()> {
    // Don't send live events title - let renderer handle section transition
    // The renderer will detect when NewStackEvents start coming and show the live section title

    // Pre-populate seen events from the previous events that were already displayed
    let mut seen: HashSet<String> = HashSet::new();
    for event in &previous_events {
        if let Some(id) = event.event_id() {
            seen.insert(id.to_string());
        }
    }
    
    let mut last_event_time = chrono::Utc::now();
    
    // Main polling loop (pure data collection - no formatting)
    let mut done = false;
    while !done {
        // Poll for new events
        let events = fetch_events(client, stack_name).await?;
        let (new_events, terminal_detected) = process_new_events(events, &mut seen, stack_name, start_time);
        
        // Process new events if any
        if !new_events.is_empty() {
            last_event_time = chrono::Utc::now();
            
            // Convert and send new events (renderer handles all formatting)
            let converted_events: Vec<StackEventWithTiming> = new_events.iter()
                .map(|aws_event| StackEventWithTiming {
                    event: crate::output::aws_conversion::convert_aws_stack_event(aws_event),
                    duration_seconds: None,
                })
                .collect();
            
            output.send_new_events(converted_events).await?;
        }
        
        // Check for completion (send completion signal to renderer)
        if terminal_detected {
            if let Some(start_time) = start_time {
                let completion_info = OperationCompleteInfo {
                    elapsed_seconds: (chrono::Utc::now() - start_time).num_seconds(),
                    operation_start_time: start_time,
                };
                let _ = output.send_operation_complete(completion_info).await;
            }
            done = true;
        }
        // Check for inactivity timeout (send timeout signal to renderer)
        else if inactivity_timeout.as_secs() > 0 && (chrono::Utc::now() - last_event_time).num_seconds() as u64 > inactivity_timeout.as_secs() {
            if let Some(start_time) = start_time {
                let timeout_info = InactivityTimeoutInfo {
                    timeout_seconds: inactivity_timeout.as_secs(),
                    elapsed_seconds: (chrono::Utc::now() - start_time).num_seconds(),
                    operation_start_time: start_time,
                };
                let _ = output.send_inactivity_timeout(timeout_info).await;
            }
            done = true;
        }
        
        if !done {
            tokio::time::sleep(poll_interval).await;
        }
    }
    
    Ok(())
}

/// Live events polling function with inactivity timeout (pure data collection)
async fn watch_stack_live_events_with_timeout(
    client: &Client,
    start_time: Option<DateTime<Utc>>,
    stack_name: &str,
    mut output: impl LiveEventsOutput,
    poll_interval: Duration,
    inactivity_timeout: Duration,
) -> Result<()> {
    // Don't send live events title - let renderer handle section transition
    // The renderer will detect when NewStackEvents start coming and show the live section title

    // Pre-populate seen events to avoid showing previous events as live events
    // Fetch all current events and mark them as seen
    let initial_events = fetch_events(client, stack_name).await?;
    let mut seen: HashSet<String> = HashSet::new();
    for event in &initial_events {
        if let Some(id) = event.event_id() {
            seen.insert(id.to_string());
        }
    }
    
    let mut last_event_time = chrono::Utc::now();
    
    // Main polling loop (pure data collection - no formatting)
    let mut done = false;
    while !done {
        // Poll for new events
        let events = fetch_events(client, stack_name).await?;
        let (new_events, terminal_detected) = process_new_events(events, &mut seen, stack_name, start_time);
        
        // Process new events if any
        if !new_events.is_empty() {
            last_event_time = chrono::Utc::now();
            
            // Convert and send new events (renderer handles all formatting)
            let converted_events: Vec<StackEventWithTiming> = new_events.iter()
                .map(|aws_event| StackEventWithTiming {
                    event: crate::output::aws_conversion::convert_aws_stack_event(aws_event),
                    duration_seconds: None,
                })
                .collect();
            
            output.send_new_events(converted_events).await?;
        }
        
        // Check for completion (send completion signal to renderer)
        if terminal_detected {
            if let Some(start_time) = start_time {
                let completion_info = OperationCompleteInfo {
                    elapsed_seconds: (chrono::Utc::now() - start_time).num_seconds(),
                    operation_start_time: start_time,
                };
                let _ = output.send_operation_complete(completion_info).await;
            }
            done = true;
        }
        // Check for inactivity timeout (send timeout signal to renderer)
        else if inactivity_timeout.as_secs() > 0 && (chrono::Utc::now() - last_event_time).num_seconds() as u64 > inactivity_timeout.as_secs() {
            if let Some(start_time) = start_time {
                let timeout_info = InactivityTimeoutInfo {
                    timeout_seconds: inactivity_timeout.as_secs(),
                    elapsed_seconds: (chrono::Utc::now() - start_time).num_seconds(),
                    operation_start_time: start_time,
                };
                let _ = output.send_inactivity_timeout(timeout_info).await;
            }
            done = true;
        }
        
        if !done {
            tokio::time::sleep(poll_interval).await;
        }
    }
    
    Ok(())
}

/// Consolidated live events polling function - works with any output method (without timeout)
async fn watch_stack_live_events_unified(
    client: &Client,
    start_time: Option<DateTime<Utc>>,
    stack_name: &str,
    output: impl LiveEventsOutput,
    poll_interval: Duration,
) -> Result<()> {
    // Use timeout version with a very long timeout (effectively infinite)
    watch_stack_live_events_with_timeout(
        client,
        start_time,
        stack_name,
        output,
        poll_interval,
        Duration::from_secs(INFINITE_TIMEOUT_SECS), // 1 year timeout (effectively infinite)
    ).await
}

// Removed duplicated helper functions - using existing functions from aws_conversion.rs and timing module

/// Collect stack contents data (controller pattern - no display logic, public for use by other operations)
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
        pending_changesets: vec![], // Would need separate query
    })
}

/// Compatibility function for other command handlers that need to watch stack progress
/// This maintains the old interface while using the new data-driven architecture internally
pub async fn watch_stack_with_data_output(
    ctx: &CfnContext,
    stack_name: &str,
    output_manager: &mut DynamicOutputManager,
    poll_interval: Duration,
) -> Result<()> {
    // Use the unified implementation with manager output
    let manager_output = ManagerOutput { manager: output_manager };
    watch_stack_live_events_unified(&ctx.client, ctx.start_time, stack_name, manager_output, poll_interval).await
}


#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_cloudformation::types::ResourceStatus;
    use aws_smithy_types::DateTime;
    use std::sync::Arc;

    fn sample_event(id: &str, ts: i64, status: ResourceStatus) -> StackEvent {
        StackEvent::builder()
            .stack_id("arn:aws:cloudformation:us-east-1:123456789012:stack/demo/1")
            .event_id(id)
            .stack_name("demo")
            .logical_resource_id("demo")
            .resource_type("AWS::CloudFormation::Stack")
            .timestamp(DateTime::from_secs(ts))
            .resource_status(status)
            .build()
    }

    // Removed test for format_event - using data-driven output architecture

    #[test]
    fn detect_terminal_event() {
        let ev = sample_event("2", 0, ResourceStatus::CreateComplete);
        assert!(event_indicates_terminal(&ev, "demo"));
    }

    #[test]
    fn filter_events_after_start_time_works() {
        use chrono::TimeZone;

        let start_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();

        // Create events before and after start time
        let old_event = sample_event(
            "1",
            start_time.timestamp() - 10,
            ResourceStatus::CreateInProgress,
        );
        let new_event = sample_event(
            "2",
            start_time.timestamp() + 10,
            ResourceStatus::CreateComplete,
        );

        let events = vec![old_event, new_event];
        let filtered = filter_events_after_start_time(events, start_time);

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].event_id().unwrap(), "2");
    }

    #[tokio::test]
    async fn watch_stack_with_context_filters_events() {
        use crate::timing::{MockTimeProvider, TokenInfo};
        use chrono::TimeZone;

        // This test would require mocking the AWS client
        // For now, just test that the context can be created with proper config
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));

        let config = aws_config::SdkConfig::builder()
            .region(aws_types::region::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        let client = Client::new(&config);

        let temp_token = TokenInfo::auto_generated("test-token".to_string(), "test-op".to_string());
        let aws_config = aws_config::SdkConfig::builder()
            .region(aws_types::region::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        let ctx = CfnContext::new(client, aws_config, time_provider, temp_token)
            .await
            .unwrap();
        assert!(ctx.start_time.is_some());

        // Test that start time is 500ms before the fixed time
        let expected_start = fixed_time - chrono::Duration::milliseconds(500);
        assert_eq!(ctx.start_time.unwrap(), expected_start);
    }
}
