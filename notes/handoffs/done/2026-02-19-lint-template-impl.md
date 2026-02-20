# lint-template -- Implementation Plan

**Date**: 2026-02-19
**Session**: `da44b6f0-9f09-47fe-9b2a-66dc5088a8e3`
**References**: `../iidy-js/src/cfn/lint.ts`, `../iidy-js/src/main.ts:385-394`

## Context

`lint-template` validates a CloudFormation template offline using structural
rules. The JS version uses the `laundry-cfn` npm package for this. The Rust
version needs an equivalent approach.

This is distinct from `template-approval request --lint-template` which
calls the AWS `ValidateTemplate` API. The standalone `lint-template` command
is offline and does not require AWS credentials.

## Key Architecture Decision

**Approach**: Use the AWS `ValidateTemplate` API. We already have working
`validate_template` code in `template_approval_request.rs`. This makes
`lint-template` a thin wrapper around template loading + API validation.
The JS version used `laundry-cfn` (offline npm package) but there's no
Rust equivalent, and the AWS API is actually stricter/better validation.

## Implementation

### What JS does

1. Load stack-args.yaml (only `Template` key, optionally `Parameters`)
2. Load and preprocess the CFN template via `loadCFNTemplate`
3. Call `laundry.lint(templateBody, parameters)` -- offline validation
4. Format errors as `{path}: {message} [{source}]`
5. Return 0 if clean, 1 if errors

### Rust implementation

Create `src/cfn/lint_template.rs`. Uses `run_command_handler_with_stack_args!`
since it needs to load stack-args.yaml for the Template path.

```rust
async fn lint_template_impl(
    output_manager: &mut DynamicOutputManager,
    context: &CfnContext,
    cli: &Cli,
    args: &LintTemplateArgs,
    opts: &NormalizedAwsOpts,
    stack_args: &StackArgs,
) -> Result<i32> {
    let template_result = load_cfn_template(
        stack_args.template.as_deref(),
        &args.argsfile,
        Some(&cli.global_opts.environment),
        TEMPLATE_MAX_BYTES,
        Some(&context.create_s3_client()),
    ).await?;

    let template_body = template_result.template_body
        .ok_or_else(|| anyhow!("Failed to load template body"))?;

    let validation = validate_template(context, &template_body).await?;
    let has_errors = !validation.errors.is_empty();

    output_manager
        .render(OutputData::TemplateValidation(validation))
        .await?;

    Ok(if has_errors { 1 } else { 0 })
}
```

Extract the `validate_template` function from `template_approval_request.rs`
to a shared location (e.g., `src/cfn/template_validation.rs`) so both
`lint-template` and `template-approval request` can use it.

**Preprocessing**: `load_cfn_template` already handles the `render:` prefix
and runs the full YAML preprocessor. If the template uses iidy syntax
(`!$map`, `$imports`, handlebars, etc.), the loaded body will be the
fully-rendered CFN output. So the lint validates the *output* of
preprocessing, which is what CloudFormation actually sees. Any preprocessing
errors (bad `!$` tags, missing imports, handlebars syntax) will surface as
`load_cfn_template` failures before we even reach the CFN API validation.

The `--use-parameters` flag is a no-op for the AWS API approach (the API
doesn't accept parameters). Keep the flag for CLI compatibility but ignore
it. The JS version's parameter support was `laundry-cfn`-specific.

## Codebase Reference

| What | Where |
|------|-------|
| CLI struct | `src/cli.rs:658-662` (LintTemplateArgs) |
| Main dispatch stub | `src/main.rs:231` |
| JS lint implementation | `../iidy-js/src/cfn/lint.ts` |
| Existing validate_template | `src/cfn/template_approval_request.rs:130-155` |
| Template loader | `src/cfn/template_loader.rs` (load_cfn_template) |
| TemplateValidation struct | `src/output/data.rs:435-439` |
| cfn module registry | `src/cfn/mod.rs` |
| run_command_handler_with_stack_args! | `src/cfn/mod.rs:132` |

## Build/Test Commands

Per CLAUDE.md. If using API validation, tests need fixture data for the
CFN client response. If offline, can test with sample templates directly.

## Delegation Strategy

- **Can delegate?** Yes
- **Sub-agent type**: Sonnet (straightforward -- extract function, create thin wrapper)
- **Why**: Clear inputs, small scope, existing validate_template to reuse

## Workflow Instructions

1. Read this file
2. Extract `validate_template` from `template_approval_request.rs` to shared module
3. Create `lint_template.rs` using `run_command_handler_with_stack_args!`
4. Wire into main.rs and cfn/mod.rs
5. `make check-fast` + `make test`

## Progress

- [x] Extract validate_template to shared module
- [x] Implement lint_template.rs
- [x] Wire into main.rs dispatch and cfn/mod.rs
- [x] make check-fast + make test pass (638 tests, 0 failures)
