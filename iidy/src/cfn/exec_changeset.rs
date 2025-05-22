use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, ExecChangeSetArgs},
};

/// Execute a CloudFormation change set.
///
/// This is currently a stub implementation.
pub async fn exec_changeset(opts: &AwsOpts, _args: &ExecChangeSetArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("exec_changeset not implemented yet")
}
