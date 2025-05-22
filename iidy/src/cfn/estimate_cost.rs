use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, StackFileArgs},
};

/// Estimate stack cost.
///
/// This is currently a stub implementation.
pub async fn estimate_cost(opts: &AwsOpts, _args: &StackFileArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("estimate_cost not implemented yet")
}
