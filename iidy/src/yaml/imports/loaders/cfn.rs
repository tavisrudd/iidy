//! AWS CloudFormation import loader
//!
//! Provides functionality for loading stack outputs and exports from CloudFormation

use anyhow::{Result, anyhow};
use async_trait::async_trait;
use serde_yaml::Value;

use crate::yaml::imports::{ImportData, ImportType};

/// CloudFormation Stack Output representation
#[derive(Debug, Clone)]
pub struct CfnOutput {
    pub output_key: String,
    pub output_value: String,
    pub description: Option<String>,
}

/// CloudFormation Stack Export representation
#[derive(Debug, Clone)]
pub struct CfnExport {
    pub name: String,
    pub value: String,
    pub exporting_stack_id: String,
}

/// Trait for CloudFormation operations (allows mocking in tests)
#[async_trait]
pub trait CfnClient: Send + Sync {
    async fn get_stack_outputs(&self, stack_name: &str) -> Result<Vec<CfnOutput>>;
    async fn get_stack_exports(&self) -> Result<Vec<CfnExport>>;
}

/// Production CloudFormation client implementation
pub struct AwsCfnClient {
    client: aws_sdk_cloudformation::Client,
}

impl AwsCfnClient {
    pub fn new(aws_config: &aws_config::SdkConfig) -> Self {
        Self {
            client: aws_sdk_cloudformation::Client::new(aws_config),
        }
    }
}

#[async_trait]
impl CfnClient for AwsCfnClient {
    async fn get_stack_outputs(&self, stack_name: &str) -> Result<Vec<CfnOutput>> {
        let response = self
            .client
            .describe_stacks()
            .stack_name(stack_name)
            .send()
            .await
            .map_err(|e| {
                anyhow!(
                    "Failed to describe CloudFormation stack {}: {}",
                    stack_name,
                    e
                )
            })?;

        let stack = response
            .stacks
            .and_then(|stacks| stacks.into_iter().next())
            .ok_or_else(|| anyhow!("CloudFormation stack {} not found", stack_name))?;

        let mut outputs = Vec::new();
        if let Some(stack_outputs) = stack.outputs {
            for output in stack_outputs {
                if let (Some(key), Some(value)) = (output.output_key, output.output_value) {
                    outputs.push(CfnOutput {
                        output_key: key,
                        output_value: value,
                        description: output.description,
                    });
                }
            }
        }

        Ok(outputs)
    }

    async fn get_stack_exports(&self) -> Result<Vec<CfnExport>> {
        let response = self
            .client
            .list_exports()
            .send()
            .await
            .map_err(|e| anyhow!("Failed to list CloudFormation exports: {}", e))?;

        let mut exports = Vec::new();
        if let Some(cfn_exports) = response.exports {
            for export in cfn_exports {
                if let (Some(name), Some(value), Some(exporting_stack_id)) =
                    (export.name, export.value, export.exporting_stack_id)
                {
                    exports.push(CfnExport {
                        name,
                        value,
                        exporting_stack_id,
                    });
                }
            }
        }

        Ok(exports)
    }
}

/// Load a CloudFormation import
pub async fn load_cfn_import(
    location: &str,
    aws_config: &aws_config::SdkConfig,
) -> Result<ImportData> {
    let client = AwsCfnClient::new(aws_config);
    load_cfn_import_with_client(location, &client).await
}

/// Load a CloudFormation import with custom client (for testing)
pub async fn load_cfn_import_with_client(
    location: &str,
    client: &dyn CfnClient,
) -> Result<ImportData> {
    // Parse cfn:stack-name.OutputKey or cfn:export:ExportName
    let (import_type, stack_name, output_key) = parse_cfn_location(location)?;

    let data = match import_type.as_str() {
        "stack" => {
            // Get specific output from stack
            let outputs = client.get_stack_outputs(&stack_name).await?;
            let output = outputs
                .iter()
                .find(|o| o.output_key == output_key)
                .ok_or_else(|| {
                    anyhow!("Output {} not found in stack {}", output_key, stack_name)
                })?;
            output.output_value.clone()
        }
        "export" => {
            // Get specific export by name
            let exports = client.get_stack_exports().await?;
            let export = exports
                .iter()
                .find(|e| e.name == output_key)
                .ok_or_else(|| anyhow!("Export {} not found", output_key))?;
            export.value.clone()
        }
        _ => {
            return Err(anyhow!(
                "Invalid CloudFormation import type: {}",
                import_type
            ));
        }
    };

    Ok(ImportData {
        import_type: ImportType::Cfn,
        resolved_location: location.to_string(),
        data: data.clone(),
        doc: Value::String(data),
    })
}

/// Parse CloudFormation location
///
/// Supported formats:
/// - cfn:stack-name.OutputKey (stack output)
/// - cfn:export:ExportName (stack export)
fn parse_cfn_location(location: &str) -> Result<(String, String, String)> {
    if !location.starts_with("cfn:") {
        return Err(anyhow!(
            "Invalid CloudFormation location format: {}",
            location
        ));
    }

    let path = location.strip_prefix("cfn:").unwrap();

    if path.starts_with("export:") {
        // cfn:export:ExportName
        let export_name = path.strip_prefix("export:").unwrap();
        if export_name.is_empty() {
            return Err(anyhow!(
                "Invalid CloudFormation export name in: {}",
                location
            ));
        }
        Ok((
            "export".to_string(),
            "".to_string(),
            export_name.to_string(),
        ))
    } else {
        // cfn:stack-name.OutputKey
        let parts: Vec<&str> = path.splitn(2, '.').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(anyhow!(
                "Invalid CloudFormation stack output format: {}",
                location
            ));
        }
        Ok((
            "stack".to_string(),
            parts[0].to_string(),
            parts[1].to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Mock CloudFormation client for testing
    struct MockCfnClient {
        stack_outputs: HashMap<String, Vec<CfnOutput>>,
        exports: Vec<CfnExport>,
    }

    impl MockCfnClient {
        fn new() -> Self {
            Self {
                stack_outputs: HashMap::new(),
                exports: Vec::new(),
            }
        }

        fn with_stack_outputs(mut self, stack_name: &str, outputs: Vec<(&str, &str)>) -> Self {
            let cfn_outputs = outputs
                .into_iter()
                .map(|(key, value)| CfnOutput {
                    output_key: key.to_string(),
                    output_value: value.to_string(),
                    description: None,
                })
                .collect();
            self.stack_outputs
                .insert(stack_name.to_string(), cfn_outputs);
            self
        }

        fn with_exports(mut self, exports: Vec<(&str, &str)>) -> Self {
            let cfn_exports = exports
                .into_iter()
                .map(|(name, value)| CfnExport {
                    name: name.to_string(),
                    value: value.to_string(),
                    exporting_stack_id:
                        "arn:aws:cloudformation:us-east-1:123456789012:stack/test/123".to_string(),
                })
                .collect();
            self.exports = cfn_exports;
            self
        }
    }

    #[async_trait]
    impl CfnClient for MockCfnClient {
        async fn get_stack_outputs(&self, stack_name: &str) -> Result<Vec<CfnOutput>> {
            match self.stack_outputs.get(stack_name) {
                Some(outputs) => Ok(outputs.clone()),
                None => Err(anyhow!("Stack {} not found", stack_name)),
            }
        }

        async fn get_stack_exports(&self) -> Result<Vec<CfnExport>> {
            Ok(self.exports.clone())
        }
    }

    #[test]
    fn test_parse_cfn_location_stack_output() -> Result<()> {
        let (import_type, stack_name, output_key) = parse_cfn_location("cfn:my-stack.VpcId")?;
        assert_eq!(import_type, "stack");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(output_key, "VpcId");

        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_export() -> Result<()> {
        let (import_type, _stack_name, export_name) = parse_cfn_location("cfn:export:SharedVpcId")?;
        assert_eq!(import_type, "export");
        assert_eq!(export_name, "SharedVpcId");

        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_invalid() {
        assert!(parse_cfn_location("invalid:format").is_err());
        assert!(parse_cfn_location("cfn:").is_err());
        assert!(parse_cfn_location("cfn:stack").is_err());
        assert!(parse_cfn_location("cfn:stack.").is_err());
        assert!(parse_cfn_location("cfn:.output").is_err());
        assert!(parse_cfn_location("cfn:export:").is_err());
    }

    #[tokio::test]
    async fn test_load_cfn_import_stack_output() -> Result<()> {
        let client = MockCfnClient::new().with_stack_outputs(
            "my-stack",
            vec![("VpcId", "vpc-12345"), ("SubnetId", "subnet-67890")],
        );

        let result = load_cfn_import_with_client("cfn:my-stack.VpcId", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.resolved_location, "cfn:my-stack.VpcId");
        assert_eq!(result.data, "vpc-12345");
        assert_eq!(
            result.doc,
            serde_yaml::Value::String("vpc-12345".to_string())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_export() -> Result<()> {
        let client = MockCfnClient::new().with_exports(vec![
            ("SharedVpcId", "vpc-shared-123"),
            ("SharedSubnetId", "subnet-shared-456"),
        ]);

        let result = load_cfn_import_with_client("cfn:export:SharedVpcId", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.resolved_location, "cfn:export:SharedVpcId");
        assert_eq!(result.data, "vpc-shared-123");
        assert_eq!(
            result.doc,
            serde_yaml::Value::String("vpc-shared-123".to_string())
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_stack_not_found() {
        let client = MockCfnClient::new();
        let result = load_cfn_import_with_client("cfn:nonexistent-stack.VpcId", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_load_cfn_import_output_not_found() {
        let client =
            MockCfnClient::new().with_stack_outputs("my-stack", vec![("VpcId", "vpc-12345")]);

        let result = load_cfn_import_with_client("cfn:my-stack.NonexistentOutput", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_load_cfn_import_export_not_found() {
        let client = MockCfnClient::new().with_exports(vec![("SharedVpcId", "vpc-shared-123")]);

        let result = load_cfn_import_with_client("cfn:export:NonexistentExport", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
