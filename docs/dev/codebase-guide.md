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

**Phase 1** (`load_imports_and_defs`): Parse document, process `$defs`
sequentially (each def can reference prior defs; see
[js-compatibility.md](js-compatibility.md) for how this differs from JS),
load `$imports` with handlebars interpolation on paths, recursively
preprocess imported documents.

**Phase 2** (`resolution/resolver.rs`): Walk the `YamlAst`, resolve all
preprocessing tags and `{{ }}` handlebars strings using the environment built
in Phase 1.

### Key Files

```
yaml/
  engine.rs            -- orchestrator: load_imports_and_defs, preprocess_yaml
  detection.rs         -- YAML spec version and document type detection (CFN, K8s)
  location.rs          -- position-finding strategies for error reporting
  tree_sitter_location.rs -- tree-sitter based precise tag/node position finding
  parsing/
    parser.rs          -- tree-sitter based, builds YamlAst
    ast.rs             -- YamlAst enum, PreprocessingTag enum, SrcMeta
  resolution/
    resolver.rs        -- resolve_ast, all tag resolution (resolve_if, resolve_map, etc.)
    context.rs         -- variable scope management during tag resolution
  imports/
    mod.rs             -- parse_import_type, ImportType enum, security checks
    loaders/           -- one file per import type (file.rs, s3.rs, ssm.rs, cfn.rs, etc.)
  handlebars/
    engine.rs          -- create_handlebars_registry, interpolate_handlebars_string
    helpers/           -- one file per helper category
  errors/
    ids.rs             -- ErrorId enum (categorized error codes ERR_1xxx through ERR_9xxx)
    enhanced.rs        -- EnhancedPreprocessingError enum, SourceLocation
    wrapper.rs         -- enhanced error display with context
  emitter.rs           -- YamlAst -> String serialization
  path_tracker.rs      -- tracks YAML paths during resolution
```

### Preprocessing Tags (PreprocessingTag enum in ast.rs)

`!$` / `!$include`, `!$if`, `!$map`, ... (20+ tags). See
[architecture.md](architecture.md) for the full listing.

NOT yet implemented: `!$expand` (template expansion with `$params`)

### Import Types (ImportType enum in imports/mod.rs)

`file`, `env`, `s3`, ... (11 types). See
[architecture.md](architecture.md) for the full listing.

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
create_changeset, exec_changeset, get_stack_template,
get_import, template_approval_request, template_approval_review

### Handler Pattern

Most handlers follow the `run_command_handler!` macro pattern:
1. Build `CfnContext` from CLI args
2. Load stack-args if applicable
3. Build AWS requests via `CfnRequestBuilder`
4. Execute operations, emit `OutputData` variants
5. Watch stack events if needed

**Exceptions**: `get_stack_template` and `get_import` are data extraction
commands that pipe raw content (YAML/JSON) to stdout. They manage their
own error handling rather than using the macro, because their output style
is fundamentally different from interactive CFN operations.

### Stubs in main.rs (println! only)

`param`, `lint-template`, `convert-stack-to-iidy`, `init-stack-args`

## Output System (`src/output/`)

```
output/
  mod.rs               -- OutputData enum (25 variants), OutputRenderer trait
  data.rs              -- all payload structs (CommandMetadata, StackDefinition, etc.)
  manager.rs           -- DynamicOutputManager: buffer, dispatch, mode switching
  status.rs            -- CFN status categorization and styling
  color.rs             -- theme system (dark/light/high-contrast)
  aws_conversion.rs    -- AWS SDK types -> output data types
  renderers/
    interactive.rs     -- primary renderer, spinners, ANSI, sections
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

- `tests/` -- integration test files
- `tests/snapshots/` -- insta snapshots
- `tests/fixtures/` -- YAML fixture data
- `example-templates/` -- auto-discovered by `example_templates_snapshots.rs`
- `#[cfg(test)]` modules throughout src/

### Running Tests

```
make check    -- cargo check + clippy (fast)
make test     -- full test suite
make build    -- release build
```

### Snapshot Tests

Uses `insta` crate. Only the user may accept snapshot changes unless
explicitly told otherwise. All example-templates/ files are automatically
snapshot-tested.

## iidy-js Reference (`../iidy-js/`)

Key intentional improvements over iidy-js:
- **`$defs` sequential resolution** -- each def can reference prior defs (JS evaluates in parallel)
- **Source-location-aware errors** -- line numbers and context snippets
- **Tree-sitter parser** -- better error recovery than `js-yaml`

The main remaining feature gap is **custom resource templates** (`!$expand`,
`$params`, ref rewriting, `GlobalAccumulator`). See
`notes/2026-02-17-project-review-and-next-steps.md` for full design analysis
and `notes/2026-02-17-custom-resource-templates-rfc.md` for the RFC.

For the full compatibility breakdown, see
[js-compatibility.md](js-compatibility.md).
