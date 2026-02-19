# Error System Code Review

Date: 2026-02-19
Context: After adding `lookup_query_error` to wrapper.rs, the pattern duplication became obvious.

## The Problem

The user-facing error output is excellent -- source context, line numbers, carets, available variables, color coding. But the implementation is heavily duplicated and fragile. wrapper.rs alone is ~1080 lines, with `type_mismatch_error_impl` at 407 lines.

## Two Competing Systems

There are two independent error rendering approaches that don't share code:

1. **EnhancedPreprocessingError** (enhanced.rs) -- an enum with 6 variants and a single `display_with_context()` method. Clean abstraction but only used by some error paths.

2. **Bespoke format! builders** (wrapper.rs) -- `tag_parsing_error`, `yaml_syntax_error`, `lookup_query_error` each construct their own display strings with inline color codes, source context formatting, and tag-specific examples. These bypass EnhancedPreprocessingError entirely.

Both converge on `EnhancedErrorWrapper { message: String }` which just wraps a pre-formatted string for `anyhow::Error`.

## Duplicated Patterns (with counts)

### 1. File path parsing: "file.yaml:42" -> (file, line) -- 4 instances
- wrapper.rs lines 38-52, 285-299, 514-528, 933-947
- Each is 15 lines of `split(':')` + `parse::<usize>()` with TODO panic warnings

### 2. Color code setup -- 3 instances in wrapper.rs + 1 in enhanced.rs
- 7-8 variables each: bold_red, red, cyan, blue_grey, light_blue, grey, reset
- ~10 lines per instance, ~40 lines total

### 3. Source context display (prev/current/next lines) -- 5 instances
- enhanced.rs `display_with_context`: 59 lines
- wrapper.rs `variable_not_found_error`: 39 lines
- wrapper.rs `tag_parsing_error`: 78 lines
- wrapper.rs `type_mismatch_error_impl`: appears twice internally
- wrapper.rs `lookup_query_error`: 18 lines

### 4. Tag-specific column detection -- duplicated within type_mismatch_error_impl
- Lines 539-677: when line number is provided
- Lines 680-892: when searching for the tag
- Nearly identical 24+ `if context_description.contains()` branches in both paths
- 348 lines of tag detection logic total

## Proposed Refactoring

### Phase 1: Extract shared helpers (low risk, big payoff)

```rust
// New file: src/yaml/errors/display.rs

struct ErrorColors { bold_red, red, cyan, blue_grey, light_blue, grey, reset }

impl ErrorColors {
    fn detect() -> Self { /* NO_COLOR + atty check */ }
}

fn parse_file_location(file_path: &str) -> (&str, Option<usize>) { ... }

fn format_source_context(
    file_path: &str,
    line: usize,
    column: usize,
    span_len: usize,
    inline_desc: &str,
    colors: &ErrorColors,
) -> String { ... }
```

This alone would eliminate ~300 lines of duplication and all 27 TODO panic warnings (fix bounds checking once).

### Phase 2: Consolidate wrapper.rs functions

`tag_parsing_error` and `lookup_query_error` should use `EnhancedPreprocessingError` variants instead of building strings directly. This means either:

- (a) Adding new variants to the enum (e.g., `TagParsing`, `LookupQuery`)
- (b) Making existing variants flexible enough (TypeMismatch with better rendering)

Option (a) is cleaner. Each new variant would define its error_type string ("Tag error", "Lookup error"), its guidance line, and its help content. The shared `display_with_context` handles all rendering.

### Phase 3: Simplify type_mismatch_error_impl

The 407-line function exists because it tries to find the exact column for each tag type by string-matching the source line. This could be:

- Moved to a lookup table: `tag_name -> search_patterns`
- Or better: pass column info from the parser (which already knows it) through to the error site, eliminating runtime source searching entirely

### Phase 4: Remove EnhancedErrorWrapper

Once all errors go through `EnhancedPreprocessingError`, the wrapper becomes unnecessary. The enum itself can implement `Display` with the formatted output, and `anyhow::Error` can wrap it directly.

## Priority

Phase 1 is the clear win -- mechanical extraction, no behavioral change, removes the most duplication and panic risk. Phases 2-4 are progressively larger refactors that could be done incrementally.

## Files Involved

- `src/yaml/errors/enhanced.rs` -- EnhancedPreprocessingError enum + display_with_context (349 lines)
- `src/yaml/errors/wrapper.rs` -- 10 error construction functions (1083 lines)
- `src/yaml/errors/ids.rs` -- ErrorId enum (397 lines)
- `src/yaml/errors/mod.rs` -- module exports
- `src/yaml/resolution/resolver.rs` -- primary consumer of error functions
- `src/yaml/parsing/parser.rs` -- uses tag_parsing_error
