use anyhow::Result;

/// Check if a template exists in S3
pub async fn check_template_exists(s3_client: &aws_sdk_s3::Client, bucket: &str, key: &str) -> Result<bool> {
    match s3_client.head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await {
        Ok(_) => Ok(true),
        Err(e) => {
            if e.to_string().contains("NotFound") {
                return Ok(false);
            }
            Err(e.into())
        }
    }
}