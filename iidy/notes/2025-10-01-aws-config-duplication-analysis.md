# AWS Config Duplication Analysis

**Date:** 2025-10-01
**Issue:** We create AWS config twice - once in `load_stack_args()` and once in `create_context_for_operation()`, with potential for inconsistency

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

1. **Two separate configs with potentially different settings:**
   - Config #1: Only knows about CLI flags
   - Config #2: Knows about CLI flags + stack-args.yaml + environment resolution

2. **Config #2 has richer information but is thrown away:**
   - It resolves environment maps (e.g., `Region: {dev: us-east-1, prod: us-west-2}`)
   - It merges stack-args.yaml settings with CLI
   - It's used for preprocessing, then discarded
   - The CloudFormation client uses Config #1 which lacks this context

3. **Current workaround is fragile:**
   - We now validate region in both places
   - But they could theoretically resolve to different regions (if merging logic differs)
   - We duplicate the AWS config creation logic

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

## Recommendation

### Short Term (Now)
**Keep Option 4 (Status Quo with Validation)**

Reasons:
1. Already implemented and working
2. Commands have different needs (some need stack args, some don't)
3. Minimal disruption
4. Safe for the current commit

### Medium Term (Future PR)
**Implement Option 3 (StackContext wrapper)**

Reasons:
1. Clean separation of concerns
2. Eliminates AWS config duplication for stack-based operations
3. Doesn't break commands that don't use stack args
4. Could introduce gradually:
   - Phase 1: Create StackContext for create/update operations
   - Phase 2: Migrate other stack-based operations
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
