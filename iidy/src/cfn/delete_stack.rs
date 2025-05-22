use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, DeleteArgs},
};

/// Delete a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn delete_stack(opts: &AwsOpts, _args: &DeleteArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("delete_stack not implemented yet")
}
