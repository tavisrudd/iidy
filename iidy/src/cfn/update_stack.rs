use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;

use crate::{
    aws,
    cli::{AwsOpts, UpdateStackArgs},
    stack_args::load_stack_args_file,
};

/// Update a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn update_stack(opts: &AwsOpts, args: &UpdateStackArgs) -> Result<()> {
    let _stack_args = load_stack_args_file(Path::new(&args.base.argsfile), None)?;
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("update_stack not implemented yet")
}
