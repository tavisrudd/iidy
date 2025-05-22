use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, CreateChangeSetArgs},
};

/// Create a CloudFormation change set.
///
/// This is currently a stub implementation.
pub async fn create_changeset(opts: &AwsOpts, _args: &CreateChangeSetArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("create_changeset not implemented yet")
}
