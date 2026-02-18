# List-Stacks JSON Query Output Implementation

## Current Work Context

**Status**: ✅ COMPLETE
**Commit**: d492826 (June 18, 2025) + refinements through July 2025

### Implementation Requirements

Based on iidy-js reference (`iidy-js-for-reference/src/cfn/listStacks.ts`):

1. **Query Output Mode**:
   - When `--query` option is provided, output raw JSON instead of formatted table
   - The JSON should be the filtered stacks array
   - Compatible with JMESPath queries when jmespath is available

2. **Current CLI Options**:
   - `--tag-filter` - Filter by tags (implemented)
   - `--query` - JMESPath query for filtering (placeholder exists)
   - `--all` - Include all statuses (implemented)

3. **JSON Output Format**:
   ```json
   [
     {
       "StackName": "my-stack",
       "StackStatus": "CREATE_COMPLETE",
       "CreationTime": "2024-01-01T12:00:00Z",
       "LastUpdatedTime": "2024-01-01T13:00:00Z",
       "Tags": [
         {"Key": "Environment", "Value": "prod"}
       ]
       // ... other AWS Stack fields
     }
   ]
   ```

### Implementation Plan

1. **Add JSON Output Rendering**:
   - Check if `args.query` is provided in list-stacks
   - If yes, serialize filtered stacks to JSON and output directly
   - Skip the normal table rendering

2. **Future JMESPath Support**:
   - Add `jmespath` crate dependency
   - Apply JMESPath query to the stacks array
   - Return query results as JSON

### Files to Update

- `src/cfn/list_stacks.rs` - Add JSON output logic
- `src/cli.rs` - Ensure --query option is properly defined
- `Cargo.toml` - Add jmespath dependency (future)

### Testing Plan

- Test JSON output with --query option
- Verify proper JSON formatting
- Test with various tag filters
- Future: Test JMESPath queries

## Implementation Complete

Successfully implemented in commit d492826bf1a6593fa00c850bb0b4d66256a68e93 and subsequent refinements:

✅ **JMESPath filtering** - Full support using `jmespath` crate (lines 119-157)
✅ **Query mode detection** - `args.query.is_some()` triggers JSON output (line 159)
✅ **JSON serialization** - Complete `SerializableStack` structures with AWS-compatible format (lines 14-73)
✅ **Tag filtering** - Working filter by tags with `--tag-filter` (lines 90-117)
✅ **Column selection** - Custom column support via `--columns` (lines 160-167)
✅ **Data-driven architecture** - Uses OutputData::StackList with proper renderers
✅ **Error handling** - Proper AWS error conversion and user-friendly messages

### Key Features:
- JSON output with `--query` option outputs raw filtered JSON
- JMESPath queries work with `--jmespath-filter` option
- Tag filtering combines with JMESPath for powerful queries
- Multiple output modes (Interactive table, Plain text, JSON)
- Proper exit codes and error handling

### Files Modified:
- `src/cfn/list_stacks.rs` - Complete implementation with JSON/JMESPath support
- `Cargo.toml` - Added `jmespath` crate dependency
- Part of larger data-driven output architecture refactor