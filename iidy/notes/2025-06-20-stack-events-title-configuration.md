# Stack Events Title Configuration Improvement

## Problem Identified

The interactive renderer was showing a generic "Stack Events:" title initially, then using ANSI escape sequences to rewrite it with the specific title (e.g., "Previous Stack Events (max 50):") when the data arrived. This approach was problematic because:

1. It created visual flicker in interactive mode
2. It didn't work in plain mode (no ANSI features)
3. It was unnecessarily complex

## Solution Implemented

Since we now have the full CLI context available during renderer construction, we can configure the correct section titles from the start.

### Changes Made

1. **Added section_titles HashMap to InteractiveRenderer**
   - Stores configured titles for each section type
   - Populated during construction based on CLI arguments

2. **Moved setup_operation() call to new()**
   - Called during construction instead of init()
   - Accepts both the operation and full CLI context
   - Follows "correct by construction" principle

3. **Added configure_section_titles() method**
   - Sets up titles based on operation type and CLI arguments
   - For describe-stack: `"Previous Stack Events (max {event_count}):"`
   - For watch-stack: `"Live Stack Events:"`
   - Default: `"Stack Events:"`

4. **Removed ANSI escape rewriting from render_stack_events()**
   - No more `\x1b[A\r\x1b[K` sequences
   - Section heading already correct from the start

### Benefits

1. **Cleaner code**: No ANSI escape sequence manipulation
2. **Better user experience**: No visual flicker
3. **Consistent behavior**: Works the same in both interactive and plain modes
4. **Correct by construction**: Titles are right from the start

### Example

When running `iidy describe-stack test-stack --events 75`:
- Before: Shows "Stack Events:" then rewrites to "Previous Stack Events (max 75):"
- After: Shows "Previous Stack Events (max 75):" immediately

## Testing

Added `tests/stack_events_title_test.rs` to verify:
1. Different event counts produce correct titles
2. No ANSI rewriting occurs in plain mode
3. Watch-stack gets "Live Stack Events:" title

All output tests pass successfully.