# AWS Configuration Resolution Order

This document describes the sources and order of precedence for AWS region and IAM credential resolution in iidy's Rust implementation, specifically for code paths triggered by `run_command_handler_with_stack_args!`.

## Overview

AWS configuration in iidy is resolved through a two-stage merging process:
1. **CLI flags** are merged with **stack-args.yaml settings** (CLI takes precedence)
2. The merged settings are passed to the AWS SDK configuration loader, which applies its own resolution chain

**Critical Note**: Environment variables like `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` **always take precedence** over profile-based credentials, even when `--profile` or `Profile:` is specified in CLI/stack-args. The AWS SDK credential provider chain checks environment variables before profile files.

## Resolution Order (Highest to Lowest Precedence)

### Stage 1: iidy-Specific Merging (CLI + stack-args.yaml)

Implemented in `src/cfn/stack_args.rs:load_stack_args()` and `src/cfn/mod.rs:run_command_handler_with_stack_args!`

#### AWS Region
1. **CLI flag**: `--region <region>`
2. **Stack args file**: `Region: <region>` in stack-args.yaml (supports environment maps)
3. Falls through to AWS SDK default chain (Stage 2)

#### AWS Profile
1. **CLI flag**: `--profile <profile>`
2. **Stack args file**: `Profile: <profile>` in stack-args.yaml (supports environment maps)
3. Falls through to AWS SDK default chain (Stage 2)

#### AssumeRole ARN
1. **CLI flag**: `--assume-role-arn <arn>`
2. **Stack args file**: `AssumeRoleARN: <arn>` in stack-args.yaml (supports environment maps)
3. No role assumption if not specified

**Merging code location**: `src/cfn/stack_args.rs:129-134`
```rust
let merged_aws_settings = AwsSettings {
    profile: cli_aws_settings.profile.clone().or_else(|| argsfile_aws_settings.profile.clone()),
    region: cli_aws_settings.region.clone().or_else(|| argsfile_aws_settings.region.clone()),
    assume_role_arn: cli_aws_settings.assume_role_arn.clone().or_else(|| argsfile_aws_settings.assume_role_arn.clone()),
};
```

**Environment map resolution**: Stack-args.yaml supports environment-specific values for `Profile`, `Region`, and `AssumeRoleARN`:
```yaml
Region:
  dev: us-east-1
  prod: us-west-2
```
These maps are resolved using the `--environment` flag value before merging (line 111-118).

### Stage 2: AWS SDK Default Provider Chain

Implemented in `src/aws/mod.rs:config_from_merged_settings()`

After iidy's merging, the merged settings are passed to the AWS SDK's configuration loader, which follows the standard AWS SDK resolution chain:

#### Region Resolution (via `aws_config::defaults()`)
1. **Explicitly set region** from Stage 1 merge (if any) - set via `loader.region()` at line 109-110
   - This **overrides** all environment variables and config files for region
2. **AWS_REGION** environment variable (only if region not set in Stage 1)
3. **AWS_DEFAULT_REGION** environment variable
4. **~/.aws/config** profile-specific region setting
5. **~/.aws/config** default profile region setting
6. Falls back to no region (causes validation error in iidy)

#### Credentials Resolution (via AWS SDK credential chain)

**IMPORTANT**: Setting `profile_name` on the loader does **NOT** override environment variable credentials. It only tells the SDK which profile to use when loading from `~/.aws/credentials` or `~/.aws/config`.

The AWS SDK credential provider chain (automatically loaded via `aws_config::defaults()`) checks in this order:

1. **AWS_ACCESS_KEY_ID** and **AWS_SECRET_ACCESS_KEY** environment variables (and optionally **AWS_SESSION_TOKEN**)
   - **These always take precedence**, even if `--profile` or `Profile:` in stack-args.yaml is set
   - The profile setting is ignored when these env vars are present
2. **AWS_PROFILE** environment variable (for selecting which profile to load)
   - **Overridden by** `loader.profile_name()` if profile was specified in Stage 1 (line 113-115)
3. **Profile specified via** `loader.profile_name()` from Stage 1 merge (CLI `--profile` or stack-args `Profile:`)
4. **Web Identity Token** credentials from environment (**AWS_WEB_IDENTITY_TOKEN_FILE** + **AWS_ROLE_ARN**)
5. **ECS Container credentials** (via **AWS_CONTAINER_CREDENTIALS_RELATIVE_URI** or **AWS_CONTAINER_CREDENTIALS_FULL_URI**)
6. **EC2 Instance Metadata Service (IMDS)** credentials
7. **~/.aws/credentials** file (uses profile from step 2-3, or "default" if none specified)
8. Falls back to no credentials (causes AWS API errors)

**After base credentials are loaded**, if `AssumeRoleARN` was specified in Stage 1, iidy wraps the base credentials with an STS AssumeRole provider (line 123-130).

## Key Configuration Files

### ~/.aws/config
Standard AWS CLI config file format:
```ini
[default]
region = us-west-2

[profile production]
region = us-east-1
role_arn = arn:aws:iam::123456789012:role/MyRole
source_profile = default
```

### ~/.aws/credentials
Standard AWS CLI credentials file format:
```ini
[default]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

[production]
aws_access_key_id = AKIAI44QH8DHBEXAMPLE
aws_secret_access_key = je7MtGbClwBF/2Zp9Utk/h3yCo8nvbEXAMPLEKEY
```

## Special Behaviors

### AWS_SDK_LOAD_CONFIG Environment Variable
- **Set automatically** by iidy if `~/.aws` directory exists (line 96-105)
- This ensures AWS SDK loads settings from `~/.aws/config` (not just credentials file)

### Region Validation
- iidy **requires** a region to be configured (line 140-148, 218-226)
- Operations will fail with helpful error message if no region found in any source
- Error message lists all ways to configure region

### AssumeRole Implementation
- If `AssumeRoleARN` specified, iidy creates an `AssumeRoleProvider` using the base credentials
- Session name is hardcoded to `"iidy"` (line 126)
- AssumeRole provider wraps base credentials from AWS SDK chain (line 123-130)

### Client Request Token
- Passed separately via `--client-request-token` CLI flag
- Not part of AWS config resolution
- Used for CloudFormation operation idempotency (line 139, 208-209)

## Code References

### Main Entry Points
- **Macro**: `src/cfn/mod.rs:107-158` - `run_command_handler_with_stack_args!`
- **Stack args loading**: `src/cfn/stack_args.rs:94-215` - `load_stack_args()`
- **AWS config creation**: `src/aws/mod.rs:95-138` - `config_from_merged_settings()`

### Supporting Functions
- **Merging logic**: `src/cfn/stack_args.rs:129-134`
- **Environment map resolution**: `src/cfn/stack_args.rs:59-75` - `resolve_env_map()`
- **AWS settings extraction**: `src/cfn/stack_args.rs:122-134`
- **Context creation**: `src/cfn/mod.rs:212-235` - `create_context_from_config()`

## Debugging Tips

### Check Effective Region
The region validation error messages will show which sources were checked (line 142-148, 220-226).

### Check Effective Profile
Profile resolution only affects which profile is loaded from credential/config files. It does **not** override environment variable credentials.

**To use profile-based credentials**: Make sure `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` are **not** set in your environment, as they always take precedence.

### Check Credential Source
AWS SDK credential chain doesn't expose which source was used. Check in order:
1. **Environment variables** (`AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_SESSION_TOKEN`) - **always checked first**
2. Named profile files (`~/.aws/credentials`, `~/.aws/config`) - only used if env vars not present
3. Instance metadata (if on EC2) - only used if env vars and profile files not present

### Verify Stack Args Merging
Stack args are loaded and merged before AWS config creation. Add debug logging at line 129-134 to see merged values.

### Test with Explicit Settings
Use CLI flags to override all settings for debugging:
```bash
iidy --region us-east-1 --profile myprofile create-stack --argsfile stack-args.yaml
```

## Comparison with iidy-js

This Rust implementation closely matches iidy-js behavior:
- ✅ CLI flags override stack-args.yaml settings
- ✅ Environment map resolution for `Profile`, `Region`, `AssumeRoleARN`
- ✅ Falls back to AWS SDK default chain
- ✅ Automatic `AWS_SDK_LOAD_CONFIG=1` if `~/.aws` exists
- ✅ AssumeRole wrapping with session name "iidy"
- ✅ Same merged config used for preprocessing and CFN operations

Key difference:
- Rust uses AWS SDK v2 credential chain (slightly different order than v1)
- Rust implementation validates region earlier (during stack args loading)
