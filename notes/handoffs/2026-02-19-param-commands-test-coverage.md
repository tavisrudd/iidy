# Param Commands Test Coverage -- Handoff

**Date**: 2026-02-19
**Source**: Finding E from `notes/handoffs/2026-02-19-remaining-findings.md`

## Context

All five param commands (`set`, `get`, `get_by_path`, `get_history`, `review`)
in `src/params/` have no integration tests. 18 unit tests exist for utility
functions only (`match_kms_alias`, `ParamOutput::from_parameter`, `format_output`).
The JS version also has no tests.

## Current Architecture

Each handler creates its own SSM/KMS client inline:
```
create_ssm_client(&opts) -> (SsmClient, SdkConfig)
create_kms_client(&config) -> KmsClient
```

This makes offline testing impossible without a trait abstraction layer.

## Commands

| Command | File | What it does |
|---------|------|-------------|
| `param set` | `set.rs` | PutParameter with optional approval workflow + KMS encryption |
| `param get` | `get.rs` | GetParameter + tags, 3 output formats |
| `param get-by-path` | `get_by_path.rs` | Paginated GetParametersByPath + per-param tags |
| `param get-history` | `get_history.rs` | Paginated GetParameterHistory, current/previous split |
| `param review` | `review.rs` | Approval workflow: fetch pending, confirm, promote |

## Implementation Plan

### Chunk 1: Create trait abstraction

**Files**: `src/params/mod.rs` (or new `src/params/traits.rs`)

Define traits:
- `SsmOperations` -- get_parameter, put_parameter, get_parameters_by_path,
  get_parameter_history, delete_parameter, list_tags_for_resource,
  add_tags_to_resource
- `KmsOperations` -- list_aliases

Implement for real AWS clients (thin wrapper).

### Chunk 2: Refactor handlers to accept trait objects

**Files**: `set.rs`, `get.rs`, `get_by_path.rs`, `get_history.rs`, `review.rs`

Change each handler to accept `&impl SsmOperations` instead of calling
`create_ssm_client()` directly. Main dispatch (`src/main.rs`) creates the
real client and passes it in.

### Chunk 3: Create test mock and write tests

**Files**: New test module (inline `#[cfg(test)]` or `tests/params/`)

Implement mock struct for `SsmOperations` with configurable responses.
Use aws-smithy-types builders to construct mock AWS response objects.

Test coverage targets:
- `set`: PutParameter args, approval workflow (.pending), KMS lookup, message tag
- `get`: GetParameter + tags, ParameterNotFound error, output formats
- `get_by_path`: Pagination (NextToken), per-param tags, empty results
- `get_history`: Pagination, sort by LastModifiedDate, current/previous split
- `review`: Pending fetch, approval promote, rejection exit code 130

### Chunk 4: Fix N+1 query in get_by_path

`get_by_path.rs` fetches tags for EVERY parameter individually. Consider
batching or making tag fetch optional (only needed for json/yaml formats,
not "simple" format).

## Already Tested (no changes needed)

- `match_kms_alias()` -- 5 tests
- `ParamOutput::from_parameter()` -- 6 tests
- `ParamHistoryOutput::from_history()` -- 2 tests
- `format_output()` -- 4 tests
- JSON serialization edge case -- 1 test

## References

- `src/params/mod.rs` -- module root, helpers, existing tests
- `src/params/set.rs`, `get.rs`, `get_by_path.rs`, `get_history.rs`, `review.rs`
- `src/aws/mod.rs` -- `config_from_normalized_opts()`
- `src/cli.rs:333-588` -- ParamCommands enum
- `src/main.rs:136-178` -- dispatch
