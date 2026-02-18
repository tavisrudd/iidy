# Handoff: Code Review Fix-Up

**Date**: 2026-02-17
**For**: Next Claude instance fixing issues from the code review
**Prerequisite reading**: `notes/2026-02-17-code-review-findings.md`

## Goal

Fix the issues identified in the code review that are on the critical path
for the custom resource template feature. Do NOT start the custom resource
template work itself -- that has its own RFC and handoff. This is cleanup
to prepare a solid foundation.

## Approach

Use sub-agents to parallelize independent fix batches. The multi-round
review approach from `../ssh-agent-guard/docs/` works well: draft changes,
review them with a sub-agent, iterate. For code changes, `make check` and
`make test` must pass at each commit boundary.

Work in this order. Each numbered section is one commit-sized unit.

---

## 1. YAML Engine Blockers (fix before custom resource templates)

These directly impact the upcoming feature.

### 1a. Parser: silent tag error dropping -- DONE (655b966)

Extracted `classify_tag` method shared by all three callers (`build_block_node`,
`build_flow_node`, `build_tagged_node`). Errors now propagate via `?` instead
of being silently converted to `UnknownYamlTag`. Added flow-style typo error
template (`example-templates/errors/unknown-tag-typo-flow.yaml`) to cover the
`build_tagged_node` code path. No test regressions.

### 1b. Parser: inconsistent unknown field handling -- DONE (0a4815b)

Added `validate_tag_fields` calls to `parse_map_values_tag` and `parse_if_tag`.
Fixed error location to use the unexpected field's own SrcMeta. Added error
example templates with snapshot tests.

### 1c. Parser: unwrap panic on non-Mapping input -- DONE (0a4815b)

Added mapping guard to `parse_map_list_to_hash_tag`. Returns proper error
instead of panicking on non-mapping input.

### 1d. Engine: process_imported_document cleanup -- DONE

Removed the unnecessary temp `YamlPreprocessor` + bare `ProductionImportLoader::new()`
in `process_imported_document`. The temp preprocessor existed as a workaround for a
mutable borrow conflict when `resolve_ast_with_context` used `self.split_args_resolver`,
but that was later simplified to call the standalone `resolve_ast()` -- making the
workaround dead code. Now calls `resolve_ast()` directly.

Also removed the `resolve_ast_with_context` wrapper method itself (sole remaining caller
in `process()` inlined to call `resolve_ast()` directly), and a stale `#[ignore]`d
placeholder test in `tests/yaml_tests.rs` that referenced it.

Note: the Value->string->AST round-trip in `process_imported_document` remains. The
AWS config propagation issue described in the original writeup was overstated -- Phase 1
import loading already uses the parent's loader correctly. The round-trip is wasteful
but not a correctness bug. The proper fix (operating on AST/Value directly) can come
during the custom resource template work.

### 1e. Engine: tag classification duplication -- DONE (655b966)

Extracted `classify_tag(&self, tag_name, tagged_content, meta) -> ParseResult<YamlAst>`.
All three callers (`build_block_node`, `build_flow_node`, `build_tagged_node`) now
use it. Net -40 lines. Done as part of the 1a fix.

---

## 2. Handlebars Registry Performance -- DONE (655b966)

Replaced `create_handlebars_registry()` with `static REGISTRY: OnceLock<Handlebars<'static>>`
and `get_registry()`. Initialized once on first use, shared across all calls.
All existing tests pass unchanged.

---

## 3. Dead Test Cleanup -- DONE (c65422b)

Deleted debug-only test files, tautology tests, keyboard switching tests,
and leftover debug output. Test count went from 608 to 576.

### 3a-followup. Consolidate integration test files to reduce link pressure -- DONE

Consolidated 27 integration test files into 5 test binaries (7 total
including lib + bin doc tests). Used `tests/<group>/main.rs` coordinator
pattern (not `tests/<group>.rs`) because crate root module resolution
looks in the parent directory, not a child directory named after the crate.

| What | Before | After |
|---|---|---|
| Test binaries | 29 | 7 |
| Integration test binaries | 27 | 5 |
| Tests listed | 575 | 570 |
| Tests passing | 575 | 569 run + 3 skipped |
| Warnings | 0 | 0 |

The 5-test drop is from `output_capture_utils.rs` tests being double-counted:
it was both a standalone binary AND imported via dead `mod output_capture_utils;`
in `output_renderer_snapshots.rs`. No tests were actually lost.

**Structure:**
- `tests/yaml/main.rs` -- coordinator for 15 yaml test modules
- `tests/output/main.rs` -- coordinator for 9 output test modules
- `tests/error_examples_snapshots.rs` -- standalone (auto-discovery, 46 snapshots)
- `tests/example_templates_snapshots.rs` -- standalone (auto-discovery, 42 snapshots)
- `tests/template_loading_integration_tests.rs` -- standalone (sole CFN test)

**Also cleaned up:**
- Deleted dead `tests/yaml_preprocessing/` directory (5 files, never compiled)
- Removed dead `mod output_capture_utils;` import from renderer snapshots
- Removed unused `get_load_contexts` method surfaced by consolidation
- Moved and renamed 13 insta snapshot files to match new module paths

---

## 4. Emoji Cleanup -- DONE (partial)

Removed emojis from comments and doc comments in:
- `src/main.rs` (comment)
- `src/yaml/imports/mod.rs` (doc comments)
- `src/yaml/imports/loaders/git.rs` (comment + joke)

Deleted `src/yaml/parsing/multiple_if_error_position_tests.rs` (debug session
tests with no real assertions -- error position already tested in
`position_error_tests.rs`).

Removed `src/pocs/` directory entirely along with its `[[bin]]` target in
Cargo.toml and `pub mod pocs` in lib.rs.

Fixed 3 unnecessary-braces warnings in `get_stack_instances.rs`,
`watch_stack.rs`, `ast.rs`. Zero warnings remaining.

**Left intentionally**:
- `src/output/` emojis (intentional UI: status icons, spinner, interactive renderer)
- `src/docs/errors/*.md` emojis (user-facing markers in error help docs)
- `src/yaml/parsing/test.rs` emoji in YAML fixture (testing unicode handling)

---

## 5. Resolver Duplication (if time permits)

These are not blockers but reduce maintenance burden before the custom
resource template work touches the resolver.

### 5a. Truthiness duplication -- DONE

Replaced inline truthiness match blocks in `resolve_if` and `resolve_not`
with calls to the existing `self.is_truthy()` method.

### 5b. ConcatMap/MergeMap cloning -- DONE

Extracted `resolve_map_items` helper on `impl Resolver` that accepts fields
directly (items, template, var, filter, tag_name). `resolve_map` delegates
to it; `resolve_concat_map` and `resolve_merge_map` call it directly,
eliminating the temporary `MapTag` construction and field cloning.

### 5c. Error handling string parsing -- DONE

Extracted `parse_variable_name_from_handlebars_error` and
`find_template_variable_location` helpers. Fixed nonsensical duplicated
`context.input_uri` extraction (else branch checked the same Option that
was already None). Moved local `use` import to module level. The handlebars
crate's `RenderError` has a public `desc` field but `interpolate_handlebars_string`
wraps it in `anyhow!()` losing the type; changing that would be a larger
refactor for marginal gain since the string format is stable (`strict_error`
in handlebars uses `{:?}` formatting).

---

## What NOT to Fix Now

These are real issues but not on the critical path:

- CFN handler inconsistencies (3 handlers bypassing macros) -- fix during a
  dedicated CFN cleanup pass
- Output manager buffer overflow -- fix when implementing proper TUI
- AWS conversion hardcoded fields -- fix when working on output accuracy
- Blocking I/O on async runtime -- low risk at current scale
- `unsafe set_var` in aws/mod.rs -- needs careful analysis of init ordering
- Concurrency issues in keyboard.rs -- fix when implementing proper event loop

## Verification

After all fixes:
- `make check` passes with zero warnings
- `make test` passes at 100% (test count will decrease from deleted tests)
- No new snapshot changes (unless emoji removal causes snapshot updates,
  in which case review and accept them)
