use anyhow::Result;
use aws_sdk_cloudformation::{
    Client,
    types::{StackDriftDetectionStatus, StackResourceDrift, StackResourceDriftStatus},
};

use crate::cfn::CfnContext;
use crate::cli::{Cli, DriftArgs};
use crate::output::{
    DriftedResource, DynamicOutputManager, OutputData, PropertyDifference, StackDrift, StatusLevel,
    StatusUpdate, convert_stack_to_definition,
};

async fn describe_stack_drift_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    _cli: &Cli,
    args: &DriftArgs,
    _opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let stack_task = {
        let client = context.client.clone();
        let stack_name = args.stackname.clone();
        tokio::spawn(async move {
            let stack_resp = client
                .describe_stacks()
                .stack_name(&stack_name)
                .send()
                .await?;

            let stack = stack_resp
                .stacks
                .and_then(|mut s| s.pop())
                .ok_or_else(|| anyhow::anyhow!("stack not found"))?;

            let stack_definition = convert_stack_to_definition(&stack, true);
            Ok::<(OutputData, aws_sdk_cloudformation::types::Stack), anyhow::Error>((
                stack_definition,
                stack,
            ))
        })
    };

    let (stack_definition, stack) = stack_task.await??;
    output_manager.render(stack_definition).await?;

    let needs_drift_check = stack
        .drift_information()
        .map(|drift| {
            drift.stack_drift_status()
                == Some(&aws_sdk_cloudformation::types::StackDriftStatus::NotChecked)
                || drift
                    .last_check_timestamp()
                    .map(|ts| {
                        // Check if older than cache period (default 5 minutes)
                        let cache_seconds = args.drift_cache as i64;
                        let cache_cutoff =
                            chrono::Utc::now() - chrono::Duration::seconds(cache_seconds);
                        let check_time =
                            chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
                                .unwrap_or(chrono::DateTime::UNIX_EPOCH);
                        check_time < cache_cutoff
                    })
                    .unwrap_or(true)
        })
        .unwrap_or(true);

    if needs_drift_check {
        let drift_start_msg = StatusUpdate {
            message: "Checking for stack drift...".to_string(),
            timestamp: chrono::Utc::now(),
            level: StatusLevel::Info,
        };
        output_manager
            .render(OutputData::StatusUpdate(drift_start_msg))
            .await?;

        let detect = context
            .client
            .detect_stack_drift()
            .stack_name(&args.stackname)
            .send()
            .await?;
        let detection_id = detect.stack_drift_detection_id().unwrap_or_default();

        // Wait for completion with progress updates
        loop {
            let status = context
                .client
                .describe_stack_drift_detection_status()
                .stack_drift_detection_id(detection_id)
                .send()
                .await?;
            match status.detection_status() {
                Some(StackDriftDetectionStatus::DetectionInProgress) => {
                    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
                }
                _ => break,
            }
        }
    }

    let drift_data = collect_stack_drift_data(&context.client, &args.stackname).await?;
    output_manager
        .render(OutputData::StackDrift(drift_data))
        .await?;

    Ok(0)
}

pub async fn describe_stack_drift(cli: &Cli, args: &DriftArgs) -> Result<i32> {
    run_command_handler!(describe_stack_drift_impl, cli, args)
}

async fn collect_stack_drift_data(client: &Client, stack_name: &str) -> Result<StackDrift> {
    let pages: Vec<_> = client
        .describe_stack_resource_drifts()
        .stack_name(stack_name)
        .into_paginator()
        .send()
        .try_collect()
        .await?;

    let mut all_drifts: Vec<StackResourceDrift> = Vec::new();
    for page in pages {
        all_drifts.extend_from_slice(page.stack_resource_drifts());
    }

    let drifted_resources: Vec<DriftedResource> = all_drifts
        .into_iter()
        .filter(|d| {
            !matches!(
                d.stack_resource_drift_status(),
                Some(StackResourceDriftStatus::InSync)
            )
        })
        .map(|drift| DriftedResource {
            logical_resource_id: drift.logical_resource_id().unwrap_or("unknown").to_string(),
            physical_resource_id: drift
                .physical_resource_id()
                .unwrap_or("unknown")
                .to_string(),
            resource_type: drift.resource_type().unwrap_or("unknown").to_string(),
            drift_status: drift
                .stack_resource_drift_status()
                .map(|s| s.as_str().to_string())
                .unwrap_or_default(),
            property_differences: drift
                .property_differences()
                .iter()
                .map(|pd| PropertyDifference {
                    property_path: pd.property_path().unwrap_or_default().to_string(),
                    expected_value: pd.expected_value().map(|s| s.to_string()),
                    actual_value: pd.actual_value().map(|s| s.to_string()),
                    difference_type: pd.difference_type().map(|dt| dt.as_str().to_string()),
                })
                .collect(),
        })
        .collect();

    Ok(StackDrift { drifted_resources })
}
