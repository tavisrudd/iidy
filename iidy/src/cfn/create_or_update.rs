use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, UpdateStackArgs},
};

/// Create or update a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn create_or_update(opts: &AwsOpts, _args: &UpdateStackArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("create_or_update not implemented yet")
}
