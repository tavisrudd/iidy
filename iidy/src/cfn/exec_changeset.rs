use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, ExecChangeSetArgs},
};

/// Execute a CloudFormation change set.
///
/// This is currently a stub implementation.
pub async fn exec_changeset(opts: &AwsOpts, _args: &ExecChangeSetArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("exec_changeset not implemented yet")
}
