use anyhow::Result;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::time::Duration as StdDuration;

/// Provides time values for CloudFormation operations.
///
/// This trait allows for dependency injection of time sources,
/// enabling both reliable NTP-based timing in production and
/// deterministic timing in tests.
#[async_trait::async_trait]
pub trait TimeProvider: Send + Sync {
    /// Get the current time from this provider
    async fn now(&self) -> Result<DateTime<Utc>>;

    /// Get a "safe" start time for CloudFormation operations.
    /// This subtracts 500ms from current time to account for timing precision.
    async fn start_time(&self) -> Result<DateTime<Utc>> {
        let mut time = self.now().await?;
        time = time - chrono::Duration::milliseconds(500);
        Ok(time)
    }
}

/// Production time provider that attempts to use NTP for accuracy,
/// falling back to system time if NTP is unavailable.
///
/// This addresses clock drift issues during long-running CloudFormation
/// operations by providing network-synchronized time when possible.
pub struct ReliableTimeProvider {
    ntp_timeout: StdDuration,
}

impl ReliableTimeProvider {
    pub fn new() -> Self {
        Self {
            ntp_timeout: StdDuration::from_secs(2),
        }
    }

    async fn try_ntp(&self) -> Result<DateTime<Utc>> {
        let timeout = self.ntp_timeout;

        // Try NTP query with timeout
        let result = tokio::time::timeout(timeout, async move {
            // Use pool.ntp.org as in the original implementation
            let response = ntp::request("pool.ntp.org")
                .map_err(|e| anyhow::anyhow!("NTP request failed: {}", e))?;

            // Convert NTP timestamp to chrono DateTime
            // The ntp crate provides transmit_time field
            let ntp_time = response.transmit_time;

            // NTP uses seconds since 1900, we need to convert to Unix timestamp
            // NTP epoch is Jan 1, 1900, Unix epoch is Jan 1, 1970
            // Difference is 70 years = 2208988800 seconds
            const NTP_TO_UNIX_OFFSET: u32 = 2208988800;
            let unix_timestamp = ntp_time.sec.saturating_sub(NTP_TO_UNIX_OFFSET) as i64;

            // Convert fractional part to nanoseconds
            let nanos = ((ntp_time.frac as f64 / u32::MAX as f64) * 1_000_000_000.0) as u32;

            DateTime::from_timestamp(unix_timestamp, nanos)
                .ok_or_else(|| anyhow::anyhow!("Invalid NTP timestamp"))
        })
        .await;

        match result {
            Ok(Ok(time)) => Ok(time),
            Ok(Err(e)) => Err(e),
            Err(_) => Err(anyhow::anyhow!("NTP request timed out")),
        }
    }
}

impl Default for ReliableTimeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TimeProvider for ReliableTimeProvider {
    async fn now(&self) -> Result<DateTime<Utc>> {
        // First attempt: try NTP
        match self.try_ntp().await {
            Ok(time) => {
                log::debug!("Using NTP time: {}", time);
                Ok(time)
            }
            Err(e) => {
                log::debug!("NTP failed, retrying once: {}", e);
                // Second attempt: retry NTP once
                match self.try_ntp().await {
                    Ok(time) => {
                        log::debug!("Using NTP time (retry): {}", time);
                        Ok(time)
                    }
                    Err(e) => {
                        log::debug!("NTP retry failed, falling back to system time: {}", e);
                        // Fallback: use system time
                        Ok(Utc::now())
                    }
                }
            }
        }
    }
}

/// System time provider for read-only operations.
///
/// Uses system time without network synchronization for fast initialization.
/// Suitable for operations that don't require precise timing like describe-stack.
pub struct SystemTimeProvider;

impl SystemTimeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SystemTimeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl TimeProvider for SystemTimeProvider {
    async fn now(&self) -> Result<DateTime<Utc>> {
        Ok(Utc::now())
    }
}

/// Mock time provider for testing.
///
/// Provides deterministic time values for reproducible tests
/// without depending on external network services.
#[cfg(test)]
pub struct MockTimeProvider {
    pub fixed_time: DateTime<Utc>,
}

#[cfg(test)]
impl MockTimeProvider {
    pub fn new(time: DateTime<Utc>) -> Self {
        Self { fixed_time: time }
    }

    pub fn from_timestamp(timestamp: i64) -> Self {
        let time = DateTime::from_timestamp(timestamp, 0).unwrap_or_else(Utc::now);
        Self::new(time)
    }
}

#[cfg(test)]
#[async_trait::async_trait]
impl TimeProvider for MockTimeProvider {
    async fn now(&self) -> Result<DateTime<Utc>> {
        Ok(self.fixed_time)
    }
}

/// Information about a client request token, including its source and derivation.
#[derive(Debug, Clone, PartialEq)]
pub struct TokenInfo {
    pub value: String,
    pub source: TokenSource,
    pub operation_id: String,
}

/// Source of a client request token for traceability.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenSource {
    /// Token was explicitly provided by the user via CLI
    UserProvided,
    /// Token was automatically generated when not provided
    AutoGenerated,
    /// Token was derived from another token for a specific operation step
    Derived {
        from: String, // Original token value
        step: String, // Step name (e.g., "create-changeset")
    },
}

impl TokenInfo {
    /// Create a new TokenInfo with user-provided token
    pub fn user_provided(value: String, operation_id: String) -> Self {
        Self {
            value,
            source: TokenSource::UserProvided,
            operation_id,
        }
    }

    /// Create a new TokenInfo with auto-generated token
    pub fn auto_generated(value: String, operation_id: String) -> Self {
        Self {
            value,
            source: TokenSource::AutoGenerated,
            operation_id,
        }
    }

    /// Derive a new token from this one for a specific operation step.
    ///
    /// Uses deterministic hashing to ensure the same primary token always
    /// generates the same derived tokens for retryability.
    pub fn derive_for_step(&self, step: &str) -> TokenInfo {
        let derived_value = derive_token(&self.value, step);

        TokenInfo {
            value: derived_value,
            source: TokenSource::Derived {
                from: self.value.clone(),
                step: step.to_string(),
            },
            operation_id: self.operation_id.clone(),
        }
    }

    /// Get the lineage of this token for debugging and tracing.
    ///
    /// Returns a vector of strings describing the token's derivation chain.
    pub fn trace_lineage(&self) -> Vec<String> {
        match &self.source {
            TokenSource::UserProvided => {
                vec![format!("User-provided token: {}", self.value)]
            }
            TokenSource::AutoGenerated => {
                vec![format!("Auto-generated token: {}", self.value)]
            }
            TokenSource::Derived { from, step } => {
                vec![
                    format!("Primary token: {}", from),
                    format!("Derived for step '{}': {}", step, self.value),
                ]
            }
        }
    }

    /// Check if this token was derived from another token
    pub fn is_derived(&self) -> bool {
        matches!(self.source, TokenSource::Derived { .. })
    }

    /// Get the root token value (either this token or the one it was derived from)
    pub fn root_token(&self) -> &str {
        match &self.source {
            TokenSource::Derived { from, .. } => from,
            _ => &self.value,
        }
    }
}

/// Derive a deterministic sub-token from a primary token and step name.
///
/// Uses SHA256 hashing to ensure the same inputs always produce the same output,
/// enabling safe retries of multi-step operations.
fn derive_token(primary_token: &str, step: &str) -> String {
    // Create deterministic hash of primary token + step
    let mut hasher = Sha256::new();
    hasher.update(primary_token.as_bytes());
    hasher.update(step.as_bytes());
    let hash_result = hasher.finalize();

    // Convert to hex and take first 8 characters
    let hash_hex = format!("{:x}", hash_result);
    let hash_short = &hash_hex[..8];

    // Take first 8 characters of original token
    let primary_short = if primary_token.len() >= 8 {
        &primary_token[..8]
    } else {
        primary_token
    };

    // Format as: primary_prefix-hash_suffix
    format!("{}-{}", primary_short, hash_short)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[tokio::test]
    async fn mock_time_provider_returns_fixed_time() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let provider = MockTimeProvider::new(fixed_time);

        let result = provider.now().await.unwrap();
        assert_eq!(result, fixed_time);
    }

    #[tokio::test]
    async fn mock_time_provider_start_time_subtracts_500ms() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let provider = MockTimeProvider::new(fixed_time);

        let start_time = provider.start_time().await.unwrap();
        let expected = fixed_time - chrono::Duration::milliseconds(500);
        assert_eq!(start_time, expected);
    }

    #[tokio::test]
    async fn reliable_time_provider_fallback_works() {
        // Test that ReliableTimeProvider falls back to system time
        // when NTP is unavailable (we can't easily test NTP failure in unit tests
        // but we can verify the provider doesn't panic)
        let provider = ReliableTimeProvider::new();
        let result = provider.now().await;
        assert!(result.is_ok());
    }

    // TokenInfo tests

    #[test]
    fn token_info_user_provided() {
        let token = TokenInfo::user_provided("user-token-123".to_string(), "op-1".to_string());

        assert_eq!(token.value, "user-token-123");
        assert_eq!(token.operation_id, "op-1");
        assert!(matches!(token.source, TokenSource::UserProvided));
        assert!(!token.is_derived());
        assert_eq!(token.root_token(), "user-token-123");
    }

    #[test]
    fn token_info_auto_generated() {
        let token = TokenInfo::auto_generated("auto-token-456".to_string(), "op-2".to_string());

        assert_eq!(token.value, "auto-token-456");
        assert_eq!(token.operation_id, "op-2");
        assert!(matches!(token.source, TokenSource::AutoGenerated));
        assert!(!token.is_derived());
        assert_eq!(token.root_token(), "auto-token-456");
    }

    #[test]
    fn token_derivation_is_deterministic() {
        let primary = TokenInfo::user_provided(
            "abc123ef-4567-89ab-cdef-0123456789ab".to_string(),
            "op-1".to_string(),
        );

        // Derive the same token multiple times
        let derived1 = primary.derive_for_step("create-changeset");
        let derived2 = primary.derive_for_step("create-changeset");

        // Should be identical
        assert_eq!(derived1.value, derived2.value);
        assert_eq!(derived1.source, derived2.source);
        assert_eq!(derived1.operation_id, derived2.operation_id);
    }

    #[test]
    fn token_derivation_different_steps_produce_different_tokens() {
        let primary = TokenInfo::user_provided(
            "abc123ef-4567-89ab-cdef-0123456789ab".to_string(),
            "op-1".to_string(),
        );

        let create_token = primary.derive_for_step("create-changeset");
        let execute_token = primary.derive_for_step("execute-changeset");

        // Should be different values
        assert_ne!(create_token.value, execute_token.value);

        // But should both start with the primary token prefix
        assert!(create_token.value.starts_with("abc123ef"));
        assert!(execute_token.value.starts_with("abc123ef"));

        // And both should be derived from the same source
        if let TokenSource::Derived { from: from1, .. } = &create_token.source {
            if let TokenSource::Derived { from: from2, .. } = &execute_token.source {
                assert_eq!(from1, from2);
                assert_eq!(from1, &primary.value);
            } else {
                panic!("Execute token should be derived");
            }
        } else {
            panic!("Create token should be derived");
        }
    }

    #[test]
    fn derived_token_properties() {
        let primary =
            TokenInfo::user_provided("test-primary-token".to_string(), "op-1".to_string());
        let derived = primary.derive_for_step("test-step");

        assert!(derived.is_derived());
        assert_eq!(derived.root_token(), "test-primary-token");
        assert_eq!(derived.operation_id, "op-1"); // Should inherit operation ID

        if let TokenSource::Derived { from, step } = &derived.source {
            assert_eq!(from, "test-primary-token");
            assert_eq!(step, "test-step");
        } else {
            panic!("Token should be derived");
        }
    }

    #[test]
    fn token_trace_lineage() {
        // Test user-provided token lineage
        let user_token = TokenInfo::user_provided("user-token".to_string(), "op-1".to_string());
        let user_lineage = user_token.trace_lineage();
        assert_eq!(user_lineage.len(), 1);
        assert!(user_lineage[0].contains("User-provided token: user-token"));

        // Test auto-generated token lineage
        let auto_token = TokenInfo::auto_generated("auto-token".to_string(), "op-2".to_string());
        let auto_lineage = auto_token.trace_lineage();
        assert_eq!(auto_lineage.len(), 1);
        assert!(auto_lineage[0].contains("Auto-generated token: auto-token"));

        // Test derived token lineage
        let primary = TokenInfo::user_provided("primary-token".to_string(), "op-3".to_string());
        let derived = primary.derive_for_step("test-step");
        let derived_lineage = derived.trace_lineage();
        assert_eq!(derived_lineage.len(), 2);
        assert!(derived_lineage[0].contains("Primary token: primary-token"));
        assert!(derived_lineage[1].contains("Derived for step 'test-step'"));
    }

    #[test]
    fn derive_token_function_is_deterministic() {
        let primary = "test-token-123";
        let step = "create-changeset";

        let result1 = derive_token(primary, step);
        let result2 = derive_token(primary, step);

        assert_eq!(result1, result2);
    }

    #[test]
    fn derive_token_format() {
        let primary = "abc123ef-4567-89ab-cdef-0123456789ab";
        let step = "create-changeset";

        let result = derive_token(primary, step);

        // Should start with first 8 chars of primary
        assert!(result.starts_with("abc123ef-"));

        // Should have format: prefix-hash
        let parts: Vec<&str> = result.split('-').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0], "abc123ef");
        assert_eq!(parts[1].len(), 8); // Hash should be 8 hex characters

        // Hash part should be valid hex
        assert!(parts[1].chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn derive_token_with_short_primary() {
        let primary = "short";
        let step = "test";

        let result = derive_token(primary, step);

        // Should start with the full primary token since it's less than 8 chars
        assert!(result.starts_with("short-"));

        let parts: Vec<&str> = result.split('-').collect();
        assert_eq!(parts[0], "short");
        assert_eq!(parts[1].len(), 8);
    }

    #[test]
    fn derive_token_different_inputs_produce_different_outputs() {
        let token1 = derive_token("token1", "step1");
        let token2 = derive_token("token1", "step2");
        let token3 = derive_token("token2", "step1");

        // All should be different
        assert_ne!(token1, token2);
        assert_ne!(token1, token3);
        assert_ne!(token2, token3);
    }
}
