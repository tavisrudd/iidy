use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, UpdateStackArgs},
};

/// Create or update a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn create_or_update(opts: &AwsOpts, _args: &UpdateStackArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("create_or_update not implemented yet")
}
