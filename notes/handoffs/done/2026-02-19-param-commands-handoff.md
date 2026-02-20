# Param Commands Implementation Handoff

**Date:** 2026-02-19

## Status

**IMPLEMENTED** (2026-02-19). All 5 param subcommands have working handlers.
607 tests passing, zero warnings.

### Implementation Summary

Created `src/params/` module with 6 files:
- `mod.rs` -- SSM/KMS client creation, KMS alias lookup, tag helpers,
  serializable ParamOutput/ParamHistoryOutput structs, format_output helper
- `set.rs` -- put_parameter with approval flow, KMS alias for SecureString, message tags
- `review.rs` -- .pending comparison, confirmation via DynamicOutputManager, promote/decline
- `get.rs` -- get_parameter with simple/json/yaml output
- `get_by_path.rs` -- paginated get_parameters_by_path, sorted output, tag merging
- `get_history.rs` -- paginated history, current/previous split, tags on current only

### Design Decisions

1. **Separate from OutputData pipeline**: Param commands have their own `--format`
   flag (simple/json/yaml) and print directly to stdout. They don't use the
   data-driven output architecture (OutputData enum / renderers) because they
   have no spinners, live updates, or complex sections.

2. **Confirmation via output manager**: The `review` command uses
   `DynamicOutputManager::request_confirmation()` for the interactive yes/no
   prompt, integrating with the existing confirmation system.

3. **Added aws-sdk-kms dependency**: Required for hierarchical KMS alias lookup
   for SecureString parameters.

4. **BTreeMap for deterministic output**: Tags and sorted parameter maps use
   BTreeMap instead of HashMap for consistent ordering.

5. **PascalCase serialization**: Output structs use `#[serde(rename_all = "PascalCase")]`
   to match the AWS SDK field naming convention used by iidy-js.

### Findings

- AWS SDK v1 methods like `resp.parameters()`, `resp.aliases()`, `resp.tag_list()`
  return `&[T]` slices (not `Option`), unlike v0.x which used `Option<&[T]>`.
- `Tag::builder().key(k).value(v).build()` returns `Result<Tag, BuildError>`,
  must be unwrapped.
- `tag.key()` and `tag.value()` return `&str` (not `Option<&str>`) in SDK v1.

## What Exists

- **CLI arg structs**: `ParamSetArgs`, `ParamGetArgs`, `ParamGetByPathArgs`,
  `ParamPathArg` -- all fully defined with correct defaults
- **`aws-sdk-ssm`**: Already in Cargo.toml
- **AWS config infra**: `aws::config_from_normalized_opts()` works; SSM
  client can be created on-demand from `SdkConfig`
- **Output system**: Data-driven `OutputData` enum + `DynamicOutputManager`
  ready for new variants
- **Error handling**: `run_command_handler!` macro, AWS error conversion

## Commands to Implement

### 1. `param set <path> <value>`

- `ssm.put_parameter()` with Name, Value, Type, Overwrite, KeyId
- If `--with-approval`: append `.pending` to the path, print review instructions
- If `--message`: tag the parameter with `iidy:message` via `ssm.add_tags_to_resource()`
- For SecureString type: resolve KMS alias via hierarchical path lookup
  (`alias/ssm/<path>/<parts>`, popping segments until match or none)

### 2. `param review <path>`

- Fetch `{path}.pending` via `ssm.get_parameter(WithDecryption=true)`
- If no pending param: print message, return exit code 1
- Fetch current param (may not exist) and pending tags
- Display current vs pending comparison with optional message
- Interactive confirmation prompt
- On confirm: `put_parameter` to real path, `delete_parameter` on `.pending`,
  copy tags
- On decline: return exit code 130

### 3. `param get <path>`

- `ssm.get_parameter(WithDecryption=decrypt)`
- Output formats:
  - `simple`: value only
  - `json`: full parameter object with tags
  - `yaml`: same as json, YAML-formatted
- Tags fetched via `ssm.list_tags_for_resource()` for json/yaml formats

### 4. `param get-by-path <path>`

- `ssm.get_parameters_by_path()` with pagination (NextToken)
- Supports `--recursive` flag
- Output formats:
  - `simple`: sorted map of path -> value
  - `json`/`yaml`: full objects indexed by path, sorted, with tags
- Tags fetched in parallel for each parameter (json/yaml only)
- If no results: print message, return exit code 1

### 5. `param get-history <path>`

- `ssm.get_parameter_history()` with pagination
- Sort by LastModifiedDate ascending
- Split into Current (latest) and Previous (all others)
- Tags fetched only for current version
- Output formats:
  - `simple`: Current/Previous sections with Value, LastModifiedDate,
    LastModifiedUser, Message (from `iidy:message` tag)
  - `json`/`yaml`: full objects with all metadata and tags

## Implementation Plan

### Module structure

Create `src/params/mod.rs` with submodules following `src/cfn/` patterns:

```
src/params/
  mod.rs          -- SSM client helpers, KMS alias lookup, tag helpers
  set.rs          -- param set handler
  review.rs       -- param review handler (interactive prompt)
  get.rs          -- param get handler
  get_by_path.rs  -- param get-by-path handler
  get_history.rs  -- param get-history handler
```

### Output data types

Add to `src/output/data.rs`:

- `ParamValue` variant -- single parameter (get)
- `ParamValues` variant -- multiple parameters (get-by-path)
- `ParamHistory` variant -- history with current/previous split
- `ParamSetResult` variant -- set confirmation message
- `ParamReview` variant -- review comparison display

Each needs a backing struct with the relevant fields.

### Shared helpers in `src/params/mod.rs`

- `get_kms_alias_for_parameter(kms_client, path)` -- paginated KMS alias
  list, hierarchical path match (`alias/ssm/...`)
- `maybe_fetch_param(ssm_client, name)` -- wraps get_parameter, returns
  `Ok(None)` on ParameterNotFound
- `get_param_tags(ssm_client, name)` -- list_tags_for_resource wrapper
- `set_param_tags(ssm_client, name, tags)` -- add_tags_to_resource wrapper

### Main dispatch

Replace the `println!` stub in `src/main.rs` with match on `ParamCommands`
variants routing to async handlers, same pattern as CFN commands.

### Testing

- All handlers should be testable offline using fixture data
- Follow the pattern in `src/cfn/` where AWS API calls are separated from
  output formatting
- Snapshot tests for each output format (simple, json, yaml)

## JS Reference

- Implementation: `../iidy-js/src/params/index.ts`
- CLI defs: `../iidy-js/src/params/cli.ts`
- No tests exist in JS for param commands

## Notes

- Param commands are simpler than CFN -- no interactive watching, changesets,
  or complex state machines
- The `review` command is the only one needing interactive user input
  (confirmation prompt)
- KMS alias lookup is the most complex helper -- paginated list of all
  aliases, hierarchical path matching
- The `.pending` suffix convention for approval workflow is simple but
  important to get right (tag copying, cleanup)
