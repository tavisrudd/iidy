# Architecture

iidy is a CloudFormation deployment tool with a YAML preprocessing language.
The Rust implementation rewrites the original iidy-js while maintaining
behavioral compatibility.

## Pipeline overview

A typical CloudFormation operation flows through four stages:

```
CLI (src/cli.rs)
  -> Stack-args loading (src/cfn/stack_args.rs)
    -> YAML preprocessing (src/yaml/engine.rs)
      -> CFN operations (src/cfn/*.rs)
        -> Output rendering (src/output/)
```

1. **CLI parsing**: Clap derives 20+ subcommands from `src/cli.rs`. Global
   options (`--environment`, `--region`, `--profile`) are available to all
   commands. `src/main.rs` dispatches on the `Commands` enum.

2. **Stack-args loading**: Commands that operate on stacks load a
   `stack-args.yaml` file containing stack configuration (`StackName`,
   `Template`, `Region`, `Parameters`, `Tags`, etc.). Environment maps
   allow per-environment overrides. See [aws-config.md](aws-config.md)
   for the full resolution chain.

3. **YAML preprocessing**: The stack-args file and the CloudFormation
   template are both preprocessed through the YAML engine, resolving
   `$imports`, `$defs`, preprocessing tags (`!$map`, `!$if`, etc.), and
   Handlebars `{{ }}` expressions. See [yaml-preprocessing.md](../yaml-preprocessing.md).

4. **CFN operations**: AWS API calls are orchestrated by command handlers
   in `src/cfn/`. Each handler emits `OutputData` variants to a
   `DynamicOutputManager`.

5. **Output rendering**: The output manager dispatches data to the active
   renderer (Interactive, Plain, or JSON). See
   [output-architecture.md](output-architecture.md).

## YAML preprocessing engine

The preprocessing engine in `src/yaml/` is the most complex subsystem.
It implements a domain-specific language embedded in YAML tags.

### Two-phase pipeline

Orchestrated by `src/yaml/engine.rs`:

**Phase 1** (`load_imports_and_defs`): Parse the document with the
tree-sitter parser (`src/yaml/parsing/parser.rs`), extract `$defs` and
`$imports` from the document header, resolve `$defs` sequentially (each definition can reference prior
definitions; see [js-compatibility.md](js-compatibility.md) for how this
differs from JS), load imports with
Handlebars interpolation on paths, and recursively preprocess imported
documents.

**Phase 2** (`src/yaml/resolution/resolver.rs`): Walk the `YamlAst` tree
and resolve all preprocessing tags and `{{ }}` Handlebars strings using the
environment built in Phase 1.

### Key abstractions

- **`YamlAst`** (`src/yaml/parsing/ast.rs`): Enum representing parsed YAML
  nodes -- scalars, sequences, mappings, preprocessing tags, and
  CloudFormation intrinsic function tags. Carries `SrcMeta` for error
  reporting with source locations.

- **`PreprocessingTag`** (`ast.rs`): Enum with variants for all
  preprocessing tags (`VarLookup`, `If`, `Map`, `Merge`, `Concat`, ...).

- **`TagContext`**: Carries the current environment (resolved `$defs` and
  `$imports`), base path for relative imports, and the processing
  environment needed during resolution.

- **`EnvValues`**: Runtime values (`region`, `environment`, `iidy.command`,
  etc.) injected into the preprocessing environment by `load_stack_args()`.

### Import system

The `ImportType` enum in `src/yaml/imports/mod.rs` supports `file`, `env`,
`s3`, `ssm`, and several others. Each type has a loader in
`src/yaml/imports/loaders/`.

Remote templates (S3, HTTP) are restricted from accessing local import types
(file, env, git, filehash) to prevent information disclosure. See
[SECURITY.md](../SECURITY.md).

### Handlebars integration

`src/yaml/handlebars/engine.rs` provides `interpolate_handlebars_string()`,
which resolves `{{ }}` expressions in scalar values. A curated set of ~25
helpers is registered across several modules in `src/yaml/handlebars/helpers/`
(string manipulation, case conversion, encoding, object access, serialization).

## CloudFormation operations

### CfnContext

`CfnContext` in `src/cfn/mod.rs` carries shared state for a CFN operation:
the AWS SDK client, SDK config, credential sources, a time provider (system
clock for reads, NTP for writes), the operation start time, and token
management for idempotency.

### Handler pattern

Two macros in `src/cfn/mod.rs` structure command handlers:

**`run_command_handler!`** (newer pattern): Handles AWS options
normalization, output manager creation, context construction, error
rendering. The implementation function receives a `DynamicOutputManager`
and `CfnContext` and focuses on business logic:

```rust
pub async fn describe_stack(cli: &Cli, args: &DescribeArgs) -> Result<i32> {
    run_command_handler!(describe_stack_impl, cli, args)
}
```

**`await_and_render!`** (legacy pattern): Awaits a spawned task and renders
its result through the output manager, handling errors consistently.

Most handlers follow the same flow:
1. Build `CfnContext` from CLI args
2. Load stack-args if applicable
3. Build AWS requests via `CfnRequestBuilder` (`src/cfn/request_builder.rs`)
4. Execute operations, emit `OutputData` variants
5. Watch stack events if needed (polling loop with terminal state detection)

### Stack-args loading

`load_stack_args()` in `src/cfn/stack_args.rs` parses a YAML file through
the preprocessing engine, resolves environment maps, merges AWS settings
(CLI overrides stack-args), injects `$envValues`, queries SSM for
account-level defaults, and returns a `StackArgs` struct plus the resolved
`SdkConfig`. See [aws-config.md](aws-config.md) for the full resolution
chain.

### Request building

`CfnRequestBuilder` in `src/cfn/request_builder.rs` constructs all AWS API
requests from `StackArgs`. It handles parameter formatting, tag injection,
template body vs. template URL selection, and capability settings.

## SSM Parameter Store commands

The `param` subcommands in `src/params/` manage SSM Parameter Store
values. Unlike CloudFormation commands, they do not use `CfnContext`,
`run_command_handler!`, or the `OutputData` rendering pipeline. Each
command creates its own SSM client via `create_ssm_client()`, makes API
calls, and prints output directly to stdout.

The `--format simple|json|yaml` flag on read commands controls output
independently from the global `--output-mode`. Serializable output types
(`ParamOutput`, `ParamHistoryOutput`) in `src/params/mod.rs` mirror the
AWS SDK response structure with PascalCase field names for iidy-js
compatibility.

The `param review` command is the only one that uses the output
manager -- specifically `DynamicOutputManager::request_confirmation()`
for its interactive confirmation prompt.

See [output-architecture.md](output-architecture.md) for details on
how param output differs from CFN output.

## Output system

The output system uses a data-driven architecture where command handlers
emit structured `OutputData` variants and renderers handle all presentation
logic.

### OutputData enum

`src/output/data.rs` defines 25 variants covering every type of output:
`CommandMetadata`, `StackDefinition`, `NewStackEvents`,
`OperationComplete`, `ConfirmationPrompt`, `Error`, etc. Each variant
carries a payload struct with the data needed for rendering.

### Renderer trait and implementations

`OutputRenderer` trait in `src/output/renderer.rs` with
`render_output_data()` as the core method.

- **InteractiveRenderer** (`src/output/renderers/interactive.rs`): Rich
  colored output with spinners, section headings, ANSI
  formatting. Handles section sequencing, out-of-order data buffering,
  and live event streaming.

- **JsonRenderer** (`src/output/renderers/json.rs`): JSONL format, one
  JSON object per event with type and timestamp metadata.

- **Plain mode**: InteractiveRenderer configured with colors, spinners,
  and ANSI features disabled. Adds timestamps for CI use.

### DynamicOutputManager

`src/output/manager.rs` sits between handlers and renderers. It buffers
the last 1000 events for mode switching (replays through new renderer),
routes data to the active renderer, and provides `request_confirmation()`
which hides `oneshot::channel` complexity from handlers.

For the full output system design, see
[output-architecture.md](output-architecture.md) and
[ADR-001](adr/001-output-sequencing.md).

## Testing strategy

### Test infrastructure

- Integration test files in `tests/`
- Insta snapshots in `tests/snapshots/`
- YAML fixtures in `tests/fixtures/`
- In-source `#[cfg(test)]` modules throughout `src/`
- `example-templates/` auto-discovered by `tests/example_templates_snapshots.rs`

### Running tests

```
make check    -- cargo check + clippy (~300ms)
make test     -- full suite
make build    -- release build
```

### Snapshot tests

All files in `example-templates/` are automatically snapshot-tested using
the `insta` crate. The test runner recursively discovers YAML files
(excluding `invalid/`, `expected-outputs/`, hidden files) and verifies
that preprocessing produces the expected output. Snapshot changes require
explicit acceptance.

### Offline testing

The architecture separates AWS API calls from output formatting, enabling
offline testing with fixture data. Command handlers can be tested by
providing mock `OutputData` sequences to renderers. AWS SDK types are
converted to output data types via `src/output/aws_conversion.rs`.

### Benchmarks

Criterion benchmarks in `benches/` measure handlebars template performance,
tag resolver overhead, and end-to-end preprocessing pipeline throughput.
See `Makefile` for coverage targets.
