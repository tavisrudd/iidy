//! Integration tests for CloudFormation template and policy loading
//!
//! Tests the complete template loading pipeline including:
//! - Local file loading
//! - S3 URL handling
//! - HTTP URL handling  
//! - render: prefix processing
//! - Stack policy loading
//! - Size limit validation

use std::fs;
use tempfile::tempdir;

use iidy::cfn::template_loader::{TEMPLATE_MAX_BYTES, load_cfn_stack_policy, load_cfn_template};

#[tokio::test]
async fn test_load_local_template_file() {
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("template.yaml");

    let template_content = r#"
AWSTemplateFormatVersion: '2010-09-09'
Description: Test template
Resources:
  TestResource:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: test-bucket
"#;

    fs::write(&template_path, template_content).unwrap();

    let result = load_cfn_template(
        Some(template_path.to_str().unwrap()),
        temp_dir.path().to_str().unwrap(),
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None,
    )
    .await
    .unwrap();

    assert!(result.template_body.is_some());
    assert!(result.template_url.is_none());

    let body = result.template_body.unwrap();
    assert!(body.contains("AWSTemplateFormatVersion"));
    assert!(body.contains("test-bucket"));
}

#[tokio::test]
async fn test_load_template_with_render_prefix() {
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("template.yaml");

    let template_content = r#"
AWSTemplateFormatVersion: '2010-09-09'
Description: Rendered template
Resources:
  TestResource:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: test-bucket
"#;

    fs::write(&template_path, template_content).unwrap();

    let location = format!("render:{}", template_path.to_str().unwrap());
    let result = load_cfn_template(
        Some(&location),
        temp_dir.path().to_str().unwrap(),
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None,
    )
    .await
    .unwrap();

    assert!(result.template_body.is_some());
    assert!(result.template_url.is_none());

    let body = result.template_body.unwrap();
    assert!(body.contains("AWSTemplateFormatVersion"));
    // Template rendering is not actually implemented yet, so just check it loaded
    assert!(body.contains("AWS::S3::Bucket"));
}

#[tokio::test]
async fn test_http_url_template() {
    // Test that HTTP URLs are returned as TemplateURL
    let http_url = "https://example.com/template.yaml";

    let result = load_cfn_template(
        Some(http_url),
        "/tmp",
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None,
    )
    .await
    .unwrap();

    assert!(result.template_body.is_none());
    assert!(result.template_url.is_some());
    assert_eq!(result.template_url.unwrap(), http_url);
}

#[tokio::test]
async fn test_s3_url_without_client() {
    // Test S3 URL handling without S3 client (should pass through)
    let s3_url = "https://mybucket.s3.us-west-2.amazonaws.com/template.yaml";

    let result = load_cfn_template(
        Some(s3_url),
        "/tmp",
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None, // No S3 client
    )
    .await
    .unwrap();

    assert!(result.template_body.is_none());
    assert!(result.template_url.is_some());
    assert_eq!(result.template_url.unwrap(), s3_url);
}

#[tokio::test]
async fn test_template_size_limit_error() {
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("large_template.yaml");

    // Create a template larger than the limit
    let large_content = "x".repeat(TEMPLATE_MAX_BYTES + 1000);
    fs::write(&template_path, large_content).unwrap();

    let result = load_cfn_template(
        Some(template_path.to_str().unwrap()),
        temp_dir.path().to_str().unwrap(),
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None,
    )
    .await;

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("larger than the max allowed size"));
    assert!(error_msg.contains("upload it to S3"));
}

#[tokio::test]
async fn test_template_with_preprocessing_syntax_error() {
    let temp_dir = tempdir().unwrap();
    let template_path = temp_dir.path().join("template.yaml");

    let template_content = r#"
$imports:
  some_value: file:./config.yaml
AWSTemplateFormatVersion: '2010-09-09'
Resources:
  TestResource:
    Type: AWS::S3::Bucket
"#;

    fs::write(&template_path, template_content).unwrap();

    // Try to load without render: prefix - should error
    let result = load_cfn_template(
        Some(template_path.to_str().unwrap()),
        temp_dir.path().to_str().unwrap(),
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None,
    )
    .await;

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("yaml pre-processor syntax"));
    assert!(error_msg.contains("prefix the template location with \"render:\""));
}

#[tokio::test]
async fn test_inline_template_content() {
    // Test loading inline JSON template
    let inline_json = r#"{
        "AWSTemplateFormatVersion": "2010-09-09",
        "Resources": {
            "TestBucket": {
                "Type": "AWS::S3::Bucket"
            }
        }
    }"#;

    let result = load_cfn_template(
        Some(inline_json),
        "/tmp",
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None,
    )
    .await
    .unwrap();

    assert!(result.template_body.is_some());
    assert!(result.template_url.is_none());

    let body = result.template_body.unwrap();
    assert!(body.contains("AWSTemplateFormatVersion"));
    assert!(body.contains("AWS::S3::Bucket"));
}

#[tokio::test]
async fn test_load_stack_policy_from_file() {
    let temp_dir = tempdir().unwrap();
    let policy_path = temp_dir.path().join("policy.json");

    let policy_content = r#"{
        "Statement": [
            {
                "Effect": "Allow",
                "Principal": "*",
                "Action": "Update:*",
                "Resource": "*"
            }
        ]
    }"#;

    fs::write(&policy_path, policy_content).unwrap();

    let policy_value = serde_yaml::Value::String(policy_path.to_str().unwrap().to_string());
    let result =
        load_cfn_stack_policy(Some(&policy_value), temp_dir.path().to_str().unwrap(), None)
            .await
            .unwrap();

    assert!(result.stack_policy_body.is_some());
    assert!(result.stack_policy_url.is_none());

    let body = result.stack_policy_body.unwrap();
    assert!(body.contains("Statement"));
    assert!(body.contains("Allow"));
}

#[tokio::test]
async fn test_load_stack_policy_inline_yaml() {
    // Test loading stack policy from inline YAML value
    let policy_yaml = serde_yaml::from_str::<serde_yaml::Value>(
        r#"
Statement:
  - Effect: Deny
    Principal: "*"
    Action: "Update:Delete"
    Resource: "*"
"#,
    )
    .unwrap();

    let result = load_cfn_stack_policy(Some(&policy_yaml), "/tmp", None)
        .await
        .unwrap();

    assert!(result.stack_policy_body.is_some());
    assert!(result.stack_policy_url.is_none());

    let body = result.stack_policy_body.unwrap();
    // Should be converted to JSON
    assert!(body.contains("\"Statement\""));
    assert!(body.contains("\"Deny\""));
}

#[tokio::test]
async fn test_empty_template_location() {
    // Test with None template location
    let result = load_cfn_template(None, "/tmp", Some("test"), TEMPLATE_MAX_BYTES, None)
        .await
        .unwrap();

    assert!(result.template_body.is_none());
    assert!(result.template_url.is_none());
}

#[tokio::test]
async fn test_s3_url_error_handling() {
    // Test invalid S3 URL format detection
    let bad_s3_url = "s3://bucket/key";

    let result = load_cfn_template(
        Some(bad_s3_url),
        "/tmp",
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None,
    )
    .await;

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Use https:// S3 path-based urls"));
}

#[tokio::test]
async fn test_nonexistent_file_error() {
    let result = load_cfn_template(
        Some("/nonexistent/path/template.yaml"),
        "/tmp",
        Some("test"),
        TEMPLATE_MAX_BYTES,
        None,
    )
    .await;

    assert!(result.is_err());
    let error_msg = result.unwrap_err().to_string();
    assert!(error_msg.contains("Failed to load template"));
}
