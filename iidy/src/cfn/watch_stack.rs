use anyhow::Result;
use aws_sdk_cloudformation::{Client, types::StackEvent};
#[cfg(test)]
use chrono::Utc;
use std::collections::HashSet;
use std::time::Duration;

// Default number of previous events to show when watching a stack
const DEFAULT_PREVIOUS_EVENTS_COUNT: usize = 10;

// Default poll interval for watch operations (seconds)
pub const DEFAULT_POLL_INTERVAL_SECS: u64 = 2;


use crate::{
    cli::Commands,
    cfn::CfnContext,
    output::{
        DynamicOutputManager, OutputData,
        StackEventWithTiming,
        OperationCompleteInfo, InactivityTimeoutInfo,
    },
};

use super::{stack_operations::{StackEventsService, StackInfoService}};

// Removed format_event function - using data-driven output architecture instead

// Event-related functions have been moved to stack_operations::StackEventsService

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
    let context = crate::cfn::create_context_for_operation(&opts, crate::cfn::CfnOperation::WatchStack).await?;

    // Get stack ARN first for reliable polling (important for delete operations)
    let client = context.client.clone();
    let stack_name = args.stackname.clone();
    
    // Get stack info and ID using the consolidated service
    let stack = StackInfoService::get_stack(&client, &stack_name).await?;
    let stack_id = StackInfoService::get_stack_id(&client, &stack_name).await?;
    
    // Start stack definition task (already have the stack data)
    let stack_task = {
        let tx = sender.clone();
        let stack_clone = stack.clone();
        tokio::spawn(async move {
            let output_data = crate::output::convert_stack_to_definition(&stack_clone, true);
            let _ = tx.send(output_data);
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Sequential execution: previous events MUST complete before live events start
    let events_and_live_task: tokio::task::JoinHandle<Result<Option<String>, anyhow::Error>> = {
        let client = client.clone();
        let stack_id = stack_id.clone();
        let _stack_name = stack_name.clone(); // Keep for display purposes but not used
        let tx = sender.clone();
        let context_clone = context.clone();
        let inactivity_timeout = args.inactivity_timeout;
        
        tokio::spawn(async move {
            // Step 1: Fetch and display previous events using stack ID
            // Note: StackEventsService::fetch_events() handles basic pagination but not our event count limit
            // For now, use the simple version and enhance the service later if needed
            let all_events = StackEventsService::fetch_events(&client, &stack_id).await?;
            
            // Create events display for PREVIOUS events (separate from live events)
            let output_data = crate::output::aws_conversion::convert_stack_events_to_display_with_max(
                all_events.clone(), // Clone for live events task to use
                &format!("Previous Stack Events (max {}):", event_count),
                Some(event_count),
            );
            
            let _ = tx.send(output_data);
            
            // Step 2: Now start live events polling with all existing events pre-marked as seen
            let sender_output = SenderOutput { sender: tx };
            let final_status = watch_stack_live_events_with_seen_events(&client, &context_clone, &stack_id, sender_output, Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS), Duration::from_secs(inactivity_timeout as u64), all_events).await?;
            Ok(final_status)
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
    let final_status = events_and_live_result??;
    
    // Final step: Show stack contents like iidy-js
    // Skip stack contents if the stack was deleted (DELETE_COMPLETE)
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            // Stack was deleted, skip stack contents collection as it will fail
            // No need to show empty stack contents
            return Ok(());
        }
    }
    
    // Normal case - show stack contents
    let stack_contents = crate::cfn::stack_operations::collect_stack_contents(&context, &stack_id).await?;
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
/// Returns the final stack status if the stack reached a terminal state
pub async fn watch_stack_live_events_with_seen_events(
    client: &Client,
    context: &CfnContext,
    stack_identifier: &str,
    mut output: impl LiveEventsOutput,
    poll_interval: Duration,
    inactivity_timeout: Duration,
    previous_events: Vec<StackEvent>,
) -> Result<Option<String>> {
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
    let mut final_stack_status = None;
    
    // Main polling loop (pure data collection - no formatting)
    let mut done = false;
    while !done {
        // Poll for new events
        let events = StackEventsService::fetch_events(client, stack_identifier).await?;
        let (new_events, terminal_detected, terminal_status) = StackEventsService::process_new_events(events, &mut seen, stack_identifier, context.start_time);
        
        // Track the final status if we detected a terminal event
        if terminal_detected {
            final_stack_status = terminal_status;
        }
        
        // Process new events if any
        if !new_events.is_empty() {
            last_event_time = chrono::Utc::now();
            
            // Convert and send new events (renderer handles all formatting)
            let converted_events: Vec<StackEventWithTiming> = new_events.iter()
                .map(|aws_event| {
                    let converted_event = crate::output::aws_conversion::convert_aws_stack_event(aws_event);
                    
                    // Calculate duration from operation start time
                    let duration_seconds = if let Some(event_time) = &converted_event.timestamp {
                        Some((event_time.timestamp() - context.start_time.timestamp()).max(0) as u64)
                    } else {
                        None
                    };
                    
                    StackEventWithTiming {
                        event: converted_event,
                        duration_seconds,
                    }
                })
                .collect();
            
            output.send_new_events(converted_events).await?;
        }
        
        // Check for completion (send completion signal to renderer)
        if terminal_detected {
            let completion_info = OperationCompleteInfo {
                elapsed_seconds: context.elapsed_seconds().await?,
                operation_start_time: context.start_time,
                skip_remaining_sections: final_stack_status.as_ref().map_or(false, |s| s == "DELETE_COMPLETE"),
            };
            let _ = output.send_operation_complete(completion_info).await;
            done = true;
        }
        // Check for inactivity timeout (send timeout signal to renderer)
        else if inactivity_timeout.as_secs() > 0 && (chrono::Utc::now() - last_event_time).num_seconds() as u64 > inactivity_timeout.as_secs() {
            let timeout_info = InactivityTimeoutInfo {
                timeout_seconds: inactivity_timeout.as_secs(),
                elapsed_seconds: context.elapsed_seconds().await?,
                operation_start_time: context.start_time,
            };
            let _ = output.send_inactivity_timeout(timeout_info).await;
            done = true;
        }
        
        if !done {
            tokio::time::sleep(poll_interval).await;
        }
    }
    
    Ok(final_stack_status)
}


// Removed duplicated helper functions - using existing functions from aws_conversion.rs and timing module


/// Compatibility function for other command handlers that need to watch stack progress
/// This maintains the old interface while using the new data-driven architecture internally
pub async fn watch_stack_with_data_output(
    ctx: &CfnContext,
    stack_identifier: &str,
    output_manager: &mut DynamicOutputManager,
    poll_interval: Duration,
) -> Result<Option<String>> {
    // Use the proper implementation that waits for terminal states
    let manager_output = ManagerOutput { manager: output_manager };
    watch_stack_live_events_with_seen_events(
        &ctx.client, 
        ctx, 
        stack_identifier, 
        manager_output, 
        poll_interval, 
        Duration::from_secs(3600), // 1 hour timeout 
        vec![] // No previous events
    ).await
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
        assert!(StackEventsService::check_terminal_event(&ev, "demo").is_some());
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
        let filtered = StackEventsService::filter_events_after_start_time(events, start_time);

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
        // Test that start time is 500ms before the fixed time
        let expected_start = fixed_time - chrono::Duration::milliseconds(500);
        assert_eq!(ctx.start_time, expected_start);
    }
}
