use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;

use crate::{
    aws,
    cli::{AwsOpts, StackFileArgs},
    stack_args::load_stack_args_file,
};

/// Estimate stack cost.
///
/// This is currently a stub implementation.
pub async fn estimate_cost(opts: &AwsOpts, args: &StackFileArgs) -> Result<()> {
    let _stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;
    let config = aws::config_from_opts(opts).await?;
    let _client = Client::new(&config);
    todo!("estimate_cost not implemented yet")
}
