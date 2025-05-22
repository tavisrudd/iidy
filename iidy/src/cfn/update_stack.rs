use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, UpdateStackArgs},
};

/// Update a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn update_stack(opts: &AwsOpts, _args: &UpdateStackArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("update_stack not implemented yet")
}
