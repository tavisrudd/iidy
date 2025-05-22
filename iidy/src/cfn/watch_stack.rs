use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, WatchArgs},
};

/// Watch a CloudFormation stack for changes.
///
/// This is currently a stub implementation.
pub async fn watch_stack(opts: &AwsOpts, _args: &WatchArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("watch_stack not implemented yet")
}
