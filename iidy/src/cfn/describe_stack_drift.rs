use anyhow::Result;
use aws_sdk_cloudformation::Client;

use crate::{
    aws,
    cli::{AwsOpts, DriftArgs},
};

/// Describe CloudFormation stack drift.
///
/// This is currently a stub implementation.
pub async fn describe_stack_drift(opts: &AwsOpts, _args: &DriftArgs) -> Result<()> {
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("describe_stack_drift not implemented yet")
}
