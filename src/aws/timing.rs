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
        time -= chrono::Duration::milliseconds(500);
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
        self.try_ntp_at(("pool.ntp.org", 123)).await
    }

    async fn try_ntp_at(&self, server: (&str, u16)) -> Result<DateTime<Utc>> {
        let timeout = self.ntp_timeout;

        // Try NTP query with timeout
        let result = tokio::time::timeout(timeout, async move {
            // Resolve and query asynchronously so the timeout covers DNS and UDP I/O.
            let addr = tokio::net::lookup_host(server)
                .await?
                .next()
                .ok_or_else(|| anyhow::anyhow!("No NTP server address found"))?;
            let bind_addr = if addr.is_ipv4() {
                "0.0.0.0:0"
            } else {
                "[::]:0"
            };
            let socket = tokio::net::UdpSocket::bind(bind_addr).await?;
            let socket = sntpc_net_tokio::UdpSocketWrapper::new(socket);
            let context = sntpc::NtpContext::new(sntpc::StdTimestampGen::default());
            let response = sntpc::get_time(addr, &socket, context)
                .await
                .map_err(|e| anyhow::anyhow!("NTP request failed: {:?}", e))?;

            // sntpc returns Unix seconds and a fraction in units of 2^-32 seconds.
            let unix_timestamp = i64::try_from(response.sec())?;
            let nanos = ((u64::from(response.sec_fraction()) * 1_000_000_000) >> 32) as u32;

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
                log::debug!("Using NTP time: {time}");
                Ok(time)
            }
            Err(e) => {
                log::debug!("NTP failed, retrying once: {e}");
                // Second attempt: retry NTP once
                match self.try_ntp().await {
                    Ok(time) => {
                        log::debug!("Using NTP time (retry): {time}");
                        Ok(time)
                    }
                    Err(e) => {
                        log::debug!("NTP retry failed, falling back to system time: {e}");
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
    async fn ntp_response_preserves_unix_seconds_and_fraction() {
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();
        let server_task = tokio::spawn(async move {
            let mut request = [0; 48];
            let (_, peer) = server.recv_from(&mut request).await.unwrap();
            let mut response = [0; 48];
            response[0] = (request[0] & 0x38) | 4; // Same NTP version, server mode.
            response[1] = 1; // Synchronized primary server.
            response[24..32].copy_from_slice(&request[40..48]); // Originate timestamp.
            let seconds = (1_700_000_000u32 + 2_208_988_800).to_be_bytes();
            response[32..36].copy_from_slice(&seconds);
            response[40..44].copy_from_slice(&seconds);
            response[44..48].copy_from_slice(&0x8000_0000u32.to_be_bytes());
            server.send_to(&response, peer).await.unwrap();
        });

        let time = ReliableTimeProvider::new()
            .try_ntp_at(("127.0.0.1", port))
            .await
            .unwrap();
        assert_eq!(time.timestamp(), 1_700_000_000);
        assert_eq!(time.timestamp_subsec_nanos(), 500_000_000);
        server_task.await.unwrap();
    }

    #[tokio::test]
    async fn ntp_query_times_out_when_server_does_not_reply() {
        let server = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let port = server.local_addr().unwrap().port();
        let provider = ReliableTimeProvider {
            ntp_timeout: StdDuration::from_millis(50),
        };
        let error = tokio::time::timeout(
            StdDuration::from_secs(1),
            provider.try_ntp_at(("127.0.0.1", port)),
        )
        .await
        .expect("NTP I/O must yield so its timeout can fire")
        .unwrap_err();
        assert_eq!(error.to_string(), "NTP request timed out");
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
