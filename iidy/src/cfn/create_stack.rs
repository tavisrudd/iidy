use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, StackFileArgs},
};

/// Create a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn create_stack(opts: &AwsOpts, _args: &StackFileArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("create_stack not implemented yet")
}
