use anyhow::Result;
use aws_sdk_cloudformation::{
    Client,
    types::StackEvent,
};
use aws_smithy_types::date_time::Format;
use std::collections::HashSet;
use std::time::Duration;
use std::io::Write;

use crate::{
    aws,
    cli::{AwsOpts, WatchArgs},
    cfn::is_terminal_status::is_terminal_resource_status,
};

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
            return is_terminal_resource_status(&status);
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

/// Display any new events and return true if the stack has reached a terminal state.
fn display_events(events: Vec<StackEvent>, seen: &mut HashSet<String>, stack_name: &str) -> bool {
    let mut done = false;
    for ev in events {
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
        Spinner { enabled, frames: ["-", "\\", "|", "/"], idx: 0 }
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

/// Watch a CloudFormation stack for changes.
pub async fn watch_stack(opts: &AwsOpts, args: &WatchArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let client = Client::new(&config);

    let stack_name = &args.stackname;
    let poll = Duration::from_secs(2);
    let mut seen = HashSet::new();
    let mut spinner = Spinner::new(atty::is(atty::Stream::Stdout));

    loop {
        let events = fetch_events(&client, stack_name).await?;

        if display_events(events, &mut seen, stack_name) {
            return Ok(());
        }

        spinner.spin(poll).await;
    }
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
}
