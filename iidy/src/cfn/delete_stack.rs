use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, DeleteArgs},
};

/// Delete a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn delete_stack(opts: &AwsOpts, _args: &DeleteArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("delete_stack not implemented yet")
}
