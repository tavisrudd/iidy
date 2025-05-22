use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;

use crate::{
    aws,
    cli::{AwsOpts, StackFileArgs},
    stack_args::load_stack_args_file,
};

/// Create a CloudFormation stack.
///
/// This is currently a stub implementation.
pub async fn create_stack(opts: &AwsOpts, args: &StackFileArgs) -> Result<()> {
    let _stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("create_stack not implemented yet")
}
