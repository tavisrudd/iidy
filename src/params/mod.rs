use anyhow::{Context, Result};
use aws_sdk_kms::Client as KmsClient;
use aws_sdk_ssm::Client as SsmClient;
use aws_sdk_ssm::types::Tag;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::aws::config_from_normalized_opts;
use crate::cli::NormalizedAwsOpts;

pub mod get;
pub mod get_by_path;
pub mod get_history;
pub mod review;
pub mod set;

const MESSAGE_TAG: &str = "iidy:message";

pub async fn create_ssm_client(
    opts: &NormalizedAwsOpts,
) -> Result<(SsmClient, aws_config::SdkConfig)> {
    let (config, _credential_sources) = config_from_normalized_opts(opts).await?;
    if config.region().is_none() {
        anyhow::bail!(
            "No AWS region configured. Set AWS_REGION, AWS_DEFAULT_REGION, or use --region."
        );
    }
    let client = SsmClient::new(&config);
    Ok((client, config))
}

pub async fn create_kms_client(config: &aws_config::SdkConfig) -> KmsClient {
    KmsClient::new(config)
}

/// Look up a KMS alias for an SSM parameter path using hierarchical matching.
/// Checks aliases like `alias/ssm/<path>/<parts>`, popping segments until a match is found.
pub async fn get_kms_alias_for_parameter(
    kms_client: &KmsClient,
    param_path: &str,
) -> Result<Option<String>> {
    let mut aliases: BTreeMap<String, String> = BTreeMap::new();

    let mut marker: Option<String> = None;
    loop {
        let mut req = kms_client.list_aliases();
        if let Some(m) = &marker {
            req = req.marker(m);
        }
        let resp = req.send().await.context("Failed to list KMS aliases")?;
        for alias in resp.aliases() {
            if let Some(name) = alias.alias_name() {
                aliases.insert(name.to_string(), name.to_string());
            }
        }
        marker = resp.next_marker().map(|s| s.to_string());
        if marker.is_none() {
            break;
        }
    }

    Ok(match_kms_alias(&aliases, param_path))
}

/// Pure function for hierarchical KMS alias matching.
/// Given a set of known aliases and a parameter path, finds the most specific
/// alias matching the pattern `alias/ssm/<path>/<parts>`, popping path segments
/// from the right until a match is found or none remains.
fn match_kms_alias(aliases: &BTreeMap<String, String>, param_path: &str) -> Option<String> {
    let path_parts: Vec<&str> = param_path.split('/').filter(|s| !s.is_empty()).collect();
    let mut search_parts: Vec<&str> = vec!["alias", "ssm"];
    search_parts.extend_from_slice(&path_parts);

    while !search_parts.is_empty() {
        let candidate_with_slash = format!("{}/", search_parts.join("/"));
        let candidate_without_slash = search_parts.join("/");
        if let Some(alias) = aliases.get(&candidate_with_slash) {
            return Some(alias.clone());
        }
        if let Some(alias) = aliases.get(&candidate_without_slash) {
            return Some(alias.clone());
        }
        search_parts.pop();
    }

    None
}

/// Fetch an SSM parameter, returning None if not found instead of erroring.
pub async fn maybe_fetch_param(
    ssm: &SsmClient,
    name: &str,
    with_decryption: bool,
) -> Result<Option<aws_sdk_ssm::types::Parameter>> {
    match ssm
        .get_parameter()
        .name(name)
        .with_decryption(with_decryption)
        .send()
        .await
    {
        Ok(resp) => Ok(resp.parameter),
        Err(err) => {
            let service_err = err.into_service_error();
            if service_err.is_parameter_not_found() {
                Ok(None)
            } else {
                Err(anyhow::Error::from(service_err)
                    .context(format!("Failed to get parameter '{name}'")))
            }
        }
    }
}

/// Get tags for an SSM parameter as a BTreeMap.
pub async fn get_param_tags(ssm: &SsmClient, name: &str) -> Result<BTreeMap<String, String>> {
    let resp = ssm
        .list_tags_for_resource()
        .resource_id(name)
        .resource_type(aws_sdk_ssm::types::ResourceTypeForTagging::Parameter)
        .send()
        .await
        .context(format!("Failed to list tags for parameter '{name}'"))?;
    let mut tags = BTreeMap::new();
    for tag in resp.tag_list() {
        tags.insert(tag.key().to_string(), tag.value().to_string());
    }
    Ok(tags)
}

/// Set tags on an SSM parameter.
pub async fn set_param_tags(ssm: &SsmClient, name: &str, tags: Vec<Tag>) -> Result<()> {
    ssm.add_tags_to_resource()
        .resource_id(name)
        .resource_type(aws_sdk_ssm::types::ResourceTypeForTagging::Parameter)
        .set_tags(Some(tags))
        .send()
        .await
        .context(format!("Failed to set tags on parameter '{name}'"))?;
    Ok(())
}

/// Serializable representation of an SSM parameter for json/yaml output.
/// Field names match the AWS SDK / JS output for iidy-js compatibility.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ParamOutput {
    pub name: Option<String>,
    pub r#type: Option<String>,
    pub value: Option<String>,
    pub version: Option<i64>,
    #[serde(rename = "LastModifiedDate")]
    pub last_modified_date: Option<String>,
    #[serde(rename = "ARN")]
    pub arn: Option<String>,
    pub data_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, String>>,
}

impl ParamOutput {
    pub fn from_parameter(param: &aws_sdk_ssm::types::Parameter) -> Self {
        Self {
            name: param.name().map(|s| s.to_string()),
            r#type: param.r#type().map(|t| t.as_str().to_string()),
            value: param.value().map(|s| s.to_string()),
            version: Some(param.version()),
            last_modified_date: param.last_modified_date().map(format_aws_datetime),
            arn: param.arn().map(|s| s.to_string()),
            data_type: param.data_type().map(|s| s.to_string()),
            tags: None,
        }
    }

    pub fn with_tags(mut self, tags: BTreeMap<String, String>) -> Self {
        self.tags = Some(tags);
        self
    }
}

/// Serializable representation of an SSM parameter history entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ParamHistoryOutput {
    pub name: Option<String>,
    pub r#type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(rename = "LastModifiedDate")]
    pub last_modified_date: Option<String>,
    #[serde(rename = "LastModifiedUser")]
    pub last_modified_user: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub value: Option<String>,
    pub version: Option<i64>,
    pub data_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<BTreeMap<String, String>>,
}

impl ParamHistoryOutput {
    pub fn from_history(entry: &aws_sdk_ssm::types::ParameterHistory) -> Self {
        Self {
            name: entry.name().map(|s| s.to_string()),
            r#type: entry.r#type().map(|t| t.as_str().to_string()),
            key_id: entry.key_id().map(|s| s.to_string()),
            last_modified_date: entry.last_modified_date().map(format_aws_datetime),
            last_modified_user: entry.last_modified_user().map(|s| s.to_string()),
            description: entry.description().map(|s| s.to_string()),
            value: entry.value().map(|s| s.to_string()),
            version: Some(entry.version()),
            data_type: entry.data_type().map(|s| s.to_string()),
            tags: None,
        }
    }

    pub fn with_tags(mut self, tags: BTreeMap<String, String>) -> Self {
        self.tags = Some(tags);
        self
    }
}

pub(crate) fn format_aws_datetime(dt: &aws_smithy_types::DateTime) -> String {
    dt.fmt(aws_smithy_types::date_time::Format::DateTime)
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Format output according to the --format flag (json or yaml).
/// For "simple" format, callers handle output directly.
pub fn format_output(format: &str, value: &impl Serialize) -> Result<String> {
    match format {
        "json" => serde_json::to_string_pretty(value).context("Failed to serialize as JSON"),
        "yaml" => serde_yaml::to_string(value).context("Failed to serialize as YAML"),
        _ => anyhow::bail!("Unsupported format: {}", format),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_sdk_ssm::types::{Parameter, ParameterHistory, ParameterType};

    fn sample_aliases() -> BTreeMap<String, String> {
        let mut m = BTreeMap::new();
        m.insert(
            "alias/ssm/myapp/prod/".to_string(),
            "alias/ssm/myapp/prod/".to_string(),
        );
        m.insert(
            "alias/ssm/myapp/".to_string(),
            "alias/ssm/myapp/".to_string(),
        );
        m.insert("alias/ssm/other".to_string(), "alias/ssm/other".to_string());
        m.insert("alias/aws/s3".to_string(), "alias/aws/s3".to_string());
        m
    }

    #[test]
    fn kms_alias_exact_match() {
        let aliases = sample_aliases();
        assert_eq!(
            match_kms_alias(&aliases, "/myapp/prod/db-password"),
            Some("alias/ssm/myapp/prod/".to_string())
        );
    }

    #[test]
    fn kms_alias_partial_match_pops_segments() {
        let aliases = sample_aliases();
        assert_eq!(
            match_kms_alias(&aliases, "/myapp/staging/db-password"),
            Some("alias/ssm/myapp/".to_string())
        );
    }

    #[test]
    fn kms_alias_no_match() {
        let aliases = sample_aliases();
        assert_eq!(match_kms_alias(&aliases, "/unknown/path"), None);
    }

    #[test]
    fn kms_alias_without_trailing_slash() {
        let aliases = sample_aliases();
        assert_eq!(
            match_kms_alias(&aliases, "/other/something"),
            Some("alias/ssm/other".to_string())
        );
    }

    #[test]
    fn kms_alias_empty_aliases() {
        let aliases = BTreeMap::new();
        assert_eq!(match_kms_alias(&aliases, "/any/path"), None);
    }

    #[test]
    fn param_output_from_parameter() {
        let param = Parameter::builder()
            .name("/myapp/prod/db-password")
            .r#type(ParameterType::SecureString)
            .value("secret-value")
            .version(3)
            .arn("arn:aws:ssm:us-east-1:123456789012:parameter/myapp/prod/db-password")
            .data_type("text")
            .build();

        let output = ParamOutput::from_parameter(&param);
        assert_eq!(output.name.as_deref(), Some("/myapp/prod/db-password"));
        assert_eq!(output.r#type.as_deref(), Some("SecureString"));
        assert_eq!(output.value.as_deref(), Some("secret-value"));
        assert_eq!(output.version, Some(3));
        assert_eq!(
            output.arn.as_deref(),
            Some("arn:aws:ssm:us-east-1:123456789012:parameter/myapp/prod/db-password")
        );
        assert_eq!(output.data_type.as_deref(), Some("text"));
        assert!(output.tags.is_none());
    }

    #[test]
    fn param_output_with_tags() {
        let param = Parameter::builder().name("/test").value("val").build();

        let mut tags = BTreeMap::new();
        tags.insert("iidy:message".to_string(), "test message".to_string());
        tags.insert("env".to_string(), "prod".to_string());

        let output = ParamOutput::from_parameter(&param).with_tags(tags.clone());
        assert_eq!(output.tags.as_ref().unwrap().len(), 2);
        assert_eq!(
            output.tags.as_ref().unwrap().get("iidy:message").unwrap(),
            "test message"
        );
    }

    #[test]
    fn param_output_json_has_pascal_case_fields() {
        let param = Parameter::builder()
            .name("/test")
            .r#type(ParameterType::String)
            .value("hello")
            .version(1)
            .data_type("text")
            .build();

        let output = ParamOutput::from_parameter(&param);
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"Name\""));
        assert!(json.contains("\"Type\""));
        assert!(json.contains("\"Value\""));
        assert!(json.contains("\"Version\""));
        assert!(json.contains("\"DataType\""));
        // Tags should be absent (skip_serializing_if)
        assert!(!json.contains("\"Tags\""));
    }

    #[test]
    fn param_output_json_includes_tags_when_present() {
        let param = Parameter::builder().name("/test").value("val").build();

        let mut tags = BTreeMap::new();
        tags.insert("key".to_string(), "value".to_string());
        let output = ParamOutput::from_parameter(&param).with_tags(tags);

        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"Tags\""));
    }

    #[test]
    fn param_history_output_from_history() {
        let entry = ParameterHistory::builder()
            .name("/myapp/prod/db-password")
            .r#type(ParameterType::SecureString)
            .key_id("alias/ssm/myapp/prod/")
            .last_modified_user("arn:aws:iam::123456789012:user/admin")
            .value("old-secret")
            .version(2)
            .data_type("text")
            .build();

        let output = ParamHistoryOutput::from_history(&entry);
        assert_eq!(output.name.as_deref(), Some("/myapp/prod/db-password"));
        assert_eq!(output.r#type.as_deref(), Some("SecureString"));
        assert_eq!(output.key_id.as_deref(), Some("alias/ssm/myapp/prod/"));
        assert_eq!(
            output.last_modified_user.as_deref(),
            Some("arn:aws:iam::123456789012:user/admin")
        );
        assert_eq!(output.value.as_deref(), Some("old-secret"));
        assert_eq!(output.version, Some(2));
        assert!(output.tags.is_none());
    }

    #[test]
    fn param_history_json_skips_optional_nones() {
        let entry = ParameterHistory::builder()
            .name("/test")
            .value("val")
            .version(1)
            .build();

        let output = ParamHistoryOutput::from_history(&entry);
        let json = serde_json::to_string(&output).unwrap();
        // key_id, description, tags should be absent
        assert!(!json.contains("\"KeyId\""));
        assert!(!json.contains("\"Description\""));
        assert!(!json.contains("\"Tags\""));
    }

    #[test]
    fn format_output_json() {
        let mut map = BTreeMap::new();
        map.insert("a".to_string(), "1".to_string());
        map.insert("b".to_string(), "2".to_string());

        let result = format_output("json", &map).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["a"], "1");
        assert_eq!(parsed["b"], "2");
    }

    #[test]
    fn format_output_yaml() {
        let mut map = BTreeMap::new();
        map.insert("key".to_string(), "value".to_string());

        let result = format_output("yaml", &map).unwrap();
        assert!(result.contains("key: value"));
    }

    #[test]
    fn format_output_unsupported() {
        let val = "test";
        let result = format_output("xml", &val);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Unsupported format")
        );
    }

    #[test]
    fn param_output_roundtrip_json() {
        let param = Parameter::builder()
            .name("/myapp/prod/key")
            .r#type(ParameterType::SecureString)
            .value("secret")
            .version(5)
            .arn("arn:aws:ssm:us-east-1:123:parameter/myapp/prod/key")
            .data_type("text")
            .build();

        let mut tags = BTreeMap::new();
        tags.insert("iidy:message".to_string(), "deployed".to_string());

        let output = ParamOutput::from_parameter(&param).with_tags(tags);
        let json = format_output("json", &output).unwrap();
        let roundtripped: ParamOutput = serde_json::from_str(&json).unwrap();

        assert_eq!(roundtripped.name, output.name);
        assert_eq!(roundtripped.value, output.value);
        assert_eq!(roundtripped.version, output.version);
        assert_eq!(roundtripped.tags, output.tags);
    }

    #[test]
    fn param_output_sorted_map_json() {
        let params = [
            Parameter::builder().name("/z/param").value("z").build(),
            Parameter::builder().name("/a/param").value("a").build(),
            Parameter::builder().name("/m/param").value("m").build(),
        ];

        let map: BTreeMap<String, ParamOutput> = params
            .iter()
            .map(|p| {
                (
                    p.name().unwrap_or("").to_string(),
                    ParamOutput::from_parameter(p),
                )
            })
            .collect();

        let json = format_output("json", &map).unwrap();
        let a_pos = json.find("/a/param").unwrap();
        let m_pos = json.find("/m/param").unwrap();
        let z_pos = json.find("/z/param").unwrap();
        assert!(a_pos < m_pos);
        assert!(m_pos < z_pos);
    }

    #[test]
    fn arn_field_name_is_uppercase() {
        let param = Parameter::builder()
            .name("/test")
            .arn("arn:aws:ssm:us-east-1:123:parameter/test")
            .build();

        let output = ParamOutput::from_parameter(&param);
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"ARN\""));
        assert!(!json.contains("\"Arn\""));
    }

    #[test]
    fn last_modified_date_field_name() {
        let output = ParamOutput {
            name: Some("/test".to_string()),
            r#type: None,
            value: Some("v".to_string()),
            version: Some(1),
            last_modified_date: Some("2024-01-15T00:00:00Z".to_string()),
            arn: None,
            data_type: None,
            tags: None,
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("\"LastModifiedDate\""));
    }
}
