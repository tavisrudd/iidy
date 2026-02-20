# Output Renderer Stdout Capture -- Handoff

**Date**: 2026-02-19
**Source**: Finding G from `notes/handoffs/2026-02-19-remaining-findings.md`
**Estimated windows**: 3

## Context

The output renderer tests in `tests/output/renderer_snapshots.rs` and
`tests/output/pixel_perfect.rs` are smoke tests that verify the renderer
doesn't crash, but they never capture or snapshot the actual output.
The renderer writes directly to stdout via `println!()` so there's no
way to capture output for testing without refactoring.

Capture infrastructure already exists in `tests/output/capture_utils.rs`:
- `OutputCapture` struct implementing `Write` trait
- `get_output_plain()` for ANSI-stripped comparison
- `RendererTestUtils::normalize_output()` for timestamp/ARN normalization
- `RendererTestUtils::extract_color_map()` for color validation

None of this can be used because `InteractiveRenderer` hardcodes `println!()`.

---

## Preliminary: Split interactive.rs (separate commit)

`interactive.rs` is 2438 lines with 94 print calls. Split into a module
directory first to make the mechanical println! replacement tractable.
Three natural boundaries at the existing `impl` blocks:

```
src/output/renderers/interactive.rs  (2438 lines, 94 print calls)
  =>
src/output/renderers/interactive/
  mod.rs       (~430 lines,  6 print calls)  -- struct, options, constructors, helpers, formatting, spinner/timing, trait impl
  ordering.rs  (~580 lines,  2 print calls)  -- operation setup, section config, ordering state machine
  render.rs    (~1390 lines, 86 print calls) -- all render_* methods
```

Why this grouping:
- `mod.rs` owns the struct, fields, constructors, trait impl, and all
  pure-formatting helpers. It's the only file that changes for the writer
  field addition.
- `ordering.rs` is section-ordering state machine logic. Almost no IO.
- `render.rs` is the bulk of the mechanical work -- 86 of 94 print calls.
  Isolating it means window 2 focuses entirely on one file with one pattern.

Rust module mechanics: child modules (`ordering.rs`, `render.rs`) can access
private fields of `InteractiveRenderer` because private items are visible
to descendant modules. Each file adds `impl InteractiveRenderer { ... }`
blocks and imports from `super::*`.

The `renderers/mod.rs` needs no changes -- Rust auto-resolves `pub mod interactive;`
to `interactive/mod.rs` when the directory exists.

### Split contents

**`mod.rs`** (lines 1-460 of current file):
- Imports, constants, type aliases
- `InteractiveOptions` struct + impls
- `InteractiveRenderer` struct definition
- First `impl InteractiveRenderer` block (lines 98-431): constructor, all
  format_*/print_*/colorize_*/pretty_format_*/calc_padding/color_by_*/
  style_*/create_api_spinner/timing methods
- `impl OutputRenderer for InteractiveRenderer` block (lines 433-460)
- Add `mod ordering;` and `mod render;` declarations

**`ordering.rs`** (lines 462-1045 of current file):
- Second `impl InteractiveRenderer` block: setup_operation,
  configure_section_titles, start_next_section, get_section_title,
  section_is_always_multiline, render_with_ordering,
  advance_through_ready_sections, render_section,
  start_next_section_if_exists, flush_buffered_live_events,
  handle_live_events_data, handle_non_section_data,
  has_section_already_started, get_section_key, cleanup_operation,
  render_data_immediately

**`render.rs`** (lines 1047-2438 of current file):
- Third `impl InteractiveRenderer` block: render_single_stack_event,
  render_command_metadata, render_stack_definition, render_stack_events,
  render_stack_contents, render_status_update, render_command_result,
  render_final_command_summary, render_stack_list, render_changeset_result,
  render_stack_drift, render_error, render_token_info,
  render_new_stack_events, handle_operation_complete,
  handle_inactivity_timeout, advance_to_next_section,
  render_confirmation_prompt, post_confirmation_execute_changeset,
  render_stack_absent_info, render_stack_change_details,
  render_changeset_change, render_cost_estimate, render_stack_template,
  render_stack_absent_error_with_context, render_approval_request_result,
  render_template_validation, render_approval_status,
  render_template_diff, render_approval_result

### Exit criteria
- `make check-fast` clean, `make test` green
- Commit: "Split interactive renderer into module directory"

---

## Window 1: Writer injection + mod.rs/ordering.rs conversion

**Goal**: Add writer field, constructors, and replace the 8 print calls
in mod.rs + ordering.rs. The small files are done; render.rs is untouched.

### 1a. Add writer field to struct

```rust
writer: Box<dyn Write + Send + Sync>,
```

Type must be `Send + Sync` because `OutputRenderer: Send + Sync`.
Both `io::Stdout` and `OutputCapture` (`Arc<Mutex<Vec<u8>>>`) qualify.

### 1b. Create constructors

```rust
pub fn new(options: InteractiveOptions) -> Self {
    Self::new_with_writer(options, Box::new(io::stdout()))
}

pub fn new_with_writer(
    options: InteractiveOptions,
    writer: Box<dyn Write + Send + Sync>,
) -> Self {
    // ... current new() body with writer field added ...
}
```

### 1c. Change 2 `&self` methods in mod.rs to `&mut self`

- `print_section_entry` -- called from `&mut self` methods only
- `create_api_spinner` -- called from `&mut self` methods only

### 1d. Replace 6 print calls in mod.rs

`print_section_heading`, `print_section_heading_with_newline`,
`add_content_spacing`, `print_section_entry`, `create_api_spinner`

### 1e. Replace 2 print calls in ordering.rs

Both in `start_next_section`.

### 1f. Replace `io::stdout().flush()` calls

Line 192 (`print_section_entry`) and line 442 (`cleanup`) become
`self.writer.flush()`. Line 2004 is in render.rs -- leave for window 2.

### 1g. Leave unchanged

- `eprintln!` in render.rs (stderr)
- `io::stdout().is_terminal()` (spinner creation)
- `io::stdin().read_line()` (input)
- All 86 print calls in render.rs

### Error handling

- Methods returning `Result<()>`: use `writeln!(self.writer, ...)?`
- Methods returning `()`: use `.unwrap()` (matches `println!` panic behavior)

### Exit criteria

- `make check-fast` clean, `make test` green
- mod.rs and ordering.rs have zero println!/print! calls
- render.rs still has 86 -- that's window 2's job
- Commit: "Add writer injection to InteractiveRenderer"

---

## Window 2: Replace 86 print calls in render.rs

**Goal**: Convert all remaining println!/print! in render.rs.

### 2a. Change 2 `&self` methods in render.rs to `&mut self`

- `render_single_stack_event`
- `render_changeset_change`

### 2b. Mechanical replacement

86 `println!()` -> `writeln!(self.writer, ...)`,
`print!()` -> `write!(self.writer, ...)`.

All render_* methods return `Result<()>`, so use `?`.
`render_single_stack_event` returns `()` -- use `.unwrap()`.

Replace remaining `io::stdout().flush()` (line 2004 in
`render_confirmation_prompt`) with `self.writer.flush()`.

### 2c. Verify

- `make check-fast` clean
- `make test` fully green
- Zero println!/print! in entire `interactive/` directory (only `eprintln!` remains)
- Commit: "Replace all println! in interactive renderer with writer"

---

## Window 3: Test wiring + snapshot generation

**Goal**: Wire `OutputCapture` into all renderer tests, convert smoke
tests to snapshot tests.

### 3a. Add capture helpers

**File**: `tests/output/renderer_snapshots.rs`

`OutputCapture` uses `Arc<Mutex<Vec<u8>>>` and derives `Clone`. Cloning
gives a second handle to the same buffer:
```rust
let capture = OutputCapture::new();
let mut renderer = InteractiveRenderer::new_with_writer(
    options,
    Box::new(capture.clone()),
);
// ... render ...
let output = capture.get_output_plain(); // reads from same shared buffer
```

### 3b. Convert 9 smoke tests in renderer_snapshots.rs

Replace data-structure assertions with:
1. `capture.get_output_plain()`
2. `RendererTestUtils::normalize_output()`
3. `assert_snapshot!()`

### 3c. Convert 4 pixel_perfect tests

**File**: `tests/output/pixel_perfect.rs` -- same pattern.

Keep unchanged: `test_fixture_expected_output_completeness`,
`test_formatting_constants_match_iidy_js`, `test_renderer_format_snapshot`

### 3d. Generate and verify snapshots

1. `make check-fast`
2. `INSTA_FORCE_PASS=1 cargo test --test output_renderer_snapshots --test output_pixel_perfect`
3. Read each `.snap.new`, verify correctness
4. Ask user to accept snapshots
5. `make test` -- full green
6. Commit: "Add output capture snapshots to renderer tests"

---

## Design Decisions

### Why `Box<dyn Write + Send + Sync>` not `Arc<Mutex<dyn Write>>`
- Simpler type, no unnecessary mutex overhead in production
- The renderer already has exclusive (`&mut self`) access in all write paths
- `Sync` required because `OutputRenderer: Send + Sync`

### Why `.unwrap()` in non-Result methods instead of changing signatures
- `println!()` already panics on write failure -- preserves behavior
- Changing helper methods to return `Result` would cascade for no benefit

### OutputCapture clone-sharing pattern
- `OutputCapture` uses `Arc<Mutex<Vec<u8>>>` internally
- `Clone` produces a second handle to same buffer (Arc semantics)
- Renderer writes through one handle, test reads through the other

### Why normalize output before snapshotting
- Timestamps from `Utc::now()` change every run
- `RendererTestUtils::normalize_output()` replaces timestamps, ARNs, durations
  with fixed placeholders

---

## Scope Summary

| Item | File | Count |
|------|------|-------|
| print calls in mod.rs | interactive/mod.rs | 6 |
| print calls in ordering.rs | interactive/ordering.rs | 2 |
| print calls in render.rs | interactive/render.rs | 86 |
| `io::stdout().flush()` to replace | mod.rs + render.rs | 3 |
| `eprintln!` to keep | render.rs | 1 |
| `&self` -> `&mut self` | mod.rs (2) + render.rs (2) | 4 |
| Tests to add snapshots | renderer_snapshots + pixel_perfect | ~13 |

## Progress

- [ ] Preliminary: Split interactive.rs into module directory (commit)
- [ ] Window 1: Writer field + 8 print calls in mod.rs/ordering.rs (commit)
- [ ] Window 2: 86 print calls in render.rs (commit)
- [ ] Window 3: Test wiring + snapshots (commit)

## References

- `tests/output/capture_utils.rs` -- existing capture infrastructure
- `tests/output/pixel_perfect.rs` -- fixture-based tests to upgrade
- `tests/output/renderer_snapshots.rs` -- smoke tests to upgrade
- `src/output/renderers/interactive.rs` -- 2438-line file to split
- `src/output/renderers/json.rs` -- future stretch (5 print calls)
