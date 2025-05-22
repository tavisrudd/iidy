use anstyle::{Ansi256Color, AnsiColor, Color, Style};
use anyhow::Result;
use atty::Stream;
use aws_sdk_cloudformation::{Client, types::Stack};
use aws_smithy_types::date_time::Format;

use crate::{
    aws,
    cli::{AwsOpts, ListArgs},
};

const TIME_PADDING: usize = 24;
const MIN_STATUS_PADDING: usize = 17;
const MAX_PADDING: usize = 60;

#[derive(Clone)]
struct ColorScheme {
    status_failed: Style,
    status_progress: Style,
    status_complete: Style,
    status_skipped: Style,
    prod: Style,
    integration: Style,
    development: Style,
    subtle: Style,
    timestamp: Style,
}

impl ColorScheme {
    fn default() -> Self {
        Self {
            status_failed: Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightRed))),
            status_progress: Style::new().fg_color(Some(Color::Ansi(AnsiColor::Yellow))),
            status_complete: Style::new().fg_color(Some(Color::Ansi(AnsiColor::Green))),
            status_skipped: Style::new().fg_color(Some(Color::Ansi(AnsiColor::Blue))),
            prod: Style::new().fg_color(Some(Color::Ansi(AnsiColor::Red))),
            integration: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(75)))),
            development: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(194)))),
            subtle: Style::new().fg_color(Some(Color::Ansi(AnsiColor::BrightBlack))),
            timestamp: Style::new().fg_color(Some(Color::Ansi256(Ansi256Color(253)))),
        }
    }
}

fn color_enabled() -> bool {
    atty::is(Stream::Stdout) && std::env::var_os("NO_COLOR").is_none()
}

fn apply_style(style: Style, text: &str, enabled: bool) -> String {
    if enabled {
        format!("{}{}{}", style.render(), text, style.render_reset())
    } else {
        text.to_string()
    }
}

fn calc_padding<'a, I, F>(items: I, selector: F) -> usize
where
    I: IntoIterator<Item = &'a Stack>,
    F: Fn(&'a Stack) -> &'a str,
{
    items
        .into_iter()
        .map(|i| selector(i).len())
        .max()
        .map(|p| p.min(MAX_PADDING))
        .unwrap_or(MIN_STATUS_PADDING)
}

fn colorize_status(status: &str, padding: usize, scheme: &ColorScheme, enabled: bool) -> String {
    let padding = padding.max(MIN_STATUS_PADDING);
    let styled = if status.contains("FAILED") {
        scheme.status_failed
    } else if status.contains("IN_PROGRESS") {
        scheme.status_progress
    } else if status.contains("COMPLETE") {
        scheme.status_complete
    } else if status.contains("SKIPPED") {
        scheme.status_skipped
    } else {
        Style::new()
    };
    apply_style(
        styled,
        &format!("{status:<width$}", width = padding),
        enabled,
    )
}

fn colorize_name(
    name: &str,
    tags: &std::collections::HashMap<String, String>,
    scheme: &ColorScheme,
    enabled: bool,
) -> String {
    let style = if name.contains("production")
        || tags
            .get("environment")
            .map(|v| v == "production")
            .unwrap_or(false)
    {
        scheme.prod
    } else if name.contains("integration")
        || tags
            .get("environment")
            .map(|v| v == "integration")
            .unwrap_or(false)
    {
        scheme.integration
    } else if name.contains("development")
        || tags
            .get("environment")
            .map(|v| v == "development")
            .unwrap_or(false)
    {
        scheme.development
    } else {
        Style::new()
    };
    apply_style(style, name, enabled)
}

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

    let scheme = ColorScheme::default();
    let use_color = color_enabled();
    let status_padding = calc_padding(&stacks, |s| {
        s.stack_status().map(|ss| ss.as_str()).unwrap_or("")
    });

    stacks
        .into_iter()
        .map(|stack| {
            let time = stack
                .creation_time()
                .or_else(|| stack.last_updated_time())
                .map(|t| t.fmt(Format::DateTime).unwrap_or_default())
                .unwrap_or_else(|| "unknown".to_string());

            let status = stack
                .stack_status()
                .map(|s| s.as_str())
                .unwrap_or("UNKNOWN");
            let name = stack.stack_name().unwrap_or("unknown");

            let tags_map: std::collections::HashMap<String, String> = stack
                .tags()
                .iter()
                .filter_map(|t| match (t.key(), t.value()) {
                    (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
                    _ => None,
                })
                .collect();

            let tags_str = tags_map
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(";");

            let ts = apply_style(
                scheme.timestamp,
                &format!("{:>width$}", time, width = TIME_PADDING),
                use_color,
            );
            let status_colored = colorize_status(status, status_padding, &scheme, use_color);
            let name_colored = colorize_name(name, &tags_map, &scheme, use_color);
            let lifecycle_icon = if stack.enable_termination_protection().unwrap_or(false)
                || tags_map
                    .get("lifetime")
                    .map(|v| v == "protected")
                    .unwrap_or(false)
            {
                "🔒 "
            } else if tags_map
                .get("lifetime")
                .map(|v| v == "long")
                .unwrap_or(false)
            {
                "∞ "
            } else if tags_map
                .get("lifetime")
                .map(|v| v == "short")
                .unwrap_or(false)
            {
                "♺ "
            } else {
                ""
            };
            let icon = apply_style(scheme.subtle, lifecycle_icon, use_color);

            if show_tags {
                if tags_str.is_empty() {
                    format!("{ts} {status_colored} {icon}{name_colored}")
                } else {
                    let tag_colored = apply_style(scheme.subtle, &tags_str, use_color);
                    format!("{ts} {status_colored} {icon}{name_colored} {tag_colored}")
                }
            } else {
                format!("{ts} {status_colored} {icon}{name_colored}")
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
