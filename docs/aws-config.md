# AWS Configuration Resolution

This document describes how iidy resolves AWS credentials, regions, and profiles,
covering the full path from CLI invocation through the AWS SDK credential chain.

## Overview

Configuration resolution is a two-stage process:

**Stage 1** — iidy-specific merging: CLI flags are merged with `stack-args.yaml`
settings, with CLI taking precedence. This produces a single `AwsSettings` struct
(`src/aws/mod.rs`) with optional `region`, `profile`, and `assume_role_arn` fields.

**Stage 2** — AWS SDK default chain: The merged settings are handed to the AWS SDK
loader. Any field not set by Stage 1 falls through to the SDK's own resolution chain
(environment variables, config files, instance metadata).

One critical constraint governs both stages: environment variables
`AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` always take precedence over any
profile-based credential, even when `--profile` or `Profile:` is configured. The
SDK checks environment variables before profile files.

## Stage 1: iidy-Specific Merging

### Entry points

For commands that use a stack-args file, merging happens in `load_stack_args()` in
`src/cfn/stack_args.rs`. For commands without a stack-args file (e.g., `list-stacks`,
`describe-stack`), the CLI settings pass directly through `config_from_normalized_opts()`
in `src/aws/mod.rs`.

### Region

1. CLI flag `--region`
2. `Region:` in stack-args.yaml (supports environment maps; see below)
3. Falls through to Stage 2

### Profile

1. CLI flag `--profile`
2. `Profile:` in stack-args.yaml (supports environment maps)
3. Falls through to Stage 2

### AssumeRole ARN

1. CLI flag `--assume-role-arn`
2. `AssumeRoleARN:` in stack-args.yaml (supports environment maps)
3. No role assumption if not specified

The merge itself uses `Option::or_else`, giving CLI flags priority:

```rust
let merged_aws_settings = AwsSettings {
    profile: cli_aws_settings.profile.clone().or_else(|| argsfile_aws_settings.profile.clone()),
    region: cli_aws_settings.region.clone().or_else(|| argsfile_aws_settings.region.clone()),
    assume_role_arn: cli_aws_settings.assume_role_arn.clone().or_else(|| argsfile_aws_settings.assume_role_arn.clone()),
};
```

## Environment Maps in stack-args.yaml

`Region`, `Profile`, and `AssumeRoleARN` each accept either a plain string or an
environment-keyed map. The active environment is selected by the `--environment`
CLI flag (e.g., `--environment prod`).

```yaml
Region:
  dev: us-east-1
  prod: us-west-2

Profile:
  dev: default
  prod: my-prod-profile

AssumeRoleARN:
  prod: arn:aws:iam::123456789012:role/DeployRole
```

Resolution happens in `resolve_env_map()` in `src/cfn/stack_args.rs` before any
AWS SDK calls. If the environment key is absent from the map, `load_stack_args()`
returns an error immediately.

The `--environment` flag also controls an automatic `Tags: {environment: <value>}`
injection into the stack args, ensuring every deployed stack is tagged with its
environment.

## Stage 2: AWS SDK Default Provider Chain

After Stage 1 produces a `AwsSettings`, `config_from_merged_settings()` in
`src/aws/mod.rs` applies those settings to the AWS SDK loader and calls
`loader.load().await`.

### Region resolution

If Stage 1 produced a region, it is set on the loader via `loader.region()` and
takes precedence over all environment variables and config files. If no region was
set in Stage 1, the SDK checks in order:

1. `AWS_REGION` environment variable
2. `AWS_DEFAULT_REGION` environment variable
3. Profile-specific region in `~/.aws/config`
4. Default profile region in `~/.aws/config`

iidy requires a region to be resolved. If none is found after Stage 2, an error
is returned listing all resolution sources.

### Credential resolution

If Stage 1 produced a profile name, it is set via `loader.profile_name()`. This
determines which profile the SDK reads from credential/config files, but does not
override environment variable credentials.

The SDK credential chain checks in this order:

1. `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY` (+ optional `AWS_SESSION_TOKEN`)
   — always checked first, takes precedence over everything else
2. `AWS_PROFILE` environment variable — overridden by `loader.profile_name()` if
   a profile was set in Stage 1
3. Profile from `loader.profile_name()` (from `--profile` or `Profile:`)
4. Web Identity Token credentials (`AWS_WEB_IDENTITY_TOKEN_FILE` + `AWS_ROLE_ARN`)
5. ECS container credentials (`AWS_CONTAINER_CREDENTIALS_RELATIVE_URI` or
   `AWS_CONTAINER_CREDENTIALS_FULL_URI`)
6. EC2 Instance Metadata Service (IMDS)

## Special Behaviors

### AWS_SDK_LOAD_CONFIG

iidy sets `AWS_SDK_LOAD_CONFIG=1` automatically if `~/.aws` exists. This causes
the AWS SDK to read `~/.aws/config` in addition to `~/.aws/credentials`. Without
it, named profiles defined only in `~/.aws/config` would not be visible to the SDK.

This happens at the top of `config_from_merged_settings()` before the loader is
constructed.

### Region validation

iidy validates that a region was resolved before proceeding with any AWS API call.
This check occurs in `load_stack_args()` immediately after the SDK config is loaded.
If no region is found, the error message explicitly lists all resolution sources:

```
No AWS region configured. Please specify a region via:
- CLI flag: --region us-east-1
- Stack args: Region: us-east-1
- Environment variable: AWS_REGION or AWS_DEFAULT_REGION
- AWS config file: ~/.aws/config
```

### AssumeRole

If `AssumeRoleARN` is present after Stage 1 merging, `config_from_merged_settings()`
wraps the base credentials from the SDK chain with an `AssumeRoleProvider`. The
session name is always `"iidy"`. The STS call uses the base credentials, so
environment variable credentials or profile credentials are used as the source
identity for the role assumption.

```rust
let provider = AssumeRoleProvider::builder(role)
    .configure(&base_config)
    .session_name("iidy")
    .build()
    .await;
```

### $envValues injection

After AWS configuration is loaded and the effective region is known,
`load_stack_args()` injects a `$envValues` block into the stack-args document
before final preprocessing. This block exposes runtime values to Handlebars
templates in the stack-args file:

```yaml
# Injected automatically; not written to disk
$envValues:
  region: us-east-1
  environment: prod
  iidy:
    command: create-stack
    environment: prod
    region: us-east-1
    profile: my-prod-profile
```

These values are accessible as `{{ region }}`, `{{ environment }}`, and
`{{ iidy.command }}` in Handlebars expressions within stack-args.yaml.

### Global SSM configuration

After stack args are parsed, `load_stack_args()` queries the SSM path `/iidy/`
for account-level defaults:

- `/iidy/default-notification-arn` — appended to `NotificationARNs` if the SNS
  topic exists and is accessible
- `/iidy/disable-template-approval` — if `"true"`, clears `ApprovedTemplateLocation`

SSM failures are silently ignored so that environments without these parameters
are unaffected.

## Debugging Tips

### Check which credential source is active

The AWS SDK does not expose which provider in the chain was used. Check manually
in this order:

1. Are `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` set? If yes, they win —
   profile settings are irrelevant.
2. Is `AWS_PROFILE` set? That controls profile selection unless `--profile` or
   `Profile:` overrides it.
3. Check `~/.aws/credentials` and `~/.aws/config` for the effective profile.

### Profile not taking effect

If `--profile myprofile` seems to have no effect on credentials, check for
`AWS_ACCESS_KEY_ID` in your environment. Environment variable credentials
always supersede profile credentials regardless of what iidy passes to the SDK.

### Region errors

The region validation error enumerates all sources. If a region is configured via
`~/.aws/config` but not being picked up, verify that `~/.aws` exists (triggering
`AWS_SDK_LOAD_CONFIG`) and that the profile name matches.

### Override everything for a single invocation

```bash
iidy --region us-east-1 --profile myprofile --environment prod create-stack stack-args.yaml
```

CLI flags always win over stack-args settings, which always win over environment
variable and config-file defaults (except for credential priority, where
`AWS_ACCESS_KEY_ID` still beats everything).

### Credential files

```ini
# ~/.aws/config
[default]
region = us-west-2

[profile production]
region = us-east-1
role_arn = arn:aws:iam::123456789012:role/MyRole
source_profile = default
```

```ini
# ~/.aws/credentials
[default]
aws_access_key_id = AKIAIOSFODNN7EXAMPLE
aws_secret_access_key = wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY

[production]
aws_access_key_id = AKIAI44QH8DHBEXAMPLE
aws_secret_access_key = je7MtGbClwBF/2Zp9Utk/h3yCo8nvbEXAMPLEKEY
```
