use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, WatchArgs},
};

/// Watch a CloudFormation stack for changes.
///
/// This is currently a stub implementation.
pub async fn watch_stack(opts: &AwsOpts, _args: &WatchArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("watch_stack not implemented yet")
}
