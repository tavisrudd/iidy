use anyhow::Result;
use aws_sdk_cloudformation::Client;
use chrono::{DateTime, Utc};
use std::sync::Arc;

use crate::timing::TimeProvider;

// CloudFormation operation modules
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
pub mod update_stack;
pub mod watch_stack;

/// Context object that carries shared state for CloudFormation operations.
/// 
/// This includes the AWS client, timing provider for reliable timestamps,
/// and the operation start time for event filtering.
pub struct CfnContext {
    pub client: Client,
    pub time_provider: Arc<dyn TimeProvider>,
    pub start_time: Option<DateTime<Utc>>,
}

impl CfnContext {
    /// Create a new CFN context with the given client and time provider.
    /// 
    /// The start time is automatically set using the time provider's start_time() method.
    pub async fn new(
        client: Client,
        time_provider: Arc<dyn TimeProvider>,
    ) -> Result<Self> {
        let start_time = Some(time_provider.start_time().await?);
        Ok(CfnContext {
            client,
            time_provider,
            start_time,
        })
    }
    
    /// Create a new CFN context without setting a start time.
    /// 
    /// Useful for operations that don't need event filtering.
    pub fn new_without_start_time(
        client: Client,
        time_provider: Arc<dyn TimeProvider>,
    ) -> Self {
        CfnContext {
            client,
            time_provider,
            start_time: None,
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::MockTimeProvider;
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
    
    #[tokio::test]
    async fn cfn_context_sets_start_time() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        
        let ctx = CfnContext::new(client, time_provider).await.unwrap();
        
        assert!(ctx.start_time.is_some());
        let expected_start = fixed_time - chrono::Duration::milliseconds(500);
        assert_eq!(ctx.start_time.unwrap(), expected_start);
    }
    
    #[tokio::test]
    async fn cfn_context_calculates_elapsed_time() {
        let fixed_time = Utc.with_ymd_and_hms(2024, 1, 1, 12, 0, 0).unwrap();
        let time_provider = Arc::new(MockTimeProvider::new(fixed_time));
        let client = mock_client();
        
        let mut ctx = CfnContext::new(client, time_provider.clone()).await.unwrap();
        
        // Simulate time passing by updating the mock provider's time
        let later_time = fixed_time + chrono::Duration::seconds(30);
        ctx.time_provider = Arc::new(MockTimeProvider::new(later_time));
        
        let elapsed = ctx.elapsed_seconds().await.unwrap();
        assert_eq!(elapsed, 30); // 30 seconds + 500ms from start_time adjustment
    }
}