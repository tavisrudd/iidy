# Documentation Verification Fixes

Date: 2026-02-18

## Findings from 4 verification agents

### docs/output-architecture.md -- 8 errors

1. **OutputData variant names wrong**: `PreviousStackEvents` doesn't exist (actual: `StackEvents`), `LiveStackEventsComplete` doesn't exist (signaled by `OperationComplete`), `ChangesetDetails` doesn't exist (actual: `ChangeSetResult`)
2. **Section arrays wrong**: CreateStack missing `command_metadata` prefix. DeleteStack uses wrong key `previous_stack_events` (actual: `stack_events`), missing `command_metadata`, missing `confirmation`, wrong order
3. **`get_expected_sections()` doesn't exist**: Sections assigned in `start_operation()` match expression
4. **`advance_through_ready_sections` is async**: Doc shows `fn`, actual is `async fn ... -> Result<()>`
5. **`show_timestamps: true` not plain-mode-exclusive**: Set for both Plain and Interactive modes
6. **25+ variants**: Exactly 25, not more

### docs/yaml-preprocessing.md -- 7 errors

1. **`!$split` format WRONG**: Doc says object format `{string, delimiter}` -- ACTUAL is array `[delimiter, string]` matching JS already
2. **`!$let` format WRONG**: Doc says nested `{bindings, expression}` -- ACTUAL parser uses flat `{var1: val1, in: expr}` matching JS already
3. **Handlebars helpers `startsWith`/`endsWith` not registered**
4. **Comparison helpers `eq`/`ne`/`gt`/`lt`/`and`/`or`/`not` are built-in block helpers, not string interpolation helpers**
5. **`base64Encode`/`base64Decode` wrong names**: Actual registered name is `base64`, no `base64Decode`
6. **`$envValues` missing `iidy.command` and `iidy.profile`**
7. **CFN tags missing `!Length`, `!ToJsonString`, `!Transform`, `!ForEach`**

### docs/js-compatibility.md -- 2 errors

1. **`!$split` claimed as "Pending fix"**: Already fixed, already uses array format
2. **`!$let` claimed as "Pending fix"**: Already fixed, parser uses flat JS format

### docs/adr/002-data-driven-output.md -- 1 error

1. **`ColorContext` called dead code**: Actually used -- `ColorContext::init_global()` called from `main.rs`

### docs/aws-config.md -- no errors found
### docs/adr/001-output-sequencing.md -- no errors found
### docs/adr/003-template-approval.md -- no errors found
### notes/index.md -- no errors found
### docs/architecture.md -- agent ran out of quota before completing (partial check passed)

## Action items for next session

1. **Fix docs/output-architecture.md**: Replace wrong variant names in table, fix section arrays to match `start_operation()` in `interactive.rs`, fix function name and async signature, change "25+" to "25"
2. **Fix docs/yaml-preprocessing.md**: Fix `!$split` to show array format `[delimiter, string]`, fix `!$let` to show flat format with `in` key, fix Handlebars helper names (`base64` not `base64Encode`), remove `startsWith`/`endsWith`/comparison helpers, add missing CFN tags, expand `$envValues` description
3. **Fix docs/js-compatibility.md**: Remove `!$split` and `!$let` from "Remaining differences" -- move to "Resolved differences" section
4. **Fix docs/adr/002-data-driven-output.md**: Remove claim that `ColorContext` is dead code
5. **Verify docs/architecture.md**: The verification agent ran out of quota. Run a full check.
6. **Also update notes/codebase-guide.md**: The "Behavioral Differences" section lists `!$split` and `!$let` as incompatible -- they are now compatible. Update accordingly.

## Context from this session

The documentation cleanup task from `notes/2026-02-17-handoff-notes-and-docs-cleanup.md` is complete except for these verification fixes. All 10 new docs were created, 46 notes archived, existing docs audited. The errors above are from writing docs based on stale source material (the IIDY_JS_COMPATIBILITY_FIXES.md was itself stale -- the fixes it lists as TODO were completed).
