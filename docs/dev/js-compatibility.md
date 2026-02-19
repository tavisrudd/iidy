# Behavioral Differences from iidy-js

This document lists behavioral differences between the Rust implementation
and the original iidy-js. Many differences are intentional improvements.
Others are features not yet ported.

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

## Not yet implemented

### `!$string` alias

- **JS**: `!$string` is an alias for `!$toYamlString`.
- **Rust**: Not registered. Using `!$string` produces a parse error.
- **Status**: Simple fix (register alias in parser).

### `!$expand`

- **JS**: `!$expand` is a preprocessing tag that takes `{template, params}`,
  looks up a named template from the environment, validates parameters
  against the template's `$params`, and expands inline. Unlike custom
  resource types, it does not perform name-prefixing or ref rewriting --
  it is a simple "inline this template with these parameters" operation.
- **Rust**: Not implemented.
- **Status**: The underlying machinery (`$params` validation, template
  lookup) exists in the custom resource template system. Implementing
  `!$expand` would reuse that but skip the CFN-specific transforms.

### `$envValues` custom values

- **JS**: Users can define custom `$envValues` entries.
- **Rust**: Runtime values (`iidy.command`, `iidy.environment`,
  `iidy.region`, `iidy.profile`) are injected and accessible, but
  user-defined custom values are not supported.
- **Status**: The core runtime injection works. Custom user-defined values
  are not yet implemented.

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
