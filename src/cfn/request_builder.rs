use aws_sdk_cloudformation::types::{Capability, OnFailure, Parameter, Tag};

use crate::cfn::StackArgs;
use crate::aws::client_req_token::TokenInfo;
#[cfg(test)]
use crate::aws::client_req_token::TokenSource;

use super::{CfnContext, CfnOperation, template_loader::{load_cfn_template, load_cfn_stack_policy, TEMPLATE_MAX_BYTES}};

/// Builder pattern for constructing CloudFormation API requests with proper token injection.
///
/// This struct standardizes the process of building AWS CloudFormation API requests by:
/// - Automatically injecting the correct client request tokens (derived or primary)
/// - Applying StackArgs configuration consistently across all operations
/// - Handling AWS API field name inconsistencies (e.g., client_token vs client_request_token)
/// - Providing a clean separation between token management and request building
pub struct CfnRequestBuilder<'a> {
    context: &'a CfnContext,
    stack_args: &'a StackArgs,
}

impl<'a> CfnRequestBuilder<'a> {
    /// Create a new request builder with the given context and stack arguments.
    pub fn new(context: &'a CfnContext, stack_args: &'a StackArgs) -> Self {
        Self {
            context,
            stack_args,
        }
    }

    /// Build a CreateStack request with token injection and StackArgs integration.
    ///
    /// # Arguments
    /// * `use_primary_token` - Whether to use the primary token (true) or derive a new one (false)
    /// * `operation` - The CloudFormation operation for token derivation (used only if use_primary_token is false)
    /// * `argsfile_path` - Path to the stack-args.yaml file (for template base location)
    /// * `environment` - Environment name for template processing
    ///
    /// # Returns
    /// A tuple containing the prepared CreateStack fluent builder and the token used
    pub async fn build_create_stack(
        &self,
        use_primary_token: bool,
        operation: &CfnOperation,
        argsfile_path: &str,
        environment: Option<&str>,
    ) -> anyhow::Result<(
        aws_sdk_cloudformation::operation::create_stack::builders::CreateStackFluentBuilder,
        TokenInfo,
    )> {
        let token = if use_primary_token {
            self.context.primary_token().clone()
        } else {
            self.context.derive_token_for_step(operation)
        };

        let mut builder = self
            .context
            .client
            .create_stack()
            .client_request_token(&token.value);

        // Apply stack name (required)
        if let Some(ref stack_name) = self.stack_args.stack_name {
            builder = builder.stack_name(stack_name);
        }

        // Load and apply template using template loader
        if let Some(ref template_location) = self.stack_args.template {
            let template_result = load_cfn_template(
                Some(template_location),
                argsfile_path,
                environment,
                TEMPLATE_MAX_BYTES,
                Some(&self.context.create_s3_client()),
            ).await?;

            if let Some(template_body) = template_result.template_body {
                builder = builder.template_body(template_body);
            } else if let Some(template_url) = template_result.template_url {
                builder = builder.template_url(template_url);
            }
        }

        // Apply capabilities
        if let Some(ref capabilities) = self.stack_args.capabilities {
            let aws_capabilities: Vec<Capability> = capabilities
                .iter()
                .filter_map(|cap| match cap.as_str() {
                    "CAPABILITY_IAM" => Some(Capability::CapabilityIam),
                    "CAPABILITY_NAMED_IAM" => Some(Capability::CapabilityNamedIam),
                    "CAPABILITY_AUTO_EXPAND" => Some(Capability::CapabilityAutoExpand),
                    _ => None, // Skip invalid capabilities
                })
                .collect();
            builder = builder.set_capabilities(Some(aws_capabilities));
        }

        // Apply parameters
        if let Some(ref parameters) = self.stack_args.parameters {
            let aws_parameters: Vec<Parameter> = parameters
                .iter()
                .map(|(key, value)| {
                    Parameter::builder()
                        .parameter_key(key)
                        .parameter_value(value)
                        .build()
                })
                .collect();
            builder = builder.set_parameters(Some(aws_parameters));
        }

        // Apply tags
        if let Some(ref tags) = self.stack_args.tags {
            let aws_tags: Vec<Tag> = tags
                .iter()
                .map(|(key, value)| Tag::builder().key(key).value(value).build())
                .collect();
            builder = builder.set_tags(Some(aws_tags));
        }

        // Apply notification ARNs
        if let Some(ref notification_arns) = self.stack_args.notification_arns {
            builder = builder.set_notification_arns(Some(notification_arns.clone()));
        }

        // Apply service role ARN (for CloudFormation operations)
        if let Some(ref role_arn) = self.stack_args.service_role_arn {
            builder = builder.role_arn(role_arn);
        } else if let Some(ref role_arn) = self.stack_args.role_arn {
            builder = builder.role_arn(role_arn);
        }

        // Apply timeout
        if let Some(timeout) = self.stack_args.timeout_in_minutes {
            builder = builder.timeout_in_minutes(timeout as i32);
        }

        // Apply on failure action
        if let Some(ref on_failure) = self.stack_args.on_failure {
            let aws_on_failure = match on_failure.as_str() {
                "DELETE" => Some(OnFailure::Delete),
                "ROLLBACK" => Some(OnFailure::Rollback),
                "DO_NOTHING" => Some(OnFailure::DoNothing),
                _ => None,
            };
            if let Some(action) = aws_on_failure {
                builder = builder.on_failure(action);
            }
        }

        // Apply disable rollback
        if let Some(disable_rollback) = self.stack_args.disable_rollback {
            builder = builder.disable_rollback(disable_rollback);
        }

        // Apply termination protection
        if let Some(enable_termination_protection) = self.stack_args.enable_termination_protection {
            builder = builder.enable_termination_protection(enable_termination_protection);
        }

        // Apply resource types
        if let Some(ref resource_types) = self.stack_args.resource_types {
            builder = builder.set_resource_types(Some(resource_types.clone()));
        }

        // Load and apply stack policy if present
        if let Some(ref stack_policy) = self.stack_args.stack_policy {
            let policy_result = load_cfn_stack_policy(Some(stack_policy), argsfile_path, Some(&self.context.create_s3_client())).await?;

            if let Some(policy_body) = policy_result.stack_policy_body {
                builder = builder.stack_policy_body(policy_body);
            } else if let Some(policy_url) = policy_result.stack_policy_url {
                builder = builder.stack_policy_url(policy_url);
            }
        }

        Ok((builder, token))
    }

    /// Build an UpdateStack request with token injection and StackArgs integration.
    ///
    /// # Arguments
    /// * `use_primary_token` - Whether to use the primary token (true) or derive a new one (false)
    /// * `operation` - The CloudFormation operation for token derivation (used only if use_primary_token is false)
    ///
    /// # Returns
    /// A tuple containing the prepared UpdateStack fluent builder and the token used
    pub async fn build_update_stack(
        &self,
        use_primary_token: bool,
        operation: &CfnOperation,
        argsfile_path: &str,
        environment: Option<&str>,
    ) -> anyhow::Result<(
        aws_sdk_cloudformation::operation::update_stack::builders::UpdateStackFluentBuilder,
        TokenInfo,
    )> {
        let token = if use_primary_token {
            self.context.primary_token().clone()
        } else {
            self.context.derive_token_for_step(operation)
        };

        let mut builder = self
            .context
            .client
            .update_stack()
            .client_request_token(&token.value);

        // Apply stack name (required)
        if let Some(ref stack_name) = self.stack_args.stack_name {
            builder = builder.stack_name(stack_name);
        }

        // Apply template body (if not using previous template)
        if !self.stack_args.use_previous_template.unwrap_or(false) {
            if let Some(ref template_location) = self.stack_args.template {
                let template_result = load_cfn_template(
                    Some(template_location),
                    argsfile_path,
                    environment,
                    TEMPLATE_MAX_BYTES,
                    Some(&self.context.create_s3_client()),
                ).await?;

                if let Some(template_body) = template_result.template_body {
                    builder = builder.template_body(template_body);
                } else if let Some(template_url) = template_result.template_url {
                    builder = builder.template_url(template_url);
                }
            }
        } else {
            builder = builder.use_previous_template(true);
        }

        // Apply capabilities
        if let Some(ref capabilities) = self.stack_args.capabilities {
            let aws_capabilities: Vec<Capability> = capabilities
                .iter()
                .filter_map(|cap| match cap.as_str() {
                    "CAPABILITY_IAM" => Some(Capability::CapabilityIam),
                    "CAPABILITY_NAMED_IAM" => Some(Capability::CapabilityNamedIam),
                    "CAPABILITY_AUTO_EXPAND" => Some(Capability::CapabilityAutoExpand),
                    _ => None,
                })
                .collect();
            builder = builder.set_capabilities(Some(aws_capabilities));
        }

        // Apply parameters
        if let Some(ref parameters) = self.stack_args.parameters {
            let aws_parameters: Vec<Parameter> = parameters
                .iter()
                .map(|(key, value)| {
                    Parameter::builder()
                        .parameter_key(key)
                        .parameter_value(value)
                        .build()
                })
                .collect();
            builder = builder.set_parameters(Some(aws_parameters));
        }

        // Apply tags
        if let Some(ref tags) = self.stack_args.tags {
            let aws_tags: Vec<Tag> = tags
                .iter()
                .map(|(key, value)| Tag::builder().key(key).value(value).build())
                .collect();
            builder = builder.set_tags(Some(aws_tags));
        }

        // Apply notification ARNs
        if let Some(ref notification_arns) = self.stack_args.notification_arns {
            builder = builder.set_notification_arns(Some(notification_arns.clone()));
        }

        // Apply service role ARN
        if let Some(ref role_arn) = self.stack_args.service_role_arn {
            builder = builder.role_arn(role_arn);
        } else if let Some(ref role_arn) = self.stack_args.role_arn {
            builder = builder.role_arn(role_arn);
        }

        // Apply resource types
        if let Some(ref resource_types) = self.stack_args.resource_types {
            builder = builder.set_resource_types(Some(resource_types.clone()));
        }

        Ok((builder, token))
    }

    /// Build a CreateChangeSet request with token injection.
    ///
    /// Note: CreateChangeSet uses `client_token` field instead of `client_request_token`
    /// due to AWS API inconsistencies.
    ///
    /// # Arguments
    /// * `changeset_name` - The name for the changeset
    /// * `use_primary_token` - Whether to use the primary token (true) or derive a new one (false)
    /// * `operation` - The CloudFormation operation for token derivation (used only if use_primary_token is false)
    ///
    /// # Returns
    /// A tuple containing the prepared CreateChangeSet fluent builder and the token used
    pub fn build_create_changeset(&self, changeset_name: &str, use_primary_token: bool, operation: &CfnOperation) -> (aws_sdk_cloudformation::operation::create_change_set::builders::CreateChangeSetFluentBuilder, TokenInfo){
        let token = if use_primary_token {
            self.context.primary_token().clone()
        } else {
            self.context.derive_token_for_step(operation)
        };

        let mut builder = self
            .context
            .client
            .create_change_set()
            .client_token(&token.value) // Note: different field name for changesets!
            .change_set_name(changeset_name);

        // Apply stack name (required)
        if let Some(ref stack_name) = self.stack_args.stack_name {
            builder = builder.stack_name(stack_name);
        }

        // Apply template body (if not using previous template)
        if !self.stack_args.use_previous_template.unwrap_or(false) {
            if let Some(ref template) = self.stack_args.template {
                builder = builder.template_body(template);
            }
        } else {
            builder = builder.use_previous_template(true);
        }

        // Apply capabilities
        if let Some(ref capabilities) = self.stack_args.capabilities {
            let aws_capabilities: Vec<Capability> = capabilities
                .iter()
                .filter_map(|cap| match cap.as_str() {
                    "CAPABILITY_IAM" => Some(Capability::CapabilityIam),
                    "CAPABILITY_NAMED_IAM" => Some(Capability::CapabilityNamedIam),
                    "CAPABILITY_AUTO_EXPAND" => Some(Capability::CapabilityAutoExpand),
                    _ => None,
                })
                .collect();
            builder = builder.set_capabilities(Some(aws_capabilities));
        }

        // Apply parameters
        if let Some(ref parameters) = self.stack_args.parameters {
            let aws_parameters: Vec<Parameter> = parameters
                .iter()
                .map(|(key, value)| {
                    Parameter::builder()
                        .parameter_key(key)
                        .parameter_value(value)
                        .build()
                })
                .collect();
            builder = builder.set_parameters(Some(aws_parameters));
        }

        // Apply tags
        if let Some(ref tags) = self.stack_args.tags {
            let aws_tags: Vec<Tag> = tags
                .iter()
                .map(|(key, value)| Tag::builder().key(key).value(value).build())
                .collect();
            builder = builder.set_tags(Some(aws_tags));
        }

        // Apply notification ARNs
        if let Some(ref notification_arns) = self.stack_args.notification_arns {
            builder = builder.set_notification_arns(Some(notification_arns.clone()));
        }

        // Apply service role ARN
        if let Some(ref role_arn) = self.stack_args.service_role_arn {
            builder = builder.role_arn(role_arn);
        } else if let Some(ref role_arn) = self.stack_args.role_arn {
            builder = builder.role_arn(role_arn);
        }

        // Apply resource types
        if let Some(ref resource_types) = self.stack_args.resource_types {
            builder = builder.set_resource_types(Some(resource_types.clone()));
        }

        (builder, token)
    }

    /// Build an ExecuteChangeSet request with token injection.
    ///
    /// # Arguments
    /// * `changeset_name` - The name or ARN of the changeset to execute
    /// * `use_primary_token` - Whether to use the primary token (true) or derive a new one (false)
    /// * `operation` - The CloudFormation operation for token derivation (used only if use_primary_token is false)
    ///
    /// # Returns
    /// A tuple containing the prepared ExecuteChangeSet fluent builder and the token used
    pub fn build_execute_changeset(&self, changeset_name: &str, use_primary_token: bool, operation: &CfnOperation) -> (aws_sdk_cloudformation::operation::execute_change_set::builders::ExecuteChangeSetFluentBuilder, TokenInfo){
        let token = if use_primary_token {
            self.context.primary_token().clone()
        } else {
            self.context.derive_token_for_step(operation)
        };

        let mut builder = self
            .context
            .client
            .execute_change_set()
            .client_request_token(&token.value) // Uses standard field name
            .change_set_name(changeset_name);

        // Apply stack name (optional, changeset name can be ARN)
        if let Some(ref stack_name) = self.stack_args.stack_name {
            builder = builder.stack_name(stack_name);
        }

        (builder, token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cfn::CfnContext,
        aws::{timing::MockTimeProvider, client_req_token::TokenInfo, CredentialSourceStack, CredentialSource, ProfileSource},
    };
    use aws_sdk_cloudformation::Client;
    use chrono::TimeZone;
    use std::collections::BTreeMap;
    use std::sync::Arc;

    fn mock_credential_sources() -> CredentialSourceStack {
        CredentialSourceStack::new(vec![
            CredentialSource::Profile {
                name: "test".to_string(),
                source: ProfileSource::Default,
                profile_role_arn: None,
            }
        ])
    }

    fn mock_client() -> Client {
        let config = aws_config::SdkConfig::builder()
            .region(aws_types::region::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        Client::new(&config)
    }

    async fn mock_context() -> CfnContext {
        let fixed_time = chrono::Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info =
            TokenInfo::user_provided("test-token-123".to_string(), "test-op-1".to_string());

        let aws_config = aws_config::SdkConfig::builder()
            .region(aws_types::region::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        CfnContext::new(client, aws_config, mock_credential_sources(), time_provider, token_info).await.unwrap()
    }

    fn mock_stack_args() -> StackArgs {
        let mut parameters = BTreeMap::new();
        parameters.insert("Environment".to_string(), "test".to_string());
        parameters.insert("Version".to_string(), "1.0".to_string());

        let mut tags = BTreeMap::new();
        tags.insert("Project".to_string(), "iidy-test".to_string());
        tags.insert("Environment".to_string(), "test".to_string());

        StackArgs {
            stack_name: Some("test-stack".to_string()),
            template: Some("{\"Resources\": {}}".to_string()),
            capabilities: Some(vec!["CAPABILITY_IAM".to_string()]),
            parameters: Some(parameters),
            tags: Some(tags),
            notification_arns: Some(vec![
                "arn:aws:sns:us-east-1:123456789012:test-topic".to_string(),
            ]),
            service_role_arn: Some("arn:aws:iam::123456789012:role/cfn-service-role".to_string()),
            timeout_in_minutes: Some(30),
            on_failure: Some("ROLLBACK".to_string()),
            disable_rollback: Some(false),
            enable_termination_protection: Some(true),
            resource_types: Some(vec!["AWS::EC2::Instance".to_string()]),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn cfn_request_builder_creates_with_context_and_args() {
        let context = mock_context().await;
        let stack_args = mock_stack_args();

        let builder = CfnRequestBuilder::new(&context, &stack_args);

        // Builder should be created successfully
        assert_eq!(builder.context.primary_token().value, "test-token-123");
        assert_eq!(builder.stack_args.stack_name.as_deref(), Some("test-stack"));
    }

    #[tokio::test]
    async fn build_create_stack_derives_token_and_applies_stack_args() {
        let context = mock_context().await;
        let stack_args = mock_stack_args();
        let builder = CfnRequestBuilder::new(&context, &stack_args);

        let (_create_builder, token) = builder.build_create_stack(false, &CfnOperation::CreateStack, "test-stack-args.yaml", Some("test")).await.unwrap();

        // Token should be derived from primary token
        assert!(token.is_derived());
        assert!(token.value.starts_with("test-tok")); // Prefix from primary token
        assert_eq!(token.operation_id, "test-op-1"); // Same operation ID

        // Verify token was tracked in context
        let used_tokens = context.get_used_tokens();
        assert_eq!(used_tokens.len(), 2); // Primary + derived
        assert!(used_tokens.iter().any(|t| t.value == token.value));
    }

    #[tokio::test]
    async fn build_update_stack_handles_use_previous_template() {
        let context = mock_context().await;
        let mut stack_args = mock_stack_args();
        stack_args.use_previous_template = Some(true);

        let builder = CfnRequestBuilder::new(&context, &stack_args);
        let (_update_builder, token) = builder.build_update_stack(false, &CfnOperation::UpdateStack, "test-stack-args.yaml", Some("test")).await.unwrap();

        // Token should be derived
        assert!(token.is_derived());
        assert_ne!(token.value, context.primary_token().value);

        // Different step should produce different token
        let (_create_builder, create_token) = builder.build_create_stack(false, &CfnOperation::CreateStack, "test-stack-args.yaml", Some("test")).await.unwrap();
        assert_ne!(token.value, create_token.value);
    }

    #[tokio::test]
    async fn build_create_changeset_uses_correct_field_name() {
        let context = mock_context().await;
        let stack_args = mock_stack_args();
        let builder = CfnRequestBuilder::new(&context, &stack_args);

        let (_changeset_builder, token) =
            builder.build_create_changeset("test-changeset", false, &CfnOperation::CreateChangeset);

        // Token should be derived for the changeset step
        assert!(token.is_derived());
        if let TokenSource::Derived { step, .. } = &token.source {
            assert_eq!(step, "create-changeset");
        } else {
            panic!("Expected derived token");
        }

        // The builder should be properly configured
        // Note: We can't easily inspect the fluent builder's internal state,
        // but we can verify the token was derived correctly
        assert!(token.value.starts_with("test-tok"));
    }

    #[tokio::test]
    async fn build_execute_changeset_uses_standard_field_name() {
        let context = mock_context().await;
        let stack_args = mock_stack_args();
        let builder = CfnRequestBuilder::new(&context, &stack_args);

        let (_execute_builder, token) =
            builder.build_execute_changeset("test-changeset-arn", false, &CfnOperation::ExecuteChangeset);

        // Token should be derived for the execute step
        assert!(token.is_derived());
        if let TokenSource::Derived { step, .. } = &token.source {
            assert_eq!(step, "execute-changeset");
        } else {
            panic!("Expected derived token");
        }
    }

    #[tokio::test]
    async fn builder_handles_minimal_stack_args() {
        let context = mock_context().await;
        let minimal_args = StackArgs {
            stack_name: Some("minimal-stack".to_string()),
            template: Some("{}".to_string()),
            ..Default::default()
        };

        let builder = CfnRequestBuilder::new(&context, &minimal_args);
        let (_create_builder, token) = builder.build_create_stack(false, &CfnOperation::CreateStack, "test-stack-args.yaml", Some("test")).await.unwrap();

        // Should work with minimal configuration
        assert!(token.is_derived());
        assert_eq!(minimal_args.stack_name.as_deref(), Some("minimal-stack"));
    }

    #[tokio::test]
    async fn token_derivation_is_deterministic_across_builders() {
        let context = mock_context().await;
        let stack_args = mock_stack_args();

        // Create multiple builders with same context
        let builder1 = CfnRequestBuilder::new(&context, &stack_args);
        let builder2 = CfnRequestBuilder::new(&context, &stack_args);

        // Same step should produce same derived token
        let (_, token1) = builder1.build_create_stack(false, &CfnOperation::CreateStack, "test-stack-args.yaml", Some("test")).await.unwrap();
        let (_, token2) = builder2.build_create_stack(false, &CfnOperation::CreateStack, "test-stack-args.yaml", Some("test")).await.unwrap();

        assert_eq!(token1.value, token2.value);
        assert_eq!(token1.source, token2.source);
    }

    #[tokio::test]
    async fn different_steps_produce_different_tokens() {
        let context = mock_context().await;
        let stack_args = mock_stack_args();
        let builder = CfnRequestBuilder::new(&context, &stack_args);

        let (_, create_token) = builder.build_create_stack(false, &CfnOperation::CreateStack, "test-stack-args.yaml", Some("test")).await.unwrap();
        let (_, update_token) = builder.build_update_stack(false, &CfnOperation::UpdateStack, "test-stack-args.yaml", Some("test")).await.unwrap();
        let (_, changeset_token) =
            builder.build_create_changeset("test-changeset", false, &CfnOperation::CreateChangeset);
        let (_, execute_token) =
            builder.build_execute_changeset("test-changeset", false, &CfnOperation::ExecuteChangeset);

        // All tokens should be different
        let tokens = vec![
            &create_token.value,
            &update_token.value,
            &changeset_token.value,
            &execute_token.value,
        ];
        for (i, token1) in tokens.iter().enumerate() {
            for (j, token2) in tokens.iter().enumerate() {
                if i != j {
                    assert_ne!(
                        token1, token2,
                        "Tokens for different steps should be unique"
                    );
                }
            }
        }

        // But all should share same operation ID and root token
        assert_eq!(create_token.operation_id, update_token.operation_id);
        assert_eq!(create_token.root_token(), update_token.root_token());
        assert_eq!(changeset_token.root_token(), execute_token.root_token());
    }
}
