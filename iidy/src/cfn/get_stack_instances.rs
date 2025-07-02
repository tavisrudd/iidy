use anyhow::Result;

use crate::cli::{Cli, GetStackInstancesArgs};
use crate::cfn::create_context_for_operation;
use crate::output::{
    DynamicOutputManager, manager::OutputOptions,
    aws_conversion::{create_command_result},
    OutputData, StatusUpdate, StatusLevel
};

/// Get stack EC2 instances that belong to a CloudFormation stack.
///
/// Queries EC2 for instances with the stack tag and displays them in either
/// short format (DNS/IP only) or detailed format with instance details.
pub async fn get_stack_instances(cli: &Cli, args: &GetStackInstancesArgs) -> Result<()> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    let output_options = OutputOptions::minimal();
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    let operation = cli.command.to_cfn_operation();
    let context = create_context_for_operation(&opts, operation).await?;
    let ec2_client = aws_sdk_ec2::Client::new(&context.aws_config);
    
    // Query EC2 for instances with the CloudFormation stack tag
    let filter = aws_sdk_ec2::types::Filter::builder()
        .name("tag:aws:cloudformation:stack-name")
        .values(&args.stackname)
        .build();

    let response = ec2_client
        .describe_instances()
        .filters(filter)
        .send()
        .await?;

    let reservations = response.reservations.unwrap_or_default();
    let mut instance_count = 0;

    for reservation in &reservations {
        for instance in reservation.instances.as_ref().unwrap_or(&vec![]) {
            instance_count += 1;
            
            if args.short {
                // Short format: show DNS name or private IP
                let address = instance.public_dns_name()
                    .and_then(|dns| if dns.is_empty() { None } else { Some(dns) })
                    .or_else(|| instance.private_ip_address())
                    .unwrap_or("unknown");
                    
                let status_update = StatusUpdate {
                    message: address.to_string(),
                    timestamp: chrono::Utc::now(),
                    level: StatusLevel::Info,
                };
                output_manager.render(OutputData::StatusUpdate(status_update)).await?;
            } else {
                // Detailed format: DNS, IP, ID, type, state, AZ, launch time
                let public_dns = instance.public_dns_name().unwrap_or("");
                let private_ip = instance.private_ip_address().unwrap_or("");
                let instance_id = instance.instance_id().unwrap_or("unknown");
                let instance_type = instance.instance_type()
                    .map(|t| t.as_str())
                    .unwrap_or("unknown");
                let state = instance.state()
                    .and_then(|s| s.name())
                    .map(|n| n.as_str())
                    .unwrap_or("unknown");
                let az = instance.placement()
                    .and_then(|p| p.availability_zone())
                    .unwrap_or("");
                let launch_time = instance.launch_time()
                    .map(|t| t.fmt(aws_smithy_types::date_time::Format::DateTime).unwrap_or_default())
                    .unwrap_or_default();

                let detail_line = format!(
                    "{:<42} {:<15} {} {:<11} {} {} {}",
                    public_dns, private_ip, instance_id, instance_type, state, az, launch_time
                );
                
                let status_update = StatusUpdate {
                    message: detail_line,
                    timestamp: chrono::Utc::now(),
                    level: StatusLevel::Info,
                };
                output_manager.render(OutputData::StatusUpdate(status_update)).await?;
            }
        }
    }

    // Show console URL at the end (following iidy-js pattern)
    let region = opts.region.as_deref().unwrap_or("us-east-1");
    let console_url = format!(
        "https://console.aws.amazon.com/ec2/v2/home?region={}#Instances:tag:aws:cloudformation:stack-name={};sort=desc:launchTime",
        region, args.stackname
    );
    
    let console_status = StatusUpdate {
        message: console_url,
        timestamp: chrono::Utc::now(),
        level: StatusLevel::Info,
    };
    output_manager.render(OutputData::StatusUpdate(console_status)).await?;

    let elapsed = context.elapsed_seconds().await?;
    let result_msg = format!("Found {} instances", instance_count);
    output_manager.render(create_command_result(true, elapsed, Some(result_msg))).await?;

    Ok(())
}
