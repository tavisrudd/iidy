use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::cfn::CfnContext;
use crate::cli::{Cli, ListArgs};
use crate::output::{
    DynamicOutputManager, OutputData, StackListColumn, StackListDisplay, StackListEntry,
    aws_conversion::convert_stack_to_list_entry,
};
use crate::run_command_handler;

/// Serializable representation of a CloudFormation stack for JSON output
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SerializableStack {
    stack_name: Option<String>,
    stack_id: Option<String>,
    stack_status: Option<String>,
    stack_status_reason: Option<String>,
    description: Option<String>,
    creation_time: Option<String>,
    last_updated_time: Option<String>,
    timeout_in_minutes: Option<i32>,
    notification_arns: Vec<String>,
    capabilities: Vec<String>,
    outputs: Vec<SerializableStackOutput>,
    tags: Vec<SerializableTag>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SerializableStackOutput {
    output_key: Option<String>,
    output_value: Option<String>,
    description: Option<String>,
    export_name: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct SerializableTag {
    key: Option<String>,
    value: Option<String>,
}

impl From<&aws_sdk_cloudformation::types::Stack> for SerializableStack {
    fn from(stack: &aws_sdk_cloudformation::types::Stack) -> Self {
        Self {
            stack_name: stack.stack_name().map(|s| s.to_string()),
            stack_id: stack.stack_id().map(|s| s.to_string()),
            stack_status: stack.stack_status().map(|s| s.as_str().to_string()),
            stack_status_reason: stack.stack_status_reason().map(|s| s.to_string()),
            description: stack.description().map(|s| s.to_string()),
            creation_time: stack.creation_time().map(|t| t.to_string()),
            last_updated_time: stack.last_updated_time().map(|t| t.to_string()),
            timeout_in_minutes: stack.timeout_in_minutes(),
            notification_arns: stack
                .notification_arns()
                .iter()
                .map(|s| s.to_string())
                .collect(),
            capabilities: stack
                .capabilities()
                .iter()
                .map(|c| c.as_str().to_string())
                .collect(),
            outputs: stack
                .outputs()
                .iter()
                .map(|o| SerializableStackOutput {
                    output_key: o.output_key().map(|s| s.to_string()),
                    output_value: o.output_value().map(|s| s.to_string()),
                    description: o.description().map(|s| s.to_string()),
                    export_name: o.export_name().map(|s| s.to_string()),
                })
                .collect(),
            tags: stack
                .tags()
                .iter()
                .map(|t| SerializableTag {
                    key: t.key().map(|s| s.to_string()),
                    value: t.value().map(|s| s.to_string()),
                })
                .collect(),
        }
    }
}

async fn list_stacks_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    _cli: &Cli,
    args: &ListArgs,
    _opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let stacks: Vec<aws_sdk_cloudformation::types::Stack> = context
        .client
        .describe_stacks()
        .into_paginator()
        .items()
        .send()
        .try_collect()
        .await?;

    let tag_filters: Vec<(String, String)> = args
        .tag_filter
        .iter()
        .map(|tf| {
            let parts: Vec<&str> = tf.splitn(2, '=').collect();
            (
                parts[0].to_string(),
                parts.get(1).unwrap_or(&"").to_string(),
            )
        })
        .collect();

    let mut filtered_stacks = stacks;
    let mut filters_applied = Vec::new();
    if !tag_filters.is_empty() {
        filtered_stacks.retain(|stack| {
            let tags: HashMap<String, String> = stack
                .tags()
                .iter()
                .filter_map(|tag| match (tag.key(), tag.value()) {
                    (Some(k), Some(v)) => Some((k.to_string(), v.to_string())),
                    _ => None,
                })
                .collect();

            tag_filters.iter().all(|(k, v)| tags.get(k) == Some(v))
        });

        for (k, v) in &tag_filters {
            filters_applied.push(format!("tag:{k}={v}"));
        }
    }

    if let Some(jmespath_filter) = &args.jmespath_filter {
        let serializable_stacks: Vec<SerializableStack> = filtered_stacks
            .iter()
            .map(SerializableStack::from)
            .collect();

        let json_value = serde_json::to_value(&serializable_stacks).map_err(|e| {
            anyhow::anyhow!("Failed to convert stacks to JSON for filtering: {}", e)
        })?;

        let expression = jmespath::compile(jmespath_filter).map_err(|e| {
            anyhow::anyhow!("Invalid JMESPath expression '{}': {}", jmespath_filter, e)
        })?;

        let filtered_json = expression
            .search(&json_value)
            .map_err(|e| anyhow::anyhow!("JMESPath filter execution failed: {}", e))?;

        let filtered_json_value = match filtered_json.as_array() {
            Some(arr) => serde_json::Value::Array(
                arr.iter()
                    .map(|item| serde_json::to_value(item).unwrap_or(serde_json::Value::Null))
                    .collect(),
            ),
            None => return Err(anyhow::anyhow!("JMESPath filter must return an array")),
        };

        let filtered_serializable: Vec<SerializableStack> =
            serde_json::from_value(filtered_json_value).map_err(|e| {
                anyhow::anyhow!("Failed to convert filtered JSON back to stacks: {}", e)
            })?;

        filtered_stacks.retain(|stack| {
            let serializable = SerializableStack::from(stack);
            filtered_serializable.iter().any(|filtered| {
                filtered.stack_name == serializable.stack_name
                    && filtered.stack_id == serializable.stack_id
            })
        });

        filters_applied.push(format!("jmespath:{jmespath_filter}"));
    }

    let is_query_mode = args.query.is_some();
    let columns = if let Some(columns_str) = &args.columns {
        columns_str
            .split(',')
            .map(|s| s.trim())
            .filter_map(StackListColumn::parse)
            .collect()
    } else {
        StackListColumn::default_columns()
    };

    let show_tags = columns.contains(&StackListColumn::Tags) || args.tags;
    let stack_list_display = convert_stacks_to_list_display_with_filters(
        filtered_stacks,
        show_tags,
        filters_applied,
        columns,
        is_query_mode,
    );
    output_manager.render(stack_list_display).await?;

    Ok(0)
}

pub async fn list_stacks(cli: &Cli, args: &ListArgs) -> Result<i32> {
    run_command_handler!(list_stacks_impl, cli, args)
}

/// Convert a list of AWS SDK Stacks to StackListDisplay with applied filters
fn convert_stacks_to_list_display_with_filters(
    stacks: Vec<aws_sdk_cloudformation::types::Stack>,
    show_tags: bool,
    filters_applied: Vec<String>,
    columns: Vec<StackListColumn>,
    query_mode: bool,
) -> OutputData {
    let mut entries: Vec<StackListEntry> = stacks.iter().map(convert_stack_to_list_entry).collect();

    entries.sort_by(|a, b| {
        let time_a = a.creation_time.or(a.last_updated_time);
        let time_b = b.creation_time.or(b.last_updated_time);
        time_a.cmp(&time_b)
    });

    OutputData::StackList(StackListDisplay {
        stacks: entries,
        show_tags,
        filters_applied,
        columns,
        query_mode,
    })
}

#[cfg(test)]
mod tests {}
