# StackFrame Removal Progress Update

**Date:** 2025-06-14  
**Status:** Partially complete - requires resolver method updates

## What's Been Completed

✅ **Removed from public API:**
- Removed `StackFrame` from mod.rs exports
- Removed `StackFrame` import from engine.rs
- Removed manual stack frame creation in engine.rs

✅ **Removed core implementation:**
- Removed `StackFrame` struct definition
- Removed `stack` field from `TagContext`
- Removed stack-related methods: `with_stack_frame`, `current_location`, `current_path`
- Removed path manipulation methods: `with_path_segment`, `with_array_index`
- Updated all TagContext constructors to remove stack field
- Removed stack-related tests

## What Needs to Be Done

❌ **Resolver methods still reference removed StackFrame methods:**

The resolvers use these methods for error reporting:
- `context.current_location()` - Used in 8 places for error context
- `context.current_path()` - Used in 4 places for error path reporting
- `context.with_path_segment()` - Used in 2 places for nested navigation
- `context.with_array_index()` - Used in 1 place for array element navigation

## Replacement Strategy

Since these are used for error reporting, we need to:

1. **Replace `current_location()`** with `context.input_uri` (already available)
2. **Replace `current_path()`** with PathTracker information (already passed to methods)
3. **Replace `with_path_segment()`** with PathTracker operations
4. **Replace `with_array_index()`** with PathTracker operations

## Alternative: Minimal StackFrame

Given the extensive usage in resolvers, we could consider keeping a minimal internal-only version:

```rust
// Internal helper for error reporting only
#[doc(hidden)]
pub(crate) struct ErrorContext {
    pub location: Option<String>,
    pub path: String,
}

impl TagContext {
    pub(crate) fn error_context(&self, path_tracker: &PathTracker) -> ErrorContext {
        ErrorContext {
            location: self.input_uri.clone(),
            path: path_tracker.segments().join("."),
        }
    }
}
```

This would require minimal changes to existing resolver code while removing the full StackFrame system.

## Recommendation

1. **Complete the removal** by updating resolvers to use PathTracker + input_uri
2. **This aligns with the original decision** that PathTracker provides better error context
3. **The scope system provides superior variable tracking** anyway

## Files That Need Updates

- `src/yaml/resolution/resolver.rs` - 10+ method references
- `src/yaml/resolution/resolver_split_args.rs` - 5+ method references

## Current Compilation Status

❌ **Does not compile** due to missing methods in resolvers
✅ **Scope system intact** and functional
✅ **Core architecture clean** with StackFrame removed