use anyhow::{Result, anyhow};

use crate::cfn::{CfnContext, stack_operations::collect_stack_contents};
use crate::cli::{Cli, DescribeArgs};
use crate::output::{
    DynamicOutputManager, OutputData, aws_conversion::convert_stack_events_to_display_with_max,
    convert_stack_to_definition,
};
use crate::run_command_handler;

async fn describe_stack_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    _cli: &Cli,
    args: &DescribeArgs,
    _opts: &crate::cli::NormalizedAwsOpts,
) -> Result<i32> {
    let event_count = args.events as usize;
    let stack_task = {
        let client = context.client.clone();
        let stack_name = args.stackname.clone();
        tokio::spawn(async move {
            let stack_resp = client
                .describe_stacks()
                .stack_name(&stack_name)
                .send()
                .await
                .map_err(anyhow::Error::from)?;

            let stack = stack_resp
                .stacks
                .and_then(|mut s| s.pop())
                .ok_or_else(|| anyhow!("stack not found"))?;

            let output_data = convert_stack_to_definition(&stack, true);
            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };

    let events_task = {
        let client = context.client.clone();
        let stack_name = args.stackname.clone();
        tokio::spawn(async move {
            let first_events_resp = client
                .describe_stack_events()
                .stack_name(&stack_name)
                .send()
                .await
                .map_err(anyhow::Error::from)?;

            let mut all_events = first_events_resp.stack_events.unwrap_or_default();
            let mut next_token = first_events_resp.next_token;
            while next_token.is_some() && all_events.len() < event_count * 2 {
                let events_resp = client
                    .describe_stack_events()
                    .stack_name(&stack_name)
                    .set_next_token(next_token)
                    .send()
                    .await?;

                let mut page_events = events_resp.stack_events.unwrap_or_default();
                all_events.append(&mut page_events);
                next_token = events_resp.next_token;
            }

            let output_data = convert_stack_events_to_display_with_max(
                all_events,
                &format!("Previous Stack Events (max {event_count}):"),
                Some(event_count),
            );

            Ok::<OutputData, anyhow::Error>(output_data)
        })
    };

    let contents_task = {
        let context = context.clone();
        let stack_name = args.stackname.clone();
        tokio::spawn(async move {
            let stack_contents = collect_stack_contents(&context, &stack_name).await?;
            Ok::<OutputData, anyhow::Error>(OutputData::StackContents(stack_contents))
        })
    };

    output_manager.render(stack_task.await??).await?;
    output_manager.render(events_task.await??).await?;
    output_manager.render(contents_task.await??).await?;

    Ok(0)
}

pub async fn describe_stack(cli: &Cli, args: &DescribeArgs) -> Result<i32> {
    run_command_handler!(describe_stack_impl, cli, args)
}

#[cfg(test)]
mod tests {}
