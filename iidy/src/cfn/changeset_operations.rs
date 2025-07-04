//! Shared changeset operations for CloudFormation
//!
//! This module provides common changeset functionality that can be used across
//! multiple command handlers (create-changeset, create-or-update --changeset, etc.)

use anyhow::Result;
use aws_sdk_cloudformation::operation::create_change_set::CreateChangeSetOutput;
use aws_sdk_cloudformation::error::{SdkError, ProvideErrorMetadata};
use tokio::time::{sleep, Duration};

use crate::cfn::{CfnContext, CfnOperation, template_loader::{load_cfn_template, TEMPLATE_MAX_BYTES}};
use crate::output::{
    DynamicOutputManager,
    aws_conversion::convert_token_info,
    data::{OutputData, ChangeSetCreationResult, ChangeSetInfo, ChangeInfo}
};
use crate::stack_args::StackArgs;
use crate::yaml::imports::loaders::random::generate_dashed_name;

/// Check if a CloudFormation stack exists.
pub async fn check_stack_exists(context: &CfnContext, stack_name: &str) -> Result<bool> {
    let describe_request = context.client.describe_stacks().stack_name(stack_name);

    match describe_request.send().await {
        Ok(_) => Ok(true),
        Err(SdkError::ServiceError(e)) => {
            let service_err = e.err();
            if service_err.code() == Some("ValidationError") &&
               service_err.message().unwrap_or("").contains("does not exist") {
                Ok(false)
            } else {
                Err(SdkError::ServiceError(e).into())
            }
        }
        Err(e) => Err(e.into()),
    }
}

/// Comprehensive changeset creation that handles both CREATE and UPDATE changesets
pub async fn create_changeset_comprehensive(
    context: &CfnContext,
    stack_args: &StackArgs,
    changeset_name: Option<&str>,
    argsfile_path: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<ChangeSetCreationResult> {
    // Determine changeset name (docker-style if not provided)
    let final_changeset_name = if let Some(name) = changeset_name {
        name.to_string()
    } else {
        generate_dashed_name()
    };

    // Check if stack exists to determine changeset type
    let stack_name = stack_args.stack_name.as_ref().unwrap();
    let stack_exists = check_stack_exists(context, stack_name).await?;
    
    // Create changeset with appropriate type
    let (changeset_response, _) = perform_changeset_creation(
        context,
        stack_args,
        &final_changeset_name,
        stack_exists,
        argsfile_path,
        output_manager,
    ).await?;

    // Wait for changeset to complete processing
    wait_for_changeset_completion(context, &changeset_response, &final_changeset_name).await?;

    // Build comprehensive changeset result
    build_changeset_result(
        context,
        &changeset_response,
        &final_changeset_name,
        stack_args,
        argsfile_path,
        stack_exists,
    ).await
}

/// Create a changeset with proper type detection and template loading
async fn perform_changeset_creation(
    context: &CfnContext,
    stack_args: &StackArgs,
    changeset_name: &str,
    stack_exists: bool,
    argsfile_path: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<(CreateChangeSetOutput, String)> {
    // Build and execute the CreateChangeSet request with proper changeset type
    let (create_request, token) = build_create_changeset_with_type(
        stack_args,
        changeset_name,
        if stack_exists { 
            aws_sdk_cloudformation::types::ChangeSetType::Update 
        } else { 
            aws_sdk_cloudformation::types::ChangeSetType::Create 
        },
        &CfnOperation::CreateChangeset,
        argsfile_path,
        context
    ).await?;
    
    // Show token info
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let response = create_request.send().await?;

    Ok((response, changeset_name.to_string()))
}

/// Build a CreateChangeSet request with proper template loading and changeset type
async fn build_create_changeset_with_type(
    stack_args: &StackArgs,
    changeset_name: &str,
    changeset_type: aws_sdk_cloudformation::types::ChangeSetType,
    operation: &CfnOperation,
    argsfile_path: &str,
    context: &CfnContext,
) -> Result<(aws_sdk_cloudformation::operation::create_change_set::builders::CreateChangeSetFluentBuilder, crate::timing::TokenInfo)> {
    use aws_sdk_cloudformation::types::{Capability, Parameter, Tag};
    
    let token = context.derive_token_for_step(operation);

    let mut create_request = context
        .client
        .create_change_set()
        .client_token(&token.value)
        .change_set_name(changeset_name)
        .change_set_type(changeset_type);

    // Apply stack name (required)
    if let Some(ref stack_name) = stack_args.stack_name {
        create_request = create_request.stack_name(stack_name);
    }

    // Apply template body (if not using previous template)
    if !stack_args.use_previous_template.unwrap_or(false) {
        if let Some(ref template_location) = stack_args.template {
            let template_result = load_cfn_template(
                Some(template_location),
                argsfile_path,
                None, // environment is already resolved in stack args
                TEMPLATE_MAX_BYTES,
                Some(&context.create_s3_client()),
            ).await?;

            if let Some(template_body) = template_result.template_body {
                create_request = create_request.template_body(template_body);
            } else if let Some(template_url) = template_result.template_url {
                create_request = create_request.template_url(template_url);
            }
        }
    } else {
        create_request = create_request.use_previous_template(true);
    }

    // Apply capabilities
    if let Some(ref capabilities) = stack_args.capabilities {
        let aws_capabilities: Vec<Capability> = capabilities
            .iter()
            .filter_map(|cap| match cap.as_str() {
                "CAPABILITY_IAM" => Some(Capability::CapabilityIam),
                "CAPABILITY_NAMED_IAM" => Some(Capability::CapabilityNamedIam),
                "CAPABILITY_AUTO_EXPAND" => Some(Capability::CapabilityAutoExpand),
                _ => None,
            })
            .collect();
        create_request = create_request.set_capabilities(Some(aws_capabilities));
    }

    // Apply parameters
    if let Some(ref parameters) = stack_args.parameters {
        let aws_parameters: Vec<Parameter> = parameters
            .iter()
            .map(|(key, value)| {
                Parameter::builder()
                    .parameter_key(key)
                    .parameter_value(value)
                    .build()
            })
            .collect();
        create_request = create_request.set_parameters(Some(aws_parameters));
    }

    // Apply tags
    if let Some(ref tags) = stack_args.tags {
        let aws_tags: Vec<Tag> = tags
            .iter()
            .map(|(key, value)| Tag::builder().key(key).value(value).build())
            .collect();
        create_request = create_request.set_tags(Some(aws_tags));
    }

    // Apply notification ARNs
    if let Some(ref notification_arns) = stack_args.notification_arns {
        create_request = create_request.set_notification_arns(Some(notification_arns.clone()));
    }

    // Apply service role ARN
    if let Some(ref role_arn) = stack_args.service_role_arn {
        create_request = create_request.role_arn(role_arn);
    } else if let Some(ref role_arn) = stack_args.role_arn {
        create_request = create_request.role_arn(role_arn);
    }

    // Apply resource types
    if let Some(ref resource_types) = stack_args.resource_types {
        create_request = create_request.set_resource_types(Some(resource_types.clone()));
    }

    Ok((create_request, token))
}

async fn build_changeset_result(
    context: &CfnContext,
    response: &CreateChangeSetOutput,
    changeset_name: &str,
    stack_args: &StackArgs,
    argsfile_path: &str,
    stack_exists: bool,
) -> Result<ChangeSetCreationResult> {
    // Generate console URL
    let console_url = generate_changeset_console_url(response)?;
    
    // Fetch pending changesets
    let stack_name = stack_args.stack_name.as_ref().unwrap();
    let pending_changesets = fetch_pending_changesets(&context.client, stack_name).await?;
    
    // Generate next steps (exact iidy-js format)
    let region = extract_region_from_stack_arn(response.stack_id().unwrap_or(""))?;
    let next_steps = vec![
        format!("Your new stack is now in REVIEW_IN_PROGRESS state. To create the resources run the following"),
        format!("  iidy --region {} exec-changeset --stack-name {} {} {}",
            region,
            stack_name,
            argsfile_path,
            changeset_name
        )
    ];
    
    Ok(ChangeSetCreationResult {
        changeset_name: changeset_name.to_string(),
        stack_name: stack_name.clone(),
        changeset_type: if stack_exists { "UPDATE" } else { "CREATE" }.to_string(),
        status: "CREATE_COMPLETE".to_string(),
        console_url,
        has_changes: !pending_changesets.is_empty(),
        pending_changesets,
        next_steps,
    })
}

fn generate_changeset_console_url(response: &CreateChangeSetOutput) -> Result<String> {
    // Extract stack ARN and changeset ARN from response
    let stack_arn = response.stack_id().unwrap_or("");
    let changeset_arn = response.id().unwrap_or("");
    
    // Parse region from stack ARN (format: arn:aws:cloudformation:region:account:stack/name/id)
    let region = stack_arn.split(':').nth(3).unwrap_or("us-east-1");
    
    // URL encode the ARNs
    let encoded_stack_arn = urlencoding::encode(stack_arn);
    let encoded_changeset_arn = urlencoding::encode(changeset_arn);
    
    // Generate AWS Console URL (exact iidy-js format)
    let console_url = format!(
        "https://{}.console.aws.amazon.com/cloudformation/home?region={}#/changeset/detail?stackId={}&changeSetId={}",
        region, region, encoded_stack_arn, encoded_changeset_arn
    );
    
    Ok(console_url)
}

async fn fetch_pending_changesets(
    client: &aws_sdk_cloudformation::Client,
    stack_name: &str,
) -> Result<Vec<ChangeSetInfo>> {
    // Fetch stack changesets
    let list_response = client
        .list_change_sets()
        .stack_name(stack_name)
        .send()
        .await?;
    
    let mut changesets = Vec::new();
    
    if let Some(changeset_summaries) = list_response.summaries {
        for summary in changeset_summaries {
            // Get detailed changeset information
            let describe_response = client
                .describe_change_set()
                .stack_name(stack_name)
                .change_set_name(summary.change_set_name().unwrap_or(""))
                .send()
                .await?;
                
            let mut changes = Vec::new();
            if let Some(ref changeset_changes) = describe_response.changes {
                for change in changeset_changes {
                    if let Some(ref resource_change) = change.resource_change {
                        changes.push(ChangeInfo {
                            action: resource_change.action().map(|a| a.as_str()).unwrap_or("Unknown").to_string(),
                            logical_resource_id: resource_change.logical_resource_id().unwrap_or("").to_string(),
                            physical_resource_id: resource_change.physical_resource_id().map(|s| s.to_string()),
                            resource_type: resource_change.resource_type().unwrap_or("").to_string(),
                            replacement: resource_change.replacement().map(|r| r.as_str().to_string()),
                            scope: Some(resource_change.scope()
                                .iter().map(|s| s.as_str().to_string()).collect()
                            ),
                            details: resource_change.details()
                                .iter().map(|detail| crate::output::data::ChangeDetail {
                                    target: detail.target().and_then(|t| t.name()).unwrap_or("").to_string(),
                                    evaluation: detail.evaluation().map(|e| e.as_str().to_string()),
                                    change_source: detail.change_source().map(|cs| cs.as_str().to_string()),
                                    causing_entity: detail.causing_entity().map(|ce| ce.to_string()),
                                }).collect(),
                        });
                    }
                }
            }
            
            changesets.push(ChangeSetInfo {
                change_set_name: summary.change_set_name().unwrap_or("").to_string(),
                change_set_id: summary.change_set_id().unwrap_or("").to_string(),
                stack_id: summary.stack_id().unwrap_or("").to_string(),
                stack_name: summary.stack_name().unwrap_or("").to_string(),
                description: summary.description().map(|s| s.to_string()),
                status: summary.status().map(|s| s.to_string()).unwrap_or("UNKNOWN".to_string()),
                status_reason: summary.status_reason().map(|s| s.to_string()),
                creation_time: summary.creation_time.and_then(|ts| {
                    chrono::DateTime::from_timestamp(ts.secs(), ts.subsec_nanos())
                }),
                execution_status: summary.execution_status().map(|s| s.to_string()),
                changes,
            });
        }
    }
    
    Ok(changesets)
}

fn extract_region_from_stack_arn(stack_arn: &str) -> Result<String> {
    stack_arn.split(':').nth(3)
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("Invalid stack ARN format"))
}

/// Wait for changeset to complete processing (transition from CREATE_PENDING to CREATE_COMPLETE)
async fn wait_for_changeset_completion(
    context: &CfnContext,
    response: &CreateChangeSetOutput,
    changeset_name: &str,
) -> Result<()> {
    let stack_id = response.stack_id().unwrap_or("");
    let max_attempts = 30; // Wait up to 30 seconds
    let poll_interval = Duration::from_secs(1);
    
    for _ in 0..max_attempts {
        let describe_response = context.client
            .describe_change_set()
            .stack_name(stack_id)
            .change_set_name(changeset_name)
            .send()
            .await?;
        
        if let Some(status) = describe_response.status() {
            match status.as_str() {
                "CREATE_COMPLETE" => {
                    return Ok(()); // Changeset is ready
                },
                "CREATE_PENDING" | "CREATE_IN_PROGRESS" => {
                    // Still processing, continue waiting
                    sleep(poll_interval).await;
                    continue;
                },
                "FAILED" => {
                    let reason = describe_response.status_reason().unwrap_or("Unknown error");
                    return Err(anyhow::anyhow!("Changeset creation failed: {}", reason));
                },
                _ => {
                    return Err(anyhow::anyhow!("Unexpected changeset status: {}", status.as_str()));
                }
            }
        }
    }
    
    Err(anyhow::anyhow!("Timeout waiting for changeset to complete"))
}