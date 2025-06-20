# List-Stacks JSON Query Output Implementation

## Current Work Context

**Status**: IN PROGRESS - Implementing JSON query output support for list-stacks

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