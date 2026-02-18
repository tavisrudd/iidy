# Codebase Guide

Quick reference for navigating and working on the iidy Rust codebase.
The original iidy-js source lives at `../iidy-js/`.

## Project Structure

```
src/
  main.rs              -- tokio runtime, command dispatch (match on Commands enum)
  cli.rs               -- clap derive structs, 20+ subcommands, global opts
  lib.rs               -- re-exports
  render.rs            -- `iidy render` command handler
  explain.rs           -- `iidy explain` error code lookup
  demo.rs              -- `iidy demo` scripted demos
  aws/                 -- AWS SDK config, credentials, NTP timing, token mgmt
  cfn/                 -- CloudFormation operation handlers
  yaml/                -- YAML preprocessing engine (the big one)
  output/              -- Data-driven output system
```

## YAML Preprocessing Engine (`src/yaml/`)

This is the most complex and most complete subsystem.

### Pipeline

Two-phase pipeline in `engine.rs`:

**Phase 1** (`load_imports_and_defs`): Parse document, process `$defs` with
let* semantics (sequential -- each def can reference prior defs), load
`$imports` with handlebars interpolation on paths, recursively preprocess
imported documents.

**Phase 2** (`resolution/resolver.rs`): Walk the `YamlAst`, resolve all
preprocessing tags and `{{ }}` handlebars strings using the environment built
in Phase 1.

### Key Files

```
yaml/
  engine.rs            -- orchestrator: load_imports_and_defs, preprocess_yaml
  parsing/
    parser.rs          -- tree-sitter based, builds YamlAst
    ast.rs             -- YamlAst enum, PreprocessingTag enum, SrcMeta
  resolution/
    resolver.rs        -- resolve_ast, all tag resolution (resolve_if, resolve_map, etc.)
  imports/
    mod.rs             -- parse_import_type, ImportType enum, security checks
    loaders/           -- one file per import type (file.rs, s3.rs, ssm.rs, cfn.rs, etc.)
  handlebars/
    engine.rs          -- create_handlebars_registry, interpolate_handlebars_string
    helpers/           -- one file per helper category
  errors/
    mod.rs             -- IidyError enum
    wrapper.rs         -- enhanced error display with context (has panic risks with UTF-8)
  emitter.rs           -- YamlAst -> String serialization
  path_tracker.rs      -- tracks YAML paths during resolution
```

### Preprocessing Tags (PreprocessingTag enum in ast.rs)

All 20 tags: `!$` / `!$include`, `!$if`, `!$let`, `!$map`, `!$merge`,
`!$concat`, `!$eq`, `!$not`, `!$split`, `!$join`, `!$concatMap`,
`!$mergeMap`, `!$mapListToHash`, `!$mapValues`, `!$groupBy`, `!$fromPairs`,
`!$toYamlString`, `!$parseYaml`, `!$toJsonString`, `!$parseJson`, `!$escape`

NOT yet implemented: `!$expand` (template expansion with `$params`)

### Import Types (ImportType enum in imports/mod.rs)

file, env, git, random, filehash, filehash-base64, s3, http/https, cfn, ssm,
ssm-path

### CloudFormation Tag Pass-through

All CFN intrinsic function tags (`!Ref`, `!Sub`, `!GetAtt`, `!Join`,
`!Select`, etc.) are recognized by the parser, their inner content is
preprocessed, and they pass through to output as tagged YAML values.

## CloudFormation Operations (`src/cfn/`)

### Key Infrastructure Files

```
cfn/
  mod.rs               -- CfnContext, run_command_handler! macro, await_and_render! macro
  stack_args.rs        -- StackArgs YAML loading, env-map resolution, AWS config merge
  stack_operations.rs  -- StackInfoService, StackEventsService, terminal state detection
  request_builder.rs   -- CfnRequestBuilder: builds all AWS API requests
  changeset_operations.rs -- shared changeset create/poll logic
  operations.rs        -- CfnOperation enum
  is_terminal_status.rs
  constants.rs         -- poll intervals, timeouts
  template_loader.rs   -- loads templates from file/S3/HTTP, `render:` prefix
  template_hash.rs     -- SHA256 versioned S3 locations for template approval
```

### Command Handlers (one file each)

create_stack, update_stack, create_or_update, delete_stack, describe_stack,
watch_stack, describe_stack_drift, list_stacks, estimate_cost,
create_changeset, exec_changeset, get_stack_template, get_stack_instances,
get_import, template_approval_request, template_approval_review

### Handler Pattern

All handlers follow the `run_command_handler!` macro pattern:
1. Build `CfnContext` from CLI args
2. Load stack-args if applicable
3. Build AWS requests via `CfnRequestBuilder`
4. Execute operations, emit `OutputData` variants
5. Watch stack events if needed

### Stubs in main.rs (println! only)

`param`, `lint-template`, `convert-stack-to-iidy`, `init-stack-args`

## Output System (`src/output/`)

```
output/
  mod.rs               -- OutputData enum (25 variants), OutputRenderer trait
  data.rs              -- all payload structs (CommandMetadata, StackDefinition, etc.)
  manager.rs           -- DynamicOutputManager: buffer, dispatch, mode switching
  keyboard.rs          -- KeyboardListener: crossterm key capture, mode switch commands
  status.rs            -- CFN status categorization and styling
  color.rs             -- theme system (dark/light/high-contrast)
  aws_conversion.rs    -- AWS SDK types -> output data types
  renderers/
    interactive.rs     -- primary renderer (~2000 lines), spinners, ANSI, sections
    json.rs            -- JSONL output, one object per event
  fixtures/            -- test fixture loading
  test_data.rs         -- sample data for tests
```

### Renderer Architecture

`OutputRenderer` trait with `render(&mut self, data: &OutputData)`.
`DynamicOutputManager` holds a VecDeque<OutputData> buffer (max 1000).
Mode switching replays entire buffer through new renderer.
Plain mode = InteractiveRenderer with spinners/colors/ANSI disabled.

## Testing

### Test Infrastructure

- `tests/` -- 33 integration test files
- `tests/snapshots/` -- 100+ insta snapshots
- `tests/fixtures/` -- YAML fixture data
- `example-templates/` -- auto-discovered by `example_templates_snapshots.rs`
- `#[cfg(test)]` modules throughout src/ -- 62 in-source test modules

### Running Tests

```
make check    -- cargo check + clippy (fast, ~300ms)
make test     -- full test suite (608 tests, ~2 min)
make build    -- release build
```

### Snapshot Tests

Uses `insta` crate. Only the user may accept snapshot changes unless
explicitly told otherwise. All example-templates/ files are automatically
snapshot-tested.

## iidy-js Reference (`../iidy-js/`)

### Key Source Files

```
src/
  main.ts              -- yargs CLI setup, command dispatch
  preprocess/
    index.ts           -- loadImports, transformPostImports, import loaders,
                          validateTemplateParameter, $param type, GlobalAccumulator
    visitor.ts         -- Visitor class: all tag resolution, custom resource
                          template expansion, ref rewriting
  yaml.ts              -- YAML tag class definitions ($include, $expand, etc.)
  cfn/                 -- CFN operations
  params/index.ts      -- SSM param CRUD
  render.ts            -- render command (has directory support, stack-args detection)
```

### Differences from iidy-js

**Intentional improvements:**
- **`$defs` semantics**: JS copies raw (parallel), Rust resolves sequentially
  (let*). Strictly more powerful and backward-compatible.
- **Error messages**: Source-location-aware with line numbers and context.
- **Tree-sitter parser**: Better error recovery and source location tracking.
- **Handlebars**: Curated set of ~25 helpers vs JS's full `handlebars-helpers`
  npm package. Covers helpers actually used in practice.

**Not yet implemented:**
- **`!$string`**: JS alias for `!$toYamlString`, not registered in Rust.
- **`!$expand`**: JS has it, Rust does not.
- **Custom resource templates**: JS has full system, Rust has none of it.
- **`$envValues` custom values**: Runtime injection works, user-defined values not yet supported.

**Previously incompatible, now fixed to match JS:**
- **`!$split`**: Now uses array format `[delimiter, string]` matching JS.
- **`!$let`**: Now uses flat format with `in` key matching JS.

## Custom Resource Template Feature (NOT YET IMPLEMENTED)

This is the main remaining feature. See
`notes/2026-02-17-project-review-and-next-steps.md` for full design analysis.

### Concepts

- **`$params`**: array of parameter definitions on a template document
- **Custom resource type**: resource whose `Type` matches an imported template name
- **`Prefix`**: defaults to the resource's logical name, drives ref rewriting
- **`NamePrefix`**: caller override for the prefix
- **`Properties`**: maps to `$params` on the template
- **`Overrides`**: deep-merged into template before expansion
- **`$global`**: flag to suppress name-prefixing for shared/singleton items
- **`$globalRefs`**: set of names exempt from ref rewriting
- **`GlobalAccumulator`**: collects Parameters/Conditions/Mappings/Outputs from
  templates, merges into root document after all Resources processed
- **Ref rewriting**: `!Ref`, `!GetAtt`, `!Sub`, `Condition`, `DependsOn`
  get Prefix prepended (unless `$global` or `AWS:` prefixed)
- **`!$expand`**: simpler non-CFN expansion (no prefixing, no promotion)

### JS Source Locations

- Template expansion: `../iidy-js/src/preprocess/visitor.ts:747-827`
- Global section promotion: `visitor.ts:829-853`
- Ref rewriting: `visitor.ts:456-547`
- `$params` validation: `../iidy-js/src/preprocess/index.ts:551-642`
- `$param` type: `index.ts:92-99`
- GlobalAccumulator init + merge: `index.ts:650-706`
- `!$expand` tag: `visitor.ts:182-208`
- Custom resource detection: `visitor.ts:686-744`
