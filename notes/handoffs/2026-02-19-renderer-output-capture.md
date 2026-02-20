# Output Renderer Stdout Capture -- Handoff

**Date**: 2026-02-19
**Source**: Finding G from `notes/handoffs/2026-02-19-remaining-findings.md`

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

## What Needs to Change

### Chunk 1: Add writer injection to InteractiveRenderer

**Files**: `src/output/renderers/interactive.rs`

1. Add a `writer: Box<dyn Write + Send>` field to `InteractiveRenderer`
2. Default to `Box::new(std::io::stdout())` in `new()`
3. Add `new_with_writer(options, writer)` constructor (or builder pattern)
4. Replace all `println!()` calls with `writeln!(self.writer, ...)`
5. Replace all `print!()` calls with `write!(self.writer, ...)`

Note: There are many `println!()` calls throughout the file. A global
search-replace is feasible since the renderer is the only code using
these prints. The method signatures don't need to change -- `&mut self`
already provides mutable access.

The spinner code in `src/output/renderers/spinner.rs` also uses
`print!()`/`println!()` and would need the same treatment if it's used
during testing, but spinners are disabled in tests (`enable_spinners: false`).

### Chunk 2: Wire up OutputCapture in tests

**Files**: `tests/output/renderer_snapshots.rs`, `tests/output/pixel_perfect.rs`

1. Create renderer with `InteractiveRenderer::new_with_writer(options, capture.clone())`
2. After rendering, call `capture.get_output_plain()`
3. Use `RendererTestUtils::normalize_output()` to strip timestamps/ARNs
4. Snapshot the normalized output with `assert_snapshot!()`

### Chunk 3: Add pixel-perfect snapshot tests

**Files**: `tests/output/pixel_perfect.rs`

Use the fixture data already loaded in `test_interactive_command_metadata_pixel_perfect`
etc. to capture and snapshot actual renderer output against the expected
output from fixtures.

## Scope

- `InteractiveRenderer` has ~50-60 `println!()` calls to convert
- The `write!()` macro returns `Result`, so error handling needs consideration
  (probably just `.expect()` or propagate via `?` since the tests use `Result`)
- JSON renderer (`src/output/renderers/json.rs`) also uses `println!()` and
  could benefit from the same refactor, but is lower priority

## References

- `tests/output/capture_utils.rs` -- existing capture infrastructure
- `tests/output/pixel_perfect.rs` -- test that loads fixtures and renders
- `tests/output/renderer_snapshots.rs` -- smoke tests to upgrade
- `src/output/renderers/interactive.rs` -- renderer to modify
