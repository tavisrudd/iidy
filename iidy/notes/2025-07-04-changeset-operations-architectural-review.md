# Changeset Operations Architectural Review

**Date:** 2025-07-04  
**Purpose:** Review and align changeset operations with data-driven output architecture established in create_stack.rs and update_stack.rs

## Executive Summary

The changeset operations (`create_changeset.rs` and `exec_changeset.rs`) do not follow the established architectural patterns used in `create_stack.rs` and `update_stack.rs`. They use outdated patterns with direct output management violations, inconsistent return types, missing data collection, and improper error handling.

## Architectural Standards (Reference Implementation)

Based on `create_stack.rs` and `update_stack.rs`, the expected pattern is:

### 1. Return Type Standard
```rust
pub async fn operation_name(cli: &Cli, args: &Args) -> Result<i32>
```
**Purpose:** Return exit codes (0=success, 1=failure, 130=interrupt) for proper shell integration

### 2. CLI Context Reconstruction
```rust
// Reconstruct CLI for OutputOptions
let aws_opts = AwsOpts {
    region: opts.region.clone(),
    profile: opts.profile.clone(), 
    assume_role_arn: opts.assume_role_arn.clone(),
    client_request_token: Some(opts.client_request_token.value.clone()),
};
let cli = Cli {
    global_opts: global_opts.clone(),
    aws_opts,
    command: Commands::OperationName(args.clone()),
};
let output_options = OutputOptions::new(cli);
```
**Purpose:** Enable proper keyboard handling and operation-specific rendering

### 3. Data Collection Pattern
```rust
// 1. Command metadata
let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

// 2. Operation execution
let result = perform_operation().await?;

// 3. Parallel data collection using sender/receiver pattern
let sender = output_manager.start();
// ... spawn tasks for stack definition, events, etc.
drop(sender);
output_manager.stop().await?;

// 4. Final summary with exit code determination
let success = determine_operation_success(&final_status, SUCCESS_STATES);
let final_summary = create_final_command_summary(success, elapsed_seconds);
output_manager.render(final_summary).await?;
Ok(if success { 0 } else { 1 })
```

## Current Changeset Operations Analysis

### Problem 1: Wrong Return Types

#### Current Code (create_changeset.rs:14)
```rust
pub async fn create_changeset(cli: &Cli, args: &CreateChangeSetArgs) -> Result<()>
```

#### Current Code (exec_changeset.rs:14)  
```rust
pub async fn exec_changeset(cli: &Cli, args: &ExecChangeSetArgs) -> Result<()>
```

#### Required Pattern
```rust
pub async fn create_changeset(cli: &Cli, args: &CreateChangeSetArgs) -> Result<i32>
pub async fn exec_changeset(cli: &Cli, args: &ExecChangeSetArgs) -> Result<i32>
```

**Impact:** No exit code propagation, inconsistent with shell scripting expectations

### Problem 2: Minimal Output Options

#### Current Code (Both Files:19-23)
```rust
let output_options = OutputOptions::minimal();
let mut output_manager = DynamicOutputManager::new(
    global_opts.effective_output_mode(),
    output_options
).await?;
```

#### Required Pattern (create_stack.rs:49-65)
```rust
// Reconstruct CLI for full context
let aws_opts = AwsOpts {
    region: opts.region.clone(),
    profile: opts.profile.clone(),
    assume_role_arn: opts.assume_role_arn.clone(),
    client_request_token: Some(opts.client_request_token.value.clone()),
};
let cli = Cli {
    global_opts: global_opts.clone(),
    aws_opts,
    command: Commands::CreateChangeset(args.clone()), // or ExecuteChangeset
};
let output_options = OutputOptions::new(cli);
let mut output_manager = DynamicOutputManager::new(
    global_opts.effective_output_mode(),
    output_options
).await?;
```

**Impact:** Missing keyboard support, operation-specific rendering features disabled

### Problem 3: Missing Command Metadata

#### Current Code
Both files skip command metadata entirely.

#### Required Pattern (create_stack.rs:67-69)
```rust
// 1. Show command metadata (exact iidy-js pattern)
let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;
```

**Impact:** Inconsistent output, missing timing/context information

### Problem 4: Manual Progress Messages

#### Current Code (create_changeset.rs:64-68)
```rust
output_manager.render(progress_message(&format!(
    "Creating changeset '{}' for stack: {}",
    changeset_name,
    final_stack_args.stack_name.as_ref().unwrap()
))).await?;
```

#### Current Code (exec_changeset.rs:55-59)
```rust
output_manager.render(progress_message(&format!(
    "Executing changeset '{}' for stack: {}",
    args.changeset_name,
    final_stack_args.stack_name.as_ref().unwrap()
)));
```

**Required Pattern**
All user-facing messages should be delegated to renderers via OutputData variants, not manually constructed progress messages.

### Problem 5: Inconsistent Success/Error Handling

#### Current Code (create_changeset.rs:93-104)
```rust
// Show final result
let elapsed = context.elapsed_seconds().await?;
match result {
    Ok(_) => {
        output_manager.render(create_command_result(true, elapsed, Some("Changeset creation completed".to_string()))).await?;
    }
    Err(ref e) => {
        output_manager.render(create_command_result(false, elapsed, Some(format!("Changeset creation failed: {}", e)))).await?;
    }
}

result
```

#### Required Pattern (create_stack.rs:151-159)
```rust
// Calculate elapsed time and determine success based on final stack status  
let elapsed_seconds = context.elapsed_seconds().await?;
let success = determine_operation_success(&final_status, SUCCESS_STATES);

// Show final command summary
let final_command_summary = create_final_command_summary(success, elapsed_seconds);
output_manager.render(final_command_summary).await?;

// Return appropriate exit code
Ok(if success { 0 } else { 1 })
```

**Impact:** Wrong result type, no exit code handling, inconsistent error patterns

### Problem 6: Missing Stack Data Collection

#### Current Code
Neither operation collects or displays stack information after execution.

#### Required Pattern (create_stack.rs:74-149)
```rust
// 3. Start parallel data collection and rendering
let sender = output_manager.start();

// Start stack definition task
let stack_task = {
    let client = context.client.clone();
    let stack_id = stack_id.clone();
    let tx = sender.clone();
    tokio::spawn(async move {
        let stack = StackInfoService::get_stack(&client, &stack_id).await?;
        let output_data = convert_stack_to_definition(&stack, true);
        let _ = tx.send(output_data);
        Ok::<(), anyhow::Error>(())
    })
};

// Start live events task
let events_task = { /* ... */ };

// Process and render all data from parallel operations
output_manager.stop().await?;

// Show stack contents
let stack_contents = collect_stack_contents(&context, &stack_id).await?;
output_manager.render(OutputData::StackContents(stack_contents)).await?;
```

**Impact:** Users don't see stack status, events, or resource information after changeset operations

### Problem 7: Primitive Error Recovery

#### Current Code (exec_changeset.rs:69-82)
```rust
if let Err(e) = watch_stack_with_data_output(
    &context,
    final_stack_args.stack_name.as_ref().unwrap(),
    &mut output_manager,
    std::time::Duration::from_secs(5),
).await {
    output_manager.render(warning_message(&format!("Error watching stack progress: {}", e))).await?;
    output_manager.render(warning_message("The changeset execution was initiated, but there was an error watching progress.")).await?;
    output_manager.render(warning_message("You can check the stack status manually in the AWS Console.")).await?;
} else {
    output_manager.render(success_message("Stack operation completed successfully")).await?;
}
```

#### Required Pattern (create_stack.rs:119-132)
```rust
// Wait for all tasks to complete and handle any errors
let (stack_result, events_result) = tokio::join!(stack_task, events_task);

// Propagate any errors from the spawned tasks
stack_result??;
let final_status = events_result??;

// Determine success using centralized helper
let success = determine_operation_success(&final_status, SUCCESS_STATES);
```

**Impact:** Poor error handling, inconsistent success determination

## Specific Missing Features

### 1. Expected Sections for Renderers

The changeset operations don't integrate with the section system used by InteractiveRenderer:

#### Current State (interactive.rs:497-498)
```rust
CfnOperation::CreateChangeset => vec!["changeset_result"],
CfnOperation::ExecuteChangeset => vec!["command_metadata", "live_stack_events", "stack_contents"],
```

#### Required Pattern Based on iidy-js Output
For CreateChangeset, we need:
```rust
CfnOperation::CreateChangeset => vec![
    "command_metadata", 
    "changeset_result"
],
```

For ExecuteChangeset, we need the full stack monitoring flow:
```rust
CfnOperation::ExecuteChangeset => vec![
    "command_metadata", 
    "stack_definition", 
    "stack_events", 
    "live_stack_events", 
    "stack_contents"
],
```

The iidy-js output shows ExecuteChangeset follows the exact same pattern as update-stack:
1. **Command Metadata** section with operation details
2. **Stack Details** section with current stack information
3. **Previous Stack Events** section (max 10)
4. **Live Stack Events** section with real-time updates
5. **Stack Resources/Outputs** section with final state

### 2. Required Data Types for iidy-js Compatible Output

#### Current State
Changeset results are handled as manual success/warning messages.

#### Required Addition to OutputData (data.rs)
Based on the iidy-js output format, we need a single comprehensive structure:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeSetCreationResult {
    pub console_url: String,
    pub changesets: Vec<PendingChangeSet>,
    pub execution_command: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PendingChangeSet {
    pub creation_time: Option<DateTime<Utc>>,
    pub changeset_name: String,
    pub status: String,
    pub changes: Vec<ChangeSetChange>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChangeSetChange {
    pub action: String,        // "Add", "Modify", "Remove"
    pub logical_resource_id: String,
    pub resource_type: String,
}

pub enum OutputData {
    // ... existing variants ...
    ChangeSetResult(ChangeSetCreationResult),
}
```

### 3. Operation Success States

#### Current State
No success state definitions for changeset operations.

#### Required Addition (mod.rs)
```rust
pub const CHANGESET_CREATE_SUCCESS_STATES: &[&str] = &["CREATE_COMPLETE"];
pub const CHANGESET_EXECUTE_SUCCESS_STATES: &[&str] = &["UPDATE_COMPLETE", "CREATE_COMPLETE"];
```

## Implementation Plan

### Phase 1: Core Infrastructure Updates

1. **Add Required Data Types to OutputData enum**
   - Define `ChangeSetCreationResult` struct in `src/output/data.rs`
   - Add ChangeSetResult variant to OutputData enum (update existing to use new structure)
   - Update both renderers to handle the comprehensive changeset result

2. **Add Docker-Style Name Generator**
   - Make `generate_dashed_name()` public in `src/yaml/imports/loaders/random.rs`
   - Or create a helper function in `src/cfn/create_changeset.rs` with proper imports:
     ```rust
     use crate::yaml::imports::loaders::random::generate_dashed_name;
     
     fn generate_docker_style_changeset_name() -> String {
         generate_dashed_name()
     }
     ```

3. **Add Success State Constants**
   - Define CHANGESET_CREATE_SUCCESS_STATES in `src/cfn/mod.rs`
   - Define CHANGESET_EXECUTE_SUCCESS_STATES in `src/cfn/mod.rs`

4. **Update Expected Sections**
   - Update CreateChangeset in `get_expected_sections()` in `interactive.rs` to include command_metadata:
     ```rust
     CfnOperation::CreateChangeset => vec!["command_metadata", "changeset_result"],
     ```
   - Update ExecuteChangeset to match the full monitoring pattern:
     ```rust
     CfnOperation::ExecuteChangeset => vec!["command_metadata", "stack_definition", "stack_events", "live_stack_events", "stack_contents"],
     ```
   - Update renderer to handle the comprehensive changeset result structure

### Phase 2: create_changeset.rs Refactoring

#### Before (create_changeset.rs:14-105)
```rust
pub async fn create_changeset(cli: &Cli, args: &CreateChangeSetArgs) -> Result<()> {
    // ... existing minimal implementation ...
    result
}
```

#### After
```rust
pub async fn create_changeset(cli: &Cli, args: &CreateChangeSetArgs) -> Result<i32> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    // Load stack configuration with full context
    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = cli.command.to_cfn_operation();
    let stack_args = load_stack_args(
        &args.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.stack_name.as_ref())?;
    if final_stack_args.template.is_none() {
        anyhow::bail!("Template is required in stack-args.yaml");
    }

    // Setup AWS context for changeset creation
    let context = create_context_for_operation(&opts, operation).await?;

    // Setup data-driven output manager with full CLI context
    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: Commands::CreateChangeset(args.clone()),
    };
    let output_options = OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Show command metadata (exact iidy-js pattern)
    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // 2. Create changeset operation  
    let (changeset_response, changeset_name) = perform_changeset_creation(&context, &final_stack_args, args, &global_opts.environment, &mut output_manager).await?;

    // 3. Build comprehensive changeset result
    let changeset_result = build_changeset_result(&context, &changeset_response, &changeset_name, &final_stack_args, args).await?;

    // 4. Render changeset result
    output_manager.render(OutputData::ChangeSetResult(changeset_result)).await?;

    // 5. Calculate elapsed time and determine success
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = true; // Changeset creation success is determined by API call success

    // 6. Show final command summary
    let final_command_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_command_summary).await?;

    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}

async fn perform_changeset_creation(
    context: &CfnContext,
    stack_args: &StackArgs,
    args: &CreateChangeSetArgs,
    environment: &str,
    output_manager: &mut DynamicOutputManager,
) -> Result<(CreateChangeSetOutput, String)> {
    // Setup request builder
    let builder = CfnRequestBuilder::new(context, stack_args);

    // Determine changeset name (docker-style if not provided)
    let changeset_name = if let Some(name) = &args.changeset_name {
        name.clone()
    } else {
        // Use existing docker-style name generator (needs to be made public)
        generate_docker_style_changeset_name()
    };

    // Build and execute the CreateChangeSet request
    let (create_request, token) = builder.build_create_changeset(changeset_name, &CfnOperation::CreateChangeset);
    
    // Show token info
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let response = create_request.send().await?;

    Ok((response, changeset_name.to_string()))
}

async fn build_changeset_result(
    context: &CfnContext,
    response: &CreateChangeSetOutput,
    changeset_name: &str,
    stack_args: &StackArgs,
    args: &CreateChangeSetArgs,
) -> Result<ChangeSetCreationResult> {
    // Generate console URL
    let console_url = generate_changeset_console_url(response)?;
    
    // Fetch pending changesets
    let stack_name = stack_args.stack_name.as_ref().unwrap();
    let changesets = fetch_pending_changesets(&context.client, stack_name, changeset_name).await?;
    
    // Generate execution command (exact iidy-js format)
    let region = extract_region_from_stack_arn(response.stack_id().unwrap_or(""))?;
    let execution_command = format!(
        "iidy --region {} exec-changeset --stack-name {} {} {}",
        region,
        stack_name,
        args.argsfile,
        changeset_name
    );
    
    Ok(ChangeSetCreationResult {
        console_url,
        changesets,
        execution_command,
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
    created_changeset_name: &str,
) -> Result<Vec<PendingChangeSet>> {
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
            if let Some(changeset_changes) = describe_response.changes {
                for change in changeset_changes {
                    if let Some(resource_change) = change.resource_change {
                        changes.push(ChangeSetChange {
                            action: resource_change.action().unwrap_or("Unknown").to_string(),
                            logical_resource_id: resource_change.logical_resource_id().unwrap_or("").to_string(),
                            resource_type: resource_change.resource_type().unwrap_or("").to_string(),
                        });
                    }
                }
            }
            
            changesets.push(PendingChangeSet {
                creation_time: summary.creation_time.map(|dt| DateTime::from(dt)),
                changeset_name: summary.change_set_name().unwrap_or("").to_string(),
                status: summary.status().unwrap_or("UNKNOWN").to_string(),
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
```

### Phase 3: exec_changeset.rs Refactoring

#### Current Problem
The exec_changeset.rs implementation doesn't follow the established patterns and produces different output than the iidy-js reference.

#### Required Implementation (Exact update-stack.rs Pattern)
Based on the expected output, exec-changeset should follow the exact same pattern as update-stack.rs with full data collection:

```rust
pub async fn exec_changeset(cli: &Cli, args: &ExecChangeSetArgs) -> Result<i32> {
    // Extract components from CLI (identical to update-stack.rs)
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;

    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = cli.command.to_cfn_operation();
    let stack_args = load_stack_args(
        &args.argsfile,
        &global_opts.environment,
        &operation,
        &cli_aws_settings,
    ).await?;

    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.stack_name.as_ref())?;

    let stack_name = final_stack_args
        .stack_name
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("Stack name is required"))?;

    // Setup AWS context
    let context = create_context_for_operation(&opts, operation).await?;

    // Setup data-driven output manager with full CLI context (identical to update-stack.rs)
    let aws_opts = AwsOpts {
        region: opts.region.clone(),
        profile: opts.profile.clone(),
        assume_role_arn: opts.assume_role_arn.clone(),
        client_request_token: Some(opts.client_request_token.value.clone()),
    };
    let cli = Cli {
        global_opts: global_opts.clone(),
        aws_opts,
        command: Commands::ExecChangeset(args.clone()),
    };
    let output_options = OutputOptions::new(cli);
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // 1. Show command metadata (exact iidy-js pattern)
    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // 2. Execute changeset operation
    let stack_id = perform_changeset_execution(&context, &final_stack_args, args, &mut output_manager).await?;

    // 3. Start parallel data collection and rendering (identical to update-stack.rs pattern)
    let sender = output_manager.start();
    
    // Start stack definition task
    let stack_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        tokio::spawn(async move {
            let stack = StackInfoService::get_stack(&client, &stack_id).await?;
            let output_data = convert_stack_to_definition(&stack, true);
            let _ = tx.send(output_data);
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Start previous events task (identical to update-stack.rs)
    let previous_events_task = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        tokio::spawn(async move {
            let events = StackEventsService::get_stack_events(&client, &stack_id, Some(10)).await?;
            let events_display = StackEventsDisplay {
                title: "Previous Stack Events (max 10):".to_string(),
                events,
            };
            let _ = tx.send(OutputData::StackEvents(events_display));
            Ok::<(), anyhow::Error>(())
        })
    };
    
    // Start live events task
    let events_task: tokio::task::JoinHandle<Result<Option<String>, anyhow::Error>> = {
        let client = context.client.clone();
        let stack_id = stack_id.clone();
        let tx = sender.clone();
        let context_clone = context.clone();
        
        tokio::spawn(async move {
            let sender_output = SenderOutput { sender: tx };
            let final_status = watch_stack_live_events_with_seen_events(
                &client, 
                &context_clone, 
                &stack_id, 
                sender_output, 
                std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS), 
                std::time::Duration::from_secs(3600), // 1 hour timeout
                vec![] // No previous events for changeset execution
            ).await?;
            Ok(final_status)
        })
    };
    
    // Drop the original sender so the receiver knows when all tasks are done
    drop(sender);
    
    // Process and render all data from parallel operations
    output_manager.stop().await?;
    
    // Wait for all tasks to complete and handle any errors
    let (stack_result, previous_events_result, events_result) = tokio::join!(
        stack_task,
        previous_events_task,
        events_task
    );
    
    // Propagate any errors from the spawned tasks
    stack_result??;
    previous_events_result??;
    let final_status = events_result??;
    
    let elapsed_seconds = context.elapsed_seconds().await?;
    let success = determine_operation_success(&final_status, CHANGESET_EXECUTE_SUCCESS_STATES);
    
    // Skip stack contents if the stack was deleted
    if let Some(ref status) = final_status {
        if status == "DELETE_COMPLETE" {
            let final_command_summary = create_final_command_summary(false, elapsed_seconds);
            output_manager.render(final_command_summary).await?;
            return Ok(1); // Return exit code 1 for failure
        }
    }
    
    // Show stack contents (identical to update-stack.rs)
    let stack_contents = collect_stack_contents(&context, &stack_id).await?;
    output_manager.render(OutputData::StackContents(stack_contents)).await?;
    
    let final_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_summary).await?;
    
    // Return appropriate exit code
    Ok(if success { 0 } else { 1 })
}

async fn perform_changeset_execution(
    context: &CfnContext,
    stack_args: &StackArgs,
    args: &ExecChangeSetArgs,
    output_manager: &mut DynamicOutputManager,
) -> Result<String> {
    // Setup request builder
    let builder = CfnRequestBuilder::new(context, stack_args);

    // Build and execute the ExecuteChangeSet request
    let (execute_request, token) = builder.build_execute_changeset(&args.changeset_name, &CfnOperation::ExecuteChangeset);
    
    // Show token info
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    let _response = execute_request.send().await?;

    // For ExecuteChangeset, we need to get the stack ID from the stack name
    let stack_id = StackInfoService::get_stack_id(&context.client, 
        stack_args.stack_name.as_ref().unwrap()).await?;

    Ok(stack_id)
}
```

### Key Changes for exec_changeset.rs:
1. **Add previous events collection** - Missing from current implementation
2. **Follow exact update-stack.rs pattern** - Same parallel data collection
3. **Include all expected sections** - command_metadata, stack_definition, stack_events, live_stack_events, stack_contents
4. **Proper error handling and exit codes** - Match other operations

## Testing Requirements

1. **Exit Code Verification**
   - Test that operations return 0 for success, 1 for failure
   - Verify error propagation maintains correct exit codes

2. **Output Consistency** 
   - Ensure changeset operations produce similar output structure to create/update
   - Verify JSON mode compatibility

3. **Data Collection**
   - Test that stack definition, events, and contents are properly collected
   - Verify parallel task error handling

4. **Section Integration**
   - Test that expected sections are rendered in correct order
   - Verify section buffering works correctly

## Benefits After Implementation

1. **Architectural Consistency**: All CFN operations follow the same data-driven patterns
2. **Better UX**: Users see complete stack information after changeset operations  
3. **Proper Exit Codes**: Shell scripting integration works correctly
4. **Enhanced Features**: Keyboard shortcuts, mode switching work for changeset operations
5. **Maintainability**: Consistent patterns across all operations make maintenance easier

This review shows that both changeset operations need significant refactoring to match the established architectural patterns. The current implementations are functional but don't provide the same level of user experience and integration as the other CFN operations.