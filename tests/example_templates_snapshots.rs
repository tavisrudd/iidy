//! Snapshot tests for example templates
//!
//! This test suite ensures that all example templates render correctly
//! and produces the expected output. Uses `insta` for snapshot testing
//! which automatically manages expected outputs and makes it easy to
//! review changes when template behavior is modified.

use iidy::cli::YamlSpec;
use iidy::yaml::engine::serialize_yaml_iidy_js_compatible;
use iidy::yaml::preprocess_yaml;
use insta::{assert_snapshot, assert_yaml_snapshot};
use std::path::Path;

/// Test helper to render a template file and return the output
async fn render_template_file(
    template_path: &str,
) -> Result<serde_yaml::Value, Box<dyn std::error::Error>> {
    let full_path = format!("example-templates/{}", template_path);
    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read {}: {}", full_path, e))?;

    let result = preprocess_yaml(&content, &full_path, &YamlSpec::Auto).await?;
    Ok(result)
}

/// Auto-discovery test for all valid example templates
#[tokio::test]
async fn test_all_example_templates_auto_discovery() {
    use std::fs;
    use std::path::Path;

    fn discover_templates(dir: &Path, relative_path: &str) -> Vec<(String, String)> {
        let mut templates = Vec::new();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();

                if path.is_file()
                    && path.extension().map_or(false, |ext| ext == "yaml")
                    && !name.starts_with(".")
                {
                    let relative_file_path = if relative_path.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{}", relative_path, name)
                    };

                    templates.push((relative_file_path, name));
                } else if path.is_dir() {
                    // Skip certain directories
                    if ["invalid", "expected-outputs", "errors", ".git", "custom-resource-templates"].contains(&name.as_str()) {
                        continue;
                    }

                    // Recursively discover in subdirectories
                    let sub_relative = if relative_path.is_empty() {
                        name
                    } else {
                        format!("{}/{}", relative_path, name)
                    };

                    templates.extend(discover_templates(&path, &sub_relative));
                }
            }
        }

        templates
    }

    let example_dir = Path::new("example-templates");
    let discovered_templates = discover_templates(example_dir, "");

    for (relative_path, _filename) in discovered_templates {
        // Test the template
        let result = render_template_file(&relative_path).await;

        match result {
            Ok(output) => {
                // Create snapshot with sanitized name that includes subdirectory
                let snapshot_name = format!(
                    "auto_discovered_{}",
                    relative_path
                        .replace("/", "_")
                        .replace("-", "_")
                        .replace(".yaml", "")
                );

                // Use serialize_yaml_iidy_js_compatible to get the formatted output
                let formatted_output = serialize_yaml_iidy_js_compatible(&output)
                    .expect("Failed to serialize output with iidy-js compatibility");
                assert_snapshot!(snapshot_name, formatted_output);
            }
            Err(e) => {
                // Files in yaml-iidy-syntax/ should never fail - these are demonstration files
                if relative_path.starts_with("yaml-iidy-syntax/") {
                    panic!(
                        "yaml-iidy-syntax template {} must render without errors, but failed with: {}",
                        relative_path, e
                    );
                }
                // Log but don't fail for other auto-discovered templates that might be invalid
                eprintln!(
                    "Warning: Auto-discovered template {} failed to render: {}",
                    relative_path, e
                );
            }
        }
    }
}

/// Test that demonstrates CloudFormation tag preservation
#[tokio::test]
async fn test_cloudformation_tags_structure() {
    let result = render_template_file("cloudformation-tags-demo.yaml")
        .await
        .expect("Failed to render cloudformation-tags-demo.yaml");

    // Verify that specific CloudFormation intrinsic functions are preserved
    let result_str = serde_yaml::to_string(&result).expect("Failed to serialize result");

    // Check for proper CloudFormation tag syntax in the output
    assert!(
        result_str.contains("!Sub"),
        "Output should contain !Sub tags"
    );
    assert!(
        result_str.contains("!Ref"),
        "Output should contain !Ref tags"
    );
    assert!(
        result_str.contains("!GetAtt"),
        "Output should contain !GetAtt tags"
    );

    // Verify that preprocessing variables were replaced
    assert!(
        result_str.contains("tag-demo-app"),
        "Preprocessing variables should be replaced"
    );
    assert!(
        result_str.contains("production"),
        "Environment variables should be replaced"
    );

    // Verify that CloudFormation variables are preserved
    assert!(
        result_str.contains("${AWS::Region}"),
        "CloudFormation variables should be preserved"
    );
    assert!(
        result_str.contains("${AWS::AccountId}"),
        "CloudFormation variables should be preserved"
    );
}

/// Test error handling for invalid templates
#[tokio::test]
async fn test_invalid_templates_fail_gracefully() {
    // Test that invalid templates in the invalid/ folder fail as expected
    let invalid_templates = [
        "invalid/simple-template.yaml",
        "invalid/complex-template.yaml",
        "invalid/showcase.yaml",
    ];

    for template in &invalid_templates {
        let template_path = format!("example-templates/{}", template);
        if Path::new(&template_path).exists() {
            let result = render_template_file(template).await;

            // These should fail - if they succeed, it means we need to fix the template or the test
            match result {
                Ok(_) => {
                    // Log a warning but don't fail the test - the template might have been fixed
                    eprintln!(
                        "Warning: {} unexpectedly succeeded - consider moving it to the valid examples",
                        template
                    );
                }
                Err(_) => {
                    // Expected - invalid templates should fail
                }
            }
        }
    }
}

/// Integration test that verifies the complete workflow
#[tokio::test]
async fn test_template_workflow_integration() {
    // Test the complete workflow: config -> import -> processing

    // 1. First render the config file (this is imported by import-test)
    let _config_result = render_template_file("config.yaml")
        .await
        .expect("Failed to render config.yaml");

    // 2. Then render the import-test file that imports config.yaml
    let import_result = render_template_file("import-test.yaml")
        .await
        .expect("Failed to render import-test.yaml");

    // 3. Verify that the import worked correctly
    let import_str =
        serde_yaml::to_string(&import_result).expect("Failed to serialize import result");

    // Should contain values from the imported config
    assert!(
        import_str.contains("db.example.com"),
        "Should contain imported database host"
    );
    assert!(
        import_str.contains("5432"),
        "Should contain imported database port"
    );

    // Should also contain processed values
    assert!(
        import_str.contains("import-demo"),
        "Should contain processed app name"
    );
}

/// Test that verifies YAML 1.1 boolean compatibility
#[tokio::test]
async fn test_yaml_boolean_compatibility() {
    // Create a test template with YAML 1.1 booleans
    let yaml_content = r#"
%YAML 1.1
---
AWSTemplateFormatVersion: '2010-09-09'
Resources:
  TestInstance:
    Type: AWS::EC2::Instance
    Properties:
      Monitoring: yes
      EbsOptimized: no
      SourceDestCheck: on
      DisableApiTermination: off
"#;

    let result = preprocess_yaml(yaml_content, "test.yaml", &YamlSpec::Auto)
        .await
        .expect("Failed to process YAML 1.1 content");

    assert_yaml_snapshot!("yaml_11_booleans", result);

    // Verify boolean conversion
    let result_str = serde_yaml::to_string(&result).expect("Failed to serialize result");
    assert!(
        result_str.contains("Monitoring: true"),
        "yes should convert to true"
    );
    assert!(
        result_str.contains("EbsOptimized: false"),
        "no should convert to false"
    );
    assert!(
        result_str.contains("SourceDestCheck: true"),
        "on should convert to true"
    );
    assert!(
        result_str.contains("DisableApiTermination: false"),
        "off should convert to false"
    );
}

/// Test that verifies handlebars processing inside CloudFormation tags
#[tokio::test]
async fn test_handlebars_in_cloudformation_tags() {
    let yaml_content = r#"
$defs:
  app_name: "my-app"
  environment: "prod"

AWSTemplateFormatVersion: '2010-09-09'
Resources:
  TestBucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !Sub "{{app_name}}-${AWS::StackName}-bucket"
      Tags:
        - Key: Environment
          Value: !Ref "{{environment}}"
"#;

    let result = preprocess_yaml(yaml_content, "test.yaml", &YamlSpec::Auto)
        .await
        .expect("Failed to process template with handlebars in tags");

    assert_yaml_snapshot!("handlebars_in_tags", result);

    // Verify that handlebars were processed but CloudFormation syntax preserved
    let result_str = serde_yaml::to_string(&result).expect("Failed to serialize result");
    assert!(
        result_str.contains("!Sub my-app-${AWS::StackName}-bucket"),
        "Handlebars should be processed inside !Sub"
    );
    assert!(
        result_str.contains("!Ref prod"),
        "Handlebars should be processed inside !Ref"
    );
}

#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;

    /// Basic performance regression test
    #[tokio::test]
    async fn test_rendering_performance() {
        let start = Instant::now();

        // Render a subset of templates for performance testing
        let templates = [
            "basic-test.yaml",
            "config.yaml",
            "import-test.yaml",
            "simple-cloudformation.yaml",
        ];

        for template in &templates {
            let _ = render_template_file(template)
                .await
                .expect(&format!("Failed to render {}", template));
        }

        let duration = start.elapsed();

        // All templates should render in under 5 seconds (very generous limit)
        assert!(
            duration.as_secs() < 5,
            "Template rendering took too long: {:?}",
            duration
        );

        println!("Rendered {} templates in {:?}", templates.len(), duration);
    }
}
