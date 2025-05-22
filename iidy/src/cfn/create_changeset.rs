use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, CreateChangeSetArgs},
};

/// Create a CloudFormation change set.
///
/// This is currently a stub implementation.
pub async fn create_changeset(opts: &AwsOpts, _args: &CreateChangeSetArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("create_changeset not implemented yet")
}
