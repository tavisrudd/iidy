# CloudFormation Command Handler Exit Code Consistency Analysis

**Date**: 2025-07-06  
**Context**: Fix for list-stacks error handling inconsistency (commit 24783cd)  
**Status**: Analysis Complete - 5 commands need updates  

## Background

During investigation of inconsistent error handling in `list-stacks` command (which showed raw AWS errors instead of user-friendly messages), we discovered that not all CloudFormation command handlers follow the same exit code and error reporting pattern.

This analysis reviews all 13 CFN command handlers for consistency in:
1. **Return type**: Should be `Result<i32>` (not `Result<()>`)
2. **Error handling**: Should use `convert_aws_error_to_error_info()` and render through `output_manager.render(OutputData::Error(...))`
3. **Exit codes**: Should return 0 for success, 1 for failure, and 130 for user interruption

## Analysis Results

### ✅ Compliant Commands (8/13)

These commands already follow the proper pattern perfectly:

- **`create_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1)
- **`update_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1/130)
- **`create_or_update.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1/130)
- **`create_changeset.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1)
- **`exec_changeset.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1)
- **`delete_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1/130)
- **`describe_stack.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1)
- **`list_stacks.rs`**: ✅ `Result<i32>`, ✅ proper error handling, ✅ exit codes (0/1) *[Fixed in 24783cd]*

### ❌ Non-Compliant Commands (5/13)

#### 1. `estimate_cost.rs` - Missing Error Handling Pattern
- ✅ **Return type**: `Result<i32>`
- ❌ **Error handling**: AWS API calls don't use `convert_aws_error_to_error_info()` pattern
- ✅ **Exit codes**: Returns 0 (though only success case is handled)
- **Fix needed**: Wrap AWS API calls with proper error handling

#### 2. `watch_stack.rs` - Wrong Return Type
- ❌ **Return type**: `Result<()>` (should be `Result<i32>`)
- ✅ **Error handling**: Uses proper error handling pattern
- ❌ **Exit codes**: No exit codes returned
- **Fix needed**: Change return type to `Result<i32>` and return proper exit codes

#### 3. `describe_stack_drift.rs` - Wrong Return Type
- ❌ **Return type**: `Result<()>` (should be `Result<i32>`)
- ✅ **Error handling**: Uses proper error handling pattern
- ❌ **Exit codes**: No exit codes returned
- **Fix needed**: Change return type to `Result<i32>` and return proper exit codes

#### 4. `get_stack_template.rs` - Special Case Architectural Issue
- ❌ **Return type**: `Result<FormattedTemplate>` (should be `Result<i32>`)
- ❌ **Error handling**: No use of `convert_aws_error_to_error_info()` pattern
- ❌ **Exit codes**: No exit codes returned
- **Fix needed**: This appears to be a special case that returns formatted template content rather than following the standard command pattern. May need architectural discussion.

#### 5. `get_stack_instances.rs` - Wrong Return Type
- ❌ **Return type**: `Result<()>` (should be `Result<i32>`)
- ✅ **Error handling**: Uses proper error handling pattern
- ❌ **Exit codes**: No exit codes returned
- **Fix needed**: Change return type to `Result<i32>` and return proper exit codes

## Implementation Priority

### High Priority (Simple Fixes)
1. **`watch_stack.rs`** - Just needs return type change and exit codes
2. **`describe_stack_drift.rs`** - Just needs return type change and exit codes
3. **`get_stack_instances.rs`** - Just needs return type change and exit codes

### Medium Priority (Requires Error Handling Work)
4. **`estimate_cost.rs`** - Needs AWS error handling pattern implementation

### Low Priority (Architectural Discussion Needed)
5. **`get_stack_template.rs`** - May need different approach due to data return requirements

## Success Metrics

- **Current**: 8/13 commands (61.5%) follow proper pattern
- **Target**: 13/13 commands (100%) follow proper pattern
- **User Impact**: Consistent error messages and exit codes across all CFN commands

## Related Work

- **Fixed in commit 24783cd**: `list_stacks.rs` error handling enhancement
- **See**: `src/output/aws_conversion.rs` for `convert_aws_error_to_error_info()` implementation
- **Pattern**: Based on `delete_stack.rs` and `create_stack.rs` implementations

## Next Steps

1. Fix the 4 simple cases (return type + exit codes)
2. Implement proper error handling in `estimate_cost.rs`
3. Discuss architectural approach for `get_stack_template.rs`
4. Test all commands for consistent error behavior
5. Update CLI integration tests to verify exit codes