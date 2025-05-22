use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, StackFileArgs},
};

/// Estimate stack cost.
///
/// This is currently a stub implementation.
pub async fn estimate_cost(opts: &AwsOpts, _args: &StackFileArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("estimate_cost not implemented yet")
}
