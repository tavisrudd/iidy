use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;

/// Calculate SHA256 hash of template content for versioned S3 key generation.
pub fn calculate_template_hash(template_content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(template_content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Parse S3 URL and generate versioned S3 location
pub fn generate_versioned_location(
    base_location: &str,
    template_content: &str,
    template_path: &str,
) -> Result<(String, String)> {
    // Parse S3 URL (should be s3://bucket/path/)
    if !base_location.starts_with("s3://") {
        anyhow::bail!("ApprovedTemplateLocation must be an S3 URL (s3://bucket/path/)");
    }

    let url_without_scheme = &base_location[5..]; // Remove "s3://"
    let parts: Vec<&str> = url_without_scheme.splitn(2, '/').collect();

    if parts.len() != 2 {
        anyhow::bail!("Invalid S3 URL format. Expected s3://bucket/path/");
    }

    let bucket = parts[0].to_string();
    let base_path = parts[1];

    // Calculate hash and generate filename
    let hash = calculate_template_hash(template_content);
    let extension = Path::new(template_path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("yaml");

    let filename = format!("{hash}.{extension}");
    let key = if base_path.ends_with('/') {
        format!("{base_path}{filename}")
    } else {
        format!("{base_path}/{filename}")
    };

    Ok((bucket, key))
}

/// Parse S3 URL into bucket and key components
pub fn parse_s3_url(s3_url: &str) -> Result<(String, String)> {
    if !s3_url.starts_with("s3://") {
        anyhow::bail!("URL must start with s3://");
    }

    let url_without_scheme = &s3_url[5..]; // Remove "s3://"
    let parts: Vec<&str> = url_without_scheme.splitn(2, '/').collect();

    if parts.len() != 2 {
        anyhow::bail!("Invalid S3 URL format. Expected s3://bucket/key");
    }

    Ok((parts[0].to_string(), parts[1].to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_template_hash() {
        let template = "AWSTemplateFormatVersion: '2010-09-09'\nResources: {}";
        let hash = calculate_template_hash(template);

        // Hash should be deterministic
        let hash2 = calculate_template_hash(template);
        assert_eq!(hash, hash2);

        // Hash should be 64 characters (SHA256 hex)
        assert_eq!(hash.len(), 64);

        // Different content should produce different hashes
        let different_template = "AWSTemplateFormatVersion: '2010-09-09'\nResources:\n  Test: {}";
        let different_hash = calculate_template_hash(different_template);
        assert_ne!(hash, different_hash);
    }

    #[test]
    fn test_generate_versioned_location() {
        let template = "AWSTemplateFormatVersion: '2010-09-09'\nResources: {}";
        let base_location = "s3://my-bucket/templates/";
        let template_path = "my-template.yaml";

        let result = generate_versioned_location(base_location, template, template_path);
        assert!(result.is_ok());

        let (bucket, key) = result.unwrap();
        assert_eq!(bucket, "my-bucket");
        assert!(key.starts_with("templates/"));
        assert!(key.ends_with(".yaml"));
        assert!(key.contains(&calculate_template_hash(template)));
    }

    #[test]
    fn test_generate_versioned_location_without_trailing_slash() {
        let template = "test";
        let base_location = "s3://my-bucket/templates";
        let template_path = "test.json";

        let result = generate_versioned_location(base_location, template, template_path);
        assert!(result.is_ok());

        let (bucket, key) = result.unwrap();
        assert_eq!(bucket, "my-bucket");
        assert!(key.starts_with("templates/"));
        assert!(key.ends_with(".json"));
    }

    #[test]
    fn test_generate_versioned_location_invalid_url() {
        let template = "test";
        let base_location = "https://example.com/bucket";
        let template_path = "test.yaml";

        let result = generate_versioned_location(base_location, template, template_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_s3_url() {
        let url = "s3://my-bucket/path/to/file.json";
        let result = parse_s3_url(url);
        assert!(result.is_ok());

        let (bucket, key) = result.unwrap();
        assert_eq!(bucket, "my-bucket");
        assert_eq!(key, "path/to/file.json");
    }

    #[test]
    fn test_parse_s3_url_invalid() {
        let url = "https://example.com/file";
        let result = parse_s3_url(url);
        assert!(result.is_err());

        let url = "s3://bucket-only";
        let result = parse_s3_url(url);
        assert!(result.is_err());
    }
}
