# Get-Import Migration Analysis

**Date**: 2025-07-07  
**Context**: Phase 1 YAML Preprocessing - Command Handler Migration  

## Decision: Do NOT Migrate get_import to run_command_handler! Pattern

### Analysis Summary

The `get_import` command should **NOT** be migrated to the `run_command_handler!` macro pattern used for CloudFormation operations. This command is fundamentally different from CFN operations and forcing it into the CFN pattern would create unnecessary complexity.

### Key Differences from CFN Commands

1. **Not a CloudFormation Operation**
   - `get_import` is a utility command for data retrieval from various sources (files, S3, HTTP, AWS services)
   - Does not interact with CloudFormation APIs
   - Missing from `CfnOperation` enum mapping

2. **Optional AWS Context**
   - Treats AWS configuration as optional (`Option<aws_config::SdkConfig>`)
   - Can work without AWS credentials for file/HTTP imports
   - CFN macro assumes AWS context is always required

3. **Specialized Data Processing**
   - Complex JMESPath query processing
   - Format conversion between YAML/JSON
   - Custom utility functions for type conversion
   - Direct output to stdout with manual format selection

4. **Different Error Handling**
   - Custom error formatting for AWS vs non-AWS errors
   - Direct `eprintln!` calls instead of output system integration
   - Specialized handling for different import source types

### Migration Complications

1. **Architectural Mismatch**: CFN-focused macro doesn't fit utility commands
2. **Missing Infrastructure**: Would need new `CfnOperation` variant and `OutputData` types
3. **Output System Conflict**: Direct format output conflicts with data-driven output architecture
4. **Code Duplication**: Would require duplicating complex data processing logic

### Recommendation

**Keep `get_import.rs` as-is** because:
- Current implementation is focused and works well for its specific use case
- Specialized data processing doesn't align with the output system's approach
- Would require significant architectural changes without clear benefits
- The command serves a different purpose than CloudFormation operations

### Migration Pattern Application

The `run_command_handler!` macro pattern should only be applied to actual CloudFormation operations that:
- Interact with AWS CloudFormation APIs
- Use the standard CfnContext setup
- Benefit from the data-driven output architecture
- Follow the consistent error handling pattern

Utility commands like `get_import` should maintain their specialized implementations.