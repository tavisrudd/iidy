# Getting Started

## Prerequisites

- **Rust toolchain**: install via [rustup](https://rustup.rs/)
- **AWS credentials**: configured via `~/.aws/credentials`, environment variables, or IAM role
- **A CloudFormation template**: any valid YAML or JSON template

## Installation

Build from source:

```
git clone https://github.com/unbounce/iidy
cd iidy
cargo install --path .
```

Verify the installation:

```
iidy --version
```

## Your First Deployment

This walkthrough deploys a minimal S3 bucket, inspects it, and deletes it.

### 1. Create a CloudFormation template

Save this as `cfn-template.yaml`:

```yaml
AWSTemplateFormatVersion: '2010-09-09'
Description: A simple S3 bucket managed by iidy

Parameters:
  BucketSuffix:
    Type: String

Resources:
  Bucket:
    Type: AWS::S3::Bucket
    Properties:
      BucketName: !Sub "${AWS::StackName}-${BucketSuffix}"

Outputs:
  BucketName:
    Value: !Ref Bucket
  BucketArn:
    Value: !GetAtt Bucket.Arn
```

### 2. Create stack-args.yaml

This file tells iidy what to deploy and how. Save it as `stack-args.yaml`:

```yaml
StackName: iidy-getting-started
Template: render:./cfn-template.yaml
Region: us-east-1

Parameters:
  BucketSuffix: demo-assets

Tags:
  Team: platform
  Purpose: getting-started

Capabilities:
  - CAPABILITY_IAM
```

Every field is explained in the [stack-args.yaml reference](#stack-argsyaml-reference)
below.

### 3. Deploy the stack

```
iidy create-stack stack-args.yaml
```

iidy preprocesses `stack-args.yaml` (resolving any `$imports`, `$defs`, and
tags), then preprocesses the template (because of the `render:` prefix), and
submits both to CloudFormation. It then watches stack events until the
operation completes.

### 4. Inspect the stack

```
iidy describe-stack iidy-getting-started
```

This shows the stack status, parameters, outputs, and recent events.

### 5. Update the stack

Change `BucketSuffix` in `stack-args.yaml` to `demo-assets-v2`, then:

```
iidy update-stack stack-args.yaml
```

iidy shows a diff of the changes and prompts for confirmation before
submitting the update.

### 6. Delete the stack

```
iidy delete-stack iidy-getting-started
```

iidy prompts for confirmation, then deletes the stack and watches until
deletion completes.

---

## Adding Preprocessing

The real value of iidy is its YAML preprocessing. Here is the same deployment
with imports and variables.

Create `shared-config.yaml`:

```yaml
team: platform
cost_center: CC-1234
```

Update `stack-args.yaml`:

```yaml
$imports:
  shared: ./shared-config.yaml
  branch: git:branch

$defs:
  env: staging
  bucket_suffix: !$join ["-", [demo, !$ env, !$ branch]]

StackName: !$join ["-", [iidy-demo, !$ env]]
Template: render:./cfn-template.yaml
Region: us-east-1

Parameters:
  BucketSuffix: !$ bucket_suffix

Tags:
  Team: !$ shared.team
  CostCenter: !$ shared.cost_center
  Environment: !$ env
  GitBranch: !$ branch
```

Preview the preprocessed output without deploying:

```
iidy render stack-args.yaml
```

This shows the resolved YAML with all variables substituted and tags
evaluated. Use `iidy render` liberally during development to verify your
templates produce the expected output.

See [yaml-preprocessing.md](yaml-preprocessing.md) for the full preprocessing
reference.

---

## The `render:` Prefix

When iidy loads `stack-args.yaml`, it always preprocesses that file. But the
CloudFormation template referenced by `Template` is only preprocessed if the
path has a `render:` prefix:

```yaml
Template: render:./cfn-template.yaml    # preprocessed by iidy first
Template: ./cfn-template.yaml           # sent to CloudFormation as-is
```

Use `render:` when your template uses iidy preprocessing tags (`!$map`,
`$imports`, `!$if`, etc.). Omit it for plain CloudFormation templates.

You can also preview a template directly:

```
iidy render cfn-template.yaml
```

---

## Environment-Based Configuration

The `--environment` flag (short: `-e`) loads environment-specific settings.
Fields like `Region`, `Profile`, and `AssumeRoleARN` in `stack-args.yaml` can
be strings or environment maps:

```yaml
StackName: !$join ["-", [my-app, "{{ environment }}"]]
Template: render:./cfn-template.yaml

Region:
  development: us-east-1
  staging: us-west-2
  production: us-east-1

Profile:
  development: dev-profile
  staging: staging-profile
  production: prod-profile
```

Deploy to different environments:

```
iidy -e development create-stack stack-args.yaml
iidy -e staging create-stack stack-args.yaml
iidy -e production create-stack stack-args.yaml
```

The `environment` value is also available in Handlebars expressions as
`{{ environment }}`.

---

## Output Modes

iidy has three output modes:

- **interactive** (default in terminals): Collapsible sections, color, keyboard
  controls. Press `j` to scroll down, `k` to scroll up, `q` to quit, `h` for
  help.
- **plain**: No ANSI codes, suitable for CI logs.
- **json**: Machine-readable JSON output for scripting.

Set the mode with `--output-mode`:

```
iidy --output-mode plain describe-stack my-stack
iidy --output-mode json describe-stack my-stack
```

---

## stack-args.yaml Reference

All fields are optional except `StackName` and `Template`.

| Field | Type | Description |
|-------|------|-------------|
| `StackName` | string | CloudFormation stack name (required) |
| `Template` | string | Path to the template file. Prefix with `render:` for iidy preprocessing (required) |
| `Region` | string or env map | AWS region |
| `Profile` | string or env map | AWS CLI profile name |
| `AssumeRoleARN` | string or env map | IAM role to assume before operations |
| `ServiceRoleARN` | string | IAM service role for CloudFormation to use |
| `RoleARN` | string | Fallback for ServiceRoleARN (used if ServiceRoleARN is not set) |
| `Parameters` | mapping | CloudFormation stack parameters (key: value) |
| `Tags` | mapping | Stack tags (key: value). An `environment` tag is added automatically |
| `Capabilities` | list | Required capabilities, e.g., `[CAPABILITY_IAM, CAPABILITY_NAMED_IAM]` |
| `NotificationARNs` | list | SNS topic ARNs for stack event notifications |
| `TimeoutInMinutes` | integer | Stack creation timeout |
| `OnFailure` | string | Action on creation failure: `ROLLBACK`, `DELETE`, or `DO_NOTHING` |
| `DisableRollback` | boolean | Disable automatic rollback on failure |
| `EnableTerminationProtection` | boolean | Prevent accidental stack deletion |
| `StackPolicy` | mapping | Stack policy document (inline YAML) |
| `ResourceTypes` | list | Allowed resource types for the template |
| `UsePreviousTemplate` | boolean | Reuse the existing template during update |
| `UsePreviousParameterValues` | list | Parameter keys to carry forward from previous values |
| `ApprovedTemplateLocation` | string | S3 location of the approved template |
| `CommandsBefore` | list | Shell commands to run before the CloudFormation operation |

Fields marked "env map" accept either a string or a mapping of environment
names to strings. The `--environment` flag selects which value to use.

---

## Next Steps

- [yaml-preprocessing.md](yaml-preprocessing.md) -- full preprocessing tag reference
- [command-reference.md](command-reference.md) -- all commands and options
- [import-types.md](import-types.md) -- import source types for `$imports`
