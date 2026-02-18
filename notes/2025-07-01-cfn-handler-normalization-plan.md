# CloudFormation Handler Normalization Plan

**Date:** 2025-07-01  
**Status:** Planning  
**Goal:** Normalize verbose code blocks in CloudFormation handlers before larger structural refactoring

## Overview

After standardizing all CloudFormation handlers to accept `&Cli`, we identified several small, verbose code patterns that can be normalized with minimal risk. This document details each pattern with concrete before/after examples and implementation approach.

## Pattern Analysis with Examples

### ✅ Pattern 1: Environment Option Wrapping (8+ handlers) - COMPLETED

**Found in:** create_stack.rs, update_stack.rs, create_changeset.rs, exec_changeset.rs, estimate_cost.rs

**Previous unnecessary pattern:**
```rust
// create_stack.rs lines 33-38
let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
let operation = CfnOperation::CreateStack;
let stack_args = load_stack_args(
    &args.base.argsfile,
    Some(&global_opts.environment),  // <-- Unnecessary Option wrapping
    &operation,
    &cli_aws_settings,
).await?;
```

**Root cause:** `load_stack_args` was taking `Option<&str>` for environment, but every caller passed `Some(&global_opts.environment)`, making the Option unnecessary.

**After normalization:**
```rust
// Updated load_stack_args signature:
pub async fn load_stack_args(
    argsfile: &str,
    environment: &str,  // Direct &str, no Option
    operation: &CfnOperation,
    cli_aws_settings: &AwsSettings,
) -> Result<StackArgs>

// Usage in handlers:
let stack_args = load_stack_args(
    &args.base.argsfile,
    &global_opts.environment,  // Direct reference, no Some() wrapper
    &operation,
    &cli_aws_settings,
).await?;
```

**✅ COMPLETED:** 
- Updated `load_stack_args` function signature to require `&str` instead of `Option<&str>`
- Removed `Some()` wrappers from all 6 CloudFormation handlers
- All 593 tests continue to pass
- Commit: `c602c8b` - "refactor: remove unnecessary Option wrapper from load_stack_args environment parameter"

**Benefits:** Eliminates unnecessary Option wrapping, simplifies function signature, makes environment requirement explicit

### ✅ Pattern 2: Stack Name Override + Validation (6+ handlers) - COMPLETED

**Found in:** create_stack.rs, update_stack.rs, create_changeset.rs, exec_changeset.rs

**Current verbose pattern:**
```rust
// create_stack.rs lines 40-47
let mut final_stack_args = stack_args;
if let Some(ref stack_name) = args.base.stack_name {
    final_stack_args.stack_name = Some(stack_name.clone());
}

if final_stack_args.stack_name.is_none() {
    anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
}

// update_stack.rs lines 40-47 (identical pattern)
let mut final_stack_args = stack_args;
if let Some(ref stack_name) = args.base.stack_name {
    final_stack_args.stack_name = Some(stack_name.clone());
}

if final_stack_args.stack_name.is_none() {
    anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
}
```

**After normalization:**
```rust
// Helper function in src/cfn/mod.rs
pub fn apply_stack_name_override_and_validate(
    mut stack_args: StackArgs, 
    cli_stack_name: Option<&String>
) -> Result<StackArgs> {
    if let Some(stack_name) = cli_stack_name {
        stack_args.stack_name = Some(stack_name.clone());
    }
    
    if stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }
    
    Ok(stack_args)
}

// Usage in handlers:
let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.base.stack_name.as_ref())?;
```

**✅ COMPLETED:**
- Added `apply_stack_name_override_and_validate` helper function to `src/cfn/mod.rs`
- Updated 5 handlers: create_stack.rs, update_stack.rs, create_changeset.rs, exec_changeset.rs, create_or_update.rs
- All 593 tests continue to pass
- Reduces 8 lines to 1 line in each handler

**Benefits:** Reduces 8 lines to 1 line, centralizes validation logic, eliminates duplication

### Pattern 3: Token Display (6+ handlers)

**Found in:** create_stack.rs, update_stack.rs, delete_stack.rs, create_changeset.rs, exec_changeset.rs

**Current verbose pattern:**
```rust
// create_stack.rs lines 65-66
let output_token = convert_token_info(&token);
output_manager.render(OutputData::TokenInfo(output_token)).await?;

// update_stack.rs lines 65-66 (identical)
let output_token = convert_token_info(&token);
output_manager.render(OutputData::TokenInfo(output_token)).await?;

// delete_stack.rs lines 26-27 (identical)
let output_token = convert_token_info(&token);
output_manager.render(OutputData::TokenInfo(output_token)).await?;
```

**After normalization:**
```rust
// Helper function in src/cfn/mod.rs
pub async fn render_token_info(
    output_manager: &mut DynamicOutputManager, 
    token: &TokenInfo
) -> Result<()> {
    let output_token = convert_token_info(token);
    output_manager.render(OutputData::TokenInfo(output_token)).await
}

// Usage in handlers:
render_token_info(&mut output_manager, &token).await?;
```

**Benefits:** Reduces 2 lines to 1 line, ensures consistent token rendering

### Pattern 4: Success State Determination (3+ handlers)

**Found in:** create_stack.rs, delete_stack.rs, update_stack.rs

**Current verbose pattern:**
```rust
// create_stack.rs lines 98-101
let expected_success_states = ["CREATE_COMPLETE"];
let success = final_status.as_ref()
    .map(|status| expected_success_states.contains(&status.as_str()))
    .unwrap_or(false);

// delete_stack.rs lines 145-148  
let expected_success_states = ["DELETE_COMPLETE"];
let success = final_status.as_ref()
    .map(|status| expected_success_states.contains(&status.as_str()))
    .unwrap_or(false);

// update_stack.rs lines 98-101
let expected_success_states = ["UPDATE_COMPLETE"];
let success = final_status.as_ref()
    .map(|status| expected_success_states.contains(&status.as_str()))
    .unwrap_or(false);
```

**After normalization:**
```rust
// Helper function in src/cfn/mod.rs
pub fn determine_operation_success(final_status: &Option<String>, expected_states: &[&str]) -> bool {
    final_status.as_ref()
        .map(|status| expected_states.contains(&status.as_str()))
        .unwrap_or(false)
}

// Constants for each operation
pub const CREATE_SUCCESS_STATES: &[&str] = &["CREATE_COMPLETE"];
pub const UPDATE_SUCCESS_STATES: &[&str] = &["UPDATE_COMPLETE"];  
pub const DELETE_SUCCESS_STATES: &[&str] = &["DELETE_COMPLETE"];

// Usage in handlers:
let success = determine_operation_success(&final_status, CREATE_SUCCESS_STATES);
```

**Benefits:** Reduces 4 lines to 1 line, centralizes success logic, makes expected states explicit

### Pattern 5: Elapsed Time + Final Summary (3+ handlers)

**Found in:** create_stack.rs, delete_stack.rs, update_stack.rs

**Current verbose pattern:**
```rust
// create_stack.rs lines 103-105
let elapsed_seconds = (Utc::now() - start_time).num_seconds();
let final_command_summary = create_final_command_summary(success, elapsed_seconds);
output_manager.render(final_command_summary).await?;

// delete_stack.rs lines 150-152 (identical)
let elapsed_seconds = (Utc::now() - start_time).num_seconds();
let final_command_summary = create_final_command_summary(success, elapsed_seconds);
output_manager.render(final_command_summary).await?;
```

**After normalization:**
```rust
// Helper function in src/cfn/mod.rs
pub async fn render_final_summary(
    output_manager: &mut DynamicOutputManager,
    start_time: DateTime<Utc>,
    success: bool
) -> Result<()> {
    let elapsed_seconds = (Utc::now() - start_time).num_seconds();
    let final_command_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_command_summary).await
}

// Usage in handlers:
render_final_summary(&mut output_manager, start_time, success).await?;
```

**Benefits:** Reduces 3 lines to 1 line, consistent summary rendering

## Complete Before/After Example

### Before (create_stack.rs excerpt):
```rust
pub async fn create_stack(cli: &Cli) -> Result<i32> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;
    let args = match &cli.command {
        Commands::CreateStack(args) => args,
        _ => anyhow::bail!("Invalid command type for create_stack"),
    };

    // Load stack configuration with full context
    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = CfnOperation::CreateStack;
    let stack_args = load_stack_args(
        &args.base.argsfile,
        Some(&global_opts.environment),
        &operation,
        &cli_aws_settings,
    ).await?;

    // Override stack name if provided via CLI
    let mut final_stack_args = stack_args;
    if let Some(ref stack_name) = args.base.stack_name {
        final_stack_args.stack_name = Some(stack_name.clone());
    }

    if final_stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }

    // ... rest of function with token rendering, success determination, final summary
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;
    
    // ... later in function
    let expected_success_states = ["CREATE_COMPLETE"];
    let success = final_status.as_ref()
        .map(|status| expected_success_states.contains(&status.as_str()))
        .unwrap_or(false);

    let elapsed_seconds = (Utc::now() - start_time).num_seconds();
    let final_command_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_command_summary).await?;

    Ok(if success { 0 } else { 1 })
}
```

### After normalization:
```rust
pub async fn create_stack(cli: &Cli) -> Result<i32> {
    // Extract components from CLI  
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;
    let args = extract_create_stack_args(cli)?;

    // Load and prepare stack configuration
    let stack_args = load_operation_stack_args(&opts, CfnOperation::CreateStack, &args.base.argsfile, &global_opts.environment).await?;
    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.base.stack_name.as_ref())?;

    // ... rest of function logic
    render_token_info(&mut output_manager, &token).await?;
    
    // ... later in function
    let success = determine_operation_success(&final_status, CREATE_SUCCESS_STATES);
    render_final_summary(&mut output_manager, start_time, success).await?;

    Ok(if success { 0 } else { 1 })
}
```

**Summary:** Reduced from 23 lines of boilerplate to 7 lines, with much clearer intent.

## Implementation Plan

### Phase 1: Add Helper Functions (1 commit)
1. Add all helper functions to `src/cfn/mod.rs`
2. Add necessary imports and exports
3. Ensure compilation with `cargo check --all`

### Phase 2: Apply to High-Usage Handlers (3 commits)
1. **Commit 1:** Refactor create_stack.rs and update_stack.rs (most complex handlers)
2. **Commit 2:** Refactor create_changeset.rs and exec_changeset.rs
3. **Commit 3:** Refactor delete_stack.rs and remaining handlers

### Phase 3: Cleanup (1 commit)
1. Remove any unused helper variants
2. Update documentation
3. Final test run

## Risk Assessment

**Low Risk Changes:**
- CLI extraction helpers (pure refactoring)
- Token rendering helpers (no logic changes)
- Success determination (pure logic extraction)

**Medium Risk Changes:**
- Stack args loading (combines multiple operations)
- Stack name override + validation (error message changes)

**Mitigation:**
- All changes preserve exact same error messages
- Each commit is small and easily revertible
- Full test suite run after each change
- No behavior changes, only code organization

## Testing Strategy

1. **After each commit:** Run `cargo nextest r --color=never --hide-progress-bar`
2. **Verify identical behavior:** Compare error messages and outputs
3. **Integration testing:** Test real CloudFormation operations if possible
4. **Code review:** Ensure helper functions are well-documented

## Files Modified

**New file:**
- Helper functions added to `src/cfn/mod.rs`

**Modified files:**
- `src/cfn/create_stack.rs`
- `src/cfn/update_stack.rs` 
- `src/cfn/delete_stack.rs`
- `src/cfn/create_changeset.rs`
- `src/cfn/exec_changeset.rs`
- `src/cfn/estimate_cost.rs`
- `src/cfn/get_stack_template.rs`

## Additional Patterns Identified

### Pattern 6: Context Creation (10+ handlers)

**Found in:** Most handlers use this pattern

**Current verbose pattern:**
```rust
// create_stack.rs line 49
let context = create_context_for_operation(&opts, CfnOperation::CreateStack).await?;

// update_stack.rs line 49  
let context = create_context_for_operation(&opts, CfnOperation::UpdateStack).await?;

// delete_stack.rs line 82
let context = create_context_for_operation(&opts, CfnOperation::DeleteStack).await?;
```

**After normalization:**
```rust
// Helper function in src/cfn/mod.rs
pub async fn create_operation_context(opts: &NormalizedAwsOpts, operation: CfnOperation) -> Result<CfnContext> {
    create_context_for_operation(opts, operation).await
}

// Usage in handlers:
let context = create_operation_context(&opts, CfnOperation::CreateStack).await?;
```

**Benefits:** Minor reduction but creates consistency, easier to enhance context creation later

### Pattern 7: Stack ID Extraction from Responses (3+ handlers)

**Found in:** create_stack.rs, update_stack.rs

**Current verbose pattern:**
```rust
// create_stack.rs lines 78-80
let stack_id = response.stack_id()
    .ok_or_else(|| anyhow::anyhow!("Stack creation response did not include stack ID"))?
    .to_string();

// update_stack.rs lines 78-80 (similar)
let stack_id = response.stack_id()
    .ok_or_else(|| anyhow::anyhow!("Stack update response did not include stack ID"))?
    .to_string();
```

**After normalization:**
```rust
// Helper function in src/cfn/mod.rs
pub fn extract_stack_id_from_response<T>(response: &T, operation_name: &str) -> Result<String>
where 
    T: StackIdProvider // trait for responses that have stack_id()
{
    response.stack_id()
        .ok_or_else(|| anyhow::anyhow!("{} response did not include stack ID", operation_name))?
        .to_string()
        .map_err(Into::into)
}

// Usage in handlers:
let stack_id = extract_stack_id_from_response(&response, "Stack creation")?;
```

**Benefits:** Reduces 3 lines to 1, consistent error messages, type-safe

### Pattern 8: Output Manager Setup (12+ handlers)

**Found in:** All handlers

**Current verbose pattern:**
```rust
// create_stack.rs lines 51-55
let output_options = crate::output::manager::OutputOptions::new(cli.clone());
let mut output_manager = DynamicOutputManager::new(
    global_opts.effective_output_mode(),
    output_options
).await?;

// estimate_cost.rs lines 24-28 (minimal variant)
let output_options = OutputOptions::minimal();
let mut output_manager = DynamicOutputManager::new(
    global_opts.effective_output_mode(),
    output_options
).await?;
```

**After normalization:**
```rust
// Helper functions in src/cfn/mod.rs
pub async fn setup_full_output_manager(cli: &Cli) -> Result<DynamicOutputManager> {
    let output_options = crate::output::manager::OutputOptions::new(cli.clone());
    DynamicOutputManager::new(
        cli.global_opts.effective_output_mode(),
        output_options
    ).await
}

pub async fn setup_minimal_output_manager(global_opts: &GlobalOpts) -> Result<DynamicOutputManager> {
    let output_options = OutputOptions::minimal();
    DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await
}

// Usage in handlers:
let mut output_manager = setup_full_output_manager(cli).await?;
// or
let mut output_manager = setup_minimal_output_manager(global_opts).await?;
```

**Benefits:** Reduces 5 lines to 1, consistent setup patterns

### Pattern 9: Command Metadata Creation (3+ handlers)

**Found in:** create_stack.rs, update_stack.rs, delete_stack.rs

**Current verbose pattern:**
```rust
// create_stack.rs lines 57-58
let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

// update_stack.rs lines 57-58 (identical)
let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;
```

**After normalization:**
```rust
// Helper function in src/cfn/mod.rs
pub async fn render_command_metadata(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    opts: &NormalizedAwsOpts,
    stack_args: &StackArgs,
    environment: &str
) -> Result<()> {
    let command_metadata = create_command_metadata(context, opts, stack_args, environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await
}

// Usage in handlers:
render_command_metadata(&mut output_manager, &context, &opts, &final_stack_args, &global_opts.environment).await?;
```

**Benefits:** Reduces 2 lines to 1, ensures consistent metadata rendering

### Pattern 10: Parallel Task Management Setup (4+ handlers)

**Found in:** create_stack.rs, update_stack.rs, delete_stack.rs, watch_stack.rs

**Current verbose pattern:**
```rust
// create_stack.rs lines 68-70
let sender = output_manager.start();
// ... spawn tasks with sender.clone()
drop(sender);
output_manager.stop().await?;
```

**After normalization:**
```rust
// Helper struct in src/cfn/mod.rs
pub struct ParallelTaskManager {
    sender: tokio::sync::mpsc::Sender<OutputData>,
    output_manager: DynamicOutputManager,
}

impl ParallelTaskManager {
    pub fn start(mut output_manager: DynamicOutputManager) -> Self {
        let sender = output_manager.start();
        Self { sender, output_manager }
    }
    
    pub fn get_sender(&self) -> tokio::sync::mpsc::Sender<OutputData> {
        self.sender.clone()
    }
    
    pub async fn finish(mut self) -> Result<DynamicOutputManager> {
        drop(self.sender);
        self.output_manager.stop().await?;
        Ok(self.output_manager)
    }
}

// Usage in handlers:
let task_manager = ParallelTaskManager::start(output_manager);
let sender = task_manager.get_sender();
// ... spawn tasks with sender
let mut output_manager = task_manager.finish().await?;
```

**Benefits:** Encapsulates parallel task lifecycle, prevents forgot to drop sender bugs

### Pattern 11: Start Time Initialization (8+ handlers)

**Found in:** Most handlers that track timing

**Current verbose pattern:**
```rust
// create_stack.rs line 21
let start_time = Instant::now();

// create_or_update.rs line 21
let start_time = Instant::now();
```

**After normalization:**
```rust
// Helper function in src/cfn/mod.rs
pub fn start_operation_timing() -> Instant {
    Instant::now()
}

// Usage in handlers:
let start_time = start_operation_timing();
```

**Benefits:** Minor but creates consistency, easier to enhance timing logic later

## Complete Comprehensive Example

### Before (create_stack.rs - full transformation):
```rust
pub async fn create_stack(cli: &Cli) -> Result<i32> {
    // Extract components from CLI
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;
    let args = match &cli.command {
        Commands::CreateStack(args) => args,
        _ => anyhow::bail!("Invalid command type for create_stack"),
    };

    let start_time = Instant::now();

    // Load stack configuration 
    let cli_aws_settings = AwsSettings::from_normalized_opts(&opts);
    let operation = CfnOperation::CreateStack;
    let stack_args = load_stack_args(
        &args.base.argsfile,
        Some(&global_opts.environment),
        &operation,
        &cli_aws_settings,
    ).await?;

    // Override stack name if provided via CLI
    let mut final_stack_args = stack_args;
    if let Some(ref stack_name) = args.base.stack_name {
        final_stack_args.stack_name = Some(stack_name.clone());
    }

    if final_stack_args.stack_name.is_none() {
        anyhow::bail!("Stack name is required (either in stack-args.yaml or via --stack-name)");
    }

    let context = create_context_for_operation(&opts, CfnOperation::CreateStack).await?;

    // Setup output manager
    let output_options = crate::output::manager::OutputOptions::new(cli.clone());
    let mut output_manager = DynamicOutputManager::new(
        global_opts.effective_output_mode(),
        output_options
    ).await?;

    // Render command metadata
    let command_metadata = create_command_metadata(&context, &opts, &final_stack_args, &global_opts.environment).await?;
    output_manager.render(OutputData::CommandMetadata(command_metadata)).await?;

    // Token info
    let output_token = convert_token_info(&token);
    output_manager.render(OutputData::TokenInfo(output_token)).await?;

    // ... middle logic ...

    // Success determination
    let expected_success_states = ["CREATE_COMPLETE"];
    let success = final_status.as_ref()
        .map(|status| expected_success_states.contains(&status.as_str()))
        .unwrap_or(false);

    // Final summary
    let elapsed_seconds = (Utc::now() - start_time).num_seconds();
    let final_command_summary = create_final_command_summary(success, elapsed_seconds);
    output_manager.render(final_command_summary).await?;

    Ok(if success { 0 } else { 1 })
}
```

### After (complete normalization):
```rust
pub async fn create_stack(cli: &Cli) -> Result<i32> {
    // Setup
    let opts = cli.aws_opts.clone().normalize();
    let global_opts = &cli.global_opts;
    let args = match &cli.command {
        Commands::CreateStack(args) => args,
        _ => anyhow::bail!("Invalid command type for create_stack"),
    };
    let start_time = start_operation_timing();

    // Prepare operation context and stack configuration
    let stack_args = load_operation_stack_args(&opts, CfnOperation::CreateStack, &args.base.argsfile, &global_opts.environment).await?;
    let final_stack_args = apply_stack_name_override_and_validate(stack_args, args.base.stack_name.as_ref())?;
    let context = create_operation_context(&opts, CfnOperation::CreateStack).await?;

    // Setup output and render initial metadata
    let mut output_manager = setup_full_output_manager(cli).await?;
    render_command_metadata(&mut output_manager, &context, &opts, &final_stack_args, &global_opts.environment).await?;
    render_token_info(&mut output_manager, &token).await?;

    // ... core business logic (unchanged) ...

    // Finalize
    let success = determine_operation_success(&final_status, CREATE_SUCCESS_STATES);
    render_final_summary(&mut output_manager, start_time, success).await?;

    Ok(if success { 0 } else { 1 })
}
```

**Transformation Summary:** 
- Reduced from ~45 lines of boilerplate to ~18 lines  
- Business logic stands out clearly
- All common patterns centralized
- Consistent error handling and rendering

## Updated Implementation Plan

### Phase 1: Fix Environment Option Wrapping (1 commit)
1. Investigate why `load_stack_args` takes `Option<&str>` for environment
2. Update `load_stack_args` signature to take `&str` if appropriate  
3. Add stack args loading helper that eliminates the Option wrapping

### Phase 2: Core Helper Functions (1 commit)
1. Add stack name override + validation helper
2. Add token rendering helper
3. Add success determination helper + constants
4. Add final summary rendering helper

### Phase 3: Setup Helper Functions (1 commit)
1. Add output manager setup helpers
2. Add context creation helper  
3. Add command metadata rendering helper
4. Add timing initialization helper

### Phase 4: Advanced Helper Functions (1 commit)
1. Add parallel task manager utilities
2. Add stack ID extraction helper
3. Add any remaining specialized helpers

### Phase 5: Apply to Handlers (4 commits)
1. **Commit 1:** create_stack.rs + update_stack.rs (most complex)
2. **Commit 2:** delete_stack.rs + create_or_update.rs
3. **Commit 3:** create_changeset.rs + exec_changeset.rs  
4. **Commit 4:** estimate_cost.rs + remaining handlers

### Phase 6: Final Cleanup (1 commit)
1. Remove unused code
2. Update documentation
3. Final verification

## Expected Outcome

- **Code reduction:** ~250 lines of duplicate code eliminated
- **Maintainability:** All common patterns centralized in one location
- **Readability:** Handler functions focus purely on business logic
- **Consistency:** Standardized error messages, setup patterns, and rendering
- **Type safety:** Helper functions provide compile-time guarantees
- **Foundation:** Excellent preparation for larger architectural refactoring
- **Documentation:** Clear, discoverable common operations in cfn/mod.rs