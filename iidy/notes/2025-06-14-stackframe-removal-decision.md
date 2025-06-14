# StackFrame Removal Decision

**Date:** 2025-06-14  
**Context:** Phase 1 YAML preprocessing refactoring  
**Decision:** Remove StackFrame system in favor of existing error reporting mechanisms

## Background

During the TagContext refactoring (commit a5dcb21), we discovered that `StackFrame` was part of the public API but provided minimal value over existing error reporting systems.

## StackFrame Design Intent

The `StackFrame` system was designed to provide enhanced error reporting and debugging context:

```rust
pub struct StackFrame {
    /// Location of the operation (file path or description)
    pub location: Option<String>,
    /// Path within the document (e.g., "config.database.host")
    pub path: String,
}
```

Intended to track:
- File locations for error reporting
- Document paths for precise error context
- Future LSP server support for jump-to-source functionality

## Current Implementation Status

**Partially implemented but underutilized:**
- ✅ Data structures exist
- ✅ Basic manipulation methods exist (`with_stack_frame`, `current_location`, `current_path`)
- ✅ Tests verify the mechanisms work
- ❌ No actual error messages use the stack information
- ❌ No stack manipulation during tag processing (no push/pop during traversal)
- ❌ Manual creation required (engine.rs manually creates root frame)

**Usage analysis:**
- **Production code:** Only engine.rs creates one root frame manually
- **Tests:** Extensive testing of stack manipulation, but no real error scenarios
- **Public API:** Exported but not needed by external users

## Overlap Analysis with Existing Systems

### Scope System Already Provides:
- **Variable origin tracking:** Where each variable was defined (file:line)
- **Variable source type:** LocalDefs, ImportedDocument, TagBinding, etc.
- **Import hierarchy:** Which document imported which
- **Variable shadowing:** Which variable overrides another

### Import Stack Already Provides:
- **Circular import detection:** Full import chain for error messages
- **Active import tracking:** What's currently being processed

### PathTracker Already Provides:
- **Current document path:** e.g., "Resources.MyBucket.Properties"
- **Dynamic path during resolution:** Updates as we traverse the document

### StackFrame Redundancy:
- `location` field: Duplicates scope system's `source_uri`
- `path` field: Duplicates PathTracker's functionality
- Cross-file context: Partially covered by import stack + scope system

## Error Scenarios Analysis

**Errors that would theoretically benefit from StackFrame:**
1. **Variable not found** - But scope system already provides variable origins
2. **Type mismatches** - PathTracker already provides document location
3. **CloudFormation validation** - Already includes tag name and path context
4. **Circular references** - Import stack handles this better

**Current error context already available:**
- Variable names and available alternatives (scope system)
- Document paths during resolution (PathTracker)
- Import chains (import stack)
- Variable source tracking (scope system)

## Decision: Remove StackFrame

**Rationale:**
1. **Redundant functionality:** Existing systems provide better error context
2. **Incomplete implementation:** Would require significant work to complete
3. **Unclear value proposition:** No compelling error scenarios that benefit
4. **API pollution:** Exported unnecessarily in public interface
5. **Future flexibility:** Can reimplement later with clearer requirements

**What StackFrame was supposed to provide that isn't covered:**
- Cross-file path tracking when traversing multiple documents
- Historical path stack showing how we got to current location

**Why this isn't essential:**
- Import stack + scope system provide sufficient context for errors
- PathTracker provides current location context
- For LSP support, would need line/column numbers anyway (different requirement)

## Implementation Plan

### Phase 1: Remove from Public API
- [x] Remove `StackFrame` from context.rs exports
- [x] Remove from mod.rs re-exports
- [ ] Remove from engine.rs imports and usage

### Phase 2: Remove Internal Implementation
- [ ] Remove `stack` field from TagContext
- [ ] Remove StackFrame type definition
- [ ] Remove stack-related methods (`with_stack_frame`, `current_location`, `current_path`, etc.)
- [ ] Remove stack manipulation in path methods (`with_path_segment`, `with_array_index`)
- [ ] Update tests to remove stack assertions

### Phase 3: Clean up Engine Usage
- [ ] Remove manual stack frame creation in engine.rs
- [ ] Simplify TagContext creation

## Future Considerations

**When to reconsider StackFrame:**
1. **LSP server implementation:** Need line/column tracking for jump-to-source
2. **Complex error scenarios:** If we discover cases where current context is insufficient
3. **Debugging tools:** If we build interactive debugging features

**If reimplemented, should include:**
- Line and column number tracking
- Source position information
- Integration with language server protocol
- Clear differentiation from PathTracker and scope system

## Related Files

- `src/yaml/resolution/context.rs` - Contains StackFrame definition
- `src/yaml/resolution/mod.rs` - Public API exports
- `src/yaml/engine.rs` - Current usage
- Tests throughout the codebase

## Commit History

- Initial TagContext refactoring: commit a5dcb21
- StackFrame removal: (pending)