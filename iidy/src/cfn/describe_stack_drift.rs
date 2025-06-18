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

/// Format stack resource drifts for display.
///
/// Returns a vector of lines that can be printed to stdout.
pub fn format_resource_drifts(drifts: Vec<StackResourceDrift>) -> Vec<String> {
    if drifts.is_empty() {
        return vec![String::from(
            "No drift detected. Stack resources are in sync with template.",
        )];
    }

    let mut lines = Vec::new();
    lines.push(String::from("Drifted Resources:"));

    for drift in drifts {
        let id = drift.logical_resource_id().unwrap_or("unknown");
        let rtype = drift.resource_type().unwrap_or("unknown");
        let phys = drift.physical_resource_id().unwrap_or("unknown");
        lines.push(format!("{id} {rtype} {phys}"));
        if let Some(status) = drift.stack_resource_drift_status() {
            lines.push(format!("  {}", status.as_str()));
        }
        if !drift.property_differences().is_empty() {
            #[derive(serde::Serialize)]
            struct Diff<'a> {
                #[serde(skip_serializing_if = "Option::is_none")]
                property_path: Option<&'a str>,
                #[serde(skip_serializing_if = "Option::is_none")]
                expected_value: Option<&'a str>,
                #[serde(skip_serializing_if = "Option::is_none")]
                actual_value: Option<&'a str>,
                #[serde(skip_serializing_if = "Option::is_none")]
                difference_type: Option<&'a str>,
            }

            let diffs: Vec<Diff<'_>> = drift
                .property_differences()
                .iter()
                .map(|d| Diff {
                    property_path: d.property_path(),
                    expected_value: d.expected_value(),
                    actual_value: d.actual_value(),
                    difference_type: d.difference_type().map(|dt| dt.as_str()),
                })
                .collect();

            if let Ok(diff_yaml) = serde_yaml::to_string(&diffs) {
                for l in diff_yaml.lines() {
                    lines.push(format!("   {l}"));
                }
            }
        }
    }

    lines
}

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
    let output_options = OutputOptions::default();
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
    use super::*;
    use aws_sdk_cloudformation::types::{PropertyDifference, StackResourceDriftStatus};
    use aws_smithy_types::DateTime;

    fn sample_drift(id: &str) -> StackResourceDrift {
        StackResourceDrift::builder()
            .stack_id("sid")
            .logical_resource_id(id)
            .physical_resource_id("pid")
            .resource_type("AWS::S3::Bucket")
            .stack_resource_drift_status(StackResourceDriftStatus::Modified)
            .timestamp(DateTime::from_secs(0))
            .property_differences(PropertyDifference::builder().build())
            .build()
    }

    #[test]
    fn formats_no_drift() {
        let lines = format_resource_drifts(Vec::new());
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("No drift"));
    }

    #[test]
    fn formats_drift() {
        let drifts = vec![sample_drift("A")];
        let lines = format_resource_drifts(drifts);
        assert!(lines.iter().any(|l| l.contains("Drifted Resources")));
        assert!(lines.iter().any(|l| l.contains("A")));
    }
}
