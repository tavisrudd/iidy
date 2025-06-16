use anyhow::Result;
use std::path::Path;

use crate::{
    cfn::{ConsoleReporter, create_context},
    cli::{NormalizedAwsOpts, StackFileArgs},
    stack_args::load_stack_args_file,
};

/// Estimate stack cost.
///
/// This is currently a stub implementation that loads the template
/// and shows the token but doesn't perform actual cost estimation.
pub async fn estimate_cost(opts: &NormalizedAwsOpts, args: &StackFileArgs) -> Result<()> {
    let _stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;
    let context = create_context(opts).await?;

    let reporter = ConsoleReporter::new("estimate-cost");
    reporter.show_primary_token(&context.primary_token());

    reporter.show_warning("Cost estimation is not yet implemented");
    reporter.show_info(
        "This operation would estimate the cost of deploying the CloudFormation template",
    );
    reporter.show_operation_summary(&context);

    Ok(())
}
