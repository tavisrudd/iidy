# Handoff: Code Review Fix-Up Round 2

**Date**: 2026-02-18
**Status**: COMPLETE
**Source**: `notes/2026-02-17-code-review-findings.md`
**Session plan**: `notes/2026-02-18-code-review-fixes-round2.md`

## Summary

Fixed 5 categories of issues from the code review. All changes verified:
560 tests passing, 0 warnings.

---

## 1. AWS conversion hardcoded fields

- Extracted `convert_aws_datetime` helper in `aws_conversion.rs`, deduplicating 3 identical
  timestamp conversion patterns (`convert_stack_to_list_entry`, `convert_stack_to_definition`,
  `convert_aws_stack_event`).
- Fixed `disable_rollback: false` to `stack.disable_rollback().unwrap_or(false)` -- the AWS SDK
  exposes this field on `Stack` but it was hardcoded.
- `stackset_name: None` and `stack_policy: None` remain unchanged -- these genuinely require
  separate API calls (ListStackInstances, GetStackPolicy). Improved TODO comments to explain why.
- Changeset status `"CREATE_COMPLETE"` in `build_changeset_result` is correct by construction
  (called only after `wait_for_changeset_completion`). Added invariant comment.

## 2. Blocking I/O on async runtime

- `stack_args.rs`: Changed `fs::read_to_string` to `tokio::fs::read_to_string` (async fn,
  straightforward fix).
- `resolver.rs` (3 sites): Blocking `std::fs::read_to_string` calls are in sync methods on
  error-only paths, reading local files already loaded by the resolver. Converting the resolver
  to async is a large refactor out of scope. Added explanatory comments.

## 3. `unsafe set_var` in aws/mod.rs

- Moved `set_var("AWS_SDK_LOAD_CONFIG", "1")` from `config_from_merged_settings` (async,
  Tokio workers alive) to `handle_command` in `main.rs` before `Runtime::new()`. The safety
  invariant (single thread) is now actually correct.

## 4. Output manager buffer overflow

- Added `events_dropped` counter to `DynamicOutputManager`. Emits `log::warn!` on first
  eviction so buffer overflow is observable rather than silent.
- Fixed pre-existing dead_code warnings on `current_mode` and `options` fields (renamed to
  `_current_mode`/`_options` -- stored for future mode-switching, not yet read).

## 5. CFN handler inconsistencies -- REVERTED

Initially refactored both handlers to use the output manager / macro system, but reverted
after risk review. These are data extraction commands (raw YAML/JSON to stdout for piping)
with intentionally different output style from interactive CFN operations. Forcing them through
the macro would change error formatting and add unnecessary coupling. Left as-is; the
"inconsistency" is by design. Cleaned up comments only.

## Files changed

- `src/output/aws_conversion.rs` -- timestamp helper, disable_rollback fix, improved TODOs
- `src/cfn/changeset_operations.rs` -- invariant comment on status
- `src/cfn/stack_args.rs` -- tokio::fs::read_to_string
- `src/yaml/resolution/resolver.rs` -- blocking I/O comments (3 sites)
- `src/aws/mod.rs` -- removed unsafe set_var
- `src/main.rs` -- added set_var before Runtime::new()
- `src/output/manager.rs` -- buffer overflow warning, dead_code field renames
- `src/cfn/get_stack_template.rs` -- comment only (intentionally not using macro)
- `src/cfn/get_import.rs` -- comment only (intentionally not using output manager)
