use anyhow::Result;
use aws_sdk_cloudformation::error::SdkError;
use aws_sdk_cloudformation::error::ProvideErrorMetadata;

use crate::cfn::{
    stack_operations::{StackInfoService, StackEventsService, collect_stack_contents}, 
    determine_operation_success, DELETE_SUCCESS_STATES
};
use crate::cli::{DeleteArgs, Cli};
use crate::output::{
    DynamicOutputManager, OutputData,
    aws_conversion::{
        create_command_metadata,
        convert_stack_to_definition, convert_stack_events_to_display_with_max,
        create_final_command_summary, convert_aws_error_to_error_info,
        get_caller_identity
    },
    data::StackAbsentInfo
};
use crate::cfn::stack_args::StackArgs;
use crate::cfn::watch_stack::{ManagerOutput, watch_stack_live_events_with_seen_events};
use crate::cfn::constants::{DEFAULT_POLL_INTERVAL_SECS, DEFAULT_POLL_TIMEOUT_SECS, DEFAULT_PREVIOUS_EVENTS_COUNT};
use crate::run_command_handler;

use super::CfnContext;

async fn check_stack_exists_for_delete(context: &CfnContext, stack_name: &str) -> Result<Option<aws_sdk_cloudformation::types::Stack>> {
    let describe_request = context.client.describe_stacks().stack_name(stack_name);

    match describe_request.send().await {
        Ok(response) => {
            let stack = response
                .stacks
                .and_then(|mut stacks| stacks.pop())
                .ok_or_else(|| anyhow::anyhow!("stack '{}' not found in response", stack_name))?;
            Ok(Some(stack))
        }
        Err(SdkError::ServiceError(e)) => {
            let service_err = e.err();
            if service_err.code() == Some("ValidationError") &&
               service_err.message().unwrap_or("").contains("does not exist") {
                Ok(None)
            } else {
                Err(SdkError::ServiceError(e).into())
            }
        }
        Err(e) => {
            Err(e.into())
        }
    }
}


async fn perform_stack_deletion_without_output(
    context: &CfnContext,
    stack_name: &str,
    stack_id: &str,
    args: &DeleteArgs,
) -> Result<String> {
    let token = context.primary_token();

    let mut request = context
        .client
        .delete_stack()
        .stack_name(stack_name)
        .client_request_token(&token.value);

    if let Some(role) = &args.role_arn {
        request = request.role_arn(role);
    }

    if !args.retain_resources.is_empty() {
        request = request.set_retain_resources(Some(args.retain_resources.clone()));
    }

    request.send().await?;
    Ok(stack_id.to_string())
}

async fn delete_stack_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &DeleteArgs,
    opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let stack_name = &args.stackname;

    let (stack, stack_id) = match check_stack_exists_for_delete(context, stack_name).await {
        Ok(Some(stack)) => {
            let stack_id = StackInfoService::get_stack_id(&context.client, stack_name).await?;
            (stack, stack_id)
        }
        Ok(None) => {
            let (account, auth_arn) = get_caller_identity(context).await?;
            let stack_absent_info = StackAbsentInfo {
                stack_name: stack_name.clone(),
                environment: cli.global_opts.environment.clone(),
                region: context.aws_config.region()
                    .map(|r| r.as_ref().to_string())
                    .unwrap_or_else(|| "unknown".to_string()),
                account,
                auth_arn,
            };
            output_manager.render(OutputData::StackAbsentInfo(stack_absent_info)).await?;
            let elapsed_seconds = context.elapsed_seconds().await?;
            let final_command_summary = create_final_command_summary(
                true,
                elapsed_seconds
            );
            output_manager.render(final_command_summary).await?;
            return Ok(0);
        }
        Err(e) => {
            let error_info = convert_aws_error_to_error_info(&e, Some((context, cli))).await;
            output_manager.render(OutputData::Error(error_info)).await?;
            return Ok(1);
        }
    };

    let minimal_stack_args = StackArgs {
        stack_name: Some(stack_name.clone()),
        template: None,
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        ..Default::default()
    };
    let command_metadata = create_command_metadata(context, opts, &minimal_stack_args, &cli.global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;
    
    let stack_definition = convert_stack_to_definition(&stack, true);
    output_manager.render(stack_definition).await?;
    
    let previous_events_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let events = StackEventsService::fetch_events(&client, &stack_id).await?;
            let events_display = convert_stack_events_to_display_with_max(
                events,
                &format!("Previous Stack Events (max {}):", DEFAULT_PREVIOUS_EVENTS_COUNT),
                Some(DEFAULT_PREVIOUS_EVENTS_COUNT),
            );
            Ok::<OutputData, anyhow::Error>(events_display)
        })
    };
    
    let stack_contents_task = {
        let context_clone = context.clone();
        let stack_id = stack_id.clone();
        tokio::spawn(async move {
            let stack_contents = collect_stack_contents(&context_clone, &stack_id).await?;
            Ok::<OutputData, anyhow::Error>(OutputData::StackContents(stack_contents))
        })
    };
    
    output_manager.render(previous_events_task.await??).await?;
    output_manager.render(stack_contents_task.await??).await?;
    
    let confirmed = if args.yes {
        true
    } else {
        let message = format!("Are you sure you want to DELETE the stack {}?", stack_name);
        output_manager.request_confirmation(message).await?
    };
    
    if !confirmed {
        let elapsed_seconds = context.elapsed_seconds().await?;
        let final_summary = create_final_command_summary(true, elapsed_seconds);
        output_manager.render(final_summary).await?;
        return Ok(130);
    }
            
    let stack_id_for_deletion = perform_stack_deletion_without_output(context, stack_name, &stack_id, args).await?;

    let final_status = {
        let live_events_context = context.clone();
        let manager_output = ManagerOutput { manager: output_manager };
        match watch_stack_live_events_with_seen_events(
            &live_events_context.client, 
            &live_events_context, 
            &stack_id_for_deletion, 
            manager_output,
            std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS), 
            std::time::Duration::from_secs(DEFAULT_POLL_TIMEOUT_SECS),
            vec![]
        ).await {
            Ok(status) => status,
            Err(error) => {
                let error_info = convert_aws_error_to_error_info(&error, Some((context, cli))).await;
                output_manager.render(OutputData::Error(error_info)).await?;
                return Ok(1);
            }
        }
    };
    
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = determine_operation_success(&final_status, DELETE_SUCCESS_STATES);
    let final_command_summary = create_final_command_summary(
        success,
        elapsed_seconds
    );
    output_manager.render(final_command_summary).await?;
    
    Ok(if success { 0 } else { 1 })
}

pub async fn delete_stack(cli: &Cli, args: &DeleteArgs) -> Result<i32> {
    run_command_handler!(delete_stack_impl, cli, args)
}
