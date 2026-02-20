# Behavioral Differences from iidy-js

This document lists behavioral differences between the Rust implementation
and the original iidy-js. Many differences are intentional improvements.

## Resolved differences

These were identified during development and fixed to match JS behavior:

- **`!$groupBy` field names**: JS uses `items`/`var`/`key`/`template`.
  Rust originally used `source`/`var_name`. Fixed to match JS.

- **`!$mergeMap`, `!$concatMap`, `!$mapListToHash` format**: JS delegates
  these to `!$map` internally, so they accept the same
  `{items, template, var, filter}` format. Rust originally used separate
  formats. Fixed to match JS delegation pattern.

- **`!$split` format**: JS uses `!$split [delimiter, string]` (array).
  Rust originally used object format `{string, delimiter}`. Fixed to use
  the same array format as JS.

- **`!$let` format**: JS uses flat format `{var1: value1, in: expression}`
  where all keys except `in` are bindings. Rust originally used nested
  `{bindings: ..., expression: ...}`. Fixed to use the same flat format
  as JS.

- **Custom resource templates**: JS has a full system for defining
  reusable CloudFormation resource templates (`$params` validation,
  automatic name-prefixing, `!Ref`/`!GetAtt`/`!Sub` rewriting,
  `GlobalAccumulator` for section promotion, `$global` flag,
  `$globalRefs`, `Overrides`). Rust now implements this in
  `src/yaml/custom_resources/`.

- **`!$string` alias**: JS supports `!$string` as an alias for
  `!$toYamlString`. Rust now registers it as a deprecated alias.

- **`!$expand`**: JS supports `!$expand` for inline template expansion
  (`{template, params}` lookup and `$params` validation without
  CFN-specific name-prefixing or ref rewriting). Now implemented in Rust.

## Not yet implemented

### `param` subcommands

- **JS**: 5 subcommands for AWS SSM Parameter Store (`set`, `review`,
  `get`, `get-by-path`, `get-history`) with KMS alias resolution,
  approval workflow (`.pending` suffix), tag management, and multiple
  output formats (`simple`, `json`, `yaml`).
- **Rust**: CLI definitions and arg structs exist in `src/cli.rs`. Handlers
  are stubs (`println!` only). No `src/params/` module yet.
- **Status**: See `notes/2026-02-19-param-commands-handoff.md` for full
  implementation plan.

## Intentionally removed

- **`list-stack-instances`**: Supported in JS but removed from this version.

## Carried-forward JS behaviors

- **`template-approval review` defaults to us-east-1**: When no `--region`
  is specified, the review command defaults to `us-east-1`. This matches
  JS behavior (`approval/index.ts:78`). Should be parameterized in the
  future.

## Intentional improvements over iidy-js

- **`$defs` let* semantics**: JS copies `$defs` values raw (effectively
  parallel -- definitions cannot reference each other). Rust resolves
  sequentially with let* semantics, so each definition can reference prior
  definitions. Strictly more powerful and backward-compatible.
- **Error messages**: Rust provides source-location-aware errors with line
  numbers and context snippets. JS errors are less precise.
- **Tree-sitter parser**: Rust uses tree-sitter for parsing instead of the
  `js-yaml` library, enabling better error recovery and source location
  tracking.
- **Handlebars helper set**: Curated set of ~25 helpers covering the
  helpers actually used in practice, rather than importing an entire
  third-party helper library (`handlebars-helpers` npm). Additional
  helpers can be added as needed.
