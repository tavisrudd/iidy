# CloudFormation Command Handler Exit Code Consistency Analysis

**Date**: 2025-07-06  
**Context**: Fix for list-stacks error handling inconsistency (commit 24783cd)  
**Status**: In Progress - 3 commands fixed (commit f202255), 2 remaining  

## Background

During investigation of inconsistent error handling in `list-stacks` command (which showed raw AWS errors instead of user-friendly messages), we discovered that not all CloudFormation command handlers follow the same exit code and error reporting pattern.

This analysis reviews all 13 CFN command handlers for consistency in:
1. **Return type**: Should be `Result<i32>` (not `Result<()>`)
2. **Error handling**: Should use `convert_aws_error_to_error_info()` and render through `output_manager.render(OutputData::Error(...))`
3. **Exit codes**: Should return 0 for success, 1 for failure, and 130 for user interruption

## Analysis Results

### ✅ Compliant Commands (11/13)

These commands follow the proper pattern perfectly:

- **`create_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1)
- **`update_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1/130)
- **`create_or_update.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1/130)
- **`create_changeset.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1)
- **`exec_changeset.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1)
- **`delete_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1/130)
- **`describe_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1)
- **`list_stacks.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1) *[Fixed in 24783cd]*
- **`watch_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0) *[Fixed in f202255]*
- **`describe_stack_drift.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0) *[Fixed in f202255]*
- **`get_stack_instances.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0) *[Fixed in f202255]*

### ❌ Non-Compliant Commands (2/13)

#### 1. `estimate_cost.rs` - Missing Error Handling Pattern
- ✅ **Return type**: `Result<i32>`
- ❌ **Error handling**: AWS API calls don't use `convert_aws_error_to_error_info()` pattern
- ✅ **Exit codes**: Returns 0 (though only success case is handled)
- **Fix needed**: Wrap AWS API calls with proper error handling

#### 2. `get_stack_template.rs` - Special Case Architectural Issue
- ❌ **Return type**: `Result<FormattedTemplate>` (should be `Result<i32>`)
- ❌ **Error handling**: No use of `convert_aws_error_to_error_info()` pattern
- ❌ **Exit codes**: No exit codes returned
- **Fix needed**: This appears to be a special case that returns formatted template content rather than following the standard command pattern. May need architectural discussion.

## Implementation Priority

### ✅ Completed (commit f202255)
1. **`watch_stack.rs`** - ✅ Fixed return type and exit codes
2. **`describe_stack_drift.rs`** - ✅ Fixed return type and exit codes  
3. **`get_stack_instances.rs`** - ✅ Fixed return type and exit codes

### Medium Priority (Requires Error Handling Work)
4. **`estimate_cost.rs`** - Needs AWS error handling pattern implementation

### Low Priority (Architectural Discussion Needed)
5. **`get_stack_template.rs`** - May need different approach due to data return requirements

## Success Metrics

- **Initial**: 8/13 commands (61.5%) follow proper pattern
- **Current**: 11/13 commands (84.6%) follow proper pattern
- **Target**: 13/13 commands (100%) follow proper pattern
- **User Impact**: Consistent error messages and exit codes across all CFN commands

## Related Work

- **Fixed in commit 24783cd**: `list_stacks.rs` error handling enhancement
- **Fixed in commit f202255**: Return type standardization for `watch_stack.rs`, `describe_stack_drift.rs`, and `get_stack_instances.rs`
- **See**: `src/output/aws_conversion.rs` for `convert_aws_error_to_error_info()` implementation
- **Pattern**: Based on `delete_stack.rs` and `create_stack.rs` implementations

## Next Steps

1. ✅ ~~Fix the 3 simple cases (return type + exit codes)~~ *[Completed in f202255]*
2. Implement proper error handling in `estimate_cost.rs` 
3. Discuss architectural approach for `get_stack_template.rs`
4. Test all commands for consistent error behavior
5. Update CLI integration tests to verify exit codes