# convert-stack-to-iidy -- Implementation Plan

**Date**: 2026-02-19
**Session**: `da44b6f0-9f09-47fe-9b2a-66dc5088a8e3`
**References**: `../iidy-js/lib/cfn/convertStackToIidy.js`

## Context

`convert-stack-to-iidy` reverse-engineers a running CloudFormation stack into
iidy project files. It's a developer convenience command -- you point it at an
existing stack and it generates `stack-args.yaml`, `cfn-template.yaml`,
`_original-template.*`, and `stack-policy.json` in the output directory.

This is the most complex of the three remaining stubs because it calls
multiple AWS APIs and performs template transformations (environment
parameterization, key sorting, SSM migration).

## Key Architecture Decisions

**No `--move-params-to-ssm` in initial implementation**: The SSM migration
feature (`--move-params-to-ssm`) requires KMS alias lookup and SSM writes.
The `param` commands are themselves stubs. Defer this flag to a follow-up
after `param` commands are implemented. Print a "not yet implemented" error
if the flag is passed.

**Key sorting**: The JS version sorts template keys in CFN-idiomatic order
(AWSTemplateFormatVersion first, then Description, Parameters, etc.). We can
implement this as a simple key-weight map. This is cosmetic -- defer to a
follow-up if it proves complex.

## Implementation

### What JS does (step by step)

1. Call `cfn.getTemplate({ StackName, TemplateStage: 'Original' })` to get raw template
2. Parse template body, detect JSON vs YAML format, optionally sort keys
3. Call `cfn.describeStacks({ StackName })` to get stack metadata
4. Call `cfn.getStackPolicy({ StackName })` to get stack policy
5. Create output directory
6. Write `stack-policy.json` (fetched policy, or default allow-all)
7. Write `_original-template.{json|yaml}` (raw template, preserving format)
8. Write `cfn-template.yaml` (processed template as YAML)
9. Build `stack-args.yaml` from stack metadata:
   - `$defs.project` from tags or `--project` flag
   - `$imports.build_number: env:build_number:0`
   - StackName parameterized: replace env names with `{{environment}}`, trailing digits with `{{build_number}}`
   - Tags parameterized: project -> `{{project}}`, environment -> `{{environment}}`
   - Parameters: Environment -> `{{environment}}`
   - Capabilities, TimeoutInMinutes, EnableTerminationProtection, NotificationARNs, RoleARN, DisableRollback
10. Write `stack-args.yaml`

### Rust implementation

Create `src/cfn/convert_stack_to_iidy.rs`.

The handler needs `CfnContext` (for CFN + S3 clients). Use `run_command_handler!`
macro (no stack args needed -- this reads from a live stack, not a file).

```rust
async fn convert_stack_to_iidy_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &ConvertArgs,
    opts: &NormalizedAwsOpts,
) -> Result<i32> {
    // 1. Fetch template
    let template_resp = context.client
        .get_template()
        .stack_name(&args.stackname)
        .template_stage(TemplateStage::Original)
        .send().await?;
    let template_body = template_resp.template_body()
        .ok_or_else(|| anyhow!("No template body"))?;

    // 2. Detect format (JSON vs YAML)
    let is_json = template_body.trim_start().starts_with('{');

    // 3. Describe stack
    let stack = describe_stack_raw(context, &args.stackname).await?;

    // 4. Get stack policy
    let policy = context.client
        .get_stack_policy()
        .stack_name(&args.stackname)
        .send().await
        .ok()
        .and_then(|r| r.stack_policy_body().map(String::from));

    // 5. Create output dir
    std::fs::create_dir_all(&args.output_dir)?;

    // 6-8. Write template files
    write_file(output_dir, "stack-policy.json", &policy_content)?;
    write_file(output_dir, &original_filename, template_body)?;
    write_file(output_dir, "cfn-template.yaml", &yaml_template)?;

    // 9-10. Build and write stack-args.yaml
    let stack_args_content = build_stack_args_yaml(&stack, args)?;
    write_file(output_dir, "stack-args.yaml", &stack_args_content)?;

    Ok(0)
}
```

Key helper functions:
- `parameterize_env(s: &str) -> String` -- replace known env names (production, staging, development, integration, testing) with `{{environment}}`
- `parameterize_stack_name(name: &str) -> String` -- also replace trailing `-\d+` with `-{{build_number}}`
- `build_stack_args_yaml(stack: &Stack, args: &ConvertArgs) -> Result<String>` -- build the YAML content string
- `default_stack_policy() -> &str` -- the allow-all policy JSON

### Existing infrastructure to reuse

- `context.client.get_template()` -- AWS SDK, already available
- `context.client.describe_stacks()` -- used in `src/cfn/describe_stack.rs`
- `context.client.get_stack_policy()` -- AWS SDK, already available
- Stack field extraction patterns from `src/output/aws_conversion.rs`
- `serde_yaml` and `serde_json` for serialization

### Chunks

**Chunk 1**: Core implementation (no SSM, no key sorting)
- Fetch template, describe stack, get policy
- Write all 4 files
- Parameterize environment/project in StackName, Tags, Parameters
- Build stack-args.yaml

**Chunk 2** (follow-up): Key sorting for cfn-template.yaml
- Implement CFN key-weight ordering
- Apply to template before writing

**Chunk 3** (after param commands): `--move-params-to-ssm`
- KMS alias lookup
- SSM parameter writes
- Update stack-args.yaml to use `!$` ssmParams references

## Codebase Reference

| What | Where |
|------|-------|
| CLI struct | `src/cli.rs:665-674` (ConvertArgs) |
| Main dispatch stub | `src/main.rs:232` |
| JS implementation | `../iidy-js/lib/cfn/convertStackToIidy.js` |
| describe_stacks usage | `src/cfn/describe_stack.rs` |
| get_template usage | `src/cfn/get_stack_template.rs` |
| Stack field extraction | `src/output/aws_conversion.rs` (convert_stack_to_definition) |
| cfn module registry | `src/cfn/mod.rs` |
| run_command_handler! | `src/cfn/mod.rs:73` |

## Build/Test Commands

Per CLAUDE.md. This command makes AWS API calls so unit tests need fixture
data. Look at how `describe_stack.rs` and `get_stack_template.rs` are tested
(or add fixture-based tests following existing patterns in `tests/`).

## Delegation Strategy

- **Chunk 1**: Can delegate to Opus sub-agent (multiple interacting AWS calls, parameterization logic)
- **Chunk 2**: Can delegate to Sonnet (isolated sorting utility)
- **Chunk 3**: Blocked on param commands implementation

## Workflow Instructions

1. Read this file
2. Implement Chunk 1 only (core without SSM or key sorting)
3. For `--move-params-to-ssm`, print error: "not yet implemented" and return 1
4. For `--sortkeys`, ignore for now (it defaults to true but is cosmetic)
5. Wire into main.rs and cfn/mod.rs
6. `make check-fast` + `make test`
7. Update Progress below

## Progress

- [x] Chunk 1: Core implementation (fetch, parameterize, write files)
- [x] Wire into main.rs dispatch and cfn/mod.rs
- [x] Add stub error for --move-params-to-ssm
- [x] make check-fast + make test pass (638/638, 0 warnings)
- [ ] Chunk 2 (follow-up): Key sorting
- [ ] Chunk 3 (after param commands): --move-params-to-ssm
