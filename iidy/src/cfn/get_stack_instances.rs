use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, StackNameArg},
};

/// Get stack EC2 instances.
///
/// This is currently a stub implementation.
pub async fn get_stack_instances(opts: &AwsOpts, _args: &StackNameArg) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("get_stack_instances not implemented yet")
}
