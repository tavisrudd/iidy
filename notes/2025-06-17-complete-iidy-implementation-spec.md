# Complete iidy-js Implementation Specification

**Date:** 2025-06-17  
**Purpose:** Complete specification for pixel-perfect iidy-js output matching

## Table of Contents
1. [Command Flows](#command-flows)
2. [Core Function Library](#core-function-library)
3. [Constants and Types](#constants-and-types)
4. [Implementation Guidelines](#implementation-guidelines)

---

## Command Flows

### 1. create-stack / update-stack / create-or-update

**Exact Flow (from AbstractCloudFormationStackCommand):**
```rust
async fn run_stack_operation() -> Result<i32> {
    // 1. Command Metadata
    show_command_summary().await?;
    
    // 2. Stack operation (create/update)
    let start_time = get_reliable_start_time().await;
    perform_stack_operation().await?;  // create or update call
    
    // 3. Watch and summarize
    watch_and_summarize(stack_id, start_time).await
}

async fn watch_and_summarize(stack_id: &str, start_time: DateTime<Utc>) -> Result<i32> {
    // 3a. Stack Details
    let stack = summarize_stack_definition(stack_id, region, show_times_in_summary).await?;
    
    // 3b. Previous Events (if enabled)
    if show_previous_events {
        println!();
        println!("{}", format_section_heading("Previous Stack Events (max 10):"));
        show_stack_events(stack_id, 10, Some(previous_events)).await?;
    }
    
    println!();
    
    // 3c. Live Events (if enabled)
    if watch_stack_events {
        watch_stack(stack_id, start_time).await?;
    }
    
    println!();
    
    // 3d. Final Stack Summary
    let final_stack = summarize_stack_contents(stack_id).await?;
    
    // 3e. Success/Failure Summary
    let success = expected_final_status.contains(&final_stack.stack_status);
    show_final_command_summary(success)
}
```

### 2. describe-stack

**Exact Flow (from describeStack.ts):**
```rust
async fn describe_stack_main(args: &Args) -> Result<i32> {
    let stack_name = get_stack_name_from_args_and_configure_aws(args).await?;
    let region = get_current_aws_region();
    let stack = get_stack_description(&stack_name).await?;
    
    // Handle --query parameter
    if let Some(query) = &args.query {
        let cfn = CloudFormationClient::new();
        let resources = cfn.describe_stack_resources()
            .stack_name(&stack_name)
            .send().await?;
        
        let resources_map: HashMap<String, StackResource> = resources.stack_resources()
            .unwrap_or_default()
            .iter()
            .map(|r| (r.logical_resource_id().unwrap().to_string(), r.clone()))
            .collect();
        
        let combined = json!({
            "Resources": resources_map,
            "Stack": stack
        });
        
        let result = jmespath::search(&combined, query)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(0);
    }
    
    // Normal describe flow
    let stack_events_promise = get_all_stack_events(&stack_name);
    
    // Stack definition with times
    summarize_stack_definition(&stack_name, &region, true, Some(stack.clone())).await?;
    
    println!();
    
    // Previous events (default 50, configurable with --events)
    let event_count = args.events.unwrap_or(50);
    println!("{}", format_section_heading(&format!("Previous Stack Events (max {}):", event_count)));
    show_stack_events(&stack_name, event_count, Some(stack_events_promise.await?)).await?;
    
    println!();
    
    // Stack contents
    summarize_stack_contents(&stack.stack_id().unwrap()).await?;
    
    Ok(0)
}
```

### 3. watch-stack

**Exact Flow (from watchStack.ts):**
```rust
async fn watch_stack_main(args: &Args) -> Result<i32> {
    let stack_name = get_stack_name_from_args_and_configure_aws(args).await?;
    let region = get_current_aws_region();
    let start_time = get_reliable_start_time().await;
    
    println!();
    
    // Stack definition summary
    let stack = summarize_stack_definition(&stack_name, &region, true).await?;
    let stack_id = stack.stack_id().unwrap();
    
    println!();
    
    // Previous events (max 10)
    println!("{}", format_section_heading("Previous Stack Events (max 10):"));
    show_stack_events(stack_id, 10, None).await?;
    
    println!();
    
    // Live watching
    watch_stack(stack_id, start_time, DEFAULT_EVENT_POLL_INTERVAL, args.inactivity_timeout).await?;
    
    println!();
    
    // Final summary
    summarize_stack_contents(stack_id).await?;
    
    Ok(0)
}
```

### 4. delete-stack

**Exact Flow (from deleteStack.ts):**
```rust
async fn delete_stack_main(args: &Args) -> Result<i32> {
    let stack_name = get_stack_name_from_args_and_configure_aws(args).await?;
    let region = get_current_aws_region();
    
    // Check if stack exists
    let stack = match get_stack_description(&stack_name).await {
        Ok(stack) => stack,
        Err(_) => {
            let sts = StsClient::new();
            let identity = sts.get_caller_identity().send().await?;
            let msg = format!(
                "The stack {} is absent in env = {}:\n      region = {}\n      account = {}\n      auth_arn = {}",
                stack_name.magenta(),
                args.environment.yellow(),
                region.truecolor(128, 128, 128),
                identity.account().unwrap().truecolor(128, 128, 128),
                identity.arn().unwrap().truecolor(128, 128, 128)
            );
            
            if args.fail_if_absent {
                eprintln!("{}", msg);
                return Ok(1);
            } else {
                println!("{}", msg);
                return Ok(0);
            }
        }
    };
    
    println!();
    
    // Show stack details
    let stack = summarize_stack_definition(&stack_name, &region, true).await?;
    let stack_id = stack.stack_id().unwrap();
    
    println!();
    
    // Previous events
    println!("{}", format_section_heading("Previous Stack Events (max 10):"));
    show_stack_events(&stack_name, 10, None).await?;
    
    println!();
    
    // Stack contents
    summarize_stack_contents(stack_id).await?;
    
    println!();
    
    // Confirmation
    let confirmed = if args.yes {
        true
    } else {
        confirmation_prompt(&format!("Are you sure you want to DELETE the stack {}?", stack_name)).await?
    };
    
    if confirmed {
        let cfn = CloudFormationClient::new();
        let start_time = get_reliable_start_time().await;
        
        cfn.delete_stack()
            .stack_name(&stack_name)
            .set_role_arn(args.role_arn.clone())
            .set_retain_resources(args.retain_resources.clone())
            .set_client_request_token(args.client_request_token.clone())
            .send().await?;
        
        watch_stack(stack_id, start_time, DEFAULT_EVENT_POLL_INTERVAL, 0).await?;
        
        println!();
        
        let final_stack = get_stack_description(stack_id).await?;
        show_final_command_summary(final_stack.stack_status().unwrap() == "DELETE_COMPLETE")
    } else {
        Ok(130) // INTERRUPT
    }
}
```

### 5. list-stacks

**Exact Flow (from listStacks.ts):**
```rust
async fn list_stacks_main(args: &Args) -> Result<i32> {
    configure_aws(args).await?;
    
    // Parse tag filters
    let tag_filters: Vec<(String, String)> = args.tag_filter.iter()
        .map(|tf| {
            let parts: Vec<&str> = tf.splitn(2, '=').collect();
            (parts[0].to_string(), parts.get(1).unwrap_or("").to_string())
        })
        .collect();
    
    list_stacks(args.tags, args.query.as_deref(), Some(&tag_filters), args.jmespath_filter.as_deref()).await?;
    
    Ok(0)
}

async fn list_stacks(show_tags: bool, query: Option<&str>, tag_filter: Option<&[(String, String)]>, jmespath_filter: Option<&str>) -> Result<()> {
    let stacks = get_all_stacks().await?;
    
    if stacks.is_empty() {
        println!("No stacks found");
        return Ok(());
    }
    
    // Sort by creation/update time
    let mut sorted_stacks = stacks;
    sorted_stacks.sort_by_key(|s| s.creation_time().or(s.last_updated_time()));
    
    let time_padding = 24;
    let status_padding = calc_padding(&sorted_stacks, |s| s.stack_status().unwrap());
    
    // Apply filters
    let mut filtered_stacks = sorted_stacks;
    
    if let Some(tag_filters) = tag_filter {
        if !tag_filters.is_empty() {
            filtered_stacks.retain(|stack| {
                let tags: HashMap<String, String> = stack.tags().unwrap_or_default()
                    .iter()
                    .map(|tag| (tag.key().unwrap().to_string(), tag.value().unwrap().to_string()))
                    .collect();
                
                tag_filters.iter().all(|(k, v)| tags.get(k) == Some(v))
            });
        }
    }
    
    if let Some(jmespath_filter) = jmespath_filter {
        filtered_stacks.retain(|stack| {
            // Convert stack to JSON and apply JMESPath
            let result = jmespath::search(stack, jmespath_filter).unwrap_or_default();
            !result.is_null()
        });
    }
    
    // Handle --query output
    if let Some(query) = query {
        let combined = json!({ "Stacks": filtered_stacks });
        let result = jmespath::search(&combined, query)?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    
    // Normal list output
    println!("{}", 
        format!("Creation/Update Time, Status, Name{}", if show_tags { ", Tags" } else { "" })
            .truecolor(128, 128, 128)
    );
    
    for stack in &filtered_stacks {
        let tags: HashMap<String, String> = stack.tags().unwrap_or_default()
            .iter()
            .map(|tag| (tag.key().unwrap().to_string(), tag.value().unwrap().to_string()))
            .collect();
        
        let lifetime = tags.get("lifetime");
        
        // Lifecycle icons
        let lifecycle_icon = if stack.enable_termination_protection() == Some(true) || lifetime == Some(&"protected".to_string()) {
            "🔒 "
        } else if lifetime == Some(&"long".to_string()) {
            "∞ "
        } else if lifetime == Some(&"short".to_string()) {
            "♺ "
        } else {
            ""
        };
        
        // Base stack name (handle StackSet)
        let base_stack_name = if stack.stack_name().unwrap().starts_with("StackSet-") {
            format!("{} {}", 
                stack.stack_name().unwrap().truecolor(128, 128, 128),
                tags.get("StackSetName")
                    .or_else(|| stack.description())
                    .unwrap_or(&"Unknown stack set instance".to_string())
            )
        } else {
            stack.stack_name().unwrap().to_string()
        };
        
        // Environment-based coloring
        let stack_name = if stack.stack_name().unwrap().contains("production") || tags.get("environment") == Some(&"production".to_string()) {
            base_stack_name.red().to_string()
        } else if stack.stack_name().unwrap().contains("integration") || tags.get("environment") == Some(&"integration".to_string()) {
            base_stack_name.color(UserDefined(75)).to_string()
        } else if stack.stack_name().unwrap().contains("development") || tags.get("environment") == Some(&"development".to_string()) {
            base_stack_name.color(UserDefined(194)).to_string()
        } else {
            base_stack_name
        };
        
        // Main line output
        println!("{} {} {} {}",
            format_timestamp(&format!("{:>width$}", 
                render_timestamp(stack.creation_time().or(stack.last_updated_time()).unwrap()), 
                width = time_padding
            )),
            colorize_resource_status(stack.stack_status().unwrap(), Some(status_padding)),
            format!("{}{}", lifecycle_icon.truecolor(128, 128, 128), stack_name),
            if show_tags { 
                pretty_format_tags(stack.tags().unwrap_or_default()).truecolor(128, 128, 128).to_string()
            } else { 
                String::new() 
            }
        );
        
        // Failure reason on next line
        if stack.stack_status().unwrap().contains("FAILED") {
            if let Some(reason) = stack.stack_status_reason() {
                if !reason.is_empty() {
                    println!("   {}", reason.truecolor(128, 128, 128));
                }
            }
        }
    }
    
    Ok(())
}
```

### 6. create-changeset

**Exact Flow (from createChangeset.ts and CreateChangeSet class):**
```rust
async fn create_changeset_main(args: &Args) -> Result<i32> {
    // Load stack args and setup
    let stack_args = load_stack_args(&args.argsfile, &args.environment).await?;
    let stack_name = args.stack_name.as_ref().unwrap_or(&stack_args.stack_name);
    let changeset_name = args.changeset_name.as_ref()
        .map(|s| s.to_string())
        .unwrap_or_else(|| generate_project_name());
    
    // Create changeset input
    let stack_exists = does_stack_exist(stack_name).await?;
    let changeset_type = if stack_exists { "UPDATE" } else { "CREATE" };
    
    let changeset_input = stack_args_to_create_changeset_input(
        &changeset_name,
        &stack_args,
        &args.argsfile,
        &args.environment,
        stack_name
    ).await?;
    
    // Template approval check
    if requires_template_approval(&changeset_input.template_url).await? {
        return exit_with_template_approval_failure();
    }
    
    // Create changeset
    let cfn = CloudFormationClient::new();
    cfn.create_change_set()
        .set_change_set_name(Some(changeset_name.clone()))
        .set_stack_name(Some(stack_name.to_string()))
        .set_change_set_type(Some(changeset_type.to_string()))
        // ... other parameters from changeset_input
        .send().await?;
    
    // Wait for completion
    wait_for_changeset_create_complete(&stack_name, &changeset_name).await.ok(); // Ignore errors for failed changesets
    
    // Describe changeset
    let changeset = cfn.describe_change_set()
        .change_set_name(&changeset_name)
        .stack_name(stack_name)
        .send().await?;
    
    let has_changes = !changeset.changes().unwrap_or_default().is_empty();
    
    if changeset.status().unwrap() == "FAILED" {
        eprintln!("{}. Deleting failed changeset.", changeset.status_reason().unwrap_or(""));
        cfn.delete_change_set()
            .change_set_name(&changeset_name)
            .stack_name(stack_name)
            .send().await?;
        return Ok(1);
    }
    
    println!();
    
    // Show AWS Console URL
    let region = get_current_aws_region();
    let escaped_stack_id = urlencoding::encode(changeset.stack_id().unwrap());
    let escaped_changeset_id = urlencoding::encode(changeset.change_set_id().unwrap());
    
    println!("AWS Console URL for full changeset review: {}",
        format!("https://{}.console.aws.amazon.com/cloudformation/home?region={}#/changeset/detail?stackId={}&changeSetId={}",
            region, region, escaped_stack_id, escaped_changeset_id
        ).truecolor(128, 128, 128)
    );
    
    // Show pending changesets
    show_pending_changesets(stack_name, None).await?;
    
    if !stack_exists {
        println!("Your new stack is now in REVIEW_IN_PROGRESS state. To create the resources run the following");
        println!("  {}", 
            normalize_iidy_cli_command(&format!("exec-changeset --stack-name {} {} {}", 
                stack_name, args.argsfile, changeset_name
            ))
        );
        println!();
    }
    
    Ok(0)
}
```

---

## Core Function Library

### 1. printSectionEntry (formatting.ts:22-25)

```rust
fn print_section_entry(label: &str, data: &str) -> bool {
    print!("{}", format_section_entry(label, data));
    true
}

fn format_section_entry(label: &str, data: &str) -> String {
    format!(" {}{}\n", 
        format_section_label(&format!("{:<width$} ", label, width = COLUMN2_START - 1)),
        data
    )
}
```

### 2. showCommandSummary (AbstractCloudFormationStackCommand.ts:87-105)

```rust
async fn show_command_summary(
    cfn_operation: &str,
    environment: &str,
    region: &str,
    profile: Option<&str>,
    args: &Args,
    stack_args: &StackArgs
) -> Result<()> {
    let sts = StsClient::new();
    let iam_identity = sts.get_caller_identity().send().await?;
    let role_arn = stack_args.service_role_arn.as_ref()
        .or(stack_args.role_arn.as_ref());
    
    println!(); // blank line
    println!("{}", format_section_heading("Command Metadata:"));
    print_section_entry("CFN Operation:", &cfn_operation.magenta().to_string());
    print_section_entry("iidy Environment:", &environment.magenta().to_string());
    print_section_entry("Region:", &region.magenta().to_string());
    
    if let Some(profile) = profile {
        if !profile.is_empty() {
            print_section_entry("Profile:", &profile.magenta().to_string());
        }
    }
    
    let cli_args = pretty_format_small_map(&[
        ("region".to_string(), args.region.as_deref().unwrap_or("null").to_string()),
        ("profile".to_string(), args.profile.as_deref().unwrap_or("null").to_string()),
        ("argsfile".to_string(), args.argsfile.clone()),
    ].into_iter().collect());
    
    print_section_entry("CLI Arguments:", &cli_args.truecolor(128, 128, 128).to_string());
    print_section_entry("IAM Service Role:", 
        &role_arn.unwrap_or(&"None".to_string()).truecolor(128, 128, 128).to_string());
    print_section_entry("Current IAM Principal:", 
        &iam_identity.arn().unwrap().truecolor(128, 128, 128).to_string());
    print_section_entry("iidy Version:", 
        &env!("CARGO_PKG_VERSION").truecolor(128, 128, 128).to_string());
    
    println!();
    
    Ok(())
}
```

### 3. summarizeStackDefinition (summarizeStackDefinition.ts:16-68)

```rust
async fn summarize_stack_definition(
    stack_name: &str, 
    region: &str, 
    show_times: bool,
    stack_promise: Option<Stack>
) -> Result<Stack> {
    println!("{}", format_section_heading("Stack Details:"));
    
    let cfn = CloudFormationClient::new();
    let stack = match stack_promise {
        Some(stack) => stack,
        None => get_stack_description(stack_name).await?,
    };
    
    let stack_policy_future = cfn.get_stack_policy()
        .stack_name(stack_name)
        .send();
    
    let stack_id = stack.stack_id().unwrap();
    
    // Convert tags to map
    let tags_map: HashMap<String, String> = stack.tags().unwrap_or_default()
        .iter()
        .map(|tag| (tag.key().unwrap().to_string(), tag.value().unwrap().to_string()))
        .collect();
    
    // Name entry (handle StackSet)
    if let Some(stackset_name) = tags_map.get("StackSetName") {
        print_section_entry("Name (StackSet):", 
            &format!("{} {}", 
                stack.stack_name().unwrap().truecolor(128, 128, 128),
                stackset_name.magenta()
            )
        );
    } else {
        print_section_entry("Name:", &stack.stack_name().unwrap().magenta().to_string());
    }
    
    // Description
    if let Some(description) = stack.description() {
        let description_color = if stack.stack_name().unwrap().starts_with("StackSet") {
            description.magenta().to_string()
        } else {
            description.truecolor(128, 128, 128).to_string()
        };
        print_section_entry("Description:", &description_color);
    }
    
    // Status
    print_section_entry("Status", &colorize_resource_status(stack.stack_status().unwrap(), None));
    
    // Capabilities
    let capabilities = if stack.capabilities().unwrap_or_default().is_empty() {
        "None".to_string()
    } else {
        stack.capabilities().unwrap()
            .iter()
            .map(|c| c.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    };
    print_section_entry("Capabilities:", &capabilities.truecolor(128, 128, 128).to_string());
    
    // Service Role
    let service_role = stack.role_arn().unwrap_or("None");
    print_section_entry("Service Role:", &service_role.truecolor(128, 128, 128).to_string());
    
    // Tags
    let tags_str = pretty_format_tags(stack.tags().unwrap_or_default());
    print_section_entry("Tags:", &tags_str.truecolor(128, 128, 128).to_string());
    
    // Parameters
    let params_str = pretty_format_parameters(stack.parameters().unwrap_or_default());
    print_section_entry("Parameters:", &params_str.truecolor(128, 128, 128).to_string());
    
    // DisableRollback
    print_section_entry("DisableRollback:", 
        &stack.disable_rollback().unwrap_or(false).to_string().truecolor(128, 128, 128).to_string());
    
    // TerminationProtection
    let termination_protection = stack.enable_termination_protection().unwrap_or(false);
    let protection_text = format!("{}{}", 
        termination_protection.to_string().truecolor(128, 128, 128),
        if termination_protection { "🔒 " } else { "" }
    );
    print_section_entry("TerminationProtection:", &protection_text);
    
    // Times (conditional)
    if show_times {
        if let Some(creation_time) = stack.creation_time() {
            print_section_entry("Creation Time:", 
                &render_timestamp(*creation_time).truecolor(128, 128, 128).to_string());
        }
        if let Some(last_updated_time) = stack.last_updated_time() {
            print_section_entry("Last Update Time:", 
                &render_timestamp(*last_updated_time).truecolor(128, 128, 128).to_string());
        }
    }
    
    // Timeout
    if let Some(timeout) = stack.timeout_in_minutes() {
        print_section_entry("Timeout In Minutes:", 
            &timeout.to_string().truecolor(128, 128, 128).to_string());
    }
    
    // NotificationARNs
    let notification_arns = if stack.notification_ar_ns().unwrap_or_default().is_empty() {
        "None".to_string()
    } else {
        stack.notification_ar_ns().unwrap().join(", ")
    };
    print_section_entry("NotificationARNs:", &notification_arns.truecolor(128, 128, 128).to_string());
    
    // Stack Policy
    let stack_policy = stack_policy_future.await?;
    if let Some(policy_body) = stack_policy.stack_policy_body() {
        // JSON roundtrip to remove whitespace
        let policy_json: serde_json::Value = serde_json::from_str(policy_body)?;
        print_section_entry("Stack Policy Source:", 
            &serde_json::to_string(&policy_json)?.truecolor(128, 128, 128).to_string());
    }
    
    // ARN
    print_section_entry("ARN:", &stack_id.truecolor(128, 128, 128).to_string());
    
    // Console URL
    let console_url = format!("https://{}.console.aws.amazon.com/cloudformation/home?region={}#/stack/detail?stackId={}",
        region, region, urlencoding::encode(stack_id));
    print_section_entry("Console URL:", &console_url.truecolor(128, 128, 128).to_string());
    
    Ok(stack)
}
```

### 4. summarizeStackContents (summarizeStackContents.ts:19-75)

```rust
async fn summarize_stack_contents(
    stack_id: &str,
    stack_promise: Option<Stack>
) -> Result<Stack> {
    let cfn = CloudFormationClient::new();
    
    // Concurrent requests
    let resources_future = cfn.describe_stack_resources()
        .stack_name(stack_id)
        .send();
    let exports_future = get_all_stack_exports_with_imports(stack_id);
    let changesets_future = cfn.list_change_sets()
        .stack_name(stack_id)
        .send();
    
    let stack = match stack_promise {
        Some(stack) => stack,
        None => get_stack_description(stack_id).await?,
    };
    
    let resources = resources_future.await?
        .stack_resources().unwrap_or_default();
    
    // Stack Resources
    if !resources.is_empty() {
        println!("{}", format_section_heading("Stack Resources:"));
        let id_padding = calc_padding(resources, |r| r.logical_resource_id().unwrap());
        let resource_type_padding = calc_padding(resources, |r| r.resource_type().unwrap());
        
        for resource in resources {
            println!("{} {} {}",
                format_logical_id(&format!(" {:<width$}", 
                    resource.logical_resource_id().unwrap(), 
                    width = id_padding
                )),
                format!("{:<width$}", 
                    resource.resource_type().unwrap(), 
                    width = resource_type_padding
                ).truecolor(128, 128, 128),
                resource.physical_resource_id().unwrap_or("").truecolor(128, 128, 128)
            );
        }
    }
    
    println!();
    
    // Stack Outputs
    print!("{}", format_section_heading("Stack Outputs:"));
    let output_key_padding = calc_padding(
        stack.outputs().unwrap_or_default(), 
        |o| o.output_key().unwrap()
    );
    
    if let Some(outputs) = stack.outputs() {
        if !outputs.is_empty() {
            println!();
            for output in outputs {
                println!("{} {}",
                    format_logical_id(&format!(" {:<width$}", 
                        output.output_key().unwrap(), 
                        width = output_key_padding
                    )),
                    output.output_value().unwrap_or("").truecolor(128, 128, 128)
                );
            }
        } else {
            println!(" {}", "None".truecolor(128, 128, 128));
        }
    } else {
        println!(" {}", "None".truecolor(128, 128, 128));
    }
    
    // Stack Exports
    let exports = exports_future.await?;
    if !exports.is_empty() {
        println!();
        println!("{}", format_section_heading("Stack Exports:"));
        let export_name_padding = calc_padding(&exports, |ex| &ex.name);
        
        for export in &exports {
            println!("{} {}",
                format_logical_id(&format!(" {:<width$}", 
                    export.name, 
                    width = export_name_padding
                )),
                export.value.truecolor(128, 128, 128)
            );
            
            // Show imports
            let imports = export.imports.await?;
            for import in imports.imports().unwrap_or_default() {
                println!("  {}", format!("imported by {}", import).truecolor(128, 128, 128));
            }
        }
    }
    
    println!();
    
    // Current Stack Status
    println!("{} {} {}",
        format_section_heading(&format!("{:<width$}", "Current Stack Status:", width = COLUMN2_START)),
        colorize_resource_status(stack.stack_status().unwrap(), None),
        stack.stack_status_reason().unwrap_or("").to_string()
    );
    
    // Pending Changesets
    let changesets = changesets_future.await?;
    show_pending_changesets(stack_id, Some(changesets)).await?;
    
    Ok(stack)
}
```

### 5. showPendingChangesets (showPendingChangesets.ts:8-33)

```rust
async fn show_pending_changesets(
    stack_id: &str,
    changesets_promise: Option<ListChangeSetsOutput>
) -> Result<()> {
    let cfn = CloudFormationClient::new();
    let changesets_output = match changesets_promise {
        Some(output) => output,
        None => cfn.list_change_sets().stack_name(stack_id).send().await?,
    };
    
    let mut changesets = changesets_output.summaries().unwrap_or_default().to_vec();
    
    // Sort by creation time
    changesets.sort_by_key(|cs| cs.creation_time());
    
    if !changesets.is_empty() {
        println!();
        println!("{}", format_section_heading("Pending Changesets:"));
        
        for changeset in &changesets {
            print_section_entry(
                &format_timestamp(&render_timestamp(changeset.creation_time().unwrap())),
                &format!("{} {} {}",
                    changeset.change_set_name().unwrap().magenta(),
                    changeset.status().unwrap(),
                    changeset.status_reason().unwrap_or("")
                )
            );
            
            if let Some(description) = changeset.description() {
                if !description.is_empty() {
                    println!("  Description: {}", description.truecolor(128, 128, 128));
                    println!();
                }
            }
            
            // Show changeset details
            let changeset_details = cfn.describe_change_set()
                .stack_name(stack_id)
                .change_set_name(changeset.change_set_name().unwrap())
                .send().await?;
            
            summarize_change_set(&changeset_details).await?;
            println!();
        }
    }
    
    Ok(())
}
```

### 6. eventIsFromSubstack (eventIsFromSubstack.ts:3-6)

```rust
fn event_is_from_substack(event: &StackEvent) -> bool {
    event.physical_resource_id().unwrap_or("") != "" &&
    event.resource_type().unwrap_or("") == "AWS::CloudFormation::Stack" &&
    event.stack_id().unwrap_or("") != event.physical_resource_id().unwrap_or("")
}
```

### 7. prettyFormatParameters (formatting.ts:74-79)

```rust
fn pretty_format_parameters(params: &[Parameter]) -> String {
    if params.is_empty() {
        return String::new();
    }
    
    let map: HashMap<String, String> = params.iter()
        .map(|p| (p.parameter_key().unwrap().to_string(), p.parameter_value().unwrap_or("").to_string()))
        .collect();
    
    pretty_format_small_map(&map)
}
```

---

## Constants and Types

```rust
// Core constants (from formatting.ts)
pub const COLUMN2_START: usize = 25;
pub const DEFAULT_STATUS_PADDING: usize = 35;
pub const MIN_STATUS_PADDING: usize = 17;
pub const MAX_PADDING: usize = 60;

// From displayStackEvent.ts
pub const RESOURCE_TYPE_PADDING: usize = 40;
pub const DEFAULT_SCREEN_WIDTH: usize = 130;

// From defaults.ts  
pub const DEFAULT_EVENT_POLL_INTERVAL: u64 = 2; // seconds

// Status type arrays (from statusTypes.ts)
pub const IN_PROGRESS: &[&str] = &[
    "CREATE_IN_PROGRESS", "REVIEW_IN_PROGRESS", "ROLLBACK_IN_PROGRESS",
    "DELETE_IN_PROGRESS", "UPDATE_IN_PROGRESS", "UPDATE_COMPLETE_CLEANUP_IN_PROGRESS",
    "UPDATE_ROLLBACK_IN_PROGRESS", "UPDATE_ROLLBACK_COMPLETE_CLEANUP_IN_PROGRESS",
    "IMPORT_IN_PROGRESS", "IMPORT_ROLLBACK_IN_PROGRESS",
];

pub const COMPLETE: &[&str] = &[
    "CREATE_COMPLETE", "ROLLBACK_COMPLETE", "DELETE_COMPLETE", 
    "UPDATE_COMPLETE", "UPDATE_ROLLBACK_COMPLETE", 
    "IMPORT_COMPLETE", "IMPORT_ROLLBACK_COMPLETE",
];

pub const FAILED: &[&str] = &[
    "CREATE_FAILED", "DELETE_FAILED", "ROLLBACK_FAILED",
    "UPDATE_ROLLBACK_FAILED", "IMPORT_ROLLBACK_FAILED"
];

pub const SKIPPED: &[&str] = &["DELETE_SKIPPED"];

pub const TERMINAL: &[&str] = &[
    // All COMPLETE + FAILED + SKIPPED + special case
    "CREATE_COMPLETE", "ROLLBACK_COMPLETE", "DELETE_COMPLETE", "UPDATE_COMPLETE",
    "UPDATE_ROLLBACK_COMPLETE", "IMPORT_COMPLETE", "IMPORT_ROLLBACK_COMPLETE",
    "CREATE_FAILED", "DELETE_FAILED", "ROLLBACK_FAILED", "UPDATE_ROLLBACK_FAILED", 
    "IMPORT_ROLLBACK_FAILED", "DELETE_SKIPPED", "REVIEW_IN_PROGRESS"
];

// Color codes (exact xterm values)
pub const COLOR_TIMESTAMP: u8 = 253;        // Light gray
pub const COLOR_LOGICAL_ID: u8 = 252;       // Light gray  
pub const COLOR_SECTION_HEADING: u8 = 255;  // White
pub const COLOR_SPINNER: u8 = 240;          // Dark gray
pub const COLOR_ENV_INTEGRATION: u8 = 75;   // Blue-ish
pub const COLOR_ENV_DEVELOPMENT: u8 = 194;  // Yellow-ish

// Return codes
pub const SUCCESS: i32 = 0;
pub const FAILURE: i32 = 1;
pub const INTERRUPT: i32 = 130;

// Data structures
#[derive(Debug, Clone)]
pub struct StackExportWithImports {
    pub name: String,
    pub value: String,
    pub imports: Future<ListImportsOutput>, // Lazy-loaded
}

#[derive(Debug, Clone)]
pub struct ResourceEventTimingEntry {
    pub start: StackEvent,
    pub complete: Option<StackEvent>,
}

#[derive(Debug)]
pub struct EventTimingsResult {
    pub time_to_completion: HashMap<String, u64>, // EventId -> seconds
    pub resource_timings: HashMap<String, Vec<ResourceEventTimingEntry>>,
}
```

---

## Implementation Guidelines

### 1. Exact String Formatting
- Use `format!("{:<width$}", value, width = width)` for left-padding (sprintf equivalent)
- Use `format!("{:>width$}", value, width = width)` for right-padding
- Always use exact character counts for padding

### 2. Color Application
- Apply colors using `owo-colors` with exact xterm codes
- Chain color methods: `text.color(UserDefined(253)).to_string()`
- Use `truecolor(128, 128, 128)` for blackBright equivalent

### 3. Output Patterns
- Use `print!()` for same-line output without newline
- Use `println!()` for output with newline
- Use `process.stdout.write()` equivalent for precise control

### 4. Error Handling
- Return appropriate exit codes (0=success, 1=failure, 130=interrupt)
- Display errors with proper formatting and colors
- Handle missing stacks gracefully with user-friendly messages

### 5. Async Patterns
- Use concurrent futures for independent operations
- Lazy-load expensive operations when possible
- Handle timeouts and interruptions gracefully

### 6. Edge Case Handling
- Empty collections display "None" in blackBright
- Missing optional fields use default values
- Long text wraps at terminal width with proper indentation

### 7. Spinner Behavior
- Update every 1000ms with exact text format
- Show elapsed time and time since last event
- Stop/start around event display for clean output

This specification provides complete implementation details for pixel-perfect iidy-js output matching.