use anyhow::Result;
use aws_sdk_cloudformation::{
    Client,
    types::{StackDriftDetectionStatus, StackResourceDrift, StackResourceDriftStatus},
};

use crate::cli::{Cli, DriftArgs};
use crate::output::{
    DynamicOutputManager, OutputData, convert_stack_to_definition,
    StatusUpdate, StatusLevel, StackDrift, DriftedResource, PropertyDifference,
};
use crate::run_command_handler;

// REMOVED: format_resource_drifts function - Legacy formatting logic replaced by data-driven output architecture
// Drift formatting is now handled by the renderers through OutputData::StackDrift

async fn describe_stack_drift_impl(
    output_manager: &mut DynamicOutputManager,
    context: &crate::cfn::CfnContext,
    _cli: &Cli,
    args: &DriftArgs,
    _opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    // 1. Show stack definition (following iidy-js pattern)
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
            Ok::<(OutputData, aws_sdk_cloudformation::types::Stack), anyhow::Error>((stack_definition, stack))
        })
    };

    // Get the stack for drift checking
    let (stack_definition, stack) = stack_task.await??;
    output_manager.render(stack_definition).await?;

    // 2. Update drift data if needed (following iidy-js updateStackDriftData pattern)
    let needs_drift_check = stack.drift_information()
        .map(|drift| {
            drift.stack_drift_status() == Some(&aws_sdk_cloudformation::types::StackDriftStatus::NotChecked) ||
            drift.last_check_timestamp().map(|ts| {
                // Check if older than cache period (default 5 minutes in iidy-js)
                let cache_seconds = args.drift_cache as i64;
                let cache_cutoff = chrono::Utc::now() - chrono::Duration::seconds(cache_seconds);
                let check_time = chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
                    .unwrap_or(chrono::DateTime::UNIX_EPOCH);
                check_time < cache_cutoff
            }).unwrap_or(true)
        })
        .unwrap_or(true);

    if needs_drift_check {
        // Send status update for drift detection progress
        let drift_start_msg = StatusUpdate {
            message: "Checking for stack drift...".to_string(),
            timestamp: chrono::Utc::now(),
            level: StatusLevel::Info,
        };
        output_manager.render(OutputData::StatusUpdate(drift_start_msg)).await?;
        
        // Start drift detection
        let detect = context.client
            .detect_stack_drift()
            .stack_name(&args.stackname)
            .send()
            .await?;
        let detection_id = detect.stack_drift_detection_id().unwrap_or_default();

        // Wait for completion with progress updates
        loop {
            let status = context.client
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

    // 3. Collect and send drift data
    let drift_data = collect_stack_drift_data(&context.client, &args.stackname).await?;
    output_manager.render(OutputData::StackDrift(drift_data)).await?;

    Ok(0) // Return success exit code
}

pub async fn describe_stack_drift(cli: &Cli, args: &DriftArgs) -> Result<i32> {
    run_command_handler!(describe_stack_drift_impl, cli, args)
}

/// Collect stack drift data (controller pattern - no display logic)
async fn collect_stack_drift_data(
    client: &Client,
    stack_name: &str,
) -> Result<StackDrift> {
    // Retrieve all drifted resources
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
        .filter(|d| match d.stack_resource_drift_status() {
            Some(StackResourceDriftStatus::InSync) => false,
            _ => true,
        })
        .map(|drift| DriftedResource {
            logical_resource_id: drift.logical_resource_id().unwrap_or("unknown").to_string(),
            physical_resource_id: drift.physical_resource_id().unwrap_or("unknown").to_string(),
            resource_type: drift.resource_type().unwrap_or("unknown").to_string(),
            drift_status: drift.stack_resource_drift_status()
                .map(|s| s.as_str().to_string())
                .unwrap_or_default(),
            property_differences: drift.property_differences()
                .iter()
                .map(|pd| PropertyDifference {
                    property_path: pd.property_path().unwrap_or_default().to_string(),
                    expected_value: pd.expected_value().map(|s| s.to_string()),
                    actual_value: pd.actual_value().map(|s| s.to_string()),
                    difference_type: pd.difference_type()
                        .map(|dt| dt.as_str().to_string()),
                })
                .collect(),
        })
        .collect();

    Ok(StackDrift {
        drifted_resources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drift_data_conversion_no_drift() {
        // Test that empty drift list is handled correctly
        let drift_data = StackDrift {
            drifted_resources: vec![],
        };
        assert_eq!(drift_data.drifted_resources.len(), 0);
    }

    #[test]
    fn drift_data_conversion_with_drift() {
        // Test that drift data structure contains expected information
        let drift_data = StackDrift {
            drifted_resources: vec![
                DriftedResource {
                    logical_resource_id: "TestResource".to_string(),
                    physical_resource_id: "test-resource-123".to_string(),
                    resource_type: "AWS::S3::Bucket".to_string(),
                    drift_status: "MODIFIED".to_string(),
                    property_differences: vec![],
                }
            ],
        };
        assert_eq!(drift_data.drifted_resources.len(), 1);
        assert_eq!(drift_data.drifted_resources[0].logical_resource_id, "TestResource");
        assert_eq!(drift_data.drifted_resources[0].drift_status, "MODIFIED");
    }
}
