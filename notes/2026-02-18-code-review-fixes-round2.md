# Code Review Fixes Round 2

**Date**: 2026-02-18
**Source**: `notes/2026-02-17-code-review-findings.md`
**Handoff**: `notes/handoffs/2026-02-18-code-review-fixes-round2.md`

---

## 1. AWS Conversion Hardcoded Fields

### Changes:
- **`disable_rollback`**: Change from `false` to `stack.disable_rollback().unwrap_or(false)`.
  The AWS SDK Stack type exposes this method (confirmed in the original design spec).
  The current hardcoded `false` is a bug.

- **Timestamp dedup**: Extract `convert_aws_datetime` helper. Currently duplicated in
  `convert_stack_to_list_entry`, `convert_stack_to_definition`, and `convert_aws_stack_event`.

- **Changeset status**: The hardcoded `"CREATE_COMPLETE"` in `build_changeset_result` is
  actually correct -- it's only called after `wait_for_changeset_completion` which guarantees
  that status. Add an explanatory comment. (Compare with `build_existing_changeset_result`
  which correctly reads from the describe response.)

- **`stackset_name: None`** and **`stack_policy: None`**: These require additional API calls
  (ListStackInstances and GetStackPolicy). Not fixing -- just improving the TODO comments.

### Files:
- `src/output/aws_conversion.rs`
- `src/cfn/changeset_operations.rs`

---

## 2. Blocking I/O on Async Runtime

### Changes:
- **`stack_args.rs:105`**: Change `fs::read_to_string` to `tokio::fs::read_to_string`.
  This is an async fn, so the fix is trivial.

- **`resolver.rs:940,982`**: These are in synchronous resolver code. Converting the resolver
  to async is a massive refactor out of scope. The blocking reads are in error paths only
  (reading source files to find line numbers for error messages). Low risk. Add `// Note:`
  comment documenting the tradeoff.

- **`aws/mod.rs:114`**: `Path::exists()` on `~/.aws` is a fast local metadata check.
  This call will move to main.rs as part of fix #3, so this becomes moot.

### Files:
- `src/cfn/stack_args.rs`
- `src/yaml/resolution/resolver.rs` (comment only)

---

## 3. `unsafe set_var` in aws/mod.rs

### Analysis:
The safety comment "before any threads are spawned" is wrong. This runs inside
`rt.block_on()` where Tokio worker threads are alive. Per Rust 2024, `set_var` is unsafe
because it mutates global state that other threads may read concurrently.

### Fix:
Move the `~/.aws` check and `set_var` call to `main.rs` in `handle_command()`, BEFORE
`Runtime::new()`. At that point, only the main thread exists, making the safety
assertion correct. Remove the now-unnecessary code from `config_from_merged_settings`.

### Files:
- `src/main.rs`
- `src/aws/mod.rs`

---

## 4. Output Manager Buffer Overflow

### Changes:
Add `log::warn!` on first eviction with count tracking. Users/developers get visibility
into data loss without changing buffer semantics.

### Files:
- `src/output/manager.rs`

---

## 5. CFN Handler Inconsistencies

### Analysis:
- **`get_stack_template.rs`**: Already has output manager setup. Uses `handle_aws_error`
  helper which is functionally equivalent to the macro's error path. The main difference
  is the fn signature doesn't match the macro's expected `(output_manager, context, cli,
  args, opts)` pattern. Refactoring to use the macro is straightforward.

- **`get_import.rs`**: Bypasses output manager entirely, uses direct `eprintln!`. However,
  this command doesn't need a CfnContext (no CloudFormation API calls). It only needs
  AWS config for the import loader. The `run_command_handler!` macro forces CfnContext
  creation which is wasteful. The right fix: add output manager but keep the custom
  AWS setup (it genuinely differs from the standard CFN pattern).

### Changes:
- Refactor `get_stack_template.rs` to use `run_command_handler!` macro
- Add output manager to `get_import.rs`, route errors through it, but keep custom
  AWS config setup since it doesn't need CfnContext

### Files:
- `src/cfn/get_stack_template.rs`
- `src/cfn/get_import.rs`
- `src/output/data.rs` (may need ImportResult variant)

---

## Progress Log

- [x] Section 1: AWS conversion hardcoded fields
  - Extracted `convert_aws_datetime` helper (3 call sites deduplicated)
  - Fixed `disable_rollback` to use `stack.disable_rollback().unwrap_or(false)`
  - Improved TODO comments for `stackset_name` and `stack_policy` (require extra API calls)
  - Added invariant comment on changeset status (correct after wait_for_changeset_completion)
- [x] Section 2: Blocking I/O
  - Changed `fs::read_to_string` to `tokio::fs::read_to_string` in stack_args.rs
  - Added notes on resolver.rs blocking reads (sync method, error-only paths, acceptable)
- [x] Section 3: unsafe set_var
  - Moved `set_var("AWS_SDK_LOAD_CONFIG")` to main.rs before `Runtime::new()`
  - Removed from async `config_from_merged_settings` where Tokio workers were alive
- [x] Section 4: Buffer overflow
  - Added `events_dropped` counter and `log::warn!` on first eviction
  - Fixed pre-existing dead_code warnings on `current_mode`/`options` fields
- [x] Section 5: Handler inconsistencies -- REVERTED
  - Initially refactored both to use macro/output manager
  - Reverted after risk review: these are data extraction commands with intentionally
    different output style (raw stdout for piping). Forcing them through the macro
    would change error formatting with no benefit.
  - Added doc comments explaining the intentional design difference

All checks passing: 560 tests, 0 warnings.
