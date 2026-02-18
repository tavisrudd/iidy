//! CloudFormation template loading with iidy-js compatibility
//!
//! This module provides template loading functionality that matches iidy-js behavior:
//! - Support for `render:` prefix to enable YAML preprocessing
//! - S3 and HTTP URL handling
//! - Template size limits (51KB inline, 1MB for S3)
//! - Auto-signing of S3 URLs for cross-region access
//! - Error detection for templates using preprocessing without render: prefix

use anyhow::{Context, Result, bail};
use aws_sdk_s3::Client as S3Client;
use serde_yaml::Value;

use crate::cli::YamlSpec;
use crate::yaml::{imports::loaders::load_file_import, preprocess_yaml};

/// Template size limits from iidy-js
pub const TEMPLATE_MAX_BYTES: usize = 51199; // 51KB for inline templates
pub const S3_TEMPLATE_MAX_BYTES: usize = 999999; // 1MB for S3 templates

/// Result of template loading
#[derive(Debug, Clone)]
pub struct TemplateResult {
    pub template_body: Option<String>,
    pub template_url: Option<String>,
}

/// Load a CloudFormation template with full iidy-js compatibility
pub async fn load_cfn_template(
    location: Option<&str>,
    base_location: &str,
    environment: Option<&str>,
    max_size: usize,
    s3_client: Option<&S3Client>,
) -> Result<TemplateResult> {
    let location = match location {
        Some(loc) if !loc.trim().is_empty() => loc.trim(),
        _ => {
            return Ok(TemplateResult {
                template_body: None,
                template_url: None,
            });
        }
    };

    // Check for render: prefix
    let should_render = location.starts_with("render:");
    let actual_location = if should_render {
        location.strip_prefix("render:").unwrap_or(location).trim()
    } else {
        location
    };

    // Auto-sign S3 URLs for cross-region access (equivalent to maybeSignS3HttpUrl)
    let processed_location = maybe_sign_s3_http_url(actual_location, s3_client).await?;

    // Check if this looks like inline template content (JSON or YAML)
    let is_inline_content = processed_location.trim().starts_with('{')
        || processed_location.trim().starts_with('[')
        || processed_location.contains('\n')
        || (processed_location.len() < 260
            && !processed_location.contains('/')
            && !processed_location.contains('\\'));

    // Handle different location types
    if !should_render && processed_location.starts_with("s3:") {
        bail!(
            "Use https:// S3 path-based urls when using a plain (non-rendered) Template from S3: {}",
            processed_location
        );
    } else if !should_render && processed_location.starts_with("http") {
        // HTTP URL - use as TemplateURL
        return Ok(TemplateResult {
            template_body: None,
            template_url: Some(processed_location.to_string()),
        });
    }

    // Load the template content
    let import_data = if is_inline_content {
        // Inline template content - use directly
        processed_location.to_string()
    } else {
        // Local file or S3/HTTP that needs processing
        let import_result = load_file_import(&processed_location, base_location)
            .await
            .with_context(|| format!("Failed to load template from: {}", processed_location))?;
        import_result.data
    };

    // Check for preprocessing syntax without render: prefix
    if import_data.contains("$imports:") && !should_render {
        let msg = if is_inline_content {
            "Your inline cloudformation Template appears to use iidy's yaml pre-processor syntax.\n\
             You need to prefix the template with \"render:\"."
        } else {
            &format!(
                "Your cloudformation Template from {} appears to use iidy's yaml pre-processor syntax.\n\
                 You need to prefix the template location with \"render:\".\n\
                 e.g.   Template: \"render:{}\"",
                processed_location, processed_location
            )
        };
        bail!("{}", msg);
    }

    let body = if should_render {
        // Parse YAML and add environment values
        let mut doc: Value = serde_yaml::from_str(&import_data).with_context(|| {
            if is_inline_content {
                "Failed to parse YAML template".to_string()
            } else {
                format!("Failed to parse YAML template: {}", processed_location)
            }
        })?;

        // Inject environment values for preprocessing
        if let Value::Mapping(map) = &mut doc {
            let mut env_values = serde_yaml::Mapping::new();
            if let Some(env) = environment {
                env_values.insert(
                    Value::String("environment".to_string()),
                    Value::String(env.to_string()),
                );
            }
            // Add region from AWS context if available
            // TODO: Pass AWS config to get actual region

            map.insert(
                Value::String("$envValues".to_string()),
                Value::Mapping(env_values),
            );
        }

        // Process with YAML preprocessing
        let yaml_spec = YamlSpec::V11;
        let processed_value = preprocess_yaml(
            &serde_yaml::to_string(&doc)?,
            &processed_location,
            &yaml_spec,
        )
        .await?;

        serde_yaml::to_string(&processed_value)?
    } else {
        import_data
    };

    // Check size limits
    if body.len() >= max_size {
        bail!(
            "Your cloudformation template is larger than the max allowed size ({} bytes). \
             You need to upload it to S3 and reference it from there.",
            max_size
        );
    }

    Ok(TemplateResult {
        template_body: Some(body),
        template_url: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_result_creation() {
        let result = TemplateResult {
            template_body: Some("test template".to_string()),
            template_url: None,
        };
        assert!(result.template_body.is_some());
        assert!(result.template_url.is_none());
    }

    #[test]
    fn test_constants() {
        assert_eq!(TEMPLATE_MAX_BYTES, 51199);
        assert_eq!(S3_TEMPLATE_MAX_BYTES, 999999);
    }

    #[test]
    fn test_parse_s3_http_url_path_style() {
        // Test path-style URL: https://s3.us-west-2.amazonaws.com/bucket/path/to/key
        let url = "https://s3.us-west-2.amazonaws.com/mybucket/path/to/template.yaml";
        let (bucket, key) = parse_s3_http_url(url).unwrap();

        assert_eq!(bucket, "mybucket");
        assert_eq!(key, "path/to/template.yaml");
    }

    #[test]
    fn test_parse_s3_http_url_virtual_hosted_style() {
        // Test virtual-hosted style URL: https://bucket.s3.us-west-2.amazonaws.com/path/to/key
        let url = "https://mybucket.s3.us-west-2.amazonaws.com/path/to/template.yaml";
        let (bucket, key) = parse_s3_http_url(url).unwrap();

        assert_eq!(bucket, "mybucket");
        assert_eq!(key, "path/to/template.yaml");
    }

    #[test]
    fn test_parse_s3_http_url_us_east_1() {
        // Test US East 1 (default region) URL: https://s3.amazonaws.com/bucket/key
        let url = "https://s3.amazonaws.com/mybucket/template.yaml";
        let (bucket, key) = parse_s3_http_url(url).unwrap();

        assert_eq!(bucket, "mybucket");
        assert_eq!(key, "template.yaml");
    }

    #[test]
    fn test_parse_s3_http_url_virtual_hosted_us_east_1() {
        // Test virtual-hosted US East 1: https://bucket.s3.amazonaws.com/key
        let url = "https://mybucket.s3.amazonaws.com/template.yaml";
        let (bucket, key) = parse_s3_http_url(url).unwrap();

        assert_eq!(bucket, "mybucket");
        assert_eq!(key, "template.yaml");
    }

    #[test]
    fn test_parse_s3_http_url_encoded_key() {
        // Test URL with encoded characters in key
        let url = "https://s3.us-west-2.amazonaws.com/mybucket/path%20with%20spaces/template.yaml";
        let (bucket, key) = parse_s3_http_url(url).unwrap();

        assert_eq!(bucket, "mybucket");
        assert_eq!(key, "path with spaces/template.yaml");
    }

    #[test]
    fn test_parse_s3_http_url_invalid() {
        // Test invalid URL (not S3)
        let url = "https://example.com/not-s3";
        let result = parse_s3_http_url(url);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("not a well-formed S3 URL")
        );
    }

    #[tokio::test]
    async fn test_maybe_sign_s3_http_url_non_s3() {
        // Test non-S3 URL (should pass through unchanged)
        let url = "https://example.com/template.yaml";
        let result = maybe_sign_s3_http_url(url, None).await.unwrap();
        assert_eq!(result, url);
    }

    #[tokio::test]
    async fn test_maybe_sign_s3_http_url_already_signed() {
        // Test already signed S3 URL (should pass through unchanged)
        let url = "https://s3.us-west-2.amazonaws.com/bucket/key?Signature=abc123";
        let result = maybe_sign_s3_http_url(url, None).await.unwrap();
        assert_eq!(result, url);
    }

    #[tokio::test]
    async fn test_maybe_sign_s3_http_url_no_client() {
        // Test S3 URL without client (should pass through unchanged)
        let url = "https://s3.us-west-2.amazonaws.com/bucket/template.yaml";
        let result = maybe_sign_s3_http_url(url, None).await.unwrap();
        assert_eq!(result, url);
    }
}

/// Extract bucket and key from an S3 HTTP URL
fn parse_s3_http_url(input: &str) -> Result<(String, String)> {
    // Basic validation that this looks like an S3 URL
    if !input.contains("s3") || !input.contains("amazonaws.com") {
        bail!("HTTP URL '{}' is not a well-formed S3 URL", input);
    }

    let url = url::Url::parse(input).with_context(|| format!("Failed to parse URL: {}", input))?;

    let hostname = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("No hostname in URL: {}", input))?;

    let path = url.path();

    // Handle different S3 URL formats
    let (bucket, key) = if hostname.starts_with("s3.") || hostname.starts_with("s3-") {
        // Path-style: s3.region.amazonaws.com/bucket/key
        let path_parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
        if path_parts.len() < 2 || path_parts[0].is_empty() {
            bail!("HTTP URL '{}' is not a well-formed S3 URL", input);
        }
        let bucket = path_parts[0].to_string();
        let key = path_parts[1..].join("/");
        (bucket, key)
    } else if hostname.contains(".s3.") || hostname.contains(".s3-") {
        // Virtual-hosted: bucket.s3.region.amazonaws.com/key
        let bucket = hostname
            .split('.')
            .next()
            .ok_or_else(|| anyhow::anyhow!("Cannot extract bucket from hostname: {}", hostname))?
            .to_string();
        let key = path.trim_start_matches('/').to_string();
        (bucket, key)
    } else {
        bail!("HTTP URL '{}' is not a well-formed S3 URL", input);
    };

    // URL decode the key
    let key = urlencoding::decode(&key)?.into_owned();

    Ok((bucket, key))
}

/// Auto-sign S3 HTTP URLs for cross-region access (maybeSignS3HttpUrl equivalent)
async fn maybe_sign_s3_http_url(location: &str, s3_client: Option<&S3Client>) -> Result<String> {
    // Check if this is an unsigned S3 HTTP URL
    let is_unsigned_s3_http_url = location.starts_with("http")
        && location.contains("s3")
        && location.contains("amazonaws.com")
        && !location.contains("Signature=");

    if is_unsigned_s3_http_url {
        if let Some(client) = s3_client {
            let (bucket, key) = parse_s3_http_url(location)?;

            // Generate presigned URL for GetObject
            let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(
                std::time::Duration::from_secs(3600), // 1 hour expiration
            )?;

            let presigned_request = client
                .get_object()
                .bucket(&bucket)
                .key(&key)
                .presigned(presigning_config)
                .await?;

            Ok(presigned_request.uri().to_string())
        } else {
            // No S3 client available, return location as-is
            // This allows graceful fallback when S3 client is not available
            Ok(location.to_string())
        }
    } else {
        Ok(location.to_string())
    }
}

/// Result of stack policy loading
#[derive(Debug, Clone)]
pub struct StackPolicyResult {
    pub stack_policy_body: Option<String>,
    pub stack_policy_url: Option<String>,
}

/// Load a CloudFormation stack policy with full iidy-js compatibility
pub async fn load_cfn_stack_policy(
    policy: Option<&Value>,
    base_location: &str,
    s3_client: Option<&S3Client>,
) -> Result<StackPolicyResult> {
    let policy = match policy {
        Some(p) => p,
        None => {
            return Ok(StackPolicyResult {
                stack_policy_body: None,
                stack_policy_url: None,
            });
        }
    };

    match policy {
        Value::String(location) if !location.trim().is_empty() => {
            let location = location.trim();

            // Check for render: prefix
            let should_render = location.starts_with("render:");
            let actual_location = if should_render {
                location.strip_prefix("render:").unwrap_or(location).trim()
            } else {
                location
            };

            // Auto-sign S3 URLs
            let processed_location = maybe_sign_s3_http_url(actual_location, s3_client).await?;

            // Check if URL (S3 or HTTP)
            if !should_render && processed_location.starts_with("s3:") {
                bail!(
                    "Use https:// S3 path-based urls when using a plain (non-rendered) StackPolicy from S3: {}",
                    processed_location
                );
            } else if !should_render && processed_location.starts_with("http") {
                // HTTP URL - use as StackPolicyURL
                return Ok(StackPolicyResult {
                    stack_policy_body: None,
                    stack_policy_url: Some(processed_location.to_string()),
                });
            }

            // Load from file
            let import_result = load_file_import(&processed_location, base_location)
                .await
                .with_context(|| {
                    format!("Failed to load stack policy from: {}", processed_location)
                })?;

            let body = if should_render {
                // Parse YAML and process
                let doc: Value = serde_yaml::from_str(&import_result.data).with_context(|| {
                    format!("Failed to parse YAML stack policy: {}", processed_location)
                })?;

                let yaml_spec = YamlSpec::V11;
                let processed_value = preprocess_yaml(
                    &serde_yaml::to_string(&doc)?,
                    &processed_location,
                    &yaml_spec,
                )
                .await?;

                serde_yaml::to_string(&processed_value)?
            } else {
                import_result.data
            };

            Ok(StackPolicyResult {
                stack_policy_body: Some(body),
                stack_policy_url: None,
            })
        }
        _ => {
            // Direct YAML value - serialize to JSON
            let body = serde_json::to_string(policy)
                .context("Failed to serialize stack policy to JSON")?;

            Ok(StackPolicyResult {
                stack_policy_body: Some(body),
                stack_policy_url: None,
            })
        }
    }
}
