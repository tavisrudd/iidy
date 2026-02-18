# Stack Args Loading Analysis & Requirements

**Date:** 2025-06-18  
**Context:** Ensuring our Rust implementation correctly loads stack-args.yaml with the same functionality as iidy-js loadStackArgs.ts

## Current State Analysis

### What We Have
Our current `src/stack_args.rs` implementation is **significantly simplified** compared to iidy-js:

1. ✅ **Basic YAML parsing** with environment map resolution
2. ✅ **Environment tag injection** 
3. ✅ **YAML preprocessing integration** (using our preprocessing pipeline)
4. ❌ **Missing AWS credential configuration**
5. ❌ **Missing global configuration via SSM**
6. ❌ **Missing CommandsBefore processing**
7. ❌ **Missing $envValues injection**
8. ❌ **Missing multi-pass preprocessing**
9. ❌ **Missing client request token handling**

### Critical Issues in Command Handlers

**All command handlers are currently passing `None` for environment:**

```rust
// WRONG - in all handlers:
let stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;

// SHOULD BE:
let stack_args = load_stack_args_file(Path::new(&args.argsfile), opts.environment.as_deref())?;
```

This means **environment-based configuration is completely broken**.

## iidy-js loadStackArgs.ts Requirements

### Core Architecture (from lines 67-196)

```typescript
export async function _loadStackArgs(
  argsfile: string,
  argv: GenericCLIArguments,  // ⭐ TAKES FULL CLI CONTEXT
  filterKeys: string[] = [],
  setupAWSCredentails = configureAWS  // ⭐ CONFIGURES AWS
): Promise<StackArgs>
```

### Step-by-Step Process

1. **Environment Resolution (lines 94-110)**
   ```typescript
   for (const key of ['Profile', 'AssumeRoleARN', 'Region']) {
     if (isArgsObject(argsdata, key)) {
       if (environment && argsdata[key][environment]) {
         argsdata[key] = argsdata[key][environment];
       } else {
         throw new Error(`environment "${environment}" not found in ${key} map`);
       }
     }
   }
   ```

2. **AWS Credential Configuration (lines 112-118)**
   ```typescript
   const cliOptionOverrides = _.pickBy(argv, (v, k) => 
     !_.isEmpty(v) && _.includes(['region', 'profile', 'assumeRoleArn'], k));
   const argsfileSettings = {
     profile: argsdata.Profile, 
     assumeRoleArn: argsdata.AssumeRoleARN, 
     region: argsdata.Region
   };
   const mergedAWSSettings = _.merge(argsfileSettings, cliOptionOverrides);
   await setupAWSCredentails(mergedAWSSettings); // CLI overrides argsfile
   ```

3. **Environment Tag Injection (lines 119-123)**
   ```typescript
   if (environment) {
     if (!_.get(argsdata, ['Tags', 'environment'])) {
       argsdata.Tags = _.merge({environment}, argsdata.Tags);
     }
   }
   ```

4. **$envValues Creation (lines 124-136)**
   ```typescript
   argsdata.$envValues = _.merge({}, argsdata.$envValues, {
     region: finalRegion,
     environment,
     iidy: {
       command: iidy_command,
       environment,
       region: finalRegion,
       profile: mergedAWSSettings.profile
     }
   });
   ```

5. **CommandsBefore Processing (lines 137-168)**
   - Two-pass preprocessing for commands that modify stacks
   - Command execution with environment variable injection
   - Complex handlebars template processing

6. **Multi-Pass Preprocessing (lines 169-180)**
   ```typescript
   const stackArgsPass2 = await transform(argsdata, argsfile) as StackArgs;
   const stackArgsPass3 = recursivelyMapValues(stackArgsPass2, (value: any) => {
     if (typeof value === 'string') {
       return value.replace(/\$0string (0\d+)/g, '$1');
     }
     return value;
   });
   ```

7. **Global Configuration (lines 184-196)**
   ```typescript
   export async function loadStackArgs(argv, filterKeys = [], setupAWSCredentails = configureAWS) {
     const args = await _loadStackArgs(argv.argsfile, argv, filterKeys, setupAWSCredentails);
     if (argv.clientRequestToken) {
       args.ClientRequestToken = argv.clientRequestToken;
     }
     await applyGlobalConfiguration(args);  // SSM parameter store
     return args;
   }
   ```

### Global Configuration (lines 37-60)

```typescript
export async function applyGlobalConfiguration(args: StackArgs, ssm = new aws.SSM()) {
  const {Parameters} = await ssm.getParametersByPath({Path: '/iidy/', WithDecryption: true}).promise();
  for (const parameter of Parameters) {
    switch(parameter.Name) {
      case '/iidy/default-notification-arn':
        await applySnsNotificationGlobalConfiguration(args, parameter.Value);
        break;
      case '/iidy/disable-template-approval':
        if (parameter.Value?.match(/true/i) && args.ApprovedTemplateLocation) {
          delete args.ApprovedTemplateLocation;
        }
        break;
    }
  }
}
```

## Required Architecture Changes

### 1. New LoadStackArgsContext Structure

```rust
pub struct LoadStackArgsContext {
    pub argsfile: String,
    pub environment: Option<String>,
    pub command: Vec<String>,           // equivalent to argv._
    pub stack_name: Option<String>,     // for CommandsBefore processing
    pub client_request_token: Option<String>,
    pub cli_aws_settings: AwsSettings,  // CLI overrides
}
```

### 2. AWS Configuration Integration

```rust
pub async fn configure_aws_from_merged_settings(
    merged_settings: &AwsSettings
) -> Result<aws_config::SdkConfig> {
    // Equivalent to iidy-js configureAWS function
    // Must be called BEFORE preprocessing due to $imports
}
```

### 3. Multi-Pass Preprocessing

```rust
pub async fn load_stack_args(
    context: LoadStackArgsContext,
    filter_keys: Vec<String>,
) -> Result<StackArgs> {
    // 1. Load and parse file
    // 2. Resolve environment maps for AWS creds
    // 3. Configure AWS credentials  
    // 4. Inject $envValues
    // 5. Process CommandsBefore (if applicable)
    // 6. Final preprocessing pass
    // 7. Apply client request token
    // 8. Apply global configuration
}
```

### 4. Command Handler Integration

All command handlers need to be updated:

```rust
// OLD:
let stack_args = load_stack_args_file(Path::new(&args.argsfile), None)?;

// NEW:
let context = LoadStackArgsContext::from_opts_and_args(
    &args.argsfile,
    opts,
    global_opts,
    &["create-stack"],  // or appropriate command
    args.stack_name.as_deref(),
);
let stack_args = load_stack_args(context, vec![]).await?;
```

## Implementation Priority

### P0 - Critical (Broken Functionality)
1. ✅ **Environment parameter passing** - Fix `None` in all command handlers  
2. ⭐ **AWS credential configuration** - Required for $imports to work
3. ⭐ **$envValues injection** - Many templates depend on this
4. ⭐ **Client request token handling** - Security/idempotency issue

### P1 - High (Feature Parity)
5. ⭐ **Global configuration via SSM** - Production environments depend on this
6. ⭐ **Multi-pass preprocessing** - Required for complex templates
7. ⭐ **CommandsBefore processing** - Build automation depends on this

### P2 - Medium (Quality of Life)
8. ⭐ **Filter keys support** - For advanced use cases
9. ⭐ **Proper error messages** - Developer experience
10. ⭐ **String replacement ($0string)** - Template compatibility

## Dependencies Required

Add to `Cargo.toml`:

```toml
aws-sdk-sns = "1"  # For global configuration SNS validation
```

## Testing Strategy

### Unit Tests
- Environment map resolution
- AWS settings merging  
- $envValues creation
- Filter functionality

### Integration Tests
- Full stack args loading with real AWS
- CommandsBefore execution
- Global configuration fetching
- Multi-environment scenarios

### Compatibility Tests
- Side-by-side comparison with iidy-js output
- Template compatibility validation

## Migration Plan

### Phase 1: Critical Fixes (Current)
1. Fix environment parameter in all command handlers
2. Implement basic AWS credential configuration
3. Add $envValues injection

### Phase 2: Feature Parity
1. Implement global configuration
2. Add CommandsBefore processing
3. Implement multi-pass preprocessing

### Phase 3: Polish
1. Add comprehensive error handling
2. Improve performance
3. Add advanced features

## Conclusion

Our current stack args loading is **fundamentally broken** for production use. The missing AWS credential configuration and environment handling means that:

1. ❌ **Environment-based configs don't work** (all handlers pass `None`)
2. ❌ **$imports can't make AWS API calls** (no credential setup)
3. ❌ **Templates with $envValues fail** (no injection)
4. ❌ **Global configuration ignored** (no SSM integration)
5. ❌ **CommandsBefore ignored** (no preprocessing)

This represents a **critical gap** that must be addressed immediately for any production usage.