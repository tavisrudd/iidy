use anyhow::{Result, anyhow};
use aws_sdk_cloudformation::{Client, types::Stack};
use aws_smithy_types::date_time::Format;
use std::sync::Arc;

use crate::display::display_lines;
use crate::{
    aws,
    cfn::{CfnContext, ConsoleReporter},
    cli::{DescribeArgs, NormalizedAwsOpts},
    timing::{ReliableTimeProvider, TimeProvider},
};

/// Format a [`Stack`] object into human readable lines.
///
/// This mirrors the output of the original Node.js implementation but only
/// includes a subset of information for now. Each string in the returned vector
/// represents a line that can be printed directly to stdout.
pub fn format_stack(stack: Stack) -> Vec<String> {
    let mut lines = Vec::new();

    let name = stack.stack_name().unwrap_or("unknown");
    lines.push(format!("Name: {name}"));

    if let Some(desc) = stack.description() {
        lines.push(format!("Description: {desc}"));
    }

    if let Some(status) = stack.stack_status() {
        lines.push(format!("Status: {}", status.as_str()));
    }

    if let Some(time) = stack.creation_time().or_else(|| stack.last_updated_time()) {
        if let Ok(ts) = time.fmt(Format::DateTime) {
            lines.push(format!("Last Updated: {ts}"));
        }
    }

    let tags = stack
        .tags()
        .iter()
        .filter_map(|t| match (t.key(), t.value()) {
            (Some(k), Some(v)) => Some(format!("{k}={v}")),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("; ");
    if !tags.is_empty() {
        lines.push(format!("Tags: {tags}"));
    }

    lines
}

/// Retrieve a stack description from AWS and format it.
///
/// This function performs the AWS API call and delegates formatting to
/// [`format_stack`].
pub async fn describe_stack(opts: &NormalizedAwsOpts, args: &DescribeArgs) -> Result<()> {
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;

    let reporter = ConsoleReporter::new("describe-stack");
    reporter.show_primary_token(&context.primary_token());

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

    display_lines(format_stack(stack));

    reporter.show_operation_summary(&context);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_cloudformation::types::{StackStatus, Tag};
    use aws_smithy_types::DateTime;

    fn sample_stack(name: &str) -> Stack {
        Stack::builder()
            .stack_name(name)
            .description("sample stack")
            .stack_status(StackStatus::CreateComplete)
            .creation_time(DateTime::from_secs(0))
            .tags(Tag::builder().key("env").value("test").build())
            .build()
    }

    #[test]
    fn formats_stack() {
        let lines = format_stack(sample_stack("demo"));
        assert!(lines.iter().any(|l| l.contains("Name: demo")));
        assert!(lines.iter().any(|l| l.contains("CREATE_COMPLETE")));
        assert!(lines.iter().any(|l| l.contains("env=test")));
    }
}
