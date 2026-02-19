# iidy -- CloudFormation with Confidence

iidy ("Is it done yet?") deploys CloudFormation stacks with a YAML
preprocessing layer that supports imports, variables, conditionals, and
collection transforms. It wraps the CloudFormation API with readable progress
output and changeset support.

## Deploy a stack in 60 seconds

```yaml
# stack-args.yaml
StackName: my-app
Template: render:./cfn-template.yaml
Region: us-east-1
Parameters:
  InstanceType: t3.micro
Tags:
  Team: platform
```

```
iidy create-stack stack-args.yaml
```

## What iidy adds over raw CloudFormation

- **YAML preprocessing**: import values from files, environment variables, SSM
  Parameter Store, S3, and other CloudFormation stacks. Define variables,
  conditionals, and collection transforms to generate repetitive template
  sections.
- **Stack lifecycle management**: `create-stack`, `update-stack`,
  `create-or-update`, `delete-stack`, and changeset workflows with diffs and
  confirmation prompts.
- **Interactive progress display**: collapsible, color-coded event stream with
  keyboard navigation. Plain and JSON output modes for CI.
- **Environment-based configuration**: a single `stack-args.yaml` can target
  multiple environments (dev, staging, production) via `--environment` flag and
  environment maps.

## Commands

| Category | Command | Description |
|----------|---------|-------------|
| Deploy | `create-stack` | Create a new stack |
| | `update-stack` | Update an existing stack |
| | `create-or-update` | Create or update a stack |
| | `delete-stack` | Delete a stack |
| Changesets | `create-changeset` | Create a changeset for review |
| | `exec-changeset` | Execute a changeset |
| Monitor | `describe-stack` | Show stack status, outputs, events |
| | `watch-stack` | Watch a stack operation in progress |
| | `describe-stack-drift` | Detect configuration drift |
| Info | `list-stacks` | List stacks in a region |
| | `get-stack-template` | Download a stack's template |
| | `get-import` | Resolve an import URI |
| Preprocess | `render` | Preview preprocessed YAML output |
| Cost | `estimate-cost` | Estimate stack costs |
| SSM | `param set/get/get-by-path` | Manage SSM parameters |
| Utilities | `explain` | Explain error codes |
| | `completion` | Generate shell completions |

## Preprocessing Example

```yaml
$imports:
  vpc: ./vpc-outputs.yaml
  branch: git:branch

$defs:
  env: production
  app: my-service

Resources: !$mergeMap
  items:
    - {name: api, port: 3000}
    - {name: web, port: 80}
  template:
    !$join ["",[!$ app, "-", !$ item.name, "-sg"]]:
      Type: AWS::EC2::SecurityGroup
      Properties:
        GroupDescription: !$join [" ", [!$ item.name, "on port", !$ item.port]]
        VpcId: !$ vpc.VpcId
```

Use `iidy render template.yaml` to see the resolved output.

## Documentation

- **[Getting Started](docs/getting-started.md)** -- installation, first
  deployment, stack-args.yaml reference
- **[YAML Preprocessing](docs/yaml-preprocessing.md)** -- tags, imports,
  Handlebars helpers, debugging
- **[Command Reference](docs/command-reference.md)** -- all commands, options,
  and exit codes
- **[Import Types](docs/import-types.md)** -- file, env, git, s3, ssm, cfn,
  and other import sources
- **[Security](docs/SECURITY.md)** -- import system security model for remote
  templates

## Origin

This is a Rust rewrite of the TypeScript [iidy](https://github.com/unbounce/iidy).
The port is feature-complete for standard usage. Custom resource templates
(`$params`, `!$expand`) are not yet implemented.

All code was written by Claude and Codex under strict guidance and review by
[@tavisrudd](https://github.com/tavisrudd).
