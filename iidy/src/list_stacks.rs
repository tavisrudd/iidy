use anyhow::Result;
use aws_sdk_cloudformation::{types::Stack, Client};
use aws_smithy_types::date_time::Format;

use crate::{aws, cli::{AwsOpts, ListArgs}};

/// Format a list of [`Stack`] objects similar to the Node.js implementation.
///
/// Stacks are sorted by creation time and rendered as comma separated
/// strings containing the timestamp, status and stack name. If `show_tags`
/// is true, tags are appended as key=value pairs separated by semicolons.
pub fn format_stacks(stacks: Vec<Stack>, show_tags: bool) -> Vec<String> {
    let mut stacks = stacks;
    stacks.sort_by_key(|s| {
        s.creation_time()
            .or_else(|| s.last_updated_time())
            .map(|dt| dt.as_nanos())
            .unwrap_or_default()
    });

    stacks
        .into_iter()
        .map(|st| {
            let time = st
                .creation_time()
                .or_else(|| st.last_updated_time())
                .map(|t| t.fmt(Format::DateTime).unwrap_or_default())
                .unwrap_or_else(|| "unknown".to_string());
            let status = st.stack_status().map(|s| s.as_str()).unwrap_or("UNKNOWN");
            let name = st.stack_name().unwrap_or("unknown");
            if show_tags {
                let tags = st
                    .tags()
                    .iter()
                    .filter_map(|t| match (t.key(), t.value()) {
                        (Some(k), Some(v)) => Some(format!("{k}={v}")),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                if tags.is_empty() {
                    format!("{time}, {status}, {name}")
                } else {
                    format!("{time}, {status}, {name}, {tags}")
                }
            } else {
                format!("{time}, {status}, {name}")
            }
        })
        .collect()
}

/// Retrieve all stacks for the configured AWS region and format them.
///
/// The returned vector of strings can be printed directly to display the list
/// of stacks. Currently no filtering is implemented.
pub async fn list_stacks(opts: &AwsOpts, args: &ListArgs) -> Result<Vec<String>> {
    let config = aws::config_from_opts(opts).await?;
    let client = Client::new(&config);

    // Use the paginator to retrieve all stacks in the region.
    let stacks: Vec<Stack> = client
        .describe_stacks()
        .into_paginator()
        .items()
        .send()
        .try_collect()
        .await?;

    Ok(format_stacks(stacks, args.tags))
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_cloudformation::types::{StackStatus, Tag};
    use aws_smithy_types::DateTime;

    fn sample_stack(name: &str, ts: i64, status: StackStatus) -> Stack {
        Stack::builder()
            .stack_name(name)
            .creation_time(DateTime::from_secs(ts))
            .stack_status(status)
            .tags(Tag::builder().key("env").value("test").build())
            .build()
    }

    #[test]
    fn formats_stacks() {
        let stacks = vec![
            sample_stack("b", 2, StackStatus::UpdateInProgress),
            sample_stack("a", 1, StackStatus::CreateComplete),
        ];
        let lines = format_stacks(stacks, false);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("1970-01-01T00:00:01Z"));
        assert!(lines[0].contains("CREATE_COMPLETE"));
        assert!(lines[0].contains(", a"));
    }
}
