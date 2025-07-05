# Demo.rs Implementation Analysis & Requirements

## Current Status

The `src/demo.rs` file is a partial port of `iidy-js-for-reference/src/demo.ts` with several TODOs and missing functionality. This document provides a comprehensive analysis of what needs to be implemented.

## Key Differences Between Current Implementation and iidy-js

### 1. YAML Preprocessing Integration (CRITICAL)

**Current Issue**: Line 31-33 in `src/demo.rs`:
```rust
// TODO load script with crate::yaml::preprocess_yaml_v11
// as we need to preprocess what is in it
let script: DemoScript = serde_yaml::from_str(&data).with_context(|| "parsing demo script")?;
```

**iidy-js Implementation**: Lines 61-62 in `demo.ts`:
```typescript
const script0 = yaml.loadString(fs.readFileSync(demoFile), demoFile);
const script: any = await transform(script0, demoFile);
```

**Required Fix**: Must use `crate::yaml::preprocess_yaml_v11` to handle:
- `$imports` directives (e.g., `random:dashed-name`)
- `$defs` variable definitions
- Handlebars template interpolation (e.g., `{{ StackName }}`, `{{ nameSuffix }}`)

### 2. Command Type Normalization (MISSING)

**Current Issue**: Direct deserialization with `#[serde(untagged)]` doesn't handle string-to-shell normalization.

**iidy-js Implementation**: `normalizeRawCommand()` function converts bare strings to shell commands.

**Required Fix**: Either:
- Implement custom deserializer for `RawCommand`
- Post-process commands after deserialization
- Use `normalizeRawCommand` equivalent

### 3. File Path Security Validation (MISSING)

**Current Issue**: No validation of file paths in `unpackFiles()` equivalent.

**iidy-js Implementation**: Lines 78-80 in `demo.ts`:
```typescript
if (pathmod.isAbsolute(fp)) {
  throw new Error(`Illegal path ${fp}. Must be relative.`);
}
```

**Required Fix**: Add path validation to prevent:
- Absolute paths
- Parent directory traversal (`../`)
- Symlink attacks

### 4. Shell Command Safety (MEDIUM PRIORITY)

**Current Issue**: Line 74-75 uses hardcoded `/bin/bash`:
```rust
// TODO use `/usr/bin/env bash` instead
let status = Command::new("/bin/bash")
```

**iidy-js Implementation**: Uses `shell: '/bin/bash'` in `spawnSync`.

**Required Fix**: Use `/usr/bin/env bash` for better portability.

### 5. Error Handling and Cleanup (MISSING)

**Current Issue**: No explicit cleanup of temporary directory.

**iidy-js Implementation**: Uses try/finally with explicit cleanup:
```typescript
try {
  this._unpackFiles(script.files);
  await this._runCommands(_.map(script.demo, normalizeRawCommand));
} catch (e) {
  throw e;
} finally {
  child_process.execSync(`rm -r "${this.tmpdir.name}"`, {cwd: this.tmpdir.name});
}
```

**Required Fix**: Rust's `tempfile::TempDir` handles cleanup automatically via Drop, but should verify this works correctly.

### 6. Banner Display Formatting (MINOR DIFFERENCES)

**Current Implementation**: Uses `crossterm` with different color scheme.

**iidy-js Implementation**: Uses `cli-color` with `bgXterm(236)` and specific formatting.

**Status**: Functional but different visual appearance. May need adjustment for consistency.

## Demo Script Requirements Analysis

Based on the example demo scripts, the preprocessor must handle:

### Template Variables
- `{{ StackName }}` - from `$defs`
- `{{ nameSuffix }}` - from `$imports`
- `${USER:-demouser}` - bash variable expansion (handled by shell)

### Import Types
- `random:dashed-name` - special import type for generating random names

### File Generation
- Multi-line YAML/text files with proper indentation
- Template interpolation within file contents

## Implementation Priority

### High Priority (Must Fix)
1. **YAML Preprocessing Integration** - Critical for functionality
2. **Command Type Normalization** - Required for proper parsing
3. **File Path Security Validation** - Security requirement

### Medium Priority (Should Fix)
4. **Shell Command Safety** - Portability improvement
5. **Error Handling Enhancement** - Robustness improvement

### Low Priority (Nice to Have)
6. **Banner Formatting Consistency** - Visual consistency with iidy-js

## Test Requirements

### Unit Tests Needed
- YAML preprocessing with `$imports` and `$defs`
- File path validation (security tests)
- Command normalization
- Error handling scenarios

### Integration Tests Needed
- Full demo script execution
- Temporary file cleanup
- Banner display formatting

## Implementation Strategy

1. **Phase 1**: Fix YAML preprocessing integration
2. **Phase 2**: Add command normalization and file path validation
3. **Phase 3**: Enhance error handling and shell safety
4. **Phase 4**: Add comprehensive tests
5. **Phase 5**: Visual consistency improvements

## Security Considerations

- File path validation prevents directory traversal attacks
- Shell command execution uses controlled environment
- Temporary directory isolation prevents file system pollution
- No arbitrary code execution beyond intended shell commands

## Compatibility Notes

The implementation should maintain compatibility with existing demo scripts while improving security and error handling compared to the original iidy-js implementation.