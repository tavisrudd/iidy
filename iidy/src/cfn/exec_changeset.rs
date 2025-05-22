use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;

use crate::{
    aws,
    cli::{AwsOpts, ExecChangeSetArgs},
    stack_args::load_stack_args_file,
};

/// Execute a CloudFormation change set.
///
/// This is currently a stub implementation.
pub async fn exec_changeset(opts: &AwsOpts, args: &ExecChangeSetArgs) -> Result<()> {
    let _stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("exec_changeset not implemented yet")
}
