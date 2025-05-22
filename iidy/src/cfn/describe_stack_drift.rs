use anyhow::Result;

use crate::{
    aws,
    cli::{AwsOpts, DriftArgs},
};

/// Describe CloudFormation stack drift.
///
/// This is currently a stub implementation.
pub async fn describe_stack_drift(opts: &AwsOpts, _args: &DriftArgs) -> Result<()> {
    let _client = aws::cfn_client_from_opts(opts).await?;
    todo!("describe_stack_drift not implemented yet")
}
