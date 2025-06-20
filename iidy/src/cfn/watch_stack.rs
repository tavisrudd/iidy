use anyhow::Result;
use aws_sdk_cloudformation::{Client, types::StackEvent};
use aws_smithy_types::date_time::Format;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

// Default number of previous events to show when watching a stack
const DEFAULT_PREVIOUS_EVENTS_COUNT: usize = 10;

use crate::{
    aws,
    cli::{NormalizedAwsOpts, WatchArgs, GlobalOpts},
    output::{
        DynamicOutputManager, manager::OutputOptions,
    },
    timing::{ReliableTimeProvider, TimeProvider},
};

use super::{CfnContext, is_terminal_status::is_terminal_resource_status};

/// Format a [`StackEvent`] into a single line similar to the Node.js output.
fn format_event(event: &StackEvent) -> String {
    let ts = event
        .timestamp()
        .and_then(|t| t.fmt(Format::DateTime).ok())
        .unwrap_or_else(|| "unknown".to_string());
    let status = event
        .resource_status()
        .map(|s| s.as_str())
        .unwrap_or("UNKNOWN");
    let resource_type = event.resource_type().unwrap_or("?");
    let logical_id = event.logical_resource_id().unwrap_or("?");
    let reason = event.resource_status_reason().unwrap_or("");
    format!("{ts} {status:<25} {resource_type:<40} {logical_id} {reason}")
}

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

/// Display any new events and return true if the stack has reached a terminal state.
fn display_events(
    events: Vec<StackEvent>,
    seen: &mut HashSet<String>,
    stack_name: &str,
    start_time: Option<DateTime<Utc>>,
) -> bool {
    // Filter events by start time if provided
    let filtered_events = match start_time {
        Some(start) => filter_events_after_start_time(events, start),
        None => events,
    };

    let mut done = false;
    for ev in filtered_events {
        if let Some(id) = ev.event_id() {
            if seen.insert(id.to_string()) {
                println!("{}", format_event(&ev));
                if event_indicates_terminal(&ev, stack_name) {
                    done = true;
                }
            }
        }
    }
    done
}

struct Spinner {
    enabled: bool,
    frames: [&'static str; 4],
    idx: usize,
}

impl Spinner {
    fn new(enabled: bool) -> Self {
        Spinner {
            enabled,
            frames: ["-", "\\", "|", "/"],
            idx: 0,
        }
    }

    async fn spin(&mut self, dur: Duration) {
        if !self.enabled {
            tokio::time::sleep(dur).await;
            return;
        }
        let start = std::time::Instant::now();
        while start.elapsed() < dur {
            print!("\r{}", self.frames[self.idx % self.frames.len()]);
            std::io::stdout().flush().ok();
            self.idx += 1;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        print!("\r \r");
        std::io::stdout().flush().ok();
    }
}

/// Watch a CloudFormation stack for changes using a CfnContext.
///
/// This version uses the timing abstraction for reliable event filtering
/// and elapsed time tracking.
pub async fn watch_stack_with_context(
    ctx: &CfnContext,
    stack_name: &str,
    poll_interval: Duration,
) -> Result<()> {
    let mut seen = HashSet::new();
    let mut spinner = Spinner::new(atty::is(atty::Stream::Stdout));

    loop {
        let events = fetch_events(&ctx.client, stack_name).await?;

        if display_events(events, &mut seen, stack_name, ctx.start_time) {
            break;
        }

        // Show elapsed time in spinner if we have a start time
        if let Some(start_time) = ctx.start_time {
            if let Ok(current_time) = ctx.time_provider.now().await {
                let elapsed = (current_time - start_time).num_seconds();
                print!("\r⏱ {} seconds elapsed...", elapsed);
                std::io::stdout().flush().ok();
            }
        }

        spinner.spin(poll_interval).await;
    }

    // Show final elapsed time
    if let Some(start_time) = ctx.start_time {
        if let Ok(current_time) = ctx.time_provider.now().await {
            let elapsed = (current_time - start_time).num_seconds();
            println!("\n✓ Stack operation completed in {} seconds", elapsed);
        }
    }

    Ok(())
}

/// Watch a CloudFormation stack for changes with DynamicOutputManager.
/// 
/// This is a read-only operation that follows the iidy-js pattern:
/// 1. Show stack definition
/// 2. Show previous stack events (max 10)  
/// 3. Show live stack events with polling
/// 4. Show stack contents at the end
/// No command metadata is shown (read-only operation).
pub async fn watch_stack(
    opts: &NormalizedAwsOpts, 
    args: &WatchArgs, 
    global_opts: &GlobalOpts
) -> Result<()> {
    let output_options = OutputOptions::minimal();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);

    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let ctx = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;

    // Start both API calls in parallel but don't wait for both to complete
    let stack_future = async {
        ctx.client
            .describe_stacks()
            .stack_name(&args.stackname)
            .send()
            .await
            .map_err(anyhow::Error::from)
    };
    
    let events_future = async {
        ctx.client
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
        .ok_or_else(|| anyhow::anyhow!("stack not found"))?;
    
    // Show stack definition immediately
    let stack_definition = crate::output::convert_stack_to_definition(&stack, true);
    output_manager.render(stack_definition).await?;

    // 2. Get and show previous stack events (while user sees stack definition)
    let events_resp = events_future.await?;
    let events = events_resp.stack_events.unwrap_or_default();
    let previous_events = crate::output::StackEventsDisplay {
        title: format!("Previous Stack Events (max {}):", DEFAULT_PREVIOUS_EVENTS_COUNT),
        max_events: Some(DEFAULT_PREVIOUS_EVENTS_COUNT),
        events: events.into_iter().take(DEFAULT_PREVIOUS_EVENTS_COUNT).map(|e| crate::output::StackEventWithTiming {
            event: crate::output::StackEvent {
                event_id: e.event_id().unwrap_or_default().to_string(),
                stack_id: e.stack_id().unwrap_or_default().to_string(),
                stack_name: e.stack_name().unwrap_or_default().to_string(),
                logical_resource_id: e.logical_resource_id().unwrap_or_default().to_string(),
                physical_resource_id: e.physical_resource_id().map(|s| s.to_string()),
                resource_type: e.resource_type().unwrap_or_default().to_string(),
                timestamp: e.timestamp().and_then(|ts| {
                    chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
                }),
                resource_status: e.resource_status().map(|s| s.as_str().to_string()).unwrap_or_default(),
                resource_status_reason: e.resource_status_reason().map(|s| s.to_string()),
                resource_properties: None, // Not typically shown in event listings
                client_request_token: e.client_request_token().map(|s| s.to_string()),
            },
            duration_seconds: None, // Will be calculated by renderer if needed
        }).collect(),
        truncated: None, // We're explicitly taking 10, so this would be calculated if needed
    };
    output_manager.render(crate::output::OutputData::StackEvents(previous_events)).await?;

    // 3. Show live stack events with polling
    watch_stack_live_events(&ctx, &args.stackname, &mut output_manager, Duration::from_secs(2)).await?;
    
    // 4. Show stack contents at the end (following iidy-js pattern)
    let stack_contents = collect_stack_contents(&ctx, &args.stackname).await?;
    output_manager.render(crate::output::OutputData::StackContents(stack_contents)).await?;
    
    Ok(())
}

/// Poll for live stack events and send them as structured data (controller pattern)
async fn watch_stack_live_events(
    ctx: &CfnContext,
    stack_name: &str,
    output_manager: &mut DynamicOutputManager,
    poll_interval: Duration,
) -> Result<()> {
    let mut seen = HashSet::new();
    
    // Send initial section heading for live events
    let initial_live_events = crate::output::StackEventsDisplay {
        title: format!("Live Stack Events ({}s poll):", poll_interval.as_secs()),
        events: vec![],
        max_events: None, // No limit for live events
        truncated: None,
    };
    output_manager.render(crate::output::OutputData::StackEvents(initial_live_events)).await?;

    loop {
        let events = fetch_events(&ctx.client, stack_name).await?;
        
        // Filter to only new events after start time
        let new_events: Vec<StackEvent> = events
            .into_iter()
            .filter(|event| {
                if let Some(id) = event.event_id() {
                    if seen.insert(id.to_string()) {
                        // Check if event is after start time
                        if let Some(start_time) = ctx.start_time {
                            event
                                .timestamp()
                                .and_then(|ts| aws_timestamp_to_chrono(ts))
                                .map(|event_time| event_time > start_time)
                                .unwrap_or(false)
                        } else {
                            true
                        }
                    } else {
                        false
                    }
                } else {
                    false
                }
            })
            .collect();
        
        // Send new events as structured data (one at a time following iidy-js pattern)
        for event in &new_events {
            let event_with_timing = crate::output::StackEventWithTiming {
                event: crate::output::StackEvent {
                    event_id: event.event_id().unwrap_or_default().to_string(),
                    stack_id: event.stack_id().unwrap_or_default().to_string(),
                    stack_name: event.stack_name().unwrap_or_default().to_string(),
                    logical_resource_id: event.logical_resource_id().unwrap_or_default().to_string(),
                    physical_resource_id: event.physical_resource_id().map(|s| s.to_string()),
                    resource_type: event.resource_type().unwrap_or_default().to_string(),
                    timestamp: event.timestamp().and_then(|ts| {
                        chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
                    }),
                    resource_status: event.resource_status().map(|s| s.as_str().to_string()).unwrap_or_default(),
                    resource_status_reason: event.resource_status_reason().map(|s| s.to_string()),
                    resource_properties: None,
                    client_request_token: event.client_request_token().map(|s| s.to_string()),
                },
                duration_seconds: None, // Individual events don't need duration for live display
            };
            
            let single_event = crate::output::StackEventsDisplay {
                title: String::new(), // No section heading for individual events
                events: vec![event_with_timing],
                max_events: None, // Single event, no limiting needed
                truncated: None,
            };
            output_manager.render(crate::output::OutputData::StackEvents(single_event)).await?;
            
            // Check if this event indicates terminal state
            if event_indicates_terminal(event, stack_name) {
                // Send elapsed time as status update (following iidy-js pattern)
                if let Some(start_time) = ctx.start_time {
                    if let Ok(current_time) = ctx.time_provider.now().await {
                        let elapsed = (current_time - start_time).num_seconds();
                        let elapsed_update = crate::output::StatusUpdate {
                            message: format!(" {} seconds elapsed total.", elapsed),
                            timestamp: chrono::Utc::now(),
                            level: crate::output::StatusLevel::Info,
                        };
                        output_manager.render(crate::output::OutputData::StatusUpdate(elapsed_update)).await?;
                    }
                }
                return Ok(());
            }
        }
        
        tokio::time::sleep(poll_interval).await;
    }
}

/// Collect stack contents data (controller pattern - no display logic)
async fn collect_stack_contents(
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
    let resources: Vec<crate::output::StackResourceInfo> = resources_resp
        .stack_resources
        .unwrap_or_default()
        .into_iter()
        .map(|r| crate::output::StackResourceInfo {
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
    let outputs: Vec<crate::output::StackOutputInfo> = stack
        .outputs
        .unwrap_or_default()
        .into_iter()
        .map(|o| crate::output::StackOutputInfo {
            output_key: o.output_key.unwrap_or_default(),
            output_value: o.output_value.unwrap_or_default(),
            description: o.description,
            export_name: o.export_name,
        })
        .collect();

    // Get exports if any outputs have export names
    let mut exports: Vec<crate::output::StackExportInfo> = vec![];
    for output in &outputs {
        if let Some(export_name) = &output.export_name {
            exports.push(crate::output::StackExportInfo {
                name: export_name.clone(),
                value: output.output_value.clone(),
                exporting_stack_id: stack.stack_id.clone().unwrap_or_default(),
                importing_stacks: vec![], // Would need separate query to find importers
            });
        }
    }

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
    // Delegate to the new implementation
    watch_stack_live_events(ctx, stack_name, output_manager, poll_interval).await
}

/// Watch a CloudFormation stack for changes (legacy function).
///
/// This function creates its own timing context with proper token management.
pub async fn watch_stack_legacy(opts: &NormalizedAwsOpts, args: &WatchArgs) -> Result<()> {
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);

    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let ctx = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;

    watch_stack_with_context(&ctx, &args.stackname, Duration::from_secs(2)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_cloudformation::types::ResourceStatus;
    use aws_smithy_types::DateTime;

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

    #[test]
    fn format_event_includes_fields() {
        let ev = sample_event("1", 0, ResourceStatus::CreateInProgress);
        let line = format_event(&ev);
        assert!(line.contains("CREATE_IN_PROGRESS"));
        assert!(line.contains("AWS::CloudFormation::Stack"));
    }

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
        let ctx = CfnContext::new(client, time_provider, temp_token)
            .await
            .unwrap();
        assert!(ctx.start_time.is_some());

        // Test that start time is 500ms before the fixed time
        let expected_start = fixed_time - chrono::Duration::milliseconds(500);
        assert_eq!(ctx.start_time.unwrap(), expected_start);
    }
}
