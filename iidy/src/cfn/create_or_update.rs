use anyhow::Result;
use aws_sdk_cloudformation::error::SdkError;
use aws_sdk_cloudformation::error::ProvideErrorMetadata;

use crate::cfn::{CfnContext, CfnRequestBuilder, CfnOperation, apply_stack_name_override_and_validate, stack_operations::{StackInfoService, watch_stack_operation_and_summarize}, CREATE_SUCCESS_STATES, UPDATE_SUCCESS_STATES, StackChangeType, UpdateResult, changeset_operations::{self, check_stack_exists, confirm_changeset_execution},
    exec_changeset::call_exec_changeset_with_reconstruction, stack_args::{load_stack_args, StackArgs}};
use crate::cli::{UpdateStackArgs, Cli};
use crate::output::{
    DynamicOutputManager,
    aws_conversion::{convert_token_info, create_command_metadata, convert_stack_to_definition},
    data::{OutputData, StackChangeDetails}
};
use crate::aws::AwsSettings;
use crate::run_command_handler;

/// Create or update a CloudFormation stack using intelligent detection with data-driven output.
pub async fn create_or_update(cli: &Cli, args: &UpdateStackArgs) -> Result<i32> {
    run_command_handler!(create_or_update_impl, cli, args)
}

async fn create_or_update_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &UpdateStackArgs,
    opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let global_opts = &cli.global_opts;

    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = cli.command.to_cfn_operation();
    let stack_args = load_stack_args(
        &args.base.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.base.stack_name.as_ref())?;
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    let stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    let stack_exists = check_stack_exists(&context, stack_name).await?;

    // Check stack existence and determine change type
    let stack_change_details = if stack_exists {
        if args.changeset {
            return update_stack_with_changeset_data(&context, args, &final_stack_args, output_manager, global_opts, &cli.aws_opts).await;
        } else {
            // Try update and check for no-changes case
            match try_update_stack(&context, args, &final_stack_args, &global_opts.environment).await {
                Ok(UpdateResult::NoChanges) => StackChangeDetails {
                    change_type: StackChangeType::UpdateNoChanges,
                    stack_name: stack_name.clone(),
                },
                Ok(UpdateResult::StackId(stack_id)) => StackChangeDetails {
                    change_type: StackChangeType::UpdateWithChanges { stack_id },
                    stack_name: stack_name.clone(),
                },
                Err(e) => return Err(e),
            }
        }
    } else {
        if args.changeset {
            // Use CREATE changeset for new stacks when --changeset is specified
            return create_stack_with_changeset_data(&context, args, &final_stack_args, output_manager, &global_opts, &opts).await;
        }
        StackChangeDetails {
            change_type: StackChangeType::Create,
            stack_name: stack_name.clone(),
        }
    };

    // Send change details to renderer
    output_manager.render(OutputData::StackChangeDetails(stack_change_details.clone())).await?;

    // Continue based on change type  
    match stack_change_details.change_type {
        StackChangeType::UpdateNoChanges => {
            // Early exit is handled in render_stack_change_details
            // Just return success - the renderer will handle cleanup and final summary
            Ok(0)
        },
        StackChangeType::Create => {
            let stack_id = create_stack_direct_data(&context, &final_stack_args, &args.base.argsfile, &global_opts.environment, output_manager).await?;
            watch_stack_operation_and_summarize(&context, &stack_id, output_manager, CREATE_SUCCESS_STATES).await
        },
        StackChangeType::UpdateWithChanges { stack_id } => {
            let token = context.derive_token_for_step(&CfnOperation::CreateOrUpdate);
            let output_token = convert_token_info(&token);
            output_manager.render(OutputData::TokenInfo(output_token)).await?;
            watch_stack_operation_and_summarize(&context, &stack_id, output_manager, UPDATE_SUCCESS_STATES).await
        },
    }
}



/// Create a new stack directly with data-driven output.
async fn create_stack_direct_data(
    context: &CfnContext,
    stack_args: &StackArgs,
    argsfile: &str,
    environment: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    let builder = CfnRequestBuilder::new(&context, stack_args);

    let (create_request, token) = builder.build_create_stack(
        true,
        &CfnOperation::CreateOrUpdate,
        argsfile,
        Some(environment),
    ).await?;
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _stack_name = stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    let response = create_request.send().await?;

    let stack_id = response.stack_id()
        .ok_or_else(|| anyhow::anyhow!("Stack creation response did not include stack ID"))?
        .to_string();

    Ok(stack_id)
}

/// Try to update a stack and return the result without any output.
/// This allows us to detect no-changes case early before showing stack details.
async fn try_update_stack(
    context: &CfnContext,
    args: &UpdateStackArgs,
    stack_args: &StackArgs,
    environment: &str,
) -> Result<UpdateResult> {
    let builder = CfnRequestBuilder::new(&context, stack_args);

    let (update_request, _token) = builder.build_update_stack(
        true,
        &CfnOperation::CreateOrUpdate,
        &args.base.argsfile,
        Some(environment),
    ).await?;

    match update_request.send().await {
        Ok(response) => {
            let stack_id = response.stack_id()
                .ok_or_else(|| anyhow::anyhow!("AWS did not return a stack ID"))?
                .to_string();
            Ok(UpdateResult::StackId(stack_id))
        }
        Err(SdkError::ServiceError(e)) => {
            let service_err = e.err();
            if service_err.code() == Some("ValidationError") &&
               (service_err.message().unwrap_or("").contains("No updates are to be performed") ||
                service_err.message().unwrap_or("").contains("No changes detected")) {
                // No changes detected - this is a success case, not an error
                Ok(UpdateResult::NoChanges)
            } else {
                Err(anyhow::anyhow!("Update failed: {}", SdkError::ServiceError(e)))
            }
        }
        Err(e) => Err(anyhow::anyhow!("Update failed: {}", e)),
    }
}

/// Update stack with changeset using data-driven output.
async fn update_stack_with_changeset_data(
    context: &CfnContext,
    args: &UpdateStackArgs,
    stack_args: &StackArgs,
    output_manager: &mut DynamicOutputManager,
    global_opts: &crate::cli::GlobalOpts,
    aws_opts: &crate::cli::AwsOpts,
) -> Result<i32> {
    // Step 1: Fetch and render stack definition first
    let stack_name = stack_args.stack_name.as_ref().unwrap();
    let stack_task = {
        let client = context.client.clone();
        let stack_name = stack_name.clone();
        tokio::spawn(async move {
            let stack = crate::cfn::stack_operations::StackInfoService::get_stack(&client, &stack_name).await?;
            let output_data = crate::output::aws_conversion::convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };
    
    // Await and render stack definition
    crate::await_and_render!(stack_task, output_manager);
    
    // Step 2: Create changeset using shared changeset operations
    let changeset_name = format!(
        "iidy-create-or-update-{}",
        &context.primary_token().value[..8]
    );
    
    let changeset_result = changeset_operations::create_changeset_comprehensive(
        context,
        stack_args,
        Some(&changeset_name),
        &args.base.argsfile,
        false, // Don't use primary token for changeset creation
        output_manager,
        None, // No description for create-or-update changeset
        Some(&global_opts.environment), 
    ).await?;

    // Render changeset result
    output_manager.render(OutputData::ChangeSetResult(changeset_result.clone())).await?;

    // Ask for confirmation unless --yes is specified
    let confirmed = confirm_changeset_execution(output_manager, context, args.yes, true).await?;

    if !confirmed {
        return Ok(130); // 130 = interrupted by user (Ctrl-C equivalent)
    }

    // Step 2: Execute changeset using shared CLI reconstruction
    call_exec_changeset_with_reconstruction(
        changeset_result.changeset_name,
        changeset_result.stack_name,
        Some(args.base.argsfile.clone()),
        global_opts,
        aws_opts
    ).await
}

/// Create new stack with changeset using shared changeset functionality.
async fn create_stack_with_changeset_data(
    context: &CfnContext,
    args: &UpdateStackArgs,
    stack_args: &StackArgs,
    output_manager: &mut DynamicOutputManager,
    global_opts: &crate::cli::GlobalOpts,
    opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let changeset_result = changeset_operations::create_changeset_comprehensive(
        context,
        stack_args,
        None,
        &args.base.argsfile,
        false,
        output_manager,
        None, // No description argument available in create-or-update
        Some(&global_opts.environment),
    ).await?;

    // Fetch and render stack definition (stack now exists in REVIEW_IN_PROGRESS state)
    let stack_name = stack_args.stack_name.as_ref().unwrap();
    let stack = StackInfoService::get_stack(&context.client, &stack_name).await?;
    let stack_definition = convert_stack_to_definition(&stack, true);
    output_manager.render(stack_definition).await?;

    output_manager.render(OutputData::ChangeSetResult(changeset_result.clone())).await?;

    // Ask for confirmation unless --yes is specified
    let confirmed = confirm_changeset_execution(output_manager, context, args.yes, true).await?;

    if !confirmed {
        return Ok(130); // 130 = interrupted by user (Ctrl-C equivalent)
    }

    // Execute changeset using shared CLI reconstruction
    call_exec_changeset_with_reconstruction(
        changeset_result.changeset_name,
        changeset_result.stack_name,
        Some(args.base.argsfile.clone()),
        global_opts,
        &crate::cli::AwsOpts {
            region: opts.region.clone(),
            profile: opts.profile.clone(),
            assume_role_arn: opts.assume_role_arn.clone(),
            client_request_token: Some(opts.client_request_token.value.clone()),
        }
    ).await
}
