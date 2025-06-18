# Critical Stack Args Implementation Plan

**Date:** 2025-06-18  
**Status:** 🚨 CRITICAL IMPLEMENTATION GAP IDENTIFIED  
**Context:** Complete rewrite of stack args loading required for production viability

## Executive Summary

Our Rust implementation has a **FUNDAMENTAL ARCHITECTURE FLAW** in stack arguments loading. After careful analysis of `@iidy-js-for-reference/src/cfn/loadStackArgs.ts`, our current implementation is missing ~80% of the critical functionality that makes iidy production-ready.

**This is not a minor feature gap - this is a foundational system that's completely broken.**

## The Critical Problem

### Current Broken State
```rust
// ALL command handlers currently do this:
let stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;
//                                                                ^^^^ 
//                                                                BROKEN: Should be environment
```

**Result:** Every production feature that depends on environment-based configuration is non-functional.

### What iidy-js Actually Does (LoadStackArgs.ts Analysis)

The iidy-js `loadStackArgs` function is a **complex, multi-stage processing pipeline** that:

1. **Loads YAML/JSON** with comprehensive error handling
2. **Resolves environment maps** for AWS credential fields BEFORE any AWS calls
3. **Configures AWS credentials** by merging CLI + argsfile settings (CLI wins)
4. **Injects $envValues** with command context, environment, region, profile
5. **Processes CommandsBefore** with two-pass preprocessing and command execution
6. **Performs multi-pass YAML preprocessing** with full import resolution
7. **Applies global configuration** from SSM parameter store
8. **Handles client request tokens** from CLI args

**Our implementation does maybe 10% of this.**

## Complete Architecture Requirements

### 1. LoadStackArgsContext (iidy-js argv equivalent)

```rust
#[derive(Debug, Clone)]
pub struct LoadStackArgsContext {
    // Core file and environment
    pub argsfile: String,
    pub environment: Option<String>,
    
    // Command context for CommandsBefore and $envValues
    pub command: Vec<String>,          // argv._ equivalent  
    pub stack_name: Option<String>,    // For handlebars in CommandsBefore
    
    // Security and idempotency  
    pub client_request_token: Option<String>,
    
    // AWS configuration (CLI overrides)
    pub cli_aws_settings: AwsSettings,
}

impl LoadStackArgsContext {
    pub fn from_cli_args(
        argsfile: &str,
        opts: &NormalizedAwsOpts,
        global_opts: &GlobalOpts,
        command_parts: &[&str],
        stack_name: Option<&str>,
    ) -> Self {
        // Convert our CLI structure to iidy-js argv equivalent
    }
}
```

### 2. AWS Configuration Pipeline

```rust
// CRITICAL: Must happen BEFORE preprocessing because $imports need AWS
pub async fn configure_aws_from_stack_args(
    argsfile_settings: &AwsSettings,
    cli_overrides: &AwsSettings,
) -> Result<aws_config::SdkConfig> {
    // 1. Merge settings (CLI overrides argsfile)
    // 2. Configure AWS SDK with merged settings
    // 3. Return configured AWS client for $imports
}
```

### 3. Multi-Stage Processing Pipeline

```rust
pub async fn load_stack_args(
    context: LoadStackArgsContext,
    filter_keys: Vec<String>,
) -> Result<StackArgs> {
    // STAGE 1: Load and parse file
    let mut argsdata = load_and_parse_file(&context.argsfile)?;
    
    // STAGE 2: Apply filter if provided
    if !filter_keys.is_empty() {
        argsdata = apply_filter(filter_keys, argsdata, &context.argsfile)?;
    }
    
    // STAGE 3: Resolve environment maps for AWS creds (BEFORE AWS config)
    resolve_aws_credential_environment_maps(&mut argsdata, &context.environment)?;
    
    // STAGE 4: Configure AWS (required for $imports)
    let aws_config = configure_aws_from_merged_settings(&argsdata, &context.cli_aws_settings).await?;
    
    // STAGE 5: Inject environment tag
    inject_environment_tag(&mut argsdata, &context.environment);
    
    // STAGE 6: Create and inject $envValues
    let env_values = create_env_values(&context, &aws_config);
    inject_env_values(&mut argsdata, env_values);
    
    // STAGE 7: Process CommandsBefore (if applicable)
    if should_process_commands_before(&context.command) {
        process_commands_before(&mut argsdata, &context, &aws_config).await?;
    }
    
    // STAGE 8: Final preprocessing pass
    let processed_argsdata = preprocess_yaml_final_pass(&argsdata, &context.argsfile).await?;
    
    // STAGE 9: Apply string replacements ($0string)
    let cleaned_argsdata = apply_string_replacements(processed_argsdata);
    
    // STAGE 10: Deserialize to StackArgs
    let mut stack_args: StackArgs = serde_yaml::from_value(cleaned_argsdata)?;
    
    // STAGE 11: Apply CLI client request token
    if let Some(token) = &context.client_request_token {
        stack_args.client_request_token = Some(token.clone());
    }
    
    // STAGE 12: Apply global configuration from SSM
    apply_global_configuration(&mut stack_args, &aws_config).await?;
    
    Ok(stack_args)
}
```

### 4. Global Configuration System

```rust
pub async fn apply_global_configuration(
    args: &mut StackArgs,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    // SSM parameter store integration:
    // - /iidy/default-notification-arn -> Add to NotificationARNs if SNS topic exists
    // - /iidy/disable-template-approval -> Remove ApprovedTemplateLocation if true
}
```

### 5. CommandsBefore Processing

```rust
pub async fn process_commands_before(
    argsdata: &mut Value,
    context: &LoadStackArgsContext,
    aws_config: &aws_config::SdkConfig,
) -> Result<()> {
    // This is COMPLEX:
    // 1. Two-pass preprocessing for handlebars templates
    // 2. Command execution with environment variable injection
    // 3. Integration with shell command runner
    // 4. Error handling and cleanup
}
```

## Critical Dependencies Analysis

### Missing Crate Dependencies

```toml
# Add to Cargo.toml:
aws-sdk-sns = "1"        # For global config SNS validation
handlebars = "4"         # For CommandsBefore template processing
uuid = "1"               # For token generation
regex = "1"              # For string replacements
shellexpand = "3"        # For command processing
```

### Missing Module Dependencies

1. **Command execution system** - Need robust shell command runner
2. **Handlebars integration** - For CommandsBefore template processing  
3. **Advanced YAML processing** - Multi-pass with state management
4. **AWS configuration system** - Beyond basic client creation

## Command Handler Integration Requirements

### Current Broken Pattern (ALL handlers)

```rust
// ❌ BROKEN - Missing environment, missing CLI context
let stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;
```

### Required New Pattern

```rust
// ✅ CORRECT - Full context with environment and CLI integration
let context = LoadStackArgsContext::from_cli_args(
    &args.argsfile,
    opts,
    global_opts,
    &["create-stack"],  // Command for $envValues and CommandsBefore
    args.stack_name.as_deref(),
);
let stack_args = load_stack_args(context, vec![]).await?;
```

### Files That Need Updates (ALL CFN handlers)

- `src/cfn/create_stack.rs`
- `src/cfn/update_stack.rs` 
- `src/cfn/delete_stack.rs`
- `src/cfn/create_changeset.rs`
- `src/cfn/exec_changeset.rs`
- `src/cfn/create_or_update.rs`
- `src/cfn/estimate_cost.rs`
- **ANY handler that loads stack args**

## Implementation Phases

### Phase 1: Emergency Fixes (P0 - Production Blocking)

**Duration:** 1-2 days  
**Outcome:** Basic functionality restored

1. ✅ **Fix environment parameter** in all command handlers
2. ⭐ **Implement basic AWS credential configuration**
3. ⭐ **Add $envValues injection**  
4. ⭐ **Add client request token handling**

**Success Criteria:** Environment-based configs work, $imports can make AWS calls

### Phase 2: Feature Parity (P1 - Production Ready)

**Duration:** 3-5 days  
**Outcome:** Full iidy-js compatibility

1. ⭐ **Implement global configuration via SSM**
2. ⭐ **Add multi-pass preprocessing**
3. ⭐ **Implement CommandsBefore processing**
4. ⭐ **Add comprehensive error handling**

**Success Criteria:** Can run production workloads that work with iidy-js

### Phase 3: Optimization (P2 - Excellence)

**Duration:** 2-3 days  
**Outcome:** Production-grade implementation

1. ⭐ **Performance optimization**
2. ⭐ **Advanced error messages**
3. ⭐ **Comprehensive testing**
4. ⭐ **Documentation**

**Success Criteria:** Better than iidy-js in reliability and performance

## Risk Assessment

### HIGH RISK - Implementation Complexity

**CommandsBefore processing** is exceptionally complex:
- Two-pass YAML preprocessing
- Handlebars template processing with complex environment 
- Shell command execution with proper error handling
- State management across preprocessing passes

**Mitigation:** Implement incrementally, with fallbacks for unsupported features

### MEDIUM RISK - AWS Integration

**Global configuration** requires:
- SSM parameter store access
- SNS topic validation  
- Complex error handling for missing permissions

**Mitigation:** Graceful degradation when AWS calls fail

### LOW RISK - Breaking Changes

**New LoadStackArgsContext** requires updates to all command handlers

**Mitigation:** Clear, systematic refactoring with comprehensive testing

## Testing Strategy

### Integration Tests with Real AWS

```rust
#[tokio::test]
async fn test_stack_args_with_environment_maps() {
    // Test environment resolution: dev -> us-east-1, prod -> us-west-2
}

#[tokio::test] 
async fn test_stack_args_with_global_config() {
    // Test SSM parameter store integration
}

#[tokio::test]
async fn test_stack_args_with_commands_before() {
    // Test CommandsBefore execution and preprocessing
}
```

### Compatibility Tests

```rust
#[tokio::test]
async fn test_iidy_js_compatibility() {
    // Load same stack-args.yaml with both implementations
    // Compare final StackArgs output
}
```

## Success Metrics

### Functional Metrics
- ✅ All environment-based configs work
- ✅ $imports can make AWS API calls
- ✅ Templates with $envValues load correctly
- ✅ Global configuration applied from SSM
- ✅ CommandsBefore execution works

### Quality Metrics  
- ✅ All existing tests pass
- ✅ New integration tests pass
- ✅ Performance equals or exceeds iidy-js
- ✅ Error messages are clear and actionable

### Production Readiness
- ✅ Can deploy complex production stacks
- ✅ Works with existing iidy-js workflows
- ✅ Proper error handling and recovery
- ✅ Security and idempotency guarantees

## Future Context Window Instructions

When this context window gets compacted, the new instance should:

1. **Read this document first** - It contains the complete analysis
2. **Check `/notes/2025-06-18-stack-args-loading-analysis.md`** - For detailed requirements
3. **Review `@iidy-js-for-reference/src/cfn/loadStackArgs.ts`** - For exact implementation details
4. **Check current todo list** - For implementation status
5. **Run tests to verify current state** - `cargo nextest r`

**CRITICAL:** This is not a minor feature - it's a foundational system rewrite. Do not underestimate the complexity or try to take shortcuts. The current implementation is fundamentally broken for production use.

## Immediate Next Actions

1. ⭐ **Update todos** with specific implementation tasks
2. ⭐ **Create new AWS configuration module** 
3. ⭐ **Implement LoadStackArgsContext**
4. ⭐ **Fix environment parameter in all command handlers**
5. ⭐ **Add integration tests**

**Priority:** This should be the #1 focus until complete. Nothing else matters if basic stack args loading is broken.