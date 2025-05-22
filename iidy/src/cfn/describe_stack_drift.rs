use anyhow::Result;
use aws_sdk_cloudformation::{
    Client,
    types::{StackDriftDetectionStatus, StackResourceDrift, StackResourceDriftStatus},
};

use crate::display::display_lines;
use crate::{
    aws,
    cli::{AwsOpts, DriftArgs},
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

/// Describe CloudFormation stack drift.
///
/// This performs drift detection (if necessary) and then retrieves any
/// drifted resources, formatting them for display.
pub async fn describe_stack_drift(opts: &AwsOpts, args: &DriftArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let client = Client::new(&config);

    // Start drift detection.
    let detect = client
        .detect_stack_drift()
        .stack_name(&args.stackname)
        .send()
        .await?;
    let detection_id = detect.stack_drift_detection_id().unwrap_or_default();

    // Wait until detection completes.
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

    // Retrieve all drifted resources.
    let pages: Vec<_> = client
        .describe_stack_resource_drifts()
        .stack_name(&args.stackname)
        .into_paginator()
        .send()
        .try_collect()
        .await?;

    let mut all_drifts: Vec<StackResourceDrift> = Vec::new();
    for page in pages {
        all_drifts.extend_from_slice(page.stack_resource_drifts());
    }

    let drifts: Vec<StackResourceDrift> = all_drifts
        .into_iter()
        .filter(|d| match d.stack_resource_drift_status() {
            Some(StackResourceDriftStatus::InSync) => false,
            _ => true,
        })
        .collect();

    display_lines(format_resource_drifts(drifts));
    Ok(())
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
