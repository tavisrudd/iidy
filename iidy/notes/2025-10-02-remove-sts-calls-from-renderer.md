# 2025-10-02: Remove STS Calls from Interactive Renderer

## Status: IN PROGRESS

## Problem

The interactive renderer makes STS `GetCallerIdentity` API calls from within rendering code (violates data-driven architecture, prevents offline testing, creates wrong AWS config).

**Location**: `src/output/renderers/interactive.rs:1900-1943`

## Plan

- [x] Step 1: Expand `StackAbsentInfo` struct with environment, region, account, auth_arn fields
- [x] Step 2: Find and update all `StackAbsentInfo` construction sites (delete_stack.rs)
- [x] Step 3: Add `get_caller_identity()` helper in aws_conversion.rs
- [x] Step 4: Update `render_stack_absent_info()` to use struct data instead of STS calls
- [ ] Step 5: Change `ErrorInfo` from struct to have `ErrorDetails` enum field
- [ ] Step 6: Detect stack-absent errors in `convert_aws_error_to_error_info()` and populate context
- [ ] Step 7: Update renderer to use `ErrorDetails::StackAbsent` context, remove fallback STS calls
- [ ] Step 8: Update all ErrorInfo construction sites for new structure
- [ ] Step 9: Update test fixtures
- [ ] Step 10: Run tests and verify

## Progress Log

### Step 1-4: Completed
- Expanded StackAbsentInfo struct
- Updated delete_stack.rs to populate full context
- Fixed bug in get_current_iam_principal (was creating new config)
- Updated render_stack_absent_info to use struct data

### Step 5-8: Completed
- Changed ErrorInfo to use ErrorDetails enum
- ErrorDetails::Generic(Option<String>) vs ErrorDetails::StackAbsent(StackAbsentInfo)
- Updated convert_aws_error_to_error_info to be async and optionally populate stack-absent context
- Updated all call sites: macros pass Some((context, cli)), others pass None
- Updated interactive renderer to match on ErrorDetails and render appropriately
- Fixed delete_stack.rs typo (_cli -> cli)
- Code compiles successfully

### Step 9-10: Completed
- Updated all test fixtures to use ErrorDetails enum
- Fixed 6 test files with ErrorInfo construction
- All 591 tests passing

## Summary

Successfully removed all STS calls from the interactive renderer:
- ✅ No more network calls during rendering
- ✅ All AWS context (account, auth_arn) provided via data structures
- ✅ Stack-absent errors automatically enriched with context in error conversion
- ✅ Clean separation: data fetching happens before rendering
- ✅ Offline testing now possible
- ✅ All tests passing (591/591)

