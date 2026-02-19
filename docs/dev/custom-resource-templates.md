# Custom Resource Templates (dev)

User-facing documentation: [docs/custom-resource-templates.md](../custom-resource-templates.md)

## Implementation files

- `src/yaml/custom_resources/mod.rs` -- `TemplateInfo` struct
- `src/yaml/custom_resources/params.rs` -- param parsing, merging, validation (Type, AllowedValues, AllowedPattern, Schema)
- `src/yaml/custom_resources/ref_rewriting.rs` -- post-expansion ref/getatt/sub/condition/dependson rewriting
- `src/yaml/custom_resources/expansion.rs` -- `expand_custom_resource`, Overrides deep-merge, global section accumulation
- `src/yaml/resolution/resolver.rs` -- `resolve_resources_mapping` dispatch at the Resources path
- `src/yaml/engine.rs` -- template detection during import, `promote_global_sections` after resolution
- `src/yaml/resolution/context.rs` -- `custom_template_defs` and `accumulated_globals` on TagContext

## How the pieces fit together

1. **Import phase** (engine.rs `load_imports_and_defs`): When an imported document has a
   top-level `$params` key, it's stored as a `TemplateInfo` in `custom_template_defs`
   with the raw YAML body, parsed param definitions, and source location.

2. **Resolution dispatch** (resolver.rs `resolve_mapping`): When the path tracker shows
   we're inside `Resources` and `custom_template_defs` is non-empty, resolution delegates
   to `resolve_resources_mapping` instead of the normal mapping resolver.

3. **Type matching** (resolver.rs `resolve_resources_mapping`): Each resource entry is
   resolved normally. If the resolved `Type` field matches a key in `custom_template_defs`,
   `expand_custom_resource` is called. Otherwise the entry passes through.

4. **Expansion** (expansion.rs): Re-parses the template body, resolves it with merged
   params in a sub-context, applies Overrides deep-merge, collects `$global` refs,
   rewrites refs, prefixes names, strips `$global`, and accumulates global sections
   into the shared `Rc<RefCell<HashMap<String, Mapping>>>`.

5. **Global promotion** (engine.rs `promote_global_sections`): After full resolution,
   accumulated global sections are merged into the top-level result (skip-if-exists).

## Testing

- Example templates in `example-templates/custom-resource-templates/` are auto-discovered
  by `tests/example_templates_snapshots.rs`. Files ending in `-template.yaml` are skipped
  (can't be processed standalone).
- Unit tests in `params.rs` (18 tests), `ref_rewriting.rs` (12 tests), `expansion.rs` (9 tests).
- When adding new example templates, use `INSTA_FORCE_PASS=1` to generate all `.snap.new`
  files if multiple new snapshots would block the test suite.

## Design decisions

- **`$global` is NOT in the resolver skip list**. Unlike `$imports`/`$defs`/`$params`,
  `$global` must survive resolution at all nesting levels so the expansion code can see
  it. It is stripped by `strip_global_key` in the expansion code.

- **Re-parsing template body during expansion**. The template's `raw_body` is re-parsed
  each time a custom resource is expanded. This allows each expansion to resolve with
  different param bindings. An alternative (caching the parsed AST) was considered but
  adds complexity for minimal gain since templates are small.

- **Skip-if-exists for promoted globals**. Outer template definitions win over promoted
  entries. This preserves richer definitions (e.g., `AllowedValues`) from the outer
  template.

## Known follow-ups

- Error messages from params.rs and expansion.rs use plain `anyhow!()`. They should
  follow the structured error reporting patterns in `src/yaml/errors/` (file paths,
  YAML paths, contextual help).
- Conditions promotion via `$global` is not practically usable due to YAML syntax
  constraints. A wrapper syntax or alternative approach may be needed.
- Name collision detection (warning when prefixed resource names conflict with existing
  resources).
