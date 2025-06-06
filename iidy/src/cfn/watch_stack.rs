use anyhow::Result;
use aws_sdk_cloudformation::{Client, types::StackEvent};
use aws_smithy_types::date_time::Format;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::io::Write;
use std::sync::Arc;
use std::time::Duration;

use crate::{
    aws,
    cli::{NormalizedAwsOpts, WatchArgs},
    timing::{ReliableTimeProvider, TimeProvider},
};

use super::{is_terminal_status::is_terminal_resource_status, CfnContext};

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
    DateTime::from_timestamp(aws_time.secs(), aws_time.subsec_nanos()).map(|dt| dt.with_timezone(&Utc))
}

/// Filter events to only include those after the given start time
fn filter_events_after_start_time(events: Vec<StackEvent>, start_time: DateTime<Utc>) -> Vec<StackEvent> {
    events.into_iter()
        .filter(|event| {
            event.timestamp()
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
    start_time: Option<DateTime<Utc>>
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

/// Watch a CloudFormation stack for changes.
/// 
/// This function creates its own timing context with proper token management.
pub async fn watch_stack(opts: &NormalizedAwsOpts, args: &WatchArgs) -> Result<()> {
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
        let old_event = sample_event("1", start_time.timestamp() - 10, ResourceStatus::CreateInProgress);
        let new_event = sample_event("2", start_time.timestamp() + 10, ResourceStatus::CreateComplete);
        
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
        let ctx = CfnContext::new(client, time_provider, temp_token).await.unwrap();
        assert!(ctx.start_time.is_some());
        
        // Test that start time is 500ms before the fixed time
        let expected_start = fixed_time - chrono::Duration::milliseconds(500);
        assert_eq!(ctx.start_time.unwrap(), expected_start);
    }
}
