//! AWS CloudFormation import loader
//!
//! Provides functionality for loading stack outputs, exports, parameters, tags,
//! resources, and full stack data from CloudFormation

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

/// CloudFormation Stack Parameter representation
#[derive(Debug, Clone)]
pub struct CfnParameter {
    pub key: String,
    pub value: String,
}

/// CloudFormation Stack Tag representation
#[derive(Debug, Clone)]
pub struct CfnTag {
    pub key: String,
    pub value: String,
}

/// CloudFormation Stack Resource representation
#[derive(Debug, Clone)]
pub struct CfnResource {
    pub logical_resource_id: String,
    pub physical_resource_id: Option<String>,
    pub resource_type: String,
    pub resource_status: String,
}

/// Trait for CloudFormation operations (allows mocking in tests)
#[async_trait]
pub trait CfnClient: Send + Sync {
    async fn get_stack_outputs(&self, stack_name: &str) -> Result<Vec<CfnOutput>>;
    async fn get_stack_exports(&self) -> Result<Vec<CfnExport>>;
    async fn get_stack_parameters(&self, stack_name: &str) -> Result<Vec<CfnParameter>>;
    async fn get_stack_tags(&self, stack_name: &str) -> Result<Vec<CfnTag>>;
    async fn get_stack_resources(&self, stack_name: &str) -> Result<Vec<CfnResource>>;
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

    async fn get_stack_parameters(&self, stack_name: &str) -> Result<Vec<CfnParameter>> {
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
            .and_then(|s| s.into_iter().next())
            .ok_or_else(|| anyhow!("CloudFormation stack {} not found", stack_name))?;

        let mut params = Vec::new();
        if let Some(stack_params) = stack.parameters {
            for p in stack_params {
                if let (Some(key), Some(value)) = (p.parameter_key, p.parameter_value) {
                    params.push(CfnParameter { key, value });
                }
            }
        }

        Ok(params)
    }

    async fn get_stack_tags(&self, stack_name: &str) -> Result<Vec<CfnTag>> {
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
            .and_then(|s| s.into_iter().next())
            .ok_or_else(|| anyhow!("CloudFormation stack {} not found", stack_name))?;

        let mut tags = Vec::new();
        if let Some(stack_tags) = stack.tags {
            for t in stack_tags {
                if let (Some(key), Some(value)) = (t.key, t.value) {
                    tags.push(CfnTag { key, value });
                }
            }
        }

        Ok(tags)
    }

    async fn get_stack_resources(&self, stack_name: &str) -> Result<Vec<CfnResource>> {
        let response = self
            .client
            .describe_stack_resources()
            .stack_name(stack_name)
            .send()
            .await
            .map_err(|e| {
                anyhow!(
                    "Failed to describe stack resources for {}: {}",
                    stack_name,
                    e
                )
            })?;

        let mut resources = Vec::new();
        if let Some(stack_resources) = response.stack_resources {
            for r in stack_resources {
                resources.push(CfnResource {
                    logical_resource_id: r.logical_resource_id.unwrap_or_default(),
                    physical_resource_id: r.physical_resource_id,
                    resource_type: r.resource_type.unwrap_or_default(),
                    resource_status: r
                        .resource_status
                        .map(|s| s.as_str().to_string())
                        .unwrap_or_default(),
                });
            }
        }

        Ok(resources)
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

fn resource_to_mapping(r: &CfnResource) -> serde_yaml::Mapping {
    let mut m = serde_yaml::Mapping::new();
    m.insert(
        Value::String("LogicalResourceId".to_string()),
        Value::String(r.logical_resource_id.clone()),
    );
    m.insert(
        Value::String("PhysicalResourceId".to_string()),
        match &r.physical_resource_id {
            Some(id) => Value::String(id.clone()),
            None => Value::Null,
        },
    );
    m.insert(
        Value::String("ResourceType".to_string()),
        Value::String(r.resource_type.clone()),
    );
    m.insert(
        Value::String("ResourceStatus".to_string()),
        Value::String(r.resource_status.clone()),
    );
    m
}

/// Load a CloudFormation import with custom client (for testing)
pub async fn load_cfn_import_with_client(
    location: &str,
    client: &dyn CfnClient,
) -> Result<ImportData> {
    let (import_type, stack_name, field_key) = parse_cfn_location(location)?;

    let (data, doc) = match import_type.as_str() {
        "output" => {
            let outputs = client.get_stack_outputs(&stack_name).await?;
            if field_key.is_empty() {
                let mapping: serde_yaml::Mapping = outputs
                    .iter()
                    .map(|o| {
                        (
                            Value::String(o.output_key.clone()),
                            Value::String(o.output_value.clone()),
                        )
                    })
                    .collect();
                let doc = Value::Mapping(mapping);
                let data = serde_yaml::to_string(&doc)?;
                (data, doc)
            } else {
                let output = outputs
                    .iter()
                    .find(|o| o.output_key == field_key)
                    .ok_or_else(|| {
                        anyhow!("Output {} not found in stack {}", field_key, stack_name)
                    })?;
                let val = output.output_value.clone();
                (val.clone(), Value::String(val))
            }
        }
        "export" => {
            let exports = client.get_stack_exports().await?;
            let export = exports
                .iter()
                .find(|e| e.name == field_key)
                .ok_or_else(|| anyhow!("Export {} not found", field_key))?;
            let val = export.value.clone();
            (val.clone(), Value::String(val))
        }
        "parameter" => {
            let params = client.get_stack_parameters(&stack_name).await?;
            if field_key.is_empty() {
                let mapping: serde_yaml::Mapping = params
                    .iter()
                    .map(|p| (Value::String(p.key.clone()), Value::String(p.value.clone())))
                    .collect();
                let doc = Value::Mapping(mapping);
                let data = serde_yaml::to_string(&doc)?;
                (data, doc)
            } else {
                let param = params.iter().find(|p| p.key == field_key).ok_or_else(|| {
                    anyhow!("Parameter {} not found in stack {}", field_key, stack_name)
                })?;
                let val = param.value.clone();
                (val.clone(), Value::String(val))
            }
        }
        "tag" => {
            let tags = client.get_stack_tags(&stack_name).await?;
            if field_key.is_empty() {
                let mapping: serde_yaml::Mapping = tags
                    .iter()
                    .map(|t| (Value::String(t.key.clone()), Value::String(t.value.clone())))
                    .collect();
                let doc = Value::Mapping(mapping);
                let data = serde_yaml::to_string(&doc)?;
                (data, doc)
            } else {
                let tag = tags.iter().find(|t| t.key == field_key).ok_or_else(|| {
                    anyhow!("Tag {} not found in stack {}", field_key, stack_name)
                })?;
                let val = tag.value.clone();
                (val.clone(), Value::String(val))
            }
        }
        "resource" => {
            let resources = client.get_stack_resources(&stack_name).await?;
            if field_key.is_empty() {
                let mapping: serde_yaml::Mapping = resources
                    .iter()
                    .map(|r| {
                        (
                            Value::String(r.logical_resource_id.clone()),
                            Value::Mapping(resource_to_mapping(r)),
                        )
                    })
                    .collect();
                let doc = Value::Mapping(mapping);
                let data = serde_yaml::to_string(&doc)?;
                (data, doc)
            } else {
                let resource = resources
                    .iter()
                    .find(|r| r.logical_resource_id == field_key)
                    .ok_or_else(|| {
                        anyhow!("Resource {} not found in stack {}", field_key, stack_name)
                    })?;
                let doc = Value::Mapping(resource_to_mapping(resource));
                let data = serde_yaml::to_string(&doc)?;
                (data, doc)
            }
        }
        "full_stack" => {
            let outputs = client.get_stack_outputs(&stack_name).await?;
            let params = client.get_stack_parameters(&stack_name).await?;
            let tags = client.get_stack_tags(&stack_name).await?;

            let mut stack_map = serde_yaml::Mapping::new();

            let output_mapping: serde_yaml::Mapping = outputs
                .iter()
                .map(|o| {
                    (
                        Value::String(o.output_key.clone()),
                        Value::String(o.output_value.clone()),
                    )
                })
                .collect();
            stack_map.insert(
                Value::String("Outputs".to_string()),
                Value::Mapping(output_mapping),
            );

            let param_mapping: serde_yaml::Mapping = params
                .iter()
                .map(|p| (Value::String(p.key.clone()), Value::String(p.value.clone())))
                .collect();
            stack_map.insert(
                Value::String("Parameters".to_string()),
                Value::Mapping(param_mapping),
            );

            let tag_mapping: serde_yaml::Mapping = tags
                .iter()
                .map(|t| (Value::String(t.key.clone()), Value::String(t.value.clone())))
                .collect();
            stack_map.insert(
                Value::String("Tags".to_string()),
                Value::Mapping(tag_mapping),
            );

            let doc = Value::Mapping(stack_map);
            let data = serde_yaml::to_string(&doc)?;
            (data, doc)
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
        data,
        doc,
    })
}

/// Split `stack-name/FieldKey` into (stack, field). Returns `(stack, "")` when no `/` is present
/// (meaning the caller wants all values). Errors on empty stack name or trailing slash with no field.
fn split_stack_field(rest: &str, location: &str) -> Result<(String, String)> {
    match rest.split_once('/') {
        Some((stack, field)) if !stack.is_empty() && !field.is_empty() => {
            Ok((stack.to_string(), field.to_string()))
        }
        Some(_) => Err(anyhow!("Invalid cfn import format: {}", location)),
        None if !rest.is_empty() => Ok((rest.to_string(), String::new())),
        None => Err(anyhow!("Invalid cfn import format: {}", location)),
    }
}

/// Parse a CloudFormation import location string into (import_type, stack_name, field_key).
///
/// Supported formats:
/// - `cfn:stack-name.OutputKey`         legacy dot syntax (stack output)
/// - `cfn:output:stack-name/OutputKey`  canonical output syntax
/// - `cfn:output:stack-name`            all outputs as a mapping
/// - `cfn:export:ExportName`            named export
/// - `cfn:parameter:stack-name/Key`     stack parameter
/// - `cfn:parameter:stack-name`         all parameters as a mapping
/// - `cfn:tag:stack-name/Key`           stack tag
/// - `cfn:tag:stack-name`               all tags as a mapping
/// - `cfn:resource:stack-name/LogicalId` stack resource
/// - `cfn:resource:stack-name`          all resources as a mapping
/// - `cfn:stack:stack-name`             entire stack object
fn parse_cfn_location(location: &str) -> Result<(String, String, String)> {
    let path = location
        .strip_prefix("cfn:")
        .ok_or_else(|| anyhow!("Invalid CloudFormation location format: {}", location))?;

    if let Some(rest) = path.strip_prefix("export:") {
        if rest.is_empty() {
            return Err(anyhow!(
                "Invalid CloudFormation export name in: {}",
                location
            ));
        }
        Ok(("export".to_string(), String::new(), rest.to_string()))
    } else if let Some(rest) = path.strip_prefix("output:") {
        let (stack, field) = split_stack_field(rest, location)?;
        Ok(("output".to_string(), stack, field))
    } else if let Some(rest) = path.strip_prefix("parameter:") {
        let (stack, field) = split_stack_field(rest, location)?;
        Ok(("parameter".to_string(), stack, field))
    } else if let Some(rest) = path.strip_prefix("tag:") {
        let (stack, field) = split_stack_field(rest, location)?;
        Ok(("tag".to_string(), stack, field))
    } else if let Some(rest) = path.strip_prefix("resource:") {
        let (stack, field) = split_stack_field(rest, location)?;
        Ok(("resource".to_string(), stack, field))
    } else if let Some(rest) = path.strip_prefix("stack:") {
        if rest.is_empty() {
            return Err(anyhow!("Invalid cfn import format: {}", location));
        }
        Ok(("full_stack".to_string(), rest.to_string(), String::new()))
    } else {
        // Legacy dot syntax: cfn:stack-name.OutputKey
        let parts: Vec<&str> = path.splitn(2, '.').collect();
        if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
            return Err(anyhow!(
                "Invalid CloudFormation stack output format: {}",
                location
            ));
        }
        Ok((
            "output".to_string(),
            parts[0].to_string(),
            parts[1].to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct MockCfnClient {
        stack_outputs: HashMap<String, Vec<CfnOutput>>,
        exports: Vec<CfnExport>,
        stack_parameters: HashMap<String, Vec<CfnParameter>>,
        stack_tags: HashMap<String, Vec<CfnTag>>,
        stack_resources: HashMap<String, Vec<CfnResource>>,
    }

    impl MockCfnClient {
        fn new() -> Self {
            Self {
                stack_outputs: HashMap::new(),
                exports: Vec::new(),
                stack_parameters: HashMap::new(),
                stack_tags: HashMap::new(),
                stack_resources: HashMap::new(),
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

        fn with_stack_parameters(mut self, stack_name: &str, params: Vec<(&str, &str)>) -> Self {
            let cfn_params = params
                .into_iter()
                .map(|(key, value)| CfnParameter {
                    key: key.to_string(),
                    value: value.to_string(),
                })
                .collect();
            self.stack_parameters
                .insert(stack_name.to_string(), cfn_params);
            self
        }

        fn with_stack_tags(mut self, stack_name: &str, tags: Vec<(&str, &str)>) -> Self {
            let cfn_tags = tags
                .into_iter()
                .map(|(key, value)| CfnTag {
                    key: key.to_string(),
                    value: value.to_string(),
                })
                .collect();
            self.stack_tags.insert(stack_name.to_string(), cfn_tags);
            self
        }

        fn with_stack_resources(mut self, stack_name: &str, resources: Vec<CfnResource>) -> Self {
            self.stack_resources
                .insert(stack_name.to_string(), resources);
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

        async fn get_stack_parameters(&self, stack_name: &str) -> Result<Vec<CfnParameter>> {
            match self.stack_parameters.get(stack_name) {
                Some(params) => Ok(params.clone()),
                None => Err(anyhow!("Stack {} not found", stack_name)),
            }
        }

        async fn get_stack_tags(&self, stack_name: &str) -> Result<Vec<CfnTag>> {
            match self.stack_tags.get(stack_name) {
                Some(tags) => Ok(tags.clone()),
                None => Err(anyhow!("Stack {} not found", stack_name)),
            }
        }

        async fn get_stack_resources(&self, stack_name: &str) -> Result<Vec<CfnResource>> {
            match self.stack_resources.get(stack_name) {
                Some(resources) => Ok(resources.clone()),
                None => Err(anyhow!("Stack {} not found", stack_name)),
            }
        }
    }

    // --- parse tests ---

    #[test]
    fn test_parse_cfn_location_stack_output() -> Result<()> {
        let (import_type, stack_name, output_key) = parse_cfn_location("cfn:my-stack.VpcId")?;
        assert_eq!(import_type, "output");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(output_key, "VpcId");
        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_output_syntax() -> Result<()> {
        let (import_type, stack_name, field) = parse_cfn_location("cfn:output:my-stack/VpcId")?;
        assert_eq!(import_type, "output");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "VpcId");
        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_output_all() -> Result<()> {
        let (import_type, stack_name, field) = parse_cfn_location("cfn:output:my-stack")?;
        assert_eq!(import_type, "output");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "");
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
    fn test_parse_cfn_location_parameter() -> Result<()> {
        let (import_type, stack_name, field) =
            parse_cfn_location("cfn:parameter:my-stack/DbPassword")?;
        assert_eq!(import_type, "parameter");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "DbPassword");
        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_parameter_all() -> Result<()> {
        let (import_type, stack_name, field) = parse_cfn_location("cfn:parameter:my-stack")?;
        assert_eq!(import_type, "parameter");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "");
        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_tag() -> Result<()> {
        let (import_type, stack_name, field) = parse_cfn_location("cfn:tag:my-stack/Environment")?;
        assert_eq!(import_type, "tag");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "Environment");
        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_tag_all() -> Result<()> {
        let (import_type, stack_name, field) = parse_cfn_location("cfn:tag:my-stack")?;
        assert_eq!(import_type, "tag");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "");
        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_resource() -> Result<()> {
        let (import_type, stack_name, field) =
            parse_cfn_location("cfn:resource:my-stack/MyBucket")?;
        assert_eq!(import_type, "resource");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "MyBucket");
        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_resource_all() -> Result<()> {
        let (import_type, stack_name, field) = parse_cfn_location("cfn:resource:my-stack")?;
        assert_eq!(import_type, "resource");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "");
        Ok(())
    }

    #[test]
    fn test_parse_cfn_location_full_stack() -> Result<()> {
        let (import_type, stack_name, field) = parse_cfn_location("cfn:stack:my-stack")?;
        assert_eq!(import_type, "full_stack");
        assert_eq!(stack_name, "my-stack");
        assert_eq!(field, "");
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
        assert!(parse_cfn_location("cfn:stack:").is_err());
        assert!(parse_cfn_location("cfn:output:").is_err());
        assert!(parse_cfn_location("cfn:output:/NoStack").is_err());
    }

    // --- load tests ---

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
        assert_eq!(result.doc, Value::String("vpc-12345".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_output_syntax() -> Result<()> {
        let client = MockCfnClient::new().with_stack_outputs(
            "my-stack",
            vec![("VpcId", "vpc-12345"), ("SubnetId", "subnet-67890")],
        );

        let result = load_cfn_import_with_client("cfn:output:my-stack/VpcId", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.resolved_location, "cfn:output:my-stack/VpcId");
        assert_eq!(result.data, "vpc-12345");
        assert_eq!(result.doc, Value::String("vpc-12345".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_output_all() -> Result<()> {
        let client = MockCfnClient::new().with_stack_outputs(
            "my-stack",
            vec![("VpcId", "vpc-12345"), ("SubnetId", "subnet-67890")],
        );

        let result = load_cfn_import_with_client("cfn:output:my-stack", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.resolved_location, "cfn:output:my-stack");

        let Value::Mapping(m) = &result.doc else {
            panic!("expected mapping, got {:?}", result.doc);
        };
        assert_eq!(
            m.get(Value::String("VpcId".to_string())),
            Some(&Value::String("vpc-12345".to_string()))
        );
        assert_eq!(
            m.get(Value::String("SubnetId".to_string())),
            Some(&Value::String("subnet-67890".to_string()))
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
        assert_eq!(result.doc, Value::String("vpc-shared-123".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_parameter() -> Result<()> {
        let client = MockCfnClient::new().with_stack_parameters(
            "my-stack",
            vec![("DbPassword", "s3cr3t"), ("DbHost", "db.example.com")],
        );

        let result =
            load_cfn_import_with_client("cfn:parameter:my-stack/DbPassword", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(
            result.resolved_location,
            "cfn:parameter:my-stack/DbPassword"
        );
        assert_eq!(result.data, "s3cr3t");
        assert_eq!(result.doc, Value::String("s3cr3t".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_parameter_all() -> Result<()> {
        let client = MockCfnClient::new().with_stack_parameters(
            "my-stack",
            vec![("DbPassword", "s3cr3t"), ("DbHost", "db.example.com")],
        );

        let result = load_cfn_import_with_client("cfn:parameter:my-stack", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);

        let Value::Mapping(m) = &result.doc else {
            panic!("expected mapping, got {:?}", result.doc);
        };
        assert_eq!(
            m.get(Value::String("DbPassword".to_string())),
            Some(&Value::String("s3cr3t".to_string()))
        );
        assert_eq!(
            m.get(Value::String("DbHost".to_string())),
            Some(&Value::String("db.example.com".to_string()))
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_tag() -> Result<()> {
        let client = MockCfnClient::new().with_stack_tags(
            "my-stack",
            vec![("Environment", "production"), ("Team", "platform")],
        );

        let result = load_cfn_import_with_client("cfn:tag:my-stack/Environment", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.resolved_location, "cfn:tag:my-stack/Environment");
        assert_eq!(result.data, "production");
        assert_eq!(result.doc, Value::String("production".to_string()));

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_tag_all() -> Result<()> {
        let client = MockCfnClient::new().with_stack_tags(
            "my-stack",
            vec![("Environment", "production"), ("Team", "platform")],
        );

        let result = load_cfn_import_with_client("cfn:tag:my-stack", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);

        let Value::Mapping(m) = &result.doc else {
            panic!("expected mapping, got {:?}", result.doc);
        };
        assert_eq!(
            m.get(Value::String("Environment".to_string())),
            Some(&Value::String("production".to_string()))
        );
        assert_eq!(
            m.get(Value::String("Team".to_string())),
            Some(&Value::String("platform".to_string()))
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_resource() -> Result<()> {
        let client = MockCfnClient::new().with_stack_resources(
            "my-stack",
            vec![CfnResource {
                logical_resource_id: "MyBucket".to_string(),
                physical_resource_id: Some("my-stack-mybucket-abc123".to_string()),
                resource_type: "AWS::S3::Bucket".to_string(),
                resource_status: "CREATE_COMPLETE".to_string(),
            }],
        );

        let result = load_cfn_import_with_client("cfn:resource:my-stack/MyBucket", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.resolved_location, "cfn:resource:my-stack/MyBucket");

        let Value::Mapping(m) = &result.doc else {
            panic!("expected mapping, got {:?}", result.doc);
        };
        assert_eq!(
            m.get(Value::String("LogicalResourceId".to_string())),
            Some(&Value::String("MyBucket".to_string()))
        );
        assert_eq!(
            m.get(Value::String("PhysicalResourceId".to_string())),
            Some(&Value::String("my-stack-mybucket-abc123".to_string()))
        );
        assert_eq!(
            m.get(Value::String("ResourceType".to_string())),
            Some(&Value::String("AWS::S3::Bucket".to_string()))
        );
        assert_eq!(
            m.get(Value::String("ResourceStatus".to_string())),
            Some(&Value::String("CREATE_COMPLETE".to_string()))
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_resource_all() -> Result<()> {
        let client = MockCfnClient::new().with_stack_resources(
            "my-stack",
            vec![
                CfnResource {
                    logical_resource_id: "MyBucket".to_string(),
                    physical_resource_id: Some("my-stack-mybucket-abc123".to_string()),
                    resource_type: "AWS::S3::Bucket".to_string(),
                    resource_status: "CREATE_COMPLETE".to_string(),
                },
                CfnResource {
                    logical_resource_id: "MyQueue".to_string(),
                    physical_resource_id: Some(
                        "https://sqs.us-east-1.amazonaws.com/123/my-queue".to_string(),
                    ),
                    resource_type: "AWS::SQS::Queue".to_string(),
                    resource_status: "CREATE_COMPLETE".to_string(),
                },
            ],
        );

        let result = load_cfn_import_with_client("cfn:resource:my-stack", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);

        let Value::Mapping(m) = &result.doc else {
            panic!("expected mapping, got {:?}", result.doc);
        };
        assert!(m.contains_key(Value::String("MyBucket".to_string())));
        assert!(m.contains_key(Value::String("MyQueue".to_string())));

        let bucket = m.get(Value::String("MyBucket".to_string())).unwrap();
        let Value::Mapping(bucket_m) = bucket else {
            panic!("expected mapping for bucket");
        };
        assert_eq!(
            bucket_m.get(Value::String("ResourceType".to_string())),
            Some(&Value::String("AWS::S3::Bucket".to_string()))
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_load_cfn_import_full_stack() -> Result<()> {
        let client = MockCfnClient::new()
            .with_stack_outputs("my-stack", vec![("VpcId", "vpc-12345")])
            .with_stack_parameters("my-stack", vec![("Env", "prod")])
            .with_stack_tags("my-stack", vec![("Team", "platform")]);

        let result = load_cfn_import_with_client("cfn:stack:my-stack", &client).await?;

        assert_eq!(result.import_type, ImportType::Cfn);
        assert_eq!(result.resolved_location, "cfn:stack:my-stack");

        let Value::Mapping(m) = &result.doc else {
            panic!("expected mapping, got {:?}", result.doc);
        };

        let outputs = m
            .get(Value::String("Outputs".to_string()))
            .expect("missing Outputs key");
        let Value::Mapping(outputs_m) = outputs else {
            panic!("expected Outputs to be a mapping");
        };
        assert_eq!(
            outputs_m.get(Value::String("VpcId".to_string())),
            Some(&Value::String("vpc-12345".to_string()))
        );

        let params = m
            .get(Value::String("Parameters".to_string()))
            .expect("missing Parameters key");
        let Value::Mapping(params_m) = params else {
            panic!("expected Parameters to be a mapping");
        };
        assert_eq!(
            params_m.get(Value::String("Env".to_string())),
            Some(&Value::String("prod".to_string()))
        );

        let tags = m
            .get(Value::String("Tags".to_string()))
            .expect("missing Tags key");
        let Value::Mapping(tags_m) = tags else {
            panic!("expected Tags to be a mapping");
        };
        assert_eq!(
            tags_m.get(Value::String("Team".to_string())),
            Some(&Value::String("platform".to_string()))
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

    #[tokio::test]
    async fn test_load_cfn_import_parameter_not_found() {
        let client =
            MockCfnClient::new().with_stack_parameters("my-stack", vec![("DbPassword", "s3cr3t")]);

        let result =
            load_cfn_import_with_client("cfn:parameter:my-stack/NonexistentParam", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_load_cfn_import_tag_not_found() {
        let client =
            MockCfnClient::new().with_stack_tags("my-stack", vec![("Environment", "production")]);

        let result = load_cfn_import_with_client("cfn:tag:my-stack/NonexistentTag", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_load_cfn_import_resource_not_found() {
        let client = MockCfnClient::new().with_stack_resources(
            "my-stack",
            vec![CfnResource {
                logical_resource_id: "MyBucket".to_string(),
                physical_resource_id: None,
                resource_type: "AWS::S3::Bucket".to_string(),
                resource_status: "CREATE_COMPLETE".to_string(),
            }],
        );

        let result =
            load_cfn_import_with_client("cfn:resource:my-stack/NonexistentResource", &client).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
