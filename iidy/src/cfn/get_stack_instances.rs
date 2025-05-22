use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, StackNameArg},
};

/// Get stack EC2 instances.
///
/// This is currently a stub implementation.
pub async fn get_stack_instances(opts: &AwsOpts, _args: &StackNameArg) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("get_stack_instances not implemented yet")
}
