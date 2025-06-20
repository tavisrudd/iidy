# Plain Renderer Race Condition Fix

## Problem Identified

When using `describe-stack --output-mode plain`, there was a race condition causing sections to appear in the wrong order. This didn't happen with interactive mode.

### Root Cause

1. **Interactive Mode**: Properly received CLI context through `OutputOptions`, enabling section ordering logic
2. **Plain Mode**: Used a `PlainTextRenderer` wrapper that created `InteractiveOptions::plain()` with `cli_context: None`

Without CLI context:
- The `init()` method couldn't call `setup_operation()` 
- `expected_sections` remained empty
- All data was rendered immediately via `render_data_immediately()` instead of `render_with_ordering()`
- This caused sections to render in arrival order (race condition)

## Solution Implemented

### 1. Removed PlainTextRenderer Wrapper
- Deleted `src/output/renderers/plain.rs`
- Updated `DynamicOutputManager::create_renderer()` to create `InteractiveRenderer` directly for plain mode

### 2. Plain Mode Configuration
Plain mode now uses `InteractiveRenderer` with these settings:
```rust
InteractiveOptions {
    color_choice: ColorChoice::Never,      // No colors
    enable_spinners: false,                // No spinners
    enable_ansi_features: false,           // No ANSI
    cli_context: Some(cli_context.clone()) // CLI context for proper ordering
}
```

### 3. Benefits
- Section ordering now works correctly in plain mode
- No code duplication between plain and interactive renderers
- Simpler architecture with one renderer handling both modes
- Plain mode gets all the ordering benefits of interactive mode

## Testing

Added `tests/plain_mode_ordering_test.rs` to verify:
1. Plain mode with CLI context renders sections in correct order
2. Plain mode without CLI context shows the race condition (for documentation)

## Files Changed

### Deleted
- `src/output/renderers/plain.rs`

### Modified
- `src/output/manager.rs` - Updated `create_renderer()` to use InteractiveRenderer for plain mode
- `src/output/renderers/mod.rs` - Removed plain module export
- `tests/output_renderer_snapshots.rs` - Updated to use InteractiveRenderer with plain options
- `tests/pixel_perfect_output_tests.rs` - Updated to use InteractiveRenderer with plain options  
- `tests/fixture_validation_tests.rs` - Updated to use InteractiveRenderer with plain options
- `src/cfn/list_stacks.rs` - Removed unused import
- `src/output/manager.rs` - Fixed import warnings

### Added
- `tests/plain_mode_ordering_test.rs` - Test to verify the fix

## Result

The race condition in plain mode is now fixed. Both interactive and plain modes use the same renderer with proper section ordering based on the CloudFormation operation being performed.