use anyhow::Result;
use aws_sdk_cloudformation::Client;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};

use crate::cli::NormalizedAwsOpts;
use crate::timing::{ReliableTimeProvider, TimeProvider, TokenInfo};

// CloudFormation operation modules
pub mod console;
pub mod create_changeset;
pub mod create_or_update;
pub mod create_stack;
pub mod delete_stack;
pub mod describe_stack;
pub mod describe_stack_drift;
pub mod estimate_cost;
pub mod exec_changeset;
pub mod get_stack_instances;
pub mod get_stack_template;
pub mod is_terminal_status;
pub mod list_stacks;
pub mod request_builder;
pub mod update_stack;
pub mod watch_stack;

// Re-exports
pub use console::ConsoleReporter;
pub use request_builder::CfnRequestBuilder;

/// Create a CfnContext from NormalizedAwsOpts, eliminating duplicate setup code.
///
/// This helper function centralizes the common pattern of creating AWS config,
/// client, time provider, and context that appears in most CloudFormation operations.
///
/// # Arguments
/// * `opts` - The normalized AWS options containing region and token info
///
/// # Returns
/// A fully initialized CfnContext ready for CloudFormation operations
pub async fn create_context(opts: &NormalizedAwsOpts) -> Result<CfnContext> {
    let config = crate::aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    CfnContext::new(client, time_provider, opts.client_request_token.clone()).await
}

/// Context object that carries shared state for CloudFormation operations.
///
/// This includes the AWS client, timing provider for reliable timestamps,
/// the operation start time for event filtering, and token management for
/// multi-step operations with idempotency support.
pub struct CfnContext {
    pub client: Client,
    pub time_provider: Arc<dyn TimeProvider>,
    pub start_time: Option<DateTime<Utc>>,
    pub token_info: TokenInfo,
    pub used_tokens: Arc<Mutex<Vec<TokenInfo>>>,
}

impl CfnContext {
    /// Create a new CFN context with the given client, time provider, and token info.
    ///
    /// The start time is automatically set using the time provider's start_time() method.
    /// The primary token is automatically added to the used_tokens tracking.
    pub async fn new(
        client: Client,
        time_provider: Arc<dyn TimeProvider>,
        token_info: TokenInfo,
    ) -> Result<Self> {
        let start_time = Some(time_provider.start_time().await?);
        let used_tokens = Arc::new(Mutex::new(vec![token_info.clone()]));

        Ok(CfnContext {
            client,
            time_provider,
            start_time,
            token_info,
            used_tokens,
        })
    }

    /// Create a new CFN context without setting a start time.
    ///
    /// Useful for operations that don't need event filtering.
    /// The primary token is automatically added to the used_tokens tracking.
    pub fn new_without_start_time(
        client: Client,
        time_provider: Arc<dyn TimeProvider>,
        token_info: TokenInfo,
    ) -> Self {
        let used_tokens = Arc::new(Mutex::new(vec![token_info.clone()]));

        CfnContext {
            client,
            time_provider,
            start_time: None,
            token_info,
            used_tokens,
        }
    }

    /// Get the start time for this context, or current time if not set.
    pub async fn get_start_time(&self) -> Result<DateTime<Utc>> {
        match self.start_time {
            Some(time) => Ok(time),
            None => self.time_provider.start_time().await,
        }
    }

    /// Calculate elapsed seconds since the start time.
    pub async fn elapsed_seconds(&self) -> Result<i64> {
        let start = self.get_start_time().await?;
        let now = self.time_provider.now().await?;
        Ok((now - start).num_seconds())
    }

    /// Derive a new token from the primary token for a specific operation step.
    ///
    /// This method creates a deterministic sub-token that can be safely used for
    /// multi-step operations. The derived token is automatically tracked in the
    /// used_tokens list for audit purposes.
    ///
    /// # Arguments
    /// * `step` - The name of the operation step (e.g., "create-changeset", "execute-changeset")
    ///
    /// # Returns
    /// A new TokenInfo with a derived token value that is deterministically generated
    /// from the primary token and step name.
    pub fn derive_token_for_step(&self, step: &str) -> TokenInfo {
        let derived = self.token_info.derive_for_step(step);

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
    pub fn has_derived_tokens(&self) -> bool {
        match self.used_tokens.lock() {
            Ok(tokens) => tokens.len() > 1, // More than just the primary token
            Err(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::{MockTimeProvider, TokenInfo};
    use chrono::TimeZone;

    fn mock_client() -> Client {
        // Create a mock client for testing
        // In real tests, you'd use a proper mock or test configuration
        let config = aws_config::SdkConfig::builder()
            .region(aws_types::region::Region::new("us-east-1"))
            .behavior_version(aws_config::BehaviorVersion::latest())
            .build();
        Client::new(&config)
    }

    fn mock_token_info() -> TokenInfo {
        TokenInfo::user_provided("test-token-123".to_string(), "test-op-1".to_string())
    }

    #[tokio::test]
    async fn cfn_context_sets_start_time() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let ctx = CfnContext::new(client, time_provider, token_info)
            .await
            .unwrap();

        assert!(ctx.start_time.is_some());
        let expected_start = fixed_time - chrono::Duration::milliseconds(500);
        assert_eq!(ctx.start_time.unwrap(), expected_start);
    }

    #[tokio::test]
    async fn cfn_context_calculates_elapsed_time() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let mut ctx = CfnContext::new(client, time_provider.clone(), token_info)
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

        let ctx = CfnContext::new(client, time_provider, token_info.clone())
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

    #[test]
    fn cfn_context_derives_tokens_for_steps() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let ctx = CfnContext::new_without_start_time(client, time_provider, token_info);

        // Derive tokens for different steps
        let create_token = ctx.derive_token_for_step("create-changeset");
        let execute_token = ctx.derive_token_for_step("execute-changeset");

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

    #[test]
    fn cfn_context_token_derivation_is_deterministic() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let ctx = CfnContext::new_without_start_time(client, time_provider, token_info);

        // Derive the same token multiple times
        let token1 = ctx.derive_token_for_step("test-step");
        let token2 = ctx.derive_token_for_step("test-step");

        // Should be identical
        assert_eq!(token1.value, token2.value);
        assert_eq!(token1.source, token2.source);
        assert_eq!(token1.operation_id, token2.operation_id);

        // But both should be tracked
        let used_tokens = ctx.get_used_tokens();
        assert_eq!(used_tokens.len(), 3); // Primary + 2 identical derived tokens
    }

    #[test]
    fn cfn_context_without_start_time_works() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        let token_info = mock_token_info();

        let ctx = CfnContext::new_without_start_time(client, time_provider, token_info.clone());

        // Should not have start time set
        assert!(ctx.start_time.is_none());

        // But should still have token tracking
        assert_eq!(ctx.primary_token().value, "test-token-123");
        let used_tokens = ctx.get_used_tokens();
        assert_eq!(used_tokens.len(), 1);
    }
}
