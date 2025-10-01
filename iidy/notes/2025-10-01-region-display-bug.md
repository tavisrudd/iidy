# Region Display Bug Analysis

**Date:** 2025-10-01
**Issue:** Command metadata displays incorrect region when region is not specified via CLI flag

## Problem Summary

The `CommandMetadata` output incorrectly displays "us-east-1" as the region whenever the `--region` CLI flag is not explicitly provided, even when the actual region is correctly resolved from other sources (stack-args.yaml, environment variables, AWS config files).

## Region Resolution Flow (Current - Working Correctly)

### 1. CLI Input (`src/cli.rs:122-130`)
- User can optionally specify `--region` flag
- Stored in `AwsOpts.region: Option<String>`

### 2. Stack Args Loading (`src/stack_args.rs:90-133`)
`load_stack_args()` is called with CLI settings and performs:

**Environment Map Resolution (lines 106-116):**
```yaml
# stack-args.yaml can have environment-based region maps
Region:
  dev: us-east-1
  prod: us-west-2
```
- Resolves based on `--environment` flag
- Or uses string directly if not a map

**Merging (lines 125-130):**
```rust
let merged_aws_settings = AwsSettings {
    profile: cli_aws_settings.profile.or(argsfile_aws_settings.profile),
    region: cli_aws_settings.region.or(argsfile_aws_settings.region),
    assume_role_arn: cli_aws_settings.assume_role_arn.or(argsfile_aws_settings.assume_role_arn),
};
```
Precedence: **CLI > Stack Args > Defaults**

### 3. AWS Config Creation (`src/aws/mod.rs:95-138`)

**Key Code:**
```rust
pub async fn config_from_merged_settings(merged_settings: &AwsSettings) -> Result<SdkConfig> {
    let mut loader = aws_config::defaults(BehaviorVersion::v2025_01_17());

    // Only set region explicitly if provided
    if let Some(ref region) = merged_settings.region {
        loader = loader.region(Region::new(region.clone()));
    }

    if let Some(ref profile) = merged_settings.profile {
        loader = loader.profile_name(profile.clone());
    }

    // Load with default provider chain
    let base_config = loader.load().await;

    // ... assume role handling ...

    Ok(config)
}
```

**AWS SDK Default Provider Chain** (when region not explicitly set):
1. `AWS_REGION` environment variable
2. `AWS_DEFAULT_REGION` environment variable
3. `~/.aws/config` file (region for the profile)
4. IMDSv2 (if running on EC2)
5. Falls back to `us-east-1`

✅ **This part works correctly** - the AWS config gets the right region.

### 4. CfnContext Creation (`src/cfn/mod.rs:141-150`)
```rust
pub async fn create_context_for_operation(opts: &NormalizedAwsOpts, operation: CfnOperation) -> Result<CfnContext> {
    let config = config_from_normalized_opts(opts).await?;
    let client = Client::new(&config);
    // ...
    CfnContext::new(client, config, time_provider, opts.client_request_token.clone()).await
}
```

The `CfnContext` stores:
- `client: Client` - CloudFormation client configured with correct region
- `aws_config: SdkConfig` - **Contains the actual resolved region**

✅ **This part works correctly** - API calls use the right region.

## The Bug: Display Layer (`src/output/aws_conversion.rs:43-84`)

### Current Code (WRONG):
```rust
pub async fn create_command_metadata(
    context: &CfnContext,
    opts: &NormalizedAwsOpts,
    stack_args: &StackArgs,
    environment: &str,
) -> Result<CommandMetadata, anyhow::Error> {
    // ...

    Ok(CommandMetadata {
        iidy_environment: environment.to_string(),
        region: opts.region.clone().unwrap_or_else(|| "us-east-1".to_string()), // ❌ BUG HERE
        profile: opts.profile.clone(),
        // ...
    })
}
```

**Problem:** Uses `opts.region` which only contains the CLI `--region` flag value, not the actual resolved region.

## Impact Examples

| Scenario | Actual Region Used | Displayed Region | Correct? |
|----------|-------------------|------------------|----------|
| `--region us-west-2` | us-west-2 | us-west-2 | ✅ |
| stack-args.yaml: `Region: us-west-2` | us-west-2 | us-east-1 | ❌ |
| stack-args.yaml: `Region: {prod: us-west-2}` + `--environment prod` | us-west-2 | us-east-1 | ❌ |
| `AWS_REGION=eu-west-1` env var | eu-west-1 | us-east-1 | ❌ |
| `~/.aws/config` profile has `region=ap-south-1` | ap-south-1 | us-east-1 | ❌ |
| No region anywhere (SDK default) | us-east-1 | us-east-1 | ✅ (accidentally) |

## Root Cause

The metadata creation function receives the `CfnContext` which has the actual AWS config with the correctly resolved region, but **ignores it** and only looks at the CLI options.

## The Fix

### Three-Part Solution

#### 1. Early Validation in `src/cfn/mod.rs:145-153`

Add validation when creating the CloudFormation context to fail fast with a helpful error:

```rust
pub async fn create_context_for_operation(opts: &NormalizedAwsOpts, operation: CfnOperation) -> Result<CfnContext> {
    let config = config_from_normalized_opts(opts).await?;

    // Validate that a region is configured before proceeding
    if config.region().is_none() {
        anyhow::bail!(
            "No AWS region configured. Please specify a region via:\n\
             - CLI flag: --region us-east-1\n\
             - Stack args: Region: us-east-1\n\
             - Environment variable: AWS_REGION or AWS_DEFAULT_REGION\n\
             - AWS config file: ~/.aws/config"
        );
    }

    let client = Client::new(&config);
    // ...
}
```

**Why:** Fail early with a clear, actionable error message instead of letting the user see metadata and then get a cryptic AWS SDK error later when the first API call is made.

#### 2. Early Validation in `src/stack_args.rs:136-144`

Add similar validation when loading stack args (which happens before context creation):

```rust
// Configure AWS BEFORE preprocessing (enables $imports with AWS calls)
let aws_config = config_from_merged_settings(&merged_aws_settings).await?;

// Validate that a region is configured (needed for AWS API calls in $imports and CommandsBefore)
let current_region = aws_config.region()
    .map(|r| r.as_ref())
    .ok_or_else(|| anyhow::anyhow!(
        "No AWS region configured. Please specify a region via:\n\
         - CLI flag: --region us-east-1\n\
         - Stack args: Region: us-east-1\n\
         - Environment variable: AWS_REGION or AWS_DEFAULT_REGION\n\
         - AWS config file: ~/.aws/config"
    ))?;
```

**Why:** Stack args loading creates its own AWS config for template preprocessing ($imports) and CommandsBefore execution, which can make AWS API calls (cfn:, s3:, ssm: imports) that require a region.

#### 3. Display Actual Region in `src/output/aws_conversion.rs:75-78`

**Before:**
```rust
region: opts.region.clone().unwrap_or_else(|| "us-east-1".to_string()),
```

**After:**
```rust
region: context.aws_config.region()
    .expect("Region must be configured - validated in create_context_for_operation")
    .as_ref()
    .to_string(),
```

**Why:**
- Uses `expect()` instead of `unwrap_or()` since we now guarantee region is present via early validation
- Extracts the **actual region from the AWS config** that will be used for API calls
- No silent defaulting to "us-east-1" which could mislead users

## Testing Strategy

After fix, verify these scenarios:

### Should Display Correctly:
1. **CLI flag**: `--region us-west-2` → displays "us-west-2"
2. **Stack args string**: `Region: us-west-2` → displays "us-west-2"
3. **Stack args env map**: `Region: {prod: us-west-2}` + `--environment prod` → displays "us-west-2"
4. **Environment variable**: `AWS_REGION=eu-west-1` → displays "eu-west-1"
5. **Profile config**: `~/.aws/config` with `region=ap-south-1` → displays "ap-south-1"
6. **CLI override**: Stack args has region but `--region` specified → displays CLI value (CLI wins)

### Should Fail Early:
7. **No region anywhere**: Should fail during context creation with helpful error message listing all the ways to configure region

## Related Files

- `src/cfn/mod.rs:145-153` - **NEW:** Early region validation in context creation
- `src/stack_args.rs:136-144` - **NEW:** Early region validation in stack args loading
- `src/output/aws_conversion.rs:75-78` - **FIXED:** Use actual region from aws_config
- `src/aws/mod.rs:95-138` - Config resolution (working correctly)
- `src/stack_args.rs:106-130` - Stack args merging (working correctly)

## Notes

- The actual CloudFormation API calls were **not affected** - they always used the correct region
- This was **purely a display/UX bug** in the command metadata output
- The AWS SDK's provider chain is working as designed
- **Bonus improvement:** Now fails early with helpful error if no region is configured anywhere, rather than waiting for the first AWS API call to fail with a cryptic error
