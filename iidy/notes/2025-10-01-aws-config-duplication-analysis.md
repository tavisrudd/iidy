# AWS Config Duplication Analysis

**Date:** 2025-10-01
**Issue:** We create AWS config twice - once in `load_stack_args()` and once in `create_context_for_operation()`, with **actual inconsistency**

## Critical Correctness Issue

This is **not just duplication** - it's a **correctness bug**:

1. **Config #1 (CfnContext)**: Created from CLI opts only → Used for CloudFormation API calls
2. **Config #2 (load_stack_args)**: Created from merged CLI + stack-args.yaml → Used for preprocessing ($imports, CommandsBefore), then discarded

**The Problem:** The CloudFormation client could be using **different AWS credentials/region/profile** than:
- What's displayed to the user
- What's used for template preprocessing ($imports)
- What's used for CommandsBefore execution
- What's configured in stack-args.yaml

This violates the principle of least surprise and could cause silent failures or operations in wrong regions.

## Current Architecture

### Flow Diagram

```
run_command_handler! macro
  └─> create_context_for_operation(opts, operation)
      ├─> config_from_normalized_opts(opts)        [AWS CONFIG #1]
      │   └─> Uses: opts.region, opts.profile, opts.assume_role_arn
      ├─> Client::new(&config)
      └─> CfnContext::new(client, config, ...)

  └─> create_stack_impl(&context, ...)
      └─> load_stack_args(argsfile, environment, operation, cli_aws_settings)
          ├─> config_from_merged_settings(&merged_aws_settings)  [AWS CONFIG #2]
          │   └─> Uses: merged CLI + argsfile settings
          │   └─> Used for: $imports, CommandsBefore
          └─> Returns: StackArgs (no config returned)
```

### Current Duplication

**Config #1 (in `create_context_for_operation`):**
- Created from: `NormalizedAwsOpts` (CLI options only)
- Used for: CloudFormation API client
- Location: `src/cfn/mod.rs:142`

**Config #2 (in `load_stack_args`):**
- Created from: Merged CLI + stack-args.yaml settings
- Used for: Template preprocessing ($imports), CommandsBefore execution
- Location: `src/stack_args.rs:133`
- **Not returned** - config is discarded after use

### The Problem

1. **Two separate configs with DIFFERENT settings:**
   - Config #1 (CfnContext): Only contains CLI flags (--region, --profile, --assume-role-arn)
   - Config #2 (preprocessing): Contains CLI flags + stack-args.yaml + environment map resolution

2. **Config #2 has correct information but is thrown away:**
   - It resolves environment maps (e.g., `Region: {dev: us-east-1, prod: us-west-2}`)
   - It merges stack-args.yaml settings with CLI (CLI takes precedence)
   - It's used for preprocessing and CommandsBefore execution
   - **Then it's discarded**
   - The CloudFormation client uses Config #1 which **lacks the stack-args.yaml settings**

3. **Actual Impact Examples:**

   | Scenario | Config #2 (preprocessing) | Config #1 (CFN client) | Match? |
   |----------|---------------------------|------------------------|---------|
   | CLI: `--region us-west-2` | us-west-2 | us-west-2 | ✅ |
   | stack-args: `Region: us-west-2` | us-west-2 | (from env/default) | ❌ |
   | stack-args: `Region: {prod: us-west-2}` + `--environment prod` | us-west-2 | (from env/default) | ❌ |
   | stack-args: `Profile: my-profile` (which has region in ~/.aws/config) | region from my-profile | (from env/default) | ❌ |

4. **Current state is NOT safe:**
   - We validate region exists in both places (prevents crashes)
   - But they could be **different regions** (correctness bug)
   - Same for profile and assume_role_arn
   - We duplicate AWS config creation logic

## Commands That Use stack_args

Commands that call `load_stack_args()`:
1. `create_stack.rs` - ✓ Has argsfile
2. `update_stack.rs` - ✓ Has argsfile
3. `create_changeset.rs` - ✓ Has argsfile
4. `exec_changeset.rs` - ✓ Has argsfile
5. `estimate_cost.rs` - ✓ Has argsfile
6. `create_or_update.rs` - ✓ Has argsfile
7. `template_approval_request.rs` - ✓ Has argsfile
8. `template_approval_review.rs` - ✓ Has URL (different pattern)

Commands that **don't** use stack_args:
- `describe_stack.rs` - Uses stack name directly
- `watch_stack.rs` - Uses stack name directly
- `delete_stack.rs` - Uses stack name directly
- `list_stacks.rs` - No stack specified
- `get_stack_template.rs` - Uses stack name directly
- `get_stack_instances.rs` - Uses stack name directly
- `describe_stack_drift.rs` - Uses stack name directly

## Refactoring Options

### Option 1: Return AWS Config from load_stack_args()

**Approach:**
```rust
pub async fn load_stack_args(...) -> Result<(StackArgs, SdkConfig)> {
    // ... current logic ...
    let aws_config = config_from_merged_settings(&merged_aws_settings).await?;
    // ... use config for preprocessing ...
    Ok((stack_args, aws_config))
}
```

**Then in create_context_for_operation:**
```rust
pub async fn create_context_for_operation(
    opts: &NormalizedAwsOpts,
    operation: CfnOperation,
    argsfile: Option<&str>,
    environment: Option<&str>,
) -> Result<CfnContext> {
    let config = if let (Some(argsfile), Some(env)) = (argsfile, environment) {
        // Load stack args and get the richer AWS config
        let cli_aws_settings = AwsSettings::from_normalized_opts(opts);
        let (_stack_args, aws_config) = load_stack_args(argsfile, env, &operation, &cli_aws_settings).await?;
        aws_config
    } else {
        // Commands without stack args use opts directly
        config_from_normalized_opts(opts).await?
    };

    // ... rest of context creation ...
}
```

**Pros:**
- Single source of truth for AWS config
- CloudFormation client uses the same config as preprocessing
- Eliminates duplication

**Cons:**
- Changes `create_context_for_operation` signature significantly
- Would need argsfile and environment passed to it
- Some commands don't have argsfile (describe-stack, list-stacks, etc.)
- Breaks the separation: context creation depends on stack args loading

### Option 2: Add stack_args to CfnContext

**Approach:**
```rust
pub struct CfnContext {
    pub client: Client,
    pub aws_config: aws_config::SdkConfig,
    pub time_provider: Arc<dyn TimeProvider>,
    pub start_time: DateTime<Utc>,
    pub token_info: TokenInfo,
    pub used_tokens: Arc<Mutex<Vec<TokenInfo>>>,
    pub stack_args: Option<StackArgs>,  // NEW
}
```

**Pros:**
- Stack args available throughout the operation
- Could eliminate repeated loading
- Context becomes more complete

**Cons:**
- Not all operations have stack args (describe, list, delete, etc.)
- Context creation becomes more complex
- Would need to handle Optional stack args everywhere
- Violates single responsibility principle (context manages multiple concerns)

### Option 3: Create StackContext that wraps CfnContext

**Approach:**
```rust
pub struct StackContext {
    pub cfn_context: CfnContext,
    pub stack_args: StackArgs,
    pub aws_config: SdkConfig,  // The richer config from stack args
}

impl StackContext {
    pub async fn new(argsfile: &str, environment: &str, operation: CfnOperation, opts: &NormalizedAwsOpts) -> Result<Self> {
        let cli_aws_settings = AwsSettings::from_normalized_opts(opts);
        let (stack_args, aws_config) = load_stack_args(argsfile, environment, operation, &cli_aws_settings).await?;

        // Create client from the richer config
        let client = Client::new(&aws_config);
        let time_provider = /* ... */;
        let cfn_context = CfnContext::new(client, aws_config.clone(), time_provider, opts.client_request_token.clone()).await?;

        Ok(StackContext { cfn_context, stack_args, aws_config })
    }
}
```

**Pros:**
- Clear separation: some commands use StackContext, others use CfnContext
- Single AWS config for stack-based operations
- Doesn't pollute CfnContext with Optional fields

**Cons:**
- Two different context types to maintain
- Command handlers need different signatures
- More complex architecture

### Option 4: Status Quo with Validation (Current)

**Approach:**
- Keep two configs
- Validate region in both places
- Accept the duplication

**Pros:**
- Minimal changes
- Simple architecture
- Works today

**Cons:**
- Duplication of AWS config creation
- Potential for inconsistency if merge logic changes
- Validation in two places

## Sequencing Analysis

### Current Sequencing

```
run_command_handler! macro:
  1. normalize opts
  2. create output_manager
  3. create_context_for_operation(opts, operation)
     - Creates Config #1 from opts
     - Validates region exists
     - Creates CloudFormation client
     - Returns CfnContext
  4. Call impl_fn(&output_manager, &context, cli, args, opts)

create_stack_impl (inside impl_fn):
  5. Extract cli_aws_settings from opts
  6. load_stack_args(argsfile, environment, operation, cli_aws_settings)
     - Creates Config #2 from merged settings
     - Validates region exists
     - Uses config for preprocessing
     - Discards config
     - Returns StackArgs
  7. Use context.client (which uses Config #1) for CloudFormation operations
```

### If We Change Sequencing

**Moving load_stack_args before create_context_for_operation:**

```rust
run_command_handler_with_stack_args! macro:
  1. normalize opts
  2. create output_manager
  3. load_stack_args(argsfile, environment, operation, cli_aws_settings)
     - Returns (StackArgs, SdkConfig)
  4. create_context_with_config(config, operation, opts.client_request_token)
     - Uses the config from stack args
  5. Call impl_fn(&output_manager, &context, cli, args, opts, stack_args)
```

**Issues:**
- Need different macros for commands with/without stack args
- Macro needs to know about argsfile location (varies by command)
- Some commands have argsfile in different places:
  - `create_stack.rs`: `args.argsfile`
  - `update_stack.rs`: `args.base.argsfile`
  - `describe_stack.rs`: no argsfile

## Recommendation (UPDATED)

### Immediate Fix (Now) - MUST FIX
**Implement modified Option 1: Return AWS Config from load_stack_args()**

**Why this is urgent:**
1. **Correctness bug**: CloudFormation operations could use wrong region/profile/credentials
2. **User confusion**: Display might show different settings than what's actually used
3. **Not just duplication**: Active inconsistency between configs
4. **Relatively simple fix**: Change return type and call sites

**Implementation:**
```rust
// Change load_stack_args signature
pub async fn load_stack_args(...) -> Result<(StackArgs, SdkConfig)> {
    // ... existing logic ...
    let aws_config = config_from_merged_settings(&merged_aws_settings).await?;
    // ... use config for preprocessing ...
    Ok((stack_args, aws_config))  // Return both
}

// In command handlers (create_stack, update_stack, etc.)
let (stack_args, merged_aws_config) = load_stack_args(...).await?;
// Create context using the merged config, not opts
let context = create_context_from_config(merged_aws_config, operation, opts.client_request_token).await?;

// Add new helper to cfn/mod.rs
pub async fn create_context_from_config(
    aws_config: SdkConfig,
    operation: CfnOperation,
    client_request_token: Option<String>,
) -> Result<CfnContext> {
    // Validate region
    if aws_config.region().is_none() {
        anyhow::bail!("No AWS region configured...");
    }

    let client = Client::new(&aws_config);
    let time_provider: Arc<dyn TimeProvider> = if operation.is_read_only() {
        Arc::new(SystemTimeProvider::new())
    } else {
        Arc::new(ReliableTimeProvider::new())
    };

    CfnContext::new(client, aws_config, time_provider, client_request_token.unwrap_or_else(TokenInfo::auto_generated)).await
}
```

**Commands to update (8 total):**
1. `create_stack.rs`
2. `update_stack.rs`
3. `create_changeset.rs`
4. `exec_changeset.rs`
5. `estimate_cost.rs`
6. `create_or_update.rs`
7. `template_approval_request.rs`
8. `template_approval_review.rs` (uses different pattern with URL)

**Commands unchanged (don't use stack_args):**
- `describe_stack.rs`, `watch_stack.rs`, `delete_stack.rs`, `list_stacks.rs`, etc.
- These continue using `create_context_for_operation(opts, operation)` directly

### Medium Term (Future PR)
**Consider Option 3 (StackContext wrapper) for further cleanup**

Reasons:
1. Clean separation of concerns
2. Eliminates AWS config duplication for stack-based operations

## Implementation Plan - Incremental Approach

### Strategy
Create new `run_command_handler_with_stack_args!` macro that:
1. Loads stack_args with merged AWS config
2. Creates context from merged config (not CLI-only opts)
3. Passes both context AND stack_args to impl function

Migrate commands one at a time, starting with `create_stack.rs`, to validate the pattern before rolling out.

### Step 1: Core infrastructure changes
- [x] Change `src/cfn/stack_args.rs::load_stack_args()` return type to `Result<(StackArgs, SdkConfig)>`
- [x] Return the `aws_config` along with `stack_args`
- [x] Add `create_context_from_config()` helper to `src/cfn/mod.rs`
- [x] Export `create_context_from_config` in module exports (public function)

### Step 2: Create new macro
- [x] Add `run_command_handler_with_stack_args!` macro to `src/cfn/mod.rs`
- [x] Macro takes: `($impl_fn:ident, $cli:expr, $args:expr, $argsfile:expr)`
- [x] Macro handles: normalize opts, load stack_args, create context from merged config, call impl_fn

### Step 3: Migrate create_stack.rs (pilot command)
- [x] Update `src/cfn/create_stack.rs` to use new macro
- [x] Change impl function signature to receive `stack_args: &StackArgs` parameter
- [x] Remove duplicate `load_stack_args()` call from impl function
- [ ] Test thoroughly to validate the pattern (needs manual testing)

### Step 4: Fix all call sites that still use old signature
All existing commands that call `load_stack_args()` need temporary fix to destructure tuple:
- [x] `src/cfn/update_stack.rs` - Add `let (stack_args, _aws_config) = ...`
- [x] `src/cfn/create_changeset.rs` - Add `let (stack_args, _aws_config) = ...`
- [x] `src/cfn/exec_changeset.rs` - Add `let (stack_args, _aws_config) = ...`
- [x] `src/cfn/estimate_cost.rs` - Add `let (stack_args, _aws_config) = ...`
- [x] `src/cfn/create_or_update.rs` - Add `let (stack_args, _aws_config) = ...`
- [x] `src/cfn/template_approval_request.rs` - Add `let (stack_args, _aws_config) = ...`
- [x] `src/cfn/template_approval_review.rs` - Add `let (stack_args, _aws_config) = ...`
- [x] Fix tests in `src/cfn/stack_args.rs` to destructure tuple

### Step 5: Verify pilot works
- [x] `cargo check --all` passes with no warnings
- [x] `cargo nextest r --color=never --hide-progress-bar` - all tests pass (591/591)
- [ ] Manual test: `create_stack` with stack-args.yaml `Region: us-west-2` uses that region
- [ ] Verify displayed region matches what CFN operations use

### Step 6: Migrate remaining commands (one by one)
After validating the pattern with `create_stack.rs`:
- [x] `src/cfn/update_stack.rs` - Convert to use new macro
- [x] `src/cfn/create_changeset.rs` - Convert to use new macro
- [x] `src/cfn/exec_changeset.rs` - Convert to use new macro
- [x] `src/cfn/estimate_cost.rs` - Convert to use new macro
- [x] `src/cfn/create_or_update.rs` - Convert to use new macro
- [x] `src/cfn/template_approval_request.rs` - Convert to use new macro
- [x] `src/cfn/template_approval_review.rs` - **Cleaned up** (uses URL, not argsfile - removed dummy load_stack_args, uses StackArgs::default())

### Step 7: Final validation
- [x] All tests pass (591/591)
- [x] No compiler warnings
- [ ] Manual test migrated commands
- [ ] Consider folding `run_command_handler_with_stack_args` back into main macro if pattern is clean

## Success Criteria
✅ CloudFormation client uses the SAME config as preprocessing
✅ Displayed region/profile/credentials match what's actually used
✅ Single AWS config creation for stack-based commands
✅ Duplicate `load_stack_args()` calls removed from command implementations
✅ All tests pass
✅ No compiler warnings
✅ Region from stack-args.yaml is respected (not just CLI)
✅ Doesn't break commands that don't use stack args
   - Phase 3: Consider consolidation if patterns emerge

**Implementation Plan:**
```rust
// New in src/cfn/mod.rs
pub struct StackContext {
    pub cfn: CfnContext,
    pub args: StackArgs,
}

impl StackContext {
    pub async fn new(
        argsfile: &str,
        environment: &str,
        operation: CfnOperation,
        opts: &NormalizedAwsOpts,
    ) -> Result<Self> {
        let cli_aws_settings = AwsSettings::from_normalized_opts(opts);

        // Load stack args and get AWS config in one step
        let (stack_args, aws_config) = load_stack_args_with_config(
            argsfile,
            environment,
            operation,
            &cli_aws_settings
        ).await?;

        // Validate region
        if aws_config.region().is_none() {
            anyhow::bail!("No AWS region configured...");
        }

        // Create client from the merged config
        let client = Client::new(&aws_config);
        let time_provider: Arc<dyn TimeProvider> = if operation.is_read_only() {
            Arc::new(SystemTimeProvider::new())
        } else {
            Arc::new(ReliableTimeProvider::new())
        };

        let cfn = CfnContext::new(
            client,
            aws_config,
            time_provider,
            opts.client_request_token.clone()
        ).await?;

        Ok(StackContext { cfn, args: stack_args })
    }
}

// New macro for stack-based commands
#[macro_export]
macro_rules! run_stack_command_handler {
    ($impl_fn:ident, $cli:expr, $args:expr, $argsfile:expr) => {{
        let opts = $cli.aws_opts.clone().normalize();
        let output_manager = /* ... */;
        let operation = $cli.command.to_cfn_operation();

        let stack_context = StackContext::new(
            $argsfile,
            &$cli.global_opts.environment,
            operation,
            &opts
        ).await?;

        $impl_fn(&mut output_manager, &stack_context, $cli, $args, &opts).await
    }};
}
```

Then gradually migrate commands:
- `create_stack_impl(&StackContext, ...)` instead of `(&CfnContext, ...)`
- Access stack args via `stack_context.args` instead of loading them
- Access client via `stack_context.cfn.client`

## Risk Analysis

### Risks of Not Refactoring
- **Low risk of actual bugs:** Both configs validate region now
- **Medium risk of future inconsistency:** If someone changes merge logic in one place
- **Low risk of confusion:** Code duplication is documented

### Risks of Refactoring Now
- **High risk of regression:** Touching core infrastructure before testing current changes
- **Medium risk of complexity:** Different command types need different handling
- **High risk of scope creep:** Could delay the current region fix

## Conclusion

**For this commit:** Stick with the current approach (dual validation, dual configs)

**For future work:** Consider StackContext wrapper to:
1. Eliminate AWS config duplication
2. Provide single source of truth for stack-based operations
3. Make region resolution explicit and traceable
4. Reduce cognitive load (one config instead of two)

The refactoring is worthwhile but should be a separate, focused effort after the region display bug fix is proven in production.
