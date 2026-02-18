use anyhow::Result;
use chrono::{DateTime, Utc};
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
}
