use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, UpdateStackArgs},
};

/// Update a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn update_stack(opts: &AwsOpts, _args: &UpdateStackArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("update_stack not implemented yet")
}
