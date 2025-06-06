use anyhow::Result;
use aws_sdk_cloudformation::Client;
use std::path::Path;
use std::sync::Arc;

use crate::{
    aws,
    cli::{NormalizedAwsOpts, StackFileArgs},
    stack_args::load_stack_args_file,
    timing::{ReliableTimeProvider, TimeProvider},
    cfn::{CfnContext, ConsoleReporter},
};

/// Estimate stack cost.
///
/// This is currently a stub implementation.
pub async fn estimate_cost(opts: &NormalizedAwsOpts, args: &StackFileArgs) -> Result<()> {
    let stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;
    let config = aws::config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    let time_provider: Arc<dyn TimeProvider> = Arc::new(ReliableTimeProvider::new());
    let context = CfnContext::new(client, time_provider, opts.client_request_token.clone()).await?;
    
    let reporter = ConsoleReporter::new("estimate-cost");
    reporter.show_primary_token(&context.primary_token());
    
    todo!("estimate_cost not implemented yet")
}
