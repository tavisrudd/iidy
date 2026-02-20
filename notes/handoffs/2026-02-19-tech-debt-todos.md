# Tech Debt TODOs -- Catalog

**Date**: 2026-02-19
**Source**: Finding L from `notes/handoffs/2026-02-19-remaining-findings.md`
**Priority**: Low -- catalog only, address opportunistically

## Items

| File | Line | Description | Category |
|------|------|-------------|----------|
| `src/cfn/stack_args.rs` | 25 | `capabilities: Vec<String>` should be enum type | Type safety |
| `src/cfn/stack_args.rs` | 238 | `apply_global_configuration` should be behind feature flag | Config |
| `src/cfn/template_loader.rs` | 126 | AWS region not threaded through | Plumbing |
| `src/cfn/mod.rs` | 560 | `apply_stack_name_override_and_validate` needs extracting | Refactor |
| `src/cfn/changeset_operations.rs` | 103,174,214 | Three fns need params-struct refactor | Refactor |
| `src/yaml/resolution/resolver.rs` | 438,455 | Remove redundant dot-prefixed and single-key query support | Cleanup |
| `src/yaml/resolution/resolver.rs` | 644 | Params-struct refactor | Refactor |
| `src/yaml/engine.rs` | 273 | Use enhanced error reporting | Error UX |
| `src/yaml/path_tracker.rs` | 13 | Consider SmallVec optimization | Perf |
| `src/main.rs` | 278 | Global color setup cleanup | Cleanup |
| `Cargo.toml` | 53 | serde_yaml deprecated, migrate to serde_yml | Dependency |

## Notes

- The `serde_yaml` -> `serde_yml` migration is the largest item. serde_yaml
  is unmaintained; serde_yml is the maintained fork. This affects the entire
  codebase (~100+ import sites).
- The params-struct refactors (changeset_operations, resolver) reduce function
  argument counts from 5-7 args to a single struct.
- The resolver dot-prefix/single-key cleanup removes legacy query syntax that
  was kept for backwards compatibility but is no longer needed.
