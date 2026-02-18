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

### 1a. Parser: silent tag error dropping

**File**: `src/yaml/parsing/parser.rs:969-975`
**Problem**: `build_tagged_node` catches errors from `parse_preprocessing_tag`
and silently converts them to `UnknownYamlTag`. Typos like `!$mapp` are
silently ignored. The `!$expand` tag (needed for custom resource templates)
will have this same problem.
**Fix**: Propagate the error. `build_block_node` and `build_flow_node` already
do this correctly -- match their behavior.
**Risk**: This may cause previously-passing tests to fail if any tests rely on
malformed tags being silently accepted. Check carefully.

### 1b. Parser: inconsistent unknown field handling

**Files**: `parser.rs:1388` (`parse_map_values_tag`), `parser.rs:1601-1635`
(`parse_if_tag`)
**Problem**: These tag parsers silently ignore unknown fields. `parse_map_tag`
correctly rejects unknown fields via `validate_tag_fields`. Misspelled keys
like `itesm:` are silently dropped.
**Fix**: Add `validate_tag_fields` calls to `parse_map_values_tag` and
`parse_if_tag`, matching `parse_map_tag`'s behavior.

### 1c. Parser: unwrap panic on non-Mapping input

**File**: `parser.rs:1530-1534`
**Problem**: `parse_map_list_to_hash_tag` calls `extract_field_from_mapping(...).unwrap()`
after `validate_tag_fields`. But `validate_tag_fields` silently passes non-Mapping
content. `!$mapListToHash someString` will panic.
**Fix**: Add a Mapping guard before the unwraps, return a proper error.

### 1d. Engine: process_imported_document round-trip

**Files**: `src/yaml/engine.rs:368-397`
**Problem**: Re-serializes Value to string then re-parses. Destroys source
locations, loses comments, creates new import loader that lacks parent's
AWS config. Nested documents with S3/SSM imports silently fail.
**Fix**: This is the hardest fix. Two options:
- **Quick**: Pass the parent's import loader (or its AWS config) through to
  the child preprocessor. This fixes the immediate AWS config loss.
- **Proper**: Avoid the round-trip entirely by operating on the AST/Value
  directly. This is a larger change but eliminates the source location loss.

Start with the quick fix. The proper fix can come later or during the
custom resource template work.

### 1e. Engine: tag classification duplication

**File**: `parser.rs:810-862,864-940`
**Problem**: `build_block_node` and `build_flow_node` have ~30 lines of
copy-pasted tag classification logic.
**Fix**: Extract a shared `classify_tag(tag_name, content, meta) -> ParseResult<YamlAst>`
helper. Both functions call it.

---

## 2. Handlebars Registry Performance

**File**: `src/yaml/handlebars/engine.rs:76`
**Problem**: `create_handlebars_registry()` called on every
`interpolate_handlebars_string` invocation. 100 interpolations = 100
registry allocations with ~20 boxed helpers each. Custom resource template
expansion will amplify this significantly.
**Fix**: Use `std::sync::OnceLock<handlebars::Handlebars<'static>>` at
module level. Initialize once, reuse. The registry is stateless (helpers
are pure functions of their arguments), so sharing is safe.
**Test**: The existing tests should pass unchanged. Consider adding a
benchmark if one doesn't exist -- the improvement should be measurable.

---

## 3. Dead Test Cleanup

These tests add noise and false confidence. Delete or fix them.

### 3a. Delete assertion-free debug tests

These files contain only `println!` calls with no assertions. Delete the
entire files:
- `tests/nested_import_debug.rs`
- `tests/tree_sitter_debug.rs`
- `tests/tree_sitter_tag_debug.rs`

Check if `tree_sitter_path_debug.rs` and `tree_sitter_array_debug.rs` follow
the same pattern -- if so, delete those too.

Remove corresponding entries from any test configuration.

### 3a-followup. Consolidate integration test files to reduce link pressure

33 integration test files = 33 separate binaries, each linking the full crate
with all AWS SDK deps (~200MB+ each). On a 24-core/27GB machine this causes
OOM during parallel linking (even with mold). Currently mitigated by
`jobs = 12` in `.cargo/config.toml`, but the real fix is fewer test binaries.
Group related integration tests into fewer files (e.g. all tree_sitter_* into
one, all yaml_* into one, all output_* into one). This also speeds up clean
builds significantly since each binary pays the full link cost.

### 3b. Delete tautology tests

- `tests/property_tests.rs:325` -- `assert!(!x.is_empty() || x.is_empty())`
- `tests/property_tests.rs:248` -- `assert!(true)`
- `tests/output_renderer_snapshots.rs:458,466,474` -- snapshot hardcoded
  placeholder strings
- `tests/keyboard.rs:264` -- `assert!(is_tty == true || is_tty == false)`
- `tests/json.rs:371` -- `assert!(true)`

### 3c. Mark stub tests as ignored with clear TODO

These tests were written as stubs early in development and never completed:
- `tests/yaml_tests.rs:20,64,112,190,252` -- parse-only assertions
- `tests/property_tests.rs:57,76` -- proptest stubs

Rather than deleting (they document intended coverage), add `#[ignore]` with
a comment: `// TODO: Complete -- currently only asserts parse succeeds, not
resolution output`.

### 3d. Remove leftover debug output

- `tests/equivalence_tests.rs:84` -- `eprint!` dumping test cases
- `tests/yaml_preprocessing_integration.rs:311,319` -- `println!("DEBUG: ...")`

---

## 4. Emoji Violations

Quick grep-and-replace. At least 12 instances across production code.

**Files to check** (non-exhaustive):
- `src/output/renderers/interactive.rs` -- lines 1035, 1257-1285, 1313-1317,
  1889, 1910-1922, 1930
- `src/output/keyboard.rs` -- lines 145, 233
- Any test files with emoji in `println!` output

**Replacements**:
- Lock emoji -> `[protected]` or `*`
- Thumbsup -> `OK` or `Success`
- Table flip -> `Failure` (plain text)
- Warning triangle -> `[!]` or `WARNING`
- Check/cross marks -> `[ok]`/`[x]` or `+`/`-`
- Clipboard -> remove entirely

Match the ASCII style used elsewhere in the codebase. The interactive
renderer already has ASCII-only code paths for some features.

---

## 5. Resolver Duplication (if time permits)

These are not blockers but reduce maintenance burden before the custom
resource template work touches the resolver.

### 5a. Truthiness duplication

**File**: `resolver.rs:1032-1040, 1207-1215`
**Problem**: `resolve_if` and `resolve_not` reimplement truthiness inline
instead of calling `self.is_truthy()`.
**Fix**: Replace inline match blocks with `self.is_truthy(value)` calls.

### 5b. ConcatMap/MergeMap cloning

**File**: `resolver.rs:1384-1426, 1429-1483`
**Problem**: Clone all fields from `ConcatMapTag`/`MergeMapTag` to construct
a temporary `MapTag`.
**Fix**: Extract the shared fields into a trait or accept the fields directly
rather than requiring a `MapTag` struct.

### 5c. Error handling string parsing

**File**: `resolver.rs:520-576`
**Problem**: Parses handlebars error messages by string searching. Brittle.
**Fix**: Check if the handlebars crate exposes structured error types. If so,
match on those. If not, at minimum extract the parsing into a helper function
and add a test that will fail if the error format changes.

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
