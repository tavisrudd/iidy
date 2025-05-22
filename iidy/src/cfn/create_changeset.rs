use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;

use crate::{
    aws,
    cli::{AwsOpts, CreateChangeSetArgs},
    stack_args::load_stack_args_file,
};

/// Create a CloudFormation change set.
///
/// This is currently a stub implementation.
pub async fn create_changeset(opts: &AwsOpts, args: &CreateChangeSetArgs) -> Result<()> {
    let _stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("create_changeset not implemented yet")
}
