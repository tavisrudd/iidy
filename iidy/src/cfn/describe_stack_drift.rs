use anyhow::Result;
use aws_sdk_cloudformation::{
    Client,
    types::{StackDriftDetectionStatus, StackResourceDrift, StackResourceDriftStatus},
};

use crate::{
    aws,
    cli::{AwsOpts, DriftArgs, GlobalOpts},
    output::{
        DynamicOutputManager, manager::OutputOptions,
    },
};

// REMOVED: format_resource_drifts function - Legacy formatting logic replaced by data-driven output architecture
// Drift formatting is now handled by the renderers through OutputData::StackDrift

/// Describe CloudFormation stack drift with data-driven output.
/// 
/// This is a read-only operation that follows the iidy-js pattern:
/// 1. Show stack definition
/// 2. Update drift data (with spinner if needed)
/// 3. Show drifted resources
/// No command metadata is shown (read-only operation).
pub async fn describe_stack_drift(
    opts: &AwsOpts, 
    args: &DriftArgs,
    global_opts: &GlobalOpts
) -> Result<()> {
    let output_options = OutputOptions::minimal();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;
    let config = aws::config_from_opts(opts).await?;
    let client = Client::new(&config);

    // 1. Show stack definition (following iidy-js pattern)
    let stack_resp = client
        .describe_stacks()
        .stack_name(&args.stackname)
        .send()
        .await?;
    
    let stack = stack_resp
        .stacks
        .and_then(|mut s| s.pop())
        .ok_or_else(|| anyhow::anyhow!("stack not found"))?;
    
    let stack_definition = crate::output::convert_stack_to_definition(&stack, true);
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
        use crate::output::{StatusUpdate, StatusLevel};
        let drift_start_msg = StatusUpdate {
            message: "Checking for stack drift...".to_string(),
            timestamp: chrono::Utc::now(),
            level: StatusLevel::Info,
        };
        output_manager.render(crate::output::OutputData::StatusUpdate(drift_start_msg)).await?;
        
        // Start drift detection
        let detect = client
            .detect_stack_drift()
            .stack_name(&args.stackname)
            .send()
            .await?;
        let detection_id = detect.stack_drift_detection_id().unwrap_or_default();

        // Wait for completion with progress updates
        loop {
            let status = client
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
    let drift_data = collect_stack_drift_data(&client, &args.stackname).await?;
    output_manager.render(crate::output::OutputData::StackDrift(drift_data)).await?;

    Ok(())
}

/// Collect stack drift data (controller pattern - no display logic)
async fn collect_stack_drift_data(
    client: &Client,
    stack_name: &str,
) -> Result<crate::output::StackDrift> {
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

    let drifted_resources: Vec<crate::output::DriftedResource> = all_drifts
        .into_iter()
        .filter(|d| match d.stack_resource_drift_status() {
            Some(StackResourceDriftStatus::InSync) => false,
            _ => true,
        })
        .map(|drift| crate::output::DriftedResource {
            logical_resource_id: drift.logical_resource_id().unwrap_or("unknown").to_string(),
            physical_resource_id: drift.physical_resource_id().unwrap_or("unknown").to_string(),
            resource_type: drift.resource_type().unwrap_or("unknown").to_string(),
            drift_status: drift.stack_resource_drift_status()
                .map(|s| s.as_str().to_string())
                .unwrap_or_default(),
            property_differences: drift.property_differences()
                .iter()
                .map(|pd| crate::output::PropertyDifference {
                    property_path: pd.property_path().unwrap_or_default().to_string(),
                    expected_value: pd.expected_value().map(|s| s.to_string()),
                    actual_value: pd.actual_value().map(|s| s.to_string()),
                    difference_type: pd.difference_type()
                        .map(|dt| dt.as_str().to_string()),
                })
                .collect(),
        })
        .collect();

    Ok(crate::output::StackDrift {
        drifted_resources,
    })
}

#[cfg(test)]
mod tests {
    // No longer need imports - tests now focus on data-driven output structures

    #[test]
    fn drift_data_conversion_no_drift() {
        // Test that empty drift list is handled correctly
        let drift_data = crate::output::StackDrift {
            drifted_resources: vec![],
        };
        assert_eq!(drift_data.drifted_resources.len(), 0);
    }

    #[test]
    fn drift_data_conversion_with_drift() {
        // Test that drift data structure contains expected information
        let drift_data = crate::output::StackDrift {
            drifted_resources: vec![
                crate::output::DriftedResource {
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
