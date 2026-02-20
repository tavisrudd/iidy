# Remaining Findings -- Triage Complete

**Date**: 2026-02-19

## Summary

Triaged 12 findings from full-codebase scan. Results:

### Resolved in this session (9 findings)

- **A** (`{{lookup}}` helper): Already implemented. Stale test early-return removed.
- **B** (`ToggleTimestamps`): Already removed from codebase.
- **C** (`render_token_info`): Intentional -- comment updated.
- **D** (`--sortkeys`): Implemented with deep recursive CFN key sorting, 5 tests.
- **F** (Property test stubs): Upgraded 7 proptests to call `resolve_ast()`.
- **H** (Commented-out tests): Deleted 95 lines of dead code.
- **I** (`.*Initiated` suffix): Fixed to match iidy-js regex behavior.
- **J** (Section title TODO): Stale TODO removed.
- **K** (`disable_rollback`): Already reads from stack data. Stale finding.

### Split into separate handoffs (3 findings)

- **E** (param test coverage): `notes/handoffs/2026-02-19-param-commands-test-coverage.md`
- **G** (renderer output capture): `notes/handoffs/2026-02-19-renderer-output-capture.md`
- **L** (tech debt TODOs): `notes/handoffs/2026-02-19-tech-debt-todos.md`
