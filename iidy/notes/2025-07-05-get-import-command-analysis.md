# Get-Import Command Analysis and Implementation Requirements

## Overview

The `get-import` command should retrieve and display data from any import location supported by the iidy import system. This command enables users to directly access and inspect data from various sources (files, S3, HTTP, AWS services, etc.) without needing to create a template.

## Current State Analysis

### CLI Structure
From `src/cli.rs`:
- Command is defined as `GetImport(GetImportArgs)` (line 343)
- Arguments structure:
  - `import: String` - The import location to retrieve
  - `format: String` - Output format (default: "yaml")
  - `query: Option<String>` - Optional JMESPath query for filtering

### Current Implementation Gap
From `src/main.rs` line 139:
```rust
Commands::GetImport(args) => println!("get-import {:?}", args),
```
Currently just prints debug info - no actual implementation exists.

## Target Behavior (from iidy-js)

### Command Flow
1. Configure AWS credentials if needed
2. Read data from the specified import location using `readFromImportLocation()`
3. Apply JMESPath query if specified
4. Output result in requested format (YAML or JSON)

### iidy-js Implementation Analysis
From `iidy-js-for-reference/src/getImport.ts`:
```typescript
export async function getImportMain(argv: GenericCLIArguments): Promise<number> {
  await configureAWS(argv);
  const loc = argv.import;
  const baseLocation = '.';
  const importData = await readFromImportLocation(loc, baseLocation);

  let outputDoc = importData.doc;
  if (argv.query) {
      outputDoc = search(outputDoc, argv.query);
  }

  if (argv.format === 'yaml') {
    console.log(yaml.dump(outputDoc))
  } else {
    console.log(JSON.stringify(outputDoc, null, ' '));
  }
  return SUCCESS;
}
```

## Rust Implementation Requirements

### 1. Import System Integration
The Rust codebase already has a comprehensive import system:
- `src/yaml/imports/mod.rs` - Core import types and security model
- `src/yaml/imports/loaders/mod.rs` - Production import loader
- Support for all import types: file, env, git, random, filehash, cfn, ssm, s3, http

### 2. Security Model Compliance
The existing import system has a robust security model:
- Local templates can import from any source
- Remote templates (S3, HTTP) cannot use local-only imports (file, env, git, filehash)
- For get-import, we should use "." as base location (local context) to allow all import types

### 3. AWS Integration
- Must configure AWS SDK for AWS-based imports (cfn, ssm, s3)
- Should use existing AWS configuration from CLI args (without loading stack-args.yaml)
- Follow pattern from `src/aws.rs` and `src/cfn/create_stack.rs` for AWS setup
- Use `config_from_normalized_opts()` to create AWS SDK config from CLI options
- Need to handle AWS authentication errors gracefully using existing error formatting

### 4. Query Support
- Need JMESPath support for filtering results
- Should handle query errors and provide helpful messages
- Empty results should be handled gracefully

### 5. Output Formatting
- YAML output (default) using serde_yaml
- JSON output with pretty printing
- Handle serialization errors for complex data types

## Implementation Plan

### Phase 1: Core Implementation
1. Create `src/cfn/get_import.rs` module following existing patterns
2. Implement `get_import()` function with proper error handling
3. Add function to main.rs command handler
4. Use existing `ProductionImportLoader` for import resolution

### Phase 2: AWS Integration
1. Configure AWS SDK using existing CLI AWS options (no stack-args.yaml loading)
2. Use `AwsSettings::from_normalized_opts()` and `config_from_normalized_opts()` pattern
3. Handle authentication and region configuration
4. Use existing AWS error formatting from `src/aws.rs`
5. Test with all AWS import types (cfn, ssm, s3)

### Phase 3: Query and Output
1. Use existing JMESPath dependency for query support
2. Implement query processing with error handling
3. Add output formatting for YAML and JSON
4. Handle edge cases (empty results, serialization errors)

### Phase 4: Offline Testing Strategy
1. **Fixture-based Testing**: Create test fixtures for all import types in `test-fixtures/get-import/`
2. **Mock Import Loader**: Implement `TestImportLoader` that returns fixture data instead of making network calls
3. **Integration Tests**: Test get-import command end-to-end using fixtures
4. **Error Scenario Testing**: Test AWS authentication errors, invalid queries, etc. using controlled fixtures
5. **Security Model Testing**: Verify remote template restrictions work correctly
6. **Snapshot Testing**: Use `insta` for output format verification

## Technical Considerations

### Dependencies
- `jmespath` crate for query support
- Existing `serde_yaml` for YAML output
- Existing `serde_json` for JSON output
- Existing AWS SDK integration

### Error Handling
- Import location not found
- AWS authentication failures
- JMESPath query syntax errors
- Network timeouts for remote imports
- Invalid format specifications

### Base Location Strategy
Use "." as base location for get-import to:
- Allow all import types (local context)
- Maintain consistency with iidy-js behavior
- Provide maximum flexibility for users

## Expected User Experience

### Examples
```bash
# Get a file
iidy get-import file:config.yaml

# Get from S3
iidy get-import s3://bucket/config.yaml

# Get CloudFormation stack output
iidy get-import cfn:stack/MyStack/output/DatabaseUrl

# Get with JMESPath query
iidy get-import s3://bucket/config.yaml --query 'database.host'

# Get as JSON
iidy get-import file:config.yaml --format json
```

### Error Messages
Should provide clear, actionable error messages:
- "Import location not found: s3://bucket/missing.yaml"
- "AWS authentication failed. Check your credentials."
- "Invalid JMESPath query: 'invalid.query['"
- "Unsupported format: 'xml'. Use 'yaml' or 'json'."

## Implementation Files

### Primary Implementation
- `src/cfn/get_import.rs` - Main implementation
- `src/main.rs` - Command handler integration

### Dependencies
- `src/yaml/imports/` - Import system (existing)
- `src/aws.rs` - AWS configuration (existing)
- `Cargo.toml` - Add jmespath dependency

### Testing
- `tests/get_import_test.rs` - Integration tests
- Test fixtures in `test-fixtures/get-import/`

## Success Criteria

1. **Functional Parity**: Behaves identically to iidy-js get-import
2. **Security Compliance**: Respects existing import security model
3. **AWS Integration**: Works with all AWS import types
4. **Error Handling**: Provides clear, helpful error messages
5. **Performance**: Efficient import resolution and output formatting
6. **Testing**: Comprehensive test coverage including edge cases

## Next Steps

1. Implement core functionality in `src/cfn/get_import.rs`
2. Add JMESPath dependency to `Cargo.toml`
3. Integrate with main command handler
4. Add comprehensive tests
5. Update documentation