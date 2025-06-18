use anyhow::Result;
use std::path::Path;
use std::time::Instant;

use crate::{
    cfn::{CfnRequestBuilder, create_context},
    cli::{NormalizedAwsOpts, StackFileArgs, GlobalOpts},
    stack_args::load_stack_args_file,
    output::{
        DynamicOutputManager, OutputData, create_command_metadata, 
        progress_message, success_message, create_command_result
    },
};

/// Create a CloudFormation stack using the request builder pattern.
///
/// This function loads stack arguments, creates the necessary context and builders,
/// and executes the CreateStack operation with proper token management.
/// Uses the data-driven output architecture for consistent rendering across output modes.
pub async fn create_stack(
    opts: &NormalizedAwsOpts, 
    args: &StackFileArgs, 
    global_opts: &GlobalOpts
) -> Result<()> {
    // Load stack configuration
    let stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;

    // Override stack name if provided via CLI
    let mut final_stack_args = stack_args;
    if let Some(ref stack_name) = args.stack_name {
        final_stack_args.stack_name = Some(stack_name.clone());
    }

    // Validate required fields
    if final_stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    let start_time = Instant::now();

    // Setup AWS client and context
    let context = create_context(opts).await?;

    // Setup data-driven output manager
    let output_options = crate::output::manager::OutputOptions {
        color_choice: global_opts.color,
        theme: global_opts.theme,
        terminal_width: None, // Will auto-detect
        buffer_limit: 100,
    };
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // Show command metadata
    let command_metadata = create_command_metadata(&context, opts, &final_stack_args, "create-stack", &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // Setup request builder
    let builder = CfnRequestBuilder::new(&context, &final_stack_args);

    // Build and execute the CreateStack request
    let (create_request, _token) = builder.build_create_stack("create-stack");

    let stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;
    
    output_manager.render(progress_message(&format!("Creating stack: {}", stack_name))).await?;

    let response = create_request.send().await?;

    let success_msg = if let Some(stack_id) = response.stack_id() {
        format!("Stack creation initiated: {}", stack_id)
    } else {
        "Stack creation initiated".to_string()
    };
    
    output_manager.render(success_message(&success_msg)).await?;

    // Show final operation result
    let elapsed = start_time.elapsed().as_secs() as i64;
    let command_result = create_command_result(true, elapsed, Some("Stack creation completed".to_string()));
    output_manager.render(command_result).await?;

    Ok(())
}
