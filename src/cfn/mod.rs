use anyhow::Result;
use aws_sdk_cloudformation::Client;
use aws_sdk_s3::Client as S3Client;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

use crate::{
    aws::{
        CredentialSourceStack,
        client_req_token::TokenInfo,
        config_from_normalized_opts,
        timing::{ReliableTimeProvider, SystemTimeProvider, TimeProvider},
    },
    cli::NormalizedAwsOpts,
};

/// Macro to consistently await tasks and handle errors via the output system
///
/// This macro reduces repetition in command handlers by providing standardized
/// error handling for parallel AWS API tasks. It renders errors through the
/// output system and returns appropriate exit codes.
///
/// # Usage
/// ```text
/// let task = tokio::spawn(async { /* AWS API call */ });
/// await_and_render!(task, output_manager);
/// ```
#[macro_export]
macro_rules! await_and_render {
    ($task:expr, $output_manager:expr) => {
        match $task.await {
            Ok(Ok(data)) => $output_manager.render(data).await?,
            Ok(Err(error)) => {
                let error_info =
                    $crate::output::aws_conversion::convert_aws_error_to_error_info(&error, None)
                        .await;
                $output_manager
                    .render($crate::output::OutputData::Error(error_info))
                    .await?;
                return Ok(1);
            }
            Err(join_error) => {
                let error_info = $crate::output::aws_conversion::convert_aws_error_to_error_info(
                    &join_error.into(),
                    None,
                )
                .await;
                $output_manager
                    .render($crate::output::OutputData::Error(error_info))
                    .await?;
                return Ok(1);
            }
        }
    };
}

/// Macro to run a command handler with automatic setup and error handling
///
/// This macro handles:
/// 1. Normalizing AWS options
/// 2. Creating the output manager
/// 3. Creating the AWS context (with error handling)
/// 4. Running the implementation function
/// 5. Converting and rendering any errors
///
/// # Usage
/// ```text
/// pub async fn my_command(cli: &Cli, args: &MyArgs) -> Result<i32> {
///     crate::run_command_handler!(my_command_impl, cli, args)
/// }
/// ```
#[macro_export]
macro_rules! run_command_handler {
    ($impl_fn:ident, $cli:expr, $args:expr) => {{
        let opts = $cli.aws_opts.clone().normalize();

        let output_options = $crate::output::manager::OutputOptions::new($cli.clone());
        let mut output_manager = $crate::output::DynamicOutputManager::new(
            $cli.global_opts.effective_output_mode(),
            output_options,
        )
        .await?;

        let operation = $cli.command.to_cfn_operation();
        let context = match $crate::cfn::create_context_for_operation(&opts, operation).await {
            Ok(ctx) => ctx,
            Err(error) => {
                let error_info =
                    $crate::output::aws_conversion::convert_aws_error_to_error_info(&error, None)
                        .await;
                output_manager
                    .render($crate::output::OutputData::Error(error_info))
                    .await?;
                return Ok(1);
            }
        };

        match $impl_fn(&mut output_manager, &context, $cli, $args, &opts).await {
            Ok(exit_code) => Ok(exit_code),
            Err(error) => {
                let error_info = $crate::output::aws_conversion::convert_aws_error_to_error_info(
                    &error,
                    Some((&context, $cli)),
                )
                .await;
                output_manager
                    .render($crate::output::OutputData::Error(error_info))
                    .await?;
                Ok(1)
            }
        }
    }};
}

/// Macro to run a command handler that requires stack args with automatic setup and error handling
///
/// This macro handles:
/// 1. Normalizing AWS options
/// 2. Creating the output manager
/// 3. Loading stack args with merged AWS config (CLI + stack-args.yaml)
/// 4. Creating the AWS context from merged config (with error handling)
/// 5. Running the implementation function with both context and stack_args
/// 6. Converting and rendering any errors
///
/// # Usage
/// ```text
/// pub async fn create_stack(cli: &Cli, args: &CreateStackArgs) -> Result<i32> {
///     crate::run_command_handler_with_stack_args!(create_stack_impl, cli, args, args.argsfile)
/// }
/// ```
#[macro_export]
macro_rules! run_command_handler_with_stack_args {
    ($impl_fn:ident, $cli:expr, $args:expr, $argsfile:expr) => {{
        let opts = $cli.aws_opts.clone().normalize();

        let output_options = $crate::output::manager::OutputOptions::new($cli.clone());
        let mut output_manager = $crate::output::DynamicOutputManager::new(
            $cli.global_opts.effective_output_mode(),
            output_options,
        )
        .await?;

        let operation = $cli.command.to_cfn_operation();

        // Load stack args with merged AWS settings (CLI + stack-args.yaml)
        let cli_aws_settings = $crate::aws::AwsSettings::from_normalized_opts(&opts);
        let (stack_args, aws_config, credential_sources) =
            match $crate::cfn::stack_args::load_stack_args(
                $argsfile,
                &$cli.global_opts.environment,
                &operation,
                &cli_aws_settings,
            )
            .await
            {
                Ok(result) => result,
                Err(error) => {
                    let error_info =
                        $crate::output::aws_conversion::convert_aws_error_to_error_info(
                            &error, None,
                        )
                        .await;
                    output_manager
                        .render($crate::output::OutputData::Error(error_info))
                        .await?;
                    return Ok(1);
                }
            };

        let context = match $crate::cfn::create_context_from_config(
            aws_config,
            credential_sources,
            operation,
            opts.client_request_token.clone(),
        )
        .await
        {
            Ok(ctx) => ctx,
            Err(error) => {
                let error_info =
                    $crate::output::aws_conversion::convert_aws_error_to_error_info(&error, None)
                        .await;
                output_manager
                    .render($crate::output::OutputData::Error(error_info))
                    .await?;
                return Ok(1);
            }
        };

        match $impl_fn(
            &mut output_manager,
            &context,
            $cli,
            $args,
            &opts,
            &stack_args,
        )
        .await
        {
            Ok(exit_code) => Ok(exit_code),
            Err(error) => {
                let error_info = $crate::output::aws_conversion::convert_aws_error_to_error_info(
                    &error,
                    Some((&context, $cli)),
                )
                .await;
                output_manager
                    .render($crate::output::OutputData::Error(error_info))
                    .await?;
                Ok(1)
            }
        }
    }};
}

// CloudFormation operation modules
// pub mod console; // REMOVED: Legacy direct output - replaced by data-driven output architecture
pub mod changeset_operations; // Shared changeset functionality
pub mod constants;
pub mod convert_stack_to_iidy;
pub mod create_changeset;
pub mod create_or_update;
pub mod create_stack;
pub mod delete_stack;
pub mod describe_stack;
pub mod describe_stack_drift;
pub mod error_handling;
pub mod estimate_cost;
pub mod exec_changeset;
pub mod get_import;
pub mod get_stack_instances;
pub mod get_stack_template;
pub mod init_stack_args;
pub mod is_terminal_status;
pub mod list_stacks;
pub mod operations;
pub mod request_builder;
pub mod s3_utils;
pub mod stack_args;
pub mod stack_change_type;
pub mod stack_operations;
pub mod template_approval_request;
pub mod template_approval_review;
pub mod template_hash;
pub mod template_loader;
pub mod update_stack;
pub mod watch_stack;

// Re-exports
pub use operations::CfnOperation;
pub use request_builder::CfnRequestBuilder;
pub use stack_args::StackArgs;
pub use stack_change_type::{StackChangeType, UpdateResult};
pub use template_loader::{
    StackPolicyResult, TemplateResult, load_cfn_stack_policy, load_cfn_template,
};

/// Create a CfnContext from an existing AWS config with operation-aware time provider selection.
///
/// This helper is used when the AWS config has already been created (e.g., from merged
/// CLI + stack-args.yaml settings). It ensures that the same config used for preprocessing
/// is also used for the CloudFormation client.
///
/// # Arguments
/// * `aws_config` - Pre-configured AWS SDK config
/// * `operation` - The CloudFormation operation to determine time provider needs
/// * `client_request_token` - Optional client request token for idempotency
///
/// # Returns
/// A fully initialized CfnContext ready for CloudFormation operations
pub async fn create_context_from_config(
    aws_config: aws_config::SdkConfig,
    credential_sources: CredentialSourceStack,
    operation: CfnOperation,
    client_request_token: TokenInfo,
) -> Result<CfnContext> {
    // Validate that a region is configured before proceeding
    if aws_config.region().is_none() {
        anyhow::bail!(
            "No AWS region configured. Please specify a region via:\n\
             - CLI flag: --region us-east-1\n\
             - Stack args: Region: us-east-1\n\
             - Environment variable: AWS_REGION or AWS_DEFAULT_REGION\n\
             - AWS config file: ~/.aws/config"
        );
    }

    let client = Client::new(&aws_config);
    let time_provider: Arc<dyn TimeProvider> = if operation.is_read_only() {
        Arc::new(SystemTimeProvider::new())
    } else {
        Arc::new(ReliableTimeProvider::new())
    };
    CfnContext::new(
        client,
        aws_config,
        credential_sources,
        time_provider,
        client_request_token,
    )
    .await
}

/// Create a CfnContext from NormalizedAwsOpts with operation-aware time provider selection.
///
/// This helper function centralizes the common pattern of creating AWS config,
/// client, time provider, and context that appears in most CloudFormation operations.
/// Automatically uses system time for read-only operations and NTP for write operations.
///
/// # Arguments
/// * `opts` - The normalized AWS options containing region and token info
/// * `operation` - The CloudFormation operation to determine time provider needs
///
/// # Returns
/// A fully initialized CfnContext ready for CloudFormation operations
pub async fn create_context_for_operation(
    opts: &NormalizedAwsOpts,
    operation: CfnOperation,
) -> Result<CfnContext> {
    let (config, credential_sources) = config_from_normalized_opts(opts).await?;

    // Validate that a region is configured before proceeding
    if config.region().is_none() {
        anyhow::bail!(
            "No AWS region configured. Please specify a region via:\n\
             - CLI flag: --region us-east-1\n\
             - Stack args: Region: us-east-1\n\
             - Environment variable: AWS_REGION or AWS_DEFAULT_REGION\n\
             - AWS config file: ~/.aws/config"
        );
    }

    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = if operation.is_read_only() {
        Arc::new(SystemTimeProvider::new())
    } else {
        Arc::new(ReliableTimeProvider::new())
    };
    CfnContext::new(
        client,
        config,
        credential_sources,
        time_provider,
        opts.client_request_token.clone(),
    )
    .await
}

/// Create a CfnContext from NormalizedAwsOpts, eliminating duplicate setup code.
///
/// This helper function centralizes the common pattern of creating AWS config,
/// client, time provider, and context that appears in most CloudFormation operations.
///
/// # Arguments
/// * `opts` - The normalized AWS options containing region and token info
/// * `need_ntp_sync` - Whether to use NTP time sync (true for write operations, false for read-only)
///
/// # Returns
/// A fully initialized CfnContext ready for CloudFormation operations
pub async fn create_context(opts: &NormalizedAwsOpts, need_ntp_sync: bool) -> Result<CfnContext> {
    let (config, credential_sources) = config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = if need_ntp_sync {
        Arc::new(ReliableTimeProvider::new())
    } else {
        Arc::new(SystemTimeProvider::new())
    };
    CfnContext::new(
        client,
        config,
        credential_sources,
        time_provider,
        opts.client_request_token.clone(),
    )
    .await
}

/// Context object that carries shared state for CloudFormation operations.
///
/// This includes the AWS client, timing provider for reliable timestamps,
/// the operation start time for event filtering, and token management for
/// multi-step operations with idempotency support.
#[derive(Clone)]
pub struct CfnContext {
    pub client: Client,
    pub aws_config: aws_config::SdkConfig,
    pub credential_sources: CredentialSourceStack,
    pub time_provider: Arc<dyn TimeProvider>,
    pub start_time: DateTime<Utc>,
    pub token_info: TokenInfo,
    pub used_tokens: Arc<Mutex<Vec<TokenInfo>>>,
}

impl CfnContext {
    /// Create an S3 client on demand using the stored AWS config
    pub fn create_s3_client(&self) -> S3Client {
        S3Client::from_conf(
            aws_sdk_s3::Config::from(&self.aws_config)
                .to_builder()
                .behavior_version_latest()
                .build(),
        )
    }
    /// Create a new CFN context with the given client, time provider, and token info.
    ///
    /// The start time is automatically set using the time provider's start_time() method.
    /// The primary token is automatically added to the used_tokens tracking.
    pub async fn new(
        client: Client,
        aws_config: aws_config::SdkConfig,
        credential_sources: CredentialSourceStack,
        time_provider: Arc<dyn TimeProvider>,
        token_info: TokenInfo,
    ) -> Result<Self> {
        let start_time = time_provider.start_time().await?;
        let used_tokens = Arc::new(Mutex::new(vec![token_info.clone()]));

        Ok(CfnContext {
            client,
            aws_config,
            credential_sources,
            time_provider,
            start_time,
            token_info,
            used_tokens,
        })
    }

    /// Get the start time for this context.
    pub async fn get_start_time(&self) -> Result<DateTime<Utc>> {
        Ok(self.start_time)
    }

    /// Calculate elapsed seconds since the start time.
    pub async fn elapsed_seconds(&self) -> Result<i64> {
        let start = self.get_start_time().await?;
        let now = self.time_provider.now().await?;
        Ok((now - start).num_seconds())
    }

    /// Derive a new token from the primary token for a specific CloudFormation operation.
    ///
    /// This method creates a deterministic sub-token that can be safely used for
    /// multi-step operations. The derived token is automatically tracked in the
    /// used_tokens list for audit purposes.
    ///
    /// # Arguments
    /// * `operation` - The CloudFormation operation for token derivation
    ///
    /// # Returns
    /// A new TokenInfo with a derived token value that is deterministically generated
    /// from the primary token and operation name.
    pub fn derive_token_for_step(&self, operation: &CfnOperation) -> TokenInfo {
        let derived = self.token_info.derive_for_step(operation.as_str());

        // Track the derived token for audit trail
        if let Ok(mut used) = self.used_tokens.lock() {
            used.push(derived.clone());
        }

        derived
    }

    /// Get a snapshot of all tokens that have been used in this context.
    ///
    /// This includes the primary token and any derived tokens that have been
    /// created via derive_token_for_step(). Useful for logging, debugging,
    /// and generating operation summaries.
    ///
    /// # Returns
    /// A vector containing copies of all TokenInfo objects that have been used.
    /// Returns an empty vector if the mutex cannot be locked.
    pub fn get_used_tokens(&self) -> Vec<TokenInfo> {
        match self.used_tokens.lock() {
            Ok(tokens) => tokens.clone(),
            Err(_) => {
                // Log warning about mutex poisoning, but don't fail the operation
                log::warn!("Failed to lock used_tokens mutex for reading");
                vec![]
            }
        }
    }

    /// Get the primary token info for this context.
    ///
    /// This is the original token (either user-provided or auto-generated)
    /// that was used to create this context.
    pub fn primary_token(&self) -> &TokenInfo {
        &self.token_info
    }

    /// Check if any derived tokens have been created in this context.
    ///
    /// # Returns
    /// True if derive_token_for_step() has been called at least once, false otherwise.
    #[cfg(test)]
    pub fn has_derived_tokens(&self) -> bool {
        match self.used_tokens.lock() {
            Ok(tokens) => tokens.len() > 1, // More than just the primary token
            Err(_) => false,
        }
    }
}

// Success state determination for CloudFormation operations
// Centralizes the common pattern of checking if an operation succeeded

/// Constants for expected success states for each CloudFormation operation
pub const CREATE_SUCCESS_STATES: &[&str] = &["CREATE_COMPLETE"];
pub const UPDATE_SUCCESS_STATES: &[&str] = &["UPDATE_COMPLETE"];
pub const DELETE_SUCCESS_STATES: &[&str] = &["DELETE_COMPLETE"];

/// Determine if a CloudFormation operation succeeded based on its final status.
///
/// This helper function centralizes the common pattern across handlers that
/// check if a final stack status indicates successful completion of the operation.
///
/// # Arguments
/// * `final_status` - The final stack status from the CloudFormation operation
/// * `expected_states` - Array of status strings that indicate success
///
/// # Returns
/// * `true` if the final status matches one of the expected success states
/// * `false` if no status is available or the status doesn't match success states
///
/// # Example
/// ```text
/// let success = determine_operation_success(&final_status, CREATE_SUCCESS_STATES);
/// ```
pub fn determine_operation_success(
    final_status: &Option<String>,
    expected_states: &[&str],
) -> bool {
    final_status
        .as_ref()
        .map(|status| expected_states.contains(&status.as_str()))
        .unwrap_or(false)
}

/// Apply stack name override from CLI and validate that a stack name is present.
///
/// This helper function centralizes the common pattern across handlers that
/// override the stack name from the CLI argument if provided, and then validate
/// that a stack name is available (either from stack-args.yaml or CLI).
///
/// # Arguments
/// * `stack_args` - The loaded stack arguments from the YAML file
/// * `cli_stack_name` - Optional stack name override from CLI arguments
///
/// # Returns
/// * `Ok(StackArgs)` with the final stack arguments including any CLI override
/// * `Err` if no stack name is available after override and validation
///
/// # Example
/// ```text
/// let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.base.stack_name.as_ref())?;
/// ```
// TODO factor out
pub fn apply_stack_name_override_and_validate(
    mut stack_args: StackArgs,
    cli_stack_name: Option<&String>,
) -> Result<StackArgs> {
    if let Some(stack_name) = cli_stack_name {
        stack_args.stack_name = Some(stack_name.clone());
    }

    if stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }

    Ok(stack_args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aws::{client_req_token::TokenInfo, timing::MockTimeProvider};
    use chrono::TimeZone;

    fn create_test_aws_config() -> aws_config::SdkConfig {
        aws_config::SdkConfig::builder()
            .region(aws_types::region::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build()
    }

    fn mock_client() -> Client {
        // Create a mock client for testing
        // In real tests, you'd use a proper mock or test configuration
        Client::new(&create_test_aws_config())
    }

    fn mock_token_info() -> TokenInfo {
        TokenInfo::user_provided("test-token-123".to_string(), "test-op-1".to_string())
    }

    fn mock_credential_sources() -> CredentialSourceStack {
        use crate::aws::{CredentialSource, ProfileSource};
        CredentialSourceStack::new(vec![CredentialSource::Profile {
            name: "test".to_string(),
            source: ProfileSource::Default,
            profile_role_arn: None,
        }])
    }

    #[tokio::test]
    async fn cfn_context_sets_start_time() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let ctx = CfnContext::new(
            client,
            create_test_aws_config(),
            mock_credential_sources(),
            time_provider,
            token_info,
        )
        .await
        .unwrap();

        let expected_start = fixed_time - chrono::Duration::milliseconds(500);
        assert_eq!(ctx.start_time, expected_start);
    }

    #[tokio::test]
    async fn cfn_context_calculates_elapsed_time() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let mut ctx = CfnContext::new(
            client,
            create_test_aws_config(),
            mock_credential_sources(),
            time_provider.clone(),
            token_info,
        )
        .await
        .unwrap();

        // Simulate time passing by updating the mock provider's time
        let later_time = fixed_time + chrono::Duration::seconds(30);
        ctx.time_provider = Arc::new(MockTimeProvider::new(later_time));

        let elapsed = ctx.elapsed_seconds().await.unwrap();
        assert_eq!(elapsed, 30); // 30 seconds + 500ms from start_time adjustment
    }

    #[tokio::test]
    async fn cfn_context_tracks_primary_token() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let ctx = CfnContext::new(
            client,
            create_test_aws_config(),
            mock_credential_sources(),
            time_provider,
            token_info.clone(),
        )
        .await
        .unwrap();

        // Primary token should be accessible
        assert_eq!(ctx.primary_token().value, "test-token-123");
        assert_eq!(ctx.primary_token().operation_id, "test-op-1");

        // Primary token should be in used_tokens
        let used_tokens = ctx.get_used_tokens();
        assert_eq!(used_tokens.len(), 1);
        assert_eq!(used_tokens[0].value, "test-token-123");

        // No derived tokens yet
        assert!(!ctx.has_derived_tokens());
    }

    #[tokio::test]
    async fn cfn_context_derives_tokens_for_steps() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let ctx = CfnContext::new(
            client,
            create_test_aws_config(),
            mock_credential_sources(),
            time_provider,
            token_info,
        )
        .await
        .unwrap();

        // Derive tokens for different steps
        let create_token = ctx.derive_token_for_step(&CfnOperation::CreateChangeset);
        let execute_token = ctx.derive_token_for_step(&CfnOperation::ExecuteChangeset);

        // Tokens should be different
        assert_ne!(create_token.value, execute_token.value);
        assert_ne!(create_token.value, ctx.primary_token().value);
        assert_ne!(execute_token.value, ctx.primary_token().value);

        // Both should start with the primary token prefix
        assert!(create_token.value.starts_with("test-tok"));
        assert!(execute_token.value.starts_with("test-tok"));

        // Should be marked as derived
        assert!(create_token.is_derived());
        assert!(execute_token.is_derived());

        // Should have same operation ID as primary
        assert_eq!(create_token.operation_id, ctx.primary_token().operation_id);
        assert_eq!(execute_token.operation_id, ctx.primary_token().operation_id);

        // Context should track all tokens
        let used_tokens = ctx.get_used_tokens();
        assert_eq!(used_tokens.len(), 3); // Primary + 2 derived
        assert!(ctx.has_derived_tokens());
    }

    #[tokio::test]
    async fn cfn_context_token_derivation_is_deterministic() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let ctx = CfnContext::new(
            client,
            create_test_aws_config(),
            mock_credential_sources(),
            time_provider,
            token_info,
        )
        .await
        .unwrap();

        // Derive the same token multiple times
        let token1 = ctx.derive_token_for_step(&CfnOperation::CreateChangeset);
        let token2 = ctx.derive_token_for_step(&CfnOperation::CreateChangeset);

        // Should be identical
        assert_eq!(token1.value, token2.value);
        assert_eq!(token1.source, token2.source);
        assert_eq!(token1.operation_id, token2.operation_id);

        // But both should be tracked
        let used_tokens = ctx.get_used_tokens();
        assert_eq!(used_tokens.len(), 3); // Primary + 2 identical derived tokens
    }
}
