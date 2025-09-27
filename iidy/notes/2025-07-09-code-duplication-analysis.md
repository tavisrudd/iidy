# Code Duplication Analysis - CloudFormation Command Handlers

**Date**: 2025-07-09  
**Status**: Analysis Complete  
**Priority**: High  

## Executive Summary

Significant code duplication exists across CloudFormation command handlers, particularly in:
- Stack existence checking
- Stack operation watching and summarization
- Changeset confirmation flows
- CLI context reconstruction
- Changeset processing logic

This duplication creates maintenance burden and increases the risk of inconsistent behavior across commands.

## Detailed Analysis

### 1. Stack Existence Checking

**Duplicate Function**: `check_stack_exists`

**Locations**:
- `src/cfn/changeset_operations.rs` lines 73-89 (public function)
- `src/cfn/create_or_update.rs` lines 168-184 (private duplicate)

**Code Snippet**:
```rust
// changeset_operations.rs:73-89
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
```

**Impact**: Identical logic exists in both places. The private version should be removed in favor of the public one.

### 2. Stack Watching and Summarization Pattern

**Duplicate Pattern**: Stack operation watching with summary generation

**Locations**:
- `src/cfn/create_or_update.rs` - `watch_and_summarize_stack_operation` function lines 107-165
- `src/cfn/exec_changeset.rs` - inline in `exec_changeset_impl` lines 43-123  
- `src/cfn/update_stack.rs` - inline in `update_stack_impl` lines 54-100

**Code Pattern**:
```rust
// Pattern repeated across all three files:
let stack_task = {
    let client = context.client.clone();
    let stack_id = stack_id.clone();
    tokio::spawn(async move {
        let stack = StackInfoService::get_stack(&client, &stack_id).await?;
        let output_data = convert_stack_to_definition(&stack, true);
        Ok::<OutputData, anyhow::Error>(output_data)
    })
};

// Render stack definition
output_manager.render(stack_task.await??).await?;

// Watch stack operation
let final_status = match watch_stack_with_data_output(
    context,
    &stack_id,
    output_manager,
    std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS),
).await {
    Ok(status) => status,
    Err(error) => {
        let error_info = convert_aws_error_to_error_info(&error);
        output_manager.render(OutputData::Error(error_info)).await?;
        return Ok(1);
    }
};

// Check for DELETE_COMPLETE and handle early exit
if let Some(ref status) = final_status {
    if status == "DELETE_COMPLETE" {
        let final_command_summary = create_final_command_summary(false, elapsed_seconds);
        output_manager.render(final_command_summary).await?;
        return Ok(1);
    }
}

// Collect and render stack contents
let stack_contents = collect_stack_contents(context, &stack_id).await?;
output_manager.render(OutputData::StackContents(stack_contents)).await?;

// Final summary
let final_summary = create_final_command_summary(success, elapsed_seconds);
output_manager.render(final_summary).await?;
```

**Impact**: This 50+ line pattern is repeated 3 times with minor variations.

### 3. Changeset Confirmation Pattern

**Duplicate Pattern**: User confirmation before changeset execution

**Locations**:
- `src/cfn/create_or_update.rs` - `update_stack_with_changeset_data` lines 302-315
- `src/cfn/create_or_update.rs` - `create_stack_with_changeset_data` lines 380-394  
- `src/cfn/update_stack.rs` - `update_stack_with_changeset` lines 177-188

**Code Pattern**:
```rust
// Pattern repeated across all three locations:
let confirmed = if args.yes {
    true
} else {
    output_manager.request_confirmation_with_key(
        "Do you want to execute this changeset now?".to_string(),
        "execute_changeset".to_string()
    ).await?
};

if !confirmed {
    let elapsed = context.elapsed_seconds().await?;
    output_manager.render(create_command_result(true, elapsed, Some("Changeset execution declined".to_string()))).await?;
    return Ok(130); // 130 = interrupted by user (Ctrl-C equivalent)
}
```

**Impact**: Identical confirmation logic repeated 3 times.

### 4. CLI Context Reconstruction Pattern

**Duplicate Pattern**: Reconstructing CLI context to call `exec_changeset`

**Locations**:
- `src/cfn/create_or_update.rs` - `update_stack_with_changeset_data` lines 319-349
- `src/cfn/create_or_update.rs` - `create_stack_with_changeset_data` lines 398-420
- `src/cfn/update_stack.rs` - `update_stack_with_changeset` lines 192-204

**Code Pattern**:
```rust
// Pattern repeated across all three locations:
let exec_args = ExecChangeSetArgs {
    changeset_name: changeset_result.changeset_name,
    argsfile: args.base.argsfile.clone(),
    stack_name: Some(changeset_result.stack_name),
};

let exec_cli = Cli {
    global_opts: cli.global_opts.clone(),
    aws_opts: cli.aws_opts.clone(),
    command: Commands::ExecChangeset(exec_args.clone()),
};

exec_changeset::exec_changeset(&exec_cli, &exec_args).await
```

**Impact**: This pattern suggests a design issue where context is lost and must be reconstructed.

### 5. Changeset Processing Logic 

**Duplicate Pattern**: Processing changeset changes for display

**Locations**:
- `src/cfn/changeset_operations.rs` - `fetch_pending_changesets` lines 369-435
- `src/cfn/stack_operations.rs` - `collect_pending_changesets` lines 94-168

**Code Pattern**:
```rust
// Near-identical changeset processing in both files:
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
                    .iter().map(|detail| ChangeDetail {
                        target: detail.target().and_then(|t| t.name()).unwrap_or("").to_string(),
                        evaluation: detail.evaluation().map(|e| e.as_str().to_string()),
                        change_source: detail.change_source().map(|cs| cs.as_str().to_string()),
                        causing_entity: detail.causing_entity().map(|ce| ce.to_string()),
                    }).collect(),
            });
        }
    }
}
```

**Impact**: Complex changeset processing logic duplicated across two files.

### 6. Stack Definition Fetching Pattern

**Duplicate Pattern**: Fetching stack and converting to definition

**Locations**:
- `src/cfn/create_or_update.rs` - `update_stack_with_changeset_data` lines 268-279
- `src/cfn/create_or_update.rs` - `create_stack_with_changeset_data` lines 374-376
- `src/cfn/update_stack.rs` - `update_stack_with_changeset` lines 150-160
- `src/cfn/exec_changeset.rs` - lines 43-51

**Code Pattern**:
```rust
// Pattern repeated across multiple files:
let stack_task = {
    let client = context.client.clone();
    let stack_name = stack_name.clone();
    tokio::spawn(async move {
        let stack = StackInfoService::get_stack(&client, &stack_name).await?;
        let output_data = convert_stack_to_definition(&stack, true);
        Ok::<OutputData, anyhow::Error>(output_data)
    })
};

output_manager.render(stack_task.await??).await?;
```

**Impact**: This spawning pattern appears 4+ times across the codebase.

## Additional Observations

### 7. AWS Request Building Logic

**Potential Duplication**: The `src/cfn/request_builder.rs` file contains extensive logic for building AWS requests, but similar patterns may exist in `src/cfn/changeset_operations.rs` lines 195-306 in the `build_create_changeset_with_type` function:

```rust
// Similar capabilities conversion pattern in both files:
let aws_capabilities: Vec<Capability> = capabilities
    .iter()
    .filter_map(|cap| match cap.as_str() {
        "CAPABILITY_IAM" => Some(Capability::CapabilityIam),
        "CAPABILITY_NAMED_IAM" => Some(Capability::CapabilityNamedIam),
        "CAPABILITY_AUTO_EXPAND" => Some(Capability::CapabilityAutoExpand),
        _ => None,
    })
    .collect();
```

This pattern appears in both `request_builder.rs` and `changeset_operations.rs`.

## Extended Analysis - Additional CloudFormation Files

### 8. AWS Error Handling Pattern

**Duplicate Function**: `handle_aws_error`

**Locations**:
- `src/cfn/get_stack_template.rs` lines 15-25
- `src/cfn/watch_stack.rs` lines 27-36 (identical implementation)

**Code Pattern**:
```rust
// Identical function in both files:
async fn handle_aws_error<T>(result: Result<T>, output_manager: &mut DynamicOutputManager) -> Result<Option<T>> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(e) => {
            let error_info = convert_aws_error_to_error_info(&e);
            output_manager.render(OutputData::Error(error_info)).await?;
            Ok(None) // Signal failure
        }
    }
}
```

**Impact**: Generic error handling logic duplicated across multiple files.

### 9. S3 Template Existence Check

**Duplicate Function**: `check_template_exists`

**Locations**:
- `src/cfn/template_approval_request.rs` lines 107-122
- `src/cfn/template_approval_review.rs` lines 135-149 (identical implementation)

**Code Pattern**:
```rust
// Identical S3 existence check in both files:
async fn check_template_exists(s3_client: &aws_sdk_s3::Client, bucket: &str, key: &str) -> Result<bool> {
    match s3_client.head_object()
        .bucket(bucket)
        .key(key)
        .send()
        .await {
        Ok(_) => Ok(true),
        Err(e) => {
            if e.to_string().contains("NotFound") {
                return Ok(false);
            }
            Err(e.into())
        }
    }
}
```

**Impact**: S3 template validation logic duplicated across template approval files.

### 10. Stack Information Fetching Extended

**Additional Locations**:
- `src/cfn/create_stack.rs` lines 47-57
- `src/cfn/delete_stack.rs` lines 125-137 (previous events variant)
- `src/cfn/describe_stack.rs` lines 20-39 (different implementation)
- `src/cfn/watch_stack.rs` (multiple similar patterns)

**Code Pattern**:
```rust
// create_stack.rs pattern:
let stack_task = {
    let client = context.client.clone();
    let stack_id = stack_id.clone();
    tokio::spawn(async move {
        let stack = StackInfoService::get_stack(&client, &stack_id).await?;
        let output_data = convert_stack_to_definition(&stack, true);
        Ok::<OutputData, anyhow::Error>(output_data)
    })
};

// delete_stack.rs variant (previous events):
let previous_events_task = {
    let client = context.client.clone();
    let stack_id = stack_id.clone();
    tokio::spawn(async move {
        let events = StackEventsService::fetch_events(&client, &stack_id).await?;
        let events_display = convert_stack_events_to_display_with_max(
            events,
            "Previous Stack Events (max 10):",
            Some(10),
        );
        Ok::<OutputData, anyhow::Error>(events_display)
    })
};
```

**Impact**: Stack information fetching patterns now found in 7+ files with variations.

### 11. Watch Stack Integration

**Duplicate Pattern**: Watch stack integration with live events

**Locations**:
- `src/cfn/create_stack.rs` lines 60-79
- `src/cfn/delete_stack.rs` lines 167-186 (near identical)

**Code Pattern**:
```rust
// Near-identical watch stack integration:
let final_status = {
    use crate::cfn::watch_stack::{ManagerOutput, watch_stack_live_events_with_seen_events};
    
    let manager_output = ManagerOutput { manager: output_manager };
    match watch_stack_live_events_with_seen_events(
        &context.client, 
        context, 
        &stack_id, 
        manager_output,
        std::time::Duration::from_secs(DEFAULT_POLL_INTERVAL_SECS), 
        std::time::Duration::from_secs(3600),
        vec![]
    ).await {
        Ok(status) => status,
        Err(error) => {
            let error_info = convert_aws_error_to_error_info(&error);
            output_manager.render(OutputData::Error(error_info)).await?;
            return Ok(1);
        }
    }
};
```

**Impact**: Complex watch stack integration duplicated across create/delete operations.

### 12. Command Metadata Creation

**Duplicate Pattern**: Command metadata creation

**Locations**:
- `src/cfn/create_stack.rs` line 42
- `src/cfn/delete_stack.rs` line 119
- `src/cfn/template_approval_request.rs` (similar pattern)
- `src/cfn/template_approval_review.rs` (similar pattern)

**Code Pattern**:
```rust
// Command metadata creation repeated across files:
let command_metadata = create_command_metadata(context, opts, &final_stack_args, &global_opts.environment).await?;
output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;
```

**Impact**: Boilerplate command metadata creation duplicated across multiple files.

### 13. Template Loading Pattern

**Duplicate Pattern**: Template loading and validation

**Locations**:
- `src/cfn/estimate_cost.rs` lines 35-45
- `src/cfn/template_approval_request.rs` lines 48-58 (similar structure)

**Code Pattern**:
```rust
// Template loading pattern:
let template_result = if let Some(ref template_location) = final_stack_args.template {
    load_cfn_template(
        Some(template_location),
        &args.argsfile,
        Some(&global_opts.environment),
        TEMPLATE_MAX_BYTES,
        Some(&context.create_s3_client()),
    ).await?
} else {
    return Err(anyhow::anyhow!("Template must be specified in stack-args.yaml"));
};
```

**Impact**: Template loading logic duplicated with minor variations.

## Updated Metrics

- **Total duplicate lines**: ~500+ lines (increased from 300+)
- **Files affected**: 12+ primary files (increased from 5)
- **Patterns identified**: 13 major duplication patterns (increased from 7)
- **Estimated effort**: 4-5 days to refactor with proper testing (increased from 2-3 days)

## Recommendations

### High Priority
1. **Remove duplicate `check_stack_exists`** - Use the public function from `changeset_operations.rs`
2. **Extract stack watching pattern** - Create shared function for the stack operation watching and summarization
3. **Consolidate changeset confirmation** - Create shared confirmation helper function
4. **Refactor CLI reconstruction** - This indicates a design issue that should be addressed

### Medium Priority  
5. **Unify changeset processing** - Consolidate changeset change processing logic
6. **Extract stack definition fetching** - Create shared helper for the spawn pattern
7. **Review request building** - Ensure `request_builder.rs` is used consistently

### Metrics
- **Total duplicate lines**: ~300+ lines
- **Files affected**: 5 primary files
- **Patterns identified**: 7 major duplication patterns
- **Estimated effort**: 2-3 days to refactor with proper testing

## Next Steps

1. Create refactoring plan prioritizing high-impact duplications
2. Implement shared utilities in `stack_operations.rs` 
3. Update all command handlers to use shared functions
4. Add tests to ensure behavior consistency
5. Remove obsolete duplicate code

This analysis should guide the creation of a systematic cleanup plan to eliminate duplication while maintaining current functionality.

## AWS API Operations Analysis

The following comprehensive analysis catalogs all AWS API method calls found in the src/cfn/ directory, organized by AWS service and operation type.

### CloudFormation Operations

#### describe_stacks

```bash
rg -n "\.describe_stacks\(" src/cfn/
```

Locations:
- [[file:src/cfn/changeset_operations.rs::29][changeset_operations.rs:29 - check_stack_state]]
- [[file:src/cfn/changeset_operations.rs::74][changeset_operations.rs:74 - check_stack_exists]]
- [[file:src/cfn/create_or_update.rs::169][create_or_update.rs:169 - check_stack_exists]]
- [[file:src/cfn/delete_stack.rs::27][delete_stack.rs:27 - check_stack_exists_for_delete]]
- [[file:src/cfn/describe_stack.rs::25][describe_stack.rs:25 - describe_stack_impl]]
- [[file:src/cfn/describe_stack_drift.rs::30][describe_stack_drift.rs:30 - describe_stack_drift_impl]]
- [[file:src/cfn/list_stacks.rs::82][list_stacks.rs:82 - list_stacks_impl]]
- [[file:src/cfn/stack_operations.rs::43][stack_operations.rs:43 - collect_stack_contents]]
- [[file:src/cfn/stack_operations.rs::288][stack_operations.rs:288 - StackInfoService::get_stack]]

#### create_stack

```bash
rg -n "\.create_stack\(" src/cfn/
```

Locations:
- [[file:src/cfn/request_builder.rs::60][request_builder.rs:60 - CfnRequestBuilder::build_create_stack]]

#### update_stack

```bash
rg -n "\.update_stack\(" src/cfn/
```

Locations:
- [[file:src/cfn/request_builder.rs::208][request_builder.rs:208 - CfnRequestBuilder::build_update_stack]]

#### delete_stack

```bash
rg -n "\.delete_stack\(" src/cfn/
```

Locations:
- [[file:src/cfn/delete_stack.rs::63][delete_stack.rs:63 - perform_stack_deletion_without_output]]

#### create_change_set

```bash
rg -n "\.create_change_set\(" src/cfn/
```

Locations:
- [[file:src/cfn/changeset_operations.rs::216][changeset_operations.rs:216 - perform_changeset_creation]]
- [[file:src/cfn/request_builder.rs::316][request_builder.rs:316 - CfnRequestBuilder::build_create_changeset]]

#### describe_change_set

```bash
rg -n "\.describe_change_set\(" src/cfn/
```

Locations:
- [[file:src/cfn/changeset_operations.rs::386][changeset_operations.rs:386 - fetch_pending_changesets]]
- [[file:src/cfn/changeset_operations.rs::456][changeset_operations.rs:456 - wait_for_changeset_completion]]
- [[file:src/cfn/changeset_operations.rs::494][changeset_operations.rs:494 - build_existing_changeset_result]]
- [[file:src/cfn/stack_operations.rs::116][stack_operations.rs:116 - collect_pending_changesets]]

#### execute_change_set

```bash
rg -n "\.execute_change_set\(" src/cfn/
```

Locations:
- [[file:src/cfn/request_builder.rs::410][request_builder.rs:410 - CfnRequestBuilder::build_execute_changeset]]

#### list_change_sets

```bash
rg -n "\.list_change_sets\(" src/cfn/
```

Locations:
- [[file:src/cfn/changeset_operations.rs::58][changeset_operations.rs:58 - check_existing_changesets]]
- [[file:src/cfn/changeset_operations.rs::374][changeset_operations.rs:374 - fetch_pending_changesets]]
- [[file:src/cfn/stack_operations.rs::99][stack_operations.rs:99 - collect_pending_changesets]]

#### describe_stack_events

```bash
rg -n "\.describe_stack_events\(" src/cfn/
```

Locations:
- [[file:src/cfn/describe_stack.rs::46][describe_stack.rs:46 - describe_stack_impl events_task]]
- [[file:src/cfn/describe_stack.rs::56][describe_stack.rs:56 - describe_stack_impl events_task pagination]]
- [[file:src/cfn/stack_operations.rs::177][stack_operations.rs:177 - StackEventsService::fetch_events]]

#### describe_stack_resources

```bash
rg -n "\.describe_stack_resources\(" src/cfn/
```

Locations:
- [[file:src/cfn/stack_operations.rs::34][stack_operations.rs:34 - collect_stack_contents]]

#### get_template

```bash
rg -n "\.get_template\(" src/cfn/
```

Locations:
- [[file:src/cfn/get_stack_template.rs::132][get_stack_template.rs:132 - get_stack_template]]

#### estimate_template_cost

```bash
rg -n "\.estimate_template_cost\(" src/cfn/
```

Locations:
- [[file:src/cfn/estimate_cost.rs::59][estimate_cost.rs:59 - estimate_cost_impl]]

#### validate_template

```bash
rg -n "\.validate_template\(" src/cfn/
```

Locations:
- [[file:src/cfn/template_approval_request.rs::137][template_approval_request.rs:137 - validate_template]]

#### detect_stack_drift

```bash
rg -n "\.detect_stack_drift\(" src/cfn/
```

Locations:
- [[file:src/cfn/describe_stack_drift.rs::75][describe_stack_drift.rs:75 - describe_stack_drift_impl]]

#### describe_stack_drift_detection_status

```bash
rg -n "\.describe_stack_drift_detection_status\(" src/cfn/
```

Locations:
- [[file:src/cfn/describe_stack_drift.rs::84][describe_stack_drift.rs:84 - describe_stack_drift_impl]]

#### describe_stack_resource_drifts

```bash
rg -n "\.describe_stack_resource_drifts\(" src/cfn/
```

Locations:
- [[file:src/cfn/describe_stack_drift.rs::115][describe_stack_drift.rs:115 - collect_stack_drift_data]]

### S3 Operations

#### head_object

```bash
rg -n "\.head_object\(" src/cfn/
```

Locations:
- [[file:src/cfn/template_approval_request.rs::108][template_approval_request.rs:108 - check_template_exists]]
- [[file:src/cfn/template_approval_review.rs::136][template_approval_review.rs:136 - check_template_exists]]

#### get_object

```bash
rg -n "\.get_object\(" src/cfn/
```

Locations:
- [[file:src/cfn/template_approval_review.rs::153][template_approval_review.rs:153 - download_template]]
- [[file:src/cfn/template_loader.rs::320][template_loader.rs:320 - maybe_sign_s3_http_url]]

#### put_object

```bash
rg -n "\.put_object\(" src/cfn/
```

Locations:
- [[file:src/cfn/template_approval_request.rs::125][template_approval_request.rs:125 - upload_template_to_s3]]
- [[file:src/cfn/template_approval_review.rs::198][template_approval_review.rs:198 - approve_template (approved location)]]
- [[file:src/cfn/template_approval_review.rs::207][template_approval_review.rs:207 - approve_template (latest copy)]]

#### delete_object

```bash
rg -n "\.delete_object\(" src/cfn/
```

Locations:
- [[file:src/cfn/template_approval_review.rs::216][template_approval_review.rs:216 - approve_template (cleanup)]]

### Operation Summary by File

#### changeset_operations.rs
CloudFormation operations for changeset management:
- describe_stacks: Stack existence checks
- list_change_sets: Finding existing changesets
- create_change_set: Creating new changesets
- describe_change_set: Fetching changeset details and monitoring status

#### create_or_update.rs
Primary stack lifecycle operations:
- describe_stacks: Stack existence detection for create vs update logic

#### create_stack.rs
Stack creation operations:
- Uses CfnRequestBuilder which internally calls create_stack

#### update_stack.rs
Stack update operations:
- Uses CfnRequestBuilder which internally calls update_stack

#### delete_stack.rs
Stack deletion operations:
- describe_stacks: Pre-deletion existence check
- delete_stack: Actual deletion operation

#### describe_stack.rs
Stack information retrieval:
- describe_stacks: Basic stack information
- describe_stack_events: Stack event history with pagination

#### describe_stack_drift.rs
Stack drift detection and analysis:
- describe_stacks: Stack information for drift analysis
- detect_stack_drift: Initiate drift detection
- describe_stack_drift_detection_status: Monitor drift detection progress
- describe_stack_resource_drifts: Retrieve detailed drift information

#### estimate_cost.rs
Cost estimation:
- estimate_template_cost: Generate cost estimation URLs

#### get_stack_template.rs
Template retrieval:
- get_template: Download stack templates in various formats

#### list_stacks.rs
Stack listing and filtering:
- describe_stacks: Retrieve all stacks with pagination

#### request_builder.rs
Request building utilities:
- create_stack: Stack creation request building
- update_stack: Stack update request building
- create_change_set: Changeset creation request building
- execute_change_set: Changeset execution request building

#### stack_operations.rs
Shared stack operations:
- describe_stacks: Stack information retrieval
- describe_stack_resources: Resource information
- describe_stack_events: Event retrieval
- list_change_sets: Changeset listing
- describe_change_set: Changeset details

#### template_approval_request.rs
Template approval workflow (S3 integration):
- validate_template: Template validation
- head_object: Check template existence in S3
- put_object: Upload pending templates to S3

#### template_approval_review.rs
Template approval review workflow:
- head_object: Check template existence
- get_object: Download templates for comparison
- put_object: Upload approved templates
- delete_object: Cleanup pending templates

#### template_loader.rs
Template loading with S3 support:
- get_object: Download templates from S3 (with presigned URLs)

### Usage Patterns

#### Authentication and Token Management
All CloudFormation operations use client_request_token or client_token fields for idempotency, managed by the CfnRequestBuilder pattern.

#### Error Handling
Consistent error handling pattern using SdkError::ServiceError for distinguishing between different error types (e.g., "ValidationError" for non-existent resources).

#### Pagination
Several operations implement pagination:
- describe_stacks in list_stacks.rs
- describe_stack_events in describe_stack.rs
- describe_stack_resource_drifts in describe_stack_drift.rs

#### Async Operations
All AWS operations are async and use tokio::spawn for concurrent execution where appropriate.

#### S3 Integration
Template approval system uses S3 for storing and managing CloudFormation templates with proper ACL settings (BucketOwnerFullControl).