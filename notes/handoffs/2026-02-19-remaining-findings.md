# Remaining Findings -- To Be Triaged and Split

**Date**: 2026-02-19
**Session**: `42cba5c8-603a-4a7b-90bd-c33096a88fdb`

## Instructions for Next Agent

This document catalogs all remaining stubs, gaps, and tech debt found
during a full-codebase scan. Your task:

1. Read this entire document
2. Propose logical groupings (3-5 handoff documents)
3. Present the groupings to the user for confirmation (use AskUserQuestion)
4. After confirmation, create individual handoff docs in `notes/handoffs/`
   following the format in `notes/handoffs/done/` for reference
5. Delete this file once all handoffs are created

Already has its own handoff (do NOT include):
- cfn import subtypes -> `notes/handoffs/2026-02-19-cfn-import-subtypes.md`

---

## Finding A: `{{lookup}}` Handlebars helper

`src/yaml/handlebars/tests.rs:330-336` -- test early-returns because the
`lookup` helper is not registered. The `lookup` helper does array/object
index access: `{{lookup myArray 0}}`, `{{lookup myObject "key"}}`.
The handlebars crate may provide this as a built-in -- check whether
`handlebars::Handlebars::new()` includes it or if it needs explicit
registration.

## Finding B: `ToggleTimestamps` keyboard command (dead code)

`src/output/keyboard.rs` -- enum variant, keybinding (T), help text all
exist, but the handler does nothing. The help text uses an emoji (lightbulb)
which violates project standards. Either implement timestamp toggling in
the interactive renderer or remove the dead keybinding entirely.

Related: `notes/2026-02-17-code-review-findings.md:110,112`

## Finding C: `render_token_info` empty implementation

`src/output/renderers/interactive.rs:1889-1895` -- silently discards
`TokenInfo` data with `let _ = data` and a TODO about verbosity/debug
flag. Token info (idempotency tokens) is rendered in JSON mode but
invisible in interactive mode. Either render it within the command
metadata section or behind `--debug`.

Related: `notes/2026-02-17-code-review-findings.md:88`

## Finding D: `convert-stack-to-iidy --sortkeys`

`src/cfn/convert_stack_to_iidy.rs` -- the `--sortkeys` flag (defaults to
true) is accepted but ignored. The JS version sorts CFN template keys in
idiomatic order (AWSTemplateFormatVersion first, then Description,
Parameters, Mappings, Conditions, Resources, Outputs). Implementation is
a simple key-weight map applied to the YAML output.

Related: `notes/handoffs/done/2026-02-19-convert-stack-to-iidy-impl.md`
(Chunk 2 still open)

## Finding E: `param` commands -- zero test coverage

All five param commands in `src/params/` (`set`, `get`, `get_by_path`,
`get_history`, `review`) have no unit or integration tests. They make
live AWS calls. Need mock-based or fixture-based offline tests.

Challenge: param commands create their own SSM/KMS clients directly via
`create_ssm_client()` rather than accepting them as parameters. May need
a thin refactor to accept a client parameter for testability.

## Finding F: Property test stubs

`tests/yaml/property_tests.rs` -- seven proptest properties are stubs
that only assert parsing succeeds, never resolve or compare output. They
carry comments "NOTE: Once AST resolution is implemented..." but AST
resolution has been implemented for months. These should be upgraded to
actually test resolution produces valid output.

## Finding G: Snapshot test placeholders

`tests/output/output_renderer_snapshots.rs:458,466,474` -- three snapshot
tests snapshot hardcoded placeholder strings instead of actual renderer
output. Need stdout capture to produce real snapshots.

## Finding H: Commented-out tests in emitter.rs

`src/yaml/emitter.rs:597-642,683-731` -- two test functions with bodies
entirely commented out:
- `test_iidy_vs_js_yaml_quoting_differences` -- tested quoting differences
- `test_yaml_version_quoting` -- tested version string quoting

Either fix and uncomment or delete the dead code.

## Finding I: `.*Initiated` suffix in failure reasons

`src/output/renderers/interactive.rs:1105` -- TODO to remove `.*Initiated`
suffix from stack event failure reasons, matching iidy-js behavior.
One-line fix with a regex or string replace.

## Finding J: Section title handling in render_stack_contents

`src/output/renderers/interactive.rs:1406` -- TODO acknowledging broken
title handling in the stack contents renderer. Need to read surrounding
code to understand the issue.

## Finding K: `disable_rollback: false` hardcoded

`src/output/aws_conversion.rs:286` -- hardcoded `disable_rollback: false`
with a TODO. Should read from the actual stack data.

## Finding L: Tech debt TODOs (low priority, catalog only)

| File | Line | Description |
|------|------|-------------|
| `src/cfn/stack_args.rs` | 25 | `capabilities: Vec<String>` should be enum type |
| `src/cfn/stack_args.rs` | 238 | `apply_global_configuration` should be behind feature flag |
| `src/cfn/template_loader.rs` | 126 | AWS region not threaded through |
| `src/cfn/mod.rs` | 560 | `apply_stack_name_override_and_validate` needs extracting |
| `src/cfn/changeset_operations.rs` | 103,174,214 | Three fns need params-struct refactor |
| `src/yaml/resolution/resolver.rs` | 438,455 | Remove redundant dot-prefixed and single-key query support |
| `src/yaml/resolution/resolver.rs` | 644 | Params-struct refactor |
| `src/yaml/engine.rs` | 273 | Use enhanced error reporting |
| `src/yaml/path_tracker.rs` | 13 | Consider SmallVec optimization |
| `src/main.rs` | 278 | Global color setup cleanup |
| `Cargo.toml` | 53 | serde_yaml deprecated, migrate to serde_yml |
