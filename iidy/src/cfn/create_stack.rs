use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, StackFileArgs},
};

/// Create a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn create_stack(opts: &AwsOpts, _args: &StackFileArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("create_stack not implemented yet")
}
