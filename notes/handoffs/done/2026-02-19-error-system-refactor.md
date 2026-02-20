# Error System Refactoring -- Handoff

**Date**: 2026-02-19
**Review doc**: `notes/2026-02-19-error-system-review.md`

## Problem Summary

The user-facing error output is excellent (source context, line numbers, carets, color coding)
but the implementation is heavily duplicated and fragile. `wrapper.rs` alone is ~1096 lines,
with `type_mismatch_error_impl` at 407 lines. There are two independent rendering approaches
that don't share code, and four categories of mechanical duplication.

This is a two-part handoff. Execute Part 1 first, get tests green, commit. Then Part 2.

---

## Architecture Overview

### Files

| File | Lines | Role |
|------|-------|------|
| `src/yaml/errors/mod.rs` | 14 | Module exports: `pub use enhanced::*; pub use wrapper::*` |
| `src/yaml/errors/ids.rs` | 397 | `ErrorId` enum (45 variants). Clean, do not touch. |
| `src/yaml/errors/enhanced.rs` | 700 | `EnhancedPreprocessingError` enum (6 variants) + `display_with_context()` + helpers (fuzzy match, levenshtein, type conversion help) |
| `src/yaml/errors/wrapper.rs` | 1096 | 6 public + 2 private functions that construct `anyhow::Error`. The duplication lives here. |

### Two Rendering Paths (the core problem)

**Path A -- via EnhancedPreprocessingError**: Three wrapper functions construct an
`EnhancedPreprocessingError` variant, call `display_with_context()`, wrap the resulting
string in `EnhancedErrorWrapper`:

- `variable_not_found_error` (lines 29-109) -> `EnhancedPreprocessingError::variable_not_found`
- `type_mismatch_error_impl` (lines 506-912) -> `EnhancedPreprocessingError::type_mismatch`
- `cloudformation_validation_error_impl` (lines 926-1001) -> `EnhancedPreprocessingError::cloudformation_validation`

**Path B -- direct string building**: Three wrapper functions bypass
`EnhancedPreprocessingError` entirely and construct formatted strings with inline color
codes, source context formatting, and tag-specific examples:

- `yaml_syntax_error` (lines 129-261) -- YAML parse errors from serde_yaml
- `tag_parsing_error` (lines 266-473) -- unknown tags, missing fields, bad structure
- `lookup_query_error` (lines 1006-1096) -- JMESPath/property query failures

Both paths converge on `EnhancedErrorWrapper { message: String }` (lines 14-25) which
just wraps a pre-formatted string for `anyhow::Error`.

### Consumer Call Sites

Only two files call into wrapper.rs:

**`src/yaml/parsing/parser.rs`** (3 thin adapter methods that convert `anyhow::Error` to `ParseError`):
- `YamlParser::create_syntax_error` (line 247) -> `yaml_syntax_error`
- `YamlParser::missing_field_error` (line 1964) -> `missing_required_field_error`
- `YamlParser::tag_error` (line 1991) -> `tag_parsing_error`

**`src/yaml/resolution/resolver.rs`** (~30 call sites):
- `Resolver::process_string_with_handlebars` (line 580) -> `variable_not_found_error_with_path_tracker`
- `Resolver::resolve_include` (lines 1023, 1029) -> `lookup_query_error`
- `Resolver::resolve_include` (lines 1104, 1145) -> `variable_not_found_error_with_path_tracker`
- `Resolver::create_type_mismatch_error` (line 346) -> `type_mismatch_error_with_path_tracker` (choke point; called from lines 690, 1227, 1362, 1398)
- ~12 direct calls to `type_mismatch_error_with_path_tracker` from resolve_split (1335), resolve_join (1384), resolve_map_values (1445), resolve_concat_map (1488), resolve_merge_map (1526, 1542), resolve_map_list_to_hash (1629, 1646), resolve_group_by (1700), resolve_from_pairs (1734, 1750)
- `Resolver::validate_cloudformation_tag` (lines 1978-2364) -> `cloudformation_validation_error_with_path_tracker` (9 call sites)

---

## Duplication Inventory

### 1. File path parsing: `"file.yaml:42"` -> `(file, line)` -- 4 instances

Each is ~15 lines of `split(':')` + `parse::<usize>()` with TODO panic warnings.

| Function | Lines in wrapper.rs |
|----------|-------------------|
| `variable_not_found_error` | 38-52 |
| `tag_parsing_error` | 285-299 |
| `type_mismatch_error_impl` | 514-528 |
| `cloudformation_validation_error_impl` | 933-947 |

All four are structurally identical. Extract to:
```rust
fn parse_file_location(file_path: &str) -> (&str, Option<usize>)
```

### 2. Color code setup -- 4 instances

7-8 variables each (`bold_red`, `red`, `cyan`, `blue_grey`, `light_blue`, `grey`, `reset`),
~10 lines per instance, all gated on `NO_COLOR` env + `atty::is(Stderr)`.

| Location | Lines |
|----------|-------|
| `enhanced.rs` `display_with_context` | 124-134 |
| `wrapper.rs` `yaml_syntax_error` | 169-176 |
| `wrapper.rs` `tag_parsing_error` | 276-282 |
| `wrapper.rs` `lookup_query_error` | 1013-1021 |

Extract to:
```rust
struct ErrorColors { bold_red, red, cyan, blue_grey, light_blue, grey, reset: &'static str }
impl ErrorColors { fn detect() -> Self { ... } }
```

### 3. Source context display (prev/current/next lines with carets) -- 5 instances

| Location | Lines | Notes |
|----------|-------|-------|
| `enhanced.rs` `display_with_context` | 192-253 | Most complete: prev, current, caret, next |
| `wrapper.rs` `variable_not_found_error` | N/A | Delegates to `display_with_context` |
| `wrapper.rs` `yaml_syntax_error` | 188-256 | Same structure, duplicated inline |
| `wrapper.rs` `tag_parsing_error` | 315-405 | Same + tag-specific caret logic |
| `wrapper.rs` `lookup_query_error` | 1039-1076 | Same structure, simplest version |

Extract to:
```rust
fn format_source_context(
    lines: &[impl AsRef<str>],
    line_num: usize,
    column: Option<usize>,
    span_len: usize,
    inline_desc: Option<&str>,
    colors: &ErrorColors,
) -> String
```

### 4. Tag-specific column detection in `type_mismatch_error_impl` -- 2 near-identical blocks

| Path | Lines | Description |
|------|-------|-------------|
| Line number provided | 539-677 | 24+ `if context_description.contains()` branches |
| Searching for tag | 680-892 | Nearly identical 24+ branches, different surrounding code |

Both blocks match on the same set of tag names (`!$split`, `!$join`, `!$groupBy`,
`!$mapListToHash`, `!$fromPairs`, `!$map`, `!$merge`) to find the column position.
The "provided line number" path indexes into `lines[line_num - 1]`, while the "searching"
path does `lines.iter().enumerate().find_map()` wrapping each match.

Extract to:
```rust
fn find_tag_column(line_content: &str, context_description: &str) -> usize
```
Then both paths call this: one with the known line, one inside the find_map closure.

---

## Part 1: Extract Shared Helpers

**Goal**: Create `src/yaml/errors/display.rs` with the three shared helpers, then rewrite
all call sites in `wrapper.rs` and `enhanced.rs` to use them. No behavioral change. No
change to public API. All existing tests must pass with identical output.

### Step-by-step

1. **Create `src/yaml/errors/display.rs`** with:
   - `pub(crate) fn parse_file_location(file_path: &str) -> (&str, Option<usize>)`
   - `pub(crate) struct ErrorColors` with `pub(crate) fn detect() -> Self`
   - `pub(crate) fn format_source_context(...)` -- the prev/current/caret/next renderer

2. **Add `pub mod display;`** to `src/yaml/errors/mod.rs` (do NOT `pub use` it -- these
   are internal helpers, not public API)

3. **Replace the 4 file-path-parsing blocks** in wrapper.rs with calls to
   `parse_file_location`. Each replacement is mechanical: the let-binding destructure
   stays the same, just the body changes from ~15 lines to 1 function call.

4. **Replace the 4 color-setup blocks** (3 in wrapper.rs, 1 in enhanced.rs) with
   `let c = ErrorColors::detect();` and update all references from bare variables
   (`bold_red`, `red`, etc.) to `c.bold_red`, `c.red`, etc.

5. **Replace the source-context blocks** in `yaml_syntax_error` (lines 188-256) and
   `lookup_query_error` (lines 1039-1076) with calls to `format_source_context`.
   The `tag_parsing_error` context block (lines 315-405) has extra caret logic per
   tag keyword that makes it slightly different -- handle this by either:
   - (a) Passing an optional caret-finder closure to `format_source_context`
   - (b) Keeping the caret logic inline and only extracting the prev/current/next rendering
   Option (b) is simpler and still eliminates most duplication.

   For `enhanced.rs` `display_with_context` (lines 192-253): this is the most complete
   version and is the natural basis for `format_source_context`. Extract it, then call
   the extracted function from `display_with_context`.

6. **Extract `find_tag_column`** from `type_mismatch_error_impl`. Both the provided-line
   path (539-677) and searching path (680-892) should call the same function on a single
   line of text. The searching path wraps it in `find_map`, the provided path calls it
   directly. This alone eliminates ~200 lines of duplication.

7. **Run `make check-fast`** after each extraction to catch compile errors early.

8. **Run `make test`** at the end. All tests must pass with identical output (the snapshot
   tests will catch any formatting drift).

### Verification

- `make check-fast` should show zero errors
- `make test` should show zero new/changed snapshots and all tests pass
- `wc -l src/yaml/errors/wrapper.rs` should drop from ~1096 to ~600-700
- The TODO panic warnings in wrapper.rs should be consolidated to safe implementations
  in `parse_file_location` and `format_source_context`

---

## Part 2: Consolidate and Simplify

**Goal**: Route all error paths through `EnhancedPreprocessingError`, eliminate Path B
(direct string building), remove `EnhancedErrorWrapper`.

### Phase 2a: Add new EnhancedPreprocessingError variants

Add to `enhanced.rs`:

```rust
YamlSyntax {
    error_id: ErrorId,
    short_message: String,
    guidance: String,
    location: SourceLocation,
    help: Option<String>,
},

TagParsing {
    error_id: ErrorId,
    tag_name: String,
    message: String,
    location: SourceLocation,
    suggestion: Option<String>,
    // example is derived from tag_name in display_with_context
},

LookupQuery {
    error_id: ErrorId,
    variable_path: String,
    message: String,
    location: SourceLocation,
    available_keys: Vec<String>,
},
```

Extend `display_with_context()` to handle these variants. The rendering logic already
exists in the wrapper functions -- move it into the match arms.

Move the tag-specific example generation from `tag_parsing_error` (wrapper.rs lines
408-463) into a method on `EnhancedPreprocessingError` (or a free function in enhanced.rs).

### Phase 2b: Rewrite the Path B wrapper functions

Rewrite `yaml_syntax_error`, `tag_parsing_error`, and `lookup_query_error` to:
1. Construct the appropriate `EnhancedPreprocessingError` variant
2. Read source file
3. Call `display_with_context()`
4. Wrap in `EnhancedErrorWrapper`

This mirrors what the Path A functions already do. After this, all 6 wrapper functions
follow the same pattern.

### Phase 3: Simplify type_mismatch_error_impl

After Part 1's `find_tag_column` extraction, the function should already be ~200 lines
shorter. Consider whether the remaining logic (file reading, line searching, column
finding) can be shared with the other wrapper functions via a common helper:

```rust
fn find_error_location(
    file_path: &str,
    tag_finder: impl Fn(&str) -> Option<usize>,
) -> (Option<Vec<String>>, usize, usize)
```

This would replace the file-reading + line-searching boilerplate that appears in
every wrapper function.

### Phase 4: Remove EnhancedErrorWrapper

Once all paths go through `EnhancedPreprocessingError`:
1. Make `EnhancedPreprocessingError` implement `Display` with formatted output
   (it already does via `display_with_context(None)`)
2. Have wrapper functions return `anyhow::Error::new(error)` directly instead of
   wrapping in `EnhancedErrorWrapper`
3. Delete `EnhancedErrorWrapper` struct

**Caution**: This changes the error type inside `anyhow::Error`. Check if any code
does `error.downcast_ref::<EnhancedErrorWrapper>()`. Search for `downcast` in the
codebase before removing.

### Verification for Part 2

Same as Part 1: `make check-fast`, then `make test`. All tests must pass with identical
snapshot output. If any snapshots change, diff them carefully -- formatting changes that
improve consistency are acceptable but must be reviewed with the user before accepting.

---

## Constraints (from CLAUDE.md)

- Zero test regressions, zero warnings
- Do not accept snapshot changes without user permission
- No emojis in code or output
- No reward hacking -- 100% tests passing
- `make check-fast` for quick iteration, `make test` via `run-quiet` for full suite
- User reviews commits -- do not commit without being asked
- Do not create branches
- Keep public APIs small -- the new display.rs helpers should be `pub(crate)` not `pub`
- Comment only the non-obvious
- All imports at module level, not inside functions

---

## Part 1 Completion Notes

**Status**: Complete (2026-02-19). All 610 tests pass, zero warnings, zero snapshot changes.

**Line counts after Part 1**:
- `wrapper.rs`: 1097 -> 584 lines (47% reduction)
- `enhanced.rs`: 701 -> 629 lines
- `display.rs`: 323 lines (new)

**What was extracted into `display.rs`**:
1. `parse_file_location()` -- replaces 4 identical file-path-parsing blocks
2. `ErrorColors` struct + `detect()` -- replaces 4 color-setup blocks (3 in wrapper.rs, 1 in enhanced.rs)
3. `format_source_context()` -- replaces 3 source-context rendering blocks
4. `find_tag_column()` + private helpers -- replaces ~200 lines of duplicated tag column detection in type_mismatch_error_impl
5. `search_field_on_subsequent_lines()` -- deduplicates multi-line field search for groupBy/mapListToHash/fromPairs

**Additional wrapper.rs cleanup** (not in original plan but natural decomposition):
- `tag_error_caret()` -- extracted caret position logic from tag_parsing_error
- `tag_example()` -- extracted tag-specific example generation from tag_parsing_error

**Lesson learned**: `find_tag_column` initially lacked tag-family fallback branches.
The resolver uses context descriptions like `"!$join sequence item"` and `"!$split delimiter argument"`
which don't match the specific variant checks (e.g., `"!$join sequence argument"`). The old Block 2
had an outer `contains("!$split")` guard that caught these with a tag-specific fallback
(`find("!$split") + 8`), but the initial `find_tag_column` fell all the way to the generic
`find("!$") + 2`. Fixed by adding tag-family fallback branches (e.g., `contains("!$split")`)
after the specific variant checks but before the generic `!$` fallback.

---

## Part 2 Completion Notes

**Status**: Complete (2026-02-19). All 610 tests pass, zero warnings, zero snapshot changes.

**Line counts (current)**:
- `wrapper.rs`: 440 lines (was 1096)
- `enhanced.rs`: 884 lines (was 700)
- `display.rs`: 432 lines (new)
- `ids.rs`: 397 (untouched)
- `mod.rs`: 14 (+1)

**Git diff stats (Parts 1+2 combined, vs last commit)**:
- `wrapper.rs`: +202 / -858 (net -656)
- `enhanced.rs`: +304 / -120 (net +184)
- `display.rs`: +432 (new file, untracked)
- `mod.rs`: +1
- **Total: 978 lines of old code deleted, 939 lines of new structured code. Net -39.**

**What changed in Part 2**:

1. **3 new `EnhancedPreprocessingError` variants**: `YamlSyntax`, `TagParsing`, `LookupQuery`.
   All error paths now go through structured error types instead of inline string building.

2. **`display_with_context()` extended**: early-returns to dedicated render methods
   (`render_yaml_syntax`, `render_tag_parsing`, `render_lookup_query`) for the new variants,
   producing byte-identical output to the old Path B functions. Existing Path A rendering
   unchanged.

3. **Path B wrapper functions rewritten**: `yaml_syntax_error`, `tag_parsing_error`,
   `lookup_query_error` now construct structured variants instead of building format strings
   inline. All 6 wrapper functions follow the same pattern: read source, compute location,
   construct variant, wrap in `FormattedError`.

4. **`EnhancedErrorWrapper` replaced with `FormattedError`**: stores the structured
   `EnhancedPreprocessingError` + `Option<Vec<String>>` source lines. Rendering happens
   at display time via `display_with_context()`, not at construction time. No code in the
   codebase does `downcast` on this type, so the type change is safe.

5. **Helpers moved**: `tag_error_caret` and `tag_example` moved from wrapper.rs to display.rs
   (made `pub(crate)`), since they're now called from `enhanced.rs` render methods.

6. **`parse_file_location_full`** added to display.rs: extracts file, line, AND column from
   path strings like `"file.yaml:2:11"`. Needed by `tag_parsing_error` where the parser's
   column (for the header display) differs from the caret column (from `tag_error_caret`).
   The `TagParsing` variant stores both: `location.column` for the header, `caret_column`
   for source context rendering.

**Phase 3 (simplify type_mismatch_error_impl)**: Already accomplished in Part 1 via
`find_tag_column` extraction. The function is now ~100 lines, manageable without further work.

**Design decisions**:
- Kept variant-specific render methods rather than forcing all variants through one template.
  The Path B error formats differ enough (different "For more info" wording, different help
  sections, different blank line placement) that a single template would need so many
  conditionals it would be harder to maintain.
- `FormattedError` stores source lines (not a pre-rendered string) so the structured error
  data remains accessible. The cost of cloning source lines is negligible on error paths.
- Preserved "For more info, run:" wording for YamlSyntax and TagParsing to avoid snapshot
  churn. Could be standardized to "For more info:" in a future cleanup.

---

## Part 3: Further Cleanup

**Status**: Complete (2026-02-19). All 610 tests pass, zero warnings, zero snapshot changes.

**Line counts (current)**:
- `wrapper.rs`: 343 lines (was 440 after Part 2, 1096 originally)
- `enhanced.rs`: 770 lines (was 884 after Part 2, 700 originally)
- `display.rs`: 510 lines (was 432 after Part 2)
- `ids.rs`: 397 (untouched)
- `mod.rs`: 14 (untouched)

**Total across all 3 parts**: wrapper.rs went from 1096 to 343 lines (69% reduction).
Net across all error files: ~130 fewer lines than after Part 2 (excluding ids.rs/mod.rs).

### Step 1: Remove dead variants -- done

Removed `ImportError`, `HandlebarsError`, `MissingRequiredField` from the enum.
Deleted match arms in `error_id()`, `location()`, `display_with_context()` (error_type,
short_message, guidance, help section), `inline_description()`, `error_span_length()`,
`help_messages()`, and the `missing_required_field()` constructor. -114 lines from enhanced.rs.

### Step 2: Data-driven tag search -- done

Added `TAG_SEARCH_PATTERNS` const and `search_for_tag_line()` to display.rs.
Replaced the 53-line if/else chain in `type_mismatch_error_impl` with a one-liner call.
Adding new tags now requires a single table entry.

### Step 3: Extract `read_source_lines` -- done

Added `read_source_lines()` to display.rs (combines `parse_file_location` + `read_to_string`
+ lines collection). Applied to `variable_not_found_error`, `type_mismatch_error_impl`,
and `cloudformation_validation_error_impl`. Also simplified the CFN validation search loop
from mutable variables to a `find_map` iterator.

`tag_parsing_error` was not converted because it uses `parse_file_location_full` (3-part).
`lookup_query_error` was not converted because it takes `line_number` as a separate parameter.

### Step 4: Extract `find_variable_column` -- done

Added `find_variable_column()` to display.rs. Deduplicated the `{{var}}` / `!$ var` /
`!$var` pattern matching in `variable_not_found_error` -- the known-line path and the
search path both call the same function now.

---

## Part 4: Footer Standardization + ErrorId Precision

**Status**: Complete (2026-02-19). All 607 tests pass, zero warnings, 41 snapshots updated.

### Footer standardization -- done

Changed "For more info, run: iidy explain" to "For more info: iidy explain" in
`render_yaml_syntax()` (3 occurrences) and `render_tag_parsing()` (1 occurrence).
All 6 "For more info" footers in enhanced.rs now use identical wording.

### TagParsing ErrorId precision -- done

Added `error_id: ErrorId` parameter to `tag_parsing_error()` in wrapper.rs.
`missing_required_field_error` passes `MissingRequiredTagField` (no change).
`parser.rs::tag_error` classifies by message content:

| Message pattern | ErrorId | Code |
|---|---|---|
| "is not a valid iidy tag" | UnknownPreprocessingTag | ERR_4001 |
| "missing required" / "missing in" | MissingRequiredTagField | ERR_4002 |
| "must be" / "must have" | InvalidTagFieldValue | ERR_4003 |
| "mutually exclusive" / "invalid format" / "unexpected field" | TagSyntaxError | ERR_4005 |
| fallback | TagSyntaxError | ERR_4005 |

### LookupQueryFailed ErrorId -- done

Added `LookupQueryFailed = 2006` to ErrorId enum (2xxx = variable/scope errors).
Used in `lookup_query_error` instead of `VariableNotFound` (ERR_2001 -> ERR_2006).

### Snapshot impact

41 snapshots updated: 11 typo detection + 30 error example templates.
Changes are errno code updates and footer wording only.
