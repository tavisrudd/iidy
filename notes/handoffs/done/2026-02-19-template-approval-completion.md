# Template Approval -- Completion Plan

**Date**: 2026-02-19
**Context**: Code review identified several issues in the template-approval implementation.
The MD5->SHA256 hash change is intentional (not a bug).

## Issues to Fix

### Issue A (Critical): `latest` key path is wrong in review

**File**: `src/cfn/template_approval_review.rs`

Lines 71 and 223 use the bare string `"latest"` for the S3 key. The JS version
(`approval/index.ts:104,135`) derives the `latest` path from the directory of the
pending key: `${bucketDir}/latest` (e.g., `templates/latest`).

**Fix**: Derive `bucketDir` from the pending key the same way JS does:
```rust
let bucket_dir = std::path::Path::new(&pending_key)
    .parent()
    .map(|p| p.to_string_lossy().to_string())
    .unwrap_or_default();
let latest_key = if bucket_dir.is_empty() {
    "latest".to_string()
} else {
    format!("{bucket_dir}/latest")
};
```

Use `latest_key` in:
- Line 71: `download_template(&s3_client, &bucket, &latest_key)`
- Line 223: `.key(&latest_key)` in `approve_template`

`approve_template` needs the `latest_key` passed as a parameter instead of hardcoding.

### Issue B (Critical): User confirmation is not wired up

**File**: `src/cfn/template_approval_review.rs`

Lines 104-114 create a `ConfirmationRequest` with `response_tx: None` and hardcode
`user_confirmed = true`. The output manager already has `request_confirmation()` which
handles the oneshot channel correctly (see `delete_stack.rs:177` for usage pattern).

**Fix**: Replace the manual ConfirmationRequest + hardcoded bool with:
```rust
let user_confirmed = output_manager
    .request_confirmation("Would you like to approve these changes?".to_string())
    .await?;
```

This matches the pattern used in `delete_stack.rs` and `changeset_operations.rs`.

### Issue C (Moderate): Validation errors don't halt the workflow

**File**: `src/cfn/template_approval_request.rs`

Lines 78-94 render the validation result but continue uploading even when
`validation_result.errors` is non-empty. The JS version returns FAILURE on lint errors
(`approval/index.ts:43-50`).

**Fix**: After rendering the validation result, check for errors and bail:
```rust
if args.lint_template {
    let validation_result = validate_template(/* ... */).await?;
    let has_errors = !validation_result.errors.is_empty();
    output_manager
        .render(OutputData::TemplateValidation(validation_result))
        .await?;
    if has_errors {
        return Ok(1);
    }
}
```

### Issue D (Moderate): Large template validation is broken

**File**: `src/cfn/template_approval_request.rs`

Lines 140-142 pass the literal string `"template_url"` to CloudFormation's
`validate_template` API when the template exceeds 51200 bytes.

**Fix**: Skip validation for large templates with a warning. The template will be
uploaded to S3 as pending regardless, and CloudFormation validates on actual
create/update. Add to warnings:
```rust
if template_body.len() > 51200 {
    warnings.push("Template exceeds 51200 bytes; skipping CFN validation (will be validated on deploy)".to_string());
} else {
    validation_request = validation_request.template_body(template_body);
    match validation_request.send().await {
        Ok(_) => {}
        Err(e) => errors.push(format!("Template validation failed: {e}")),
    }
}
```

### Issue E (Moderate): Parameter validation is a stub

**File**: `src/cfn/template_approval_request.rs`

Lines 160-163 have a stub for `--lint-using-parameters`. The JS version uses a local
`lintTemplate` function, not the CFN API.

**Fix**: Remove the `--lint-using-parameters` CLI flag and the stub code. It can be
re-added when properly implemented. This avoids exposing a broken feature.

Remove from `cli.rs`:
```rust
#[arg(long = "lint-using-parameters")]
pub lint_using_parameters: bool,
```

Remove from `validate_template`:
- The `using_parameters` parameter
- The `stack_args` parameter (only used for this)
- The stub warning

Simplify `TemplateValidation` to remove `using_parameters` field, and update renderers.

### Issue F (Minor): `pending_exists` not verified

**File**: `src/cfn/template_approval_review.rs`

Line 52 hardcodes `pending_exists: true`. If the pending template doesn't exist, the
subsequent `download_template` call on line 70 will fail with an opaque S3 error.

**Fix**: Check for the pending template before downloading:
```rust
let pending_exists = check_template_exists(&s3_client, &bucket, &pending_key).await?;
if !pending_exists {
    anyhow::bail!("Pending template not found at {}", args.url);
}
```

### Issue G (Minor): Diff context default should be 500

**File**: `src/cli.rs`

Line 608 has `default_value_t = 100`, JS uses 500.

**Fix**: Change to `default_value_t = 500`.

### Issue H (Minor): Hard-code us-east-1 for review (match JS behavior)

**File**: `src/cfn/template_approval_review.rs`

The JS version forces `us-east-1` for the review command (`approval/index.ts:78`).
The Rust version doesn't do this.

**Fix**: For now, replicate the JS behavior -- when no region is specified, default to
`us-east-1` for the review command. Document in `docs/dev/js-compatibility.md` as a
known JS behavior carried forward that should be parameterized in the future.

This requires passing a region override into the `run_command_handler!` macro context
creation, or overriding the region after context creation. Look at how `CfnContext` is
built -- the simplest approach may be to check if `opts.region` is None and default it
for this command before context creation.

### Issue I (Minor): TemplateDiff stores full template bodies

**File**: `src/cfn/template_approval_review.rs`

Lines 79-85 clone both templates into `TemplateDiff` alongside the pre-computed diff.
The renderers only use `diff_output`.

**Fix**: Remove `old_template` and `new_template` from `TemplateDiff`. Update the
struct in `output/data.rs` and adjust any renderer code that references these fields.

---

## Execution Plan

Single chunk -- all fixes are small and independent. The changes touch:
- `src/cfn/template_approval_review.rs` (Issues A, B, F, H)
- `src/cfn/template_approval_request.rs` (Issues C, D, E)
- `src/cli.rs` (Issues E, G)
- `src/output/data.rs` (Issues E, I)
- `src/output/renderers/interactive.rs` (Issues E, I)
- `src/output/renderers/json.rs` (Issues E, I)
- `docs/dev/js-compatibility.md` (Issue H documentation)

### Ordering

1. Issues G, I (trivial changes, no dependencies)
2. Issue E (remove stub flag, simplify validation struct)
3. Issues C, D (fix validation flow in request handler)
4. Issues A, F (fix latest path and pending check in review handler)
5. Issue B (wire up confirmation -- most impactful behavioral change)
6. Issue H (us-east-1 default + docs)
7. `make check-fast`, `make test`

---

## Verification

- `make check-fast` after each file change
- `make test` at the end -- all tests must pass, zero warnings
- Manual review: confirm `approve_template` signature now takes `latest_key`
- Manual review: confirm `ConfirmationRequest` with `response_tx: None` is gone

---

## Progress

- [x] Issue G: Change diff context default to 500
- [x] Issue I: Remove unused template bodies from TemplateDiff
- [x] Issue E: Remove --lint-using-parameters stub
- [x] Issue C: Halt on validation errors
- [x] Issue D: Fix large template validation
- [x] Issue A: Fix latest key path
- [x] Issue F: Verify pending template exists before download
- [x] Issue B: Wire up user confirmation
- [x] Issue H: Hard-code us-east-1 for review + document
- [x] Final: make check-fast + make test pass (625/625 pass)
