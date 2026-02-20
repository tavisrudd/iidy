# RFC: Custom Resource Templates

**Date**: 2026-02-17
**Status**: Implemented (2026-02-19) -- see `handoffs/done/2026-02-19-custom-resource-templates-implementation.md`

## Problem

iidy's YAML preprocessor supports a module/component system where imported
YAML documents with `$params` serve as synthetic resource types in CFN
`Resources:` sections. Each usage expands into real AWS resources with
automatic name-prefixing to avoid collisions. The Rust rewrite has none of
this: no `$params`, no `!$expand`, no custom type detection, no ref
rewriting, no global section promotion.

## Real-World Usage Patterns

From production templates (sanitized examples in
`example-templates/custom-resource-templates/`):

**Pattern 1: Stamping out parameterized resource groups** -- A
`MonitoredQueue` template defines an SQS queue + two CloudWatch alarms.
A single CFN template uses it 16 times with different parameters, turning
~80 lines of template into ~250 lines of real CFN resources.

**Pattern 2: Reusable IAM role templates** -- A `BaseDeployRole` template
defines a complex IAM role with configurable policy resources. Multiple
projects import it from S3 and instantiate it with project-specific ARNs.

Key characteristics:
- Templates declare `$params` with Name, Type, Default
- Templates contain `Resources`, `Parameters`, `Outputs` (CFN sections)
- `$global: true` on Parameters prevents duplication when template is
  used multiple times (e.g., shared `Environment` parameter)
- Consumers reference expanded resources by prefixed names
  (e.g., `!GetAtt AppDeployRoleDeployRole.Arn`)

## Design Space

### Where to implement: Phase 1 vs Phase 2

**Option A: Phase 2 only (resolver-level)**

Custom resource detection and expansion happens entirely during AST
resolution in `resolve_mapping`. When resolving a mapping whose path is
`Root.Resources`, inspect each resource's `Type` field. If it matches an
imported template (present in `context.variables` and has `$params`), expand
it inline.

Pros: Single-phase, no engine changes needed. All existing tag resolution
machinery works.

Cons: The resolver currently has no concept of "I'm inside Resources" --
it treats all mappings uniformly. Adding semantic awareness of CFN document
structure to the resolver mixes concerns.

**Option B: Phase 1.5 (engine-level pre-pass)**

After `load_imports_and_defs` but before `resolve_ast`, walk the root AST
to find `Resources` mappings. For each resource with a non-AWS Type that
matches an import key, perform template expansion as an AST-to-AST
transform. The result is a new AST with only real AWS resources, which
then goes through normal Phase 2 resolution.

Pros: Clean separation -- the resolver stays generic. The expansion is an
AST rewrite that the resolver doesn't need to know about. Ref rewriting
is explicit AST surgery rather than context-dependent behavior.

Cons: More complex -- requires AST manipulation (not just Value
production). Need to handle handlebars/variable resolution within the
expansion before Phase 2 runs on the result.

**Option C: Hybrid (recommended)**

Expand custom resources during Phase 2 resolution but encapsulate it in
a dedicated module, not spread across the resolver. The resolver's
`resolve_mapping` gets a single hook point: when path indicates we're in
`Resources`, delegate to a `custom_resource` module that handles detection,
expansion, prefixing, and accumulation. This module produces `Value`
results that the resolver inserts into the output mapping.

Pros: Leverages existing resolution machinery (handlebars, tag resolution,
variable lookup all work). Keeps the resolver's core logic clean via
delegation. No AST manipulation needed.

Cons: Requires passing `GlobalAccumulator` through the resolution context
(the field already exists but is unused).

**Recommendation: Option C.** The resolver already passes `TagContext`
everywhere. Adding a `resources_resolver` module that the main resolver
delegates to keeps concerns separated while avoiding AST-level surgery.

### How to detect custom resource types

**Option 1: Check `context.variables` for templates with `$params`**

When an imported document has `$params`, mark it in the environment. During
resource resolution, look up the `Type` value in `context.variables`. If
found and it has the template marker, treat it as custom.

**Option 2: Type name heuristic**

Any `Type` that doesn't start with `AWS::` or `Custom::` is checked against
imports.

**Recommendation: Option 2 with Option 1 validation.** Use the heuristic
for detection (matching JS behavior), then validate the import actually
has `$params`. Error if a non-AWS type doesn't resolve to a valid template.

### How to carry `$params` metadata

Imported documents are currently stored as `serde_yaml::Value` in
`EnvValues`. A template with `$params` is a Value::Mapping. The `$params`
key is currently stripped during resolution.

**Option A: Preserve `$params` in a side channel**

During `load_imports_and_defs`, when processing an imported doc that has
`$params`, extract the params metadata into a separate
`HashMap<String, Vec<ParamDef>>` on the context.

**Option B: Keep `$params` in the Value**

Don't strip `$params` during import processing. Let it live in the Value
tree. Extract it at expansion time.

**Recommendation: Option A.** Structured `ParamDef` types enable proper
validation. The `$params` array has specific semantics (Name, Default,
Type, AllowedValues, AllowedPattern, Schema) that deserve a real Rust
struct, not ad-hoc Value inspection.

### Ref rewriting strategy

Inside a custom resource expansion, `!Ref Foo` becomes `!Ref PrefixFoo`.
This affects `!Ref`, `!GetAtt`, `!Sub` (`${Foo}`), `Condition:`, and
`DependsOn:`.

**Option A: Context-driven rewriting**

Add a `prefix: Option<String>` and `global_refs: HashSet<String>` to
`TagContext`. The existing `resolve_cloudformation_tag` checks these and
rewrites ref targets. When not inside a custom resource expansion,
`prefix` is `None` and no rewriting happens.

**Option B: Post-expansion AST/Value walk**

Expand the template to a `Value`, then walk it to find and rewrite all
refs. This is a separate pass.

**Recommendation: Option A.** Context-driven rewriting integrates with
the existing resolution flow. The resolver already handles every CFN tag;
adding prefix logic to `resolve_cloudformation_tag` (specifically `Ref`,
`GetAtt`, `Sub`) is natural. The `prefix` field on `TagContext` makes the
behavior purely scoped -- it only activates inside custom resource
expansion via `context.with_bindings(...)`.

### `!$expand` tag

`!$expand` is the non-CFN form of template expansion. It takes
`{template: name, params: {key: val}}`, resolves params, creates a
sub-context, and resolves the template body. No prefixing, no global
section promotion.

This is a straightforward `PreprocessingTag` variant. It should be
implemented first as a stepping stone since it exercises `$params`
validation without the complexity of prefixing and global accumulation.

## Proposed Architecture

### New types

```
src/yaml/
  custom_resources/
    mod.rs              -- public API: expand_custom_resources()
    params.rs           -- ParamDef, validate_params()
    expansion.rs        -- expand_single_resource()
    ref_rewriting.rs    -- apply_prefix_to_ref(), should_rewrite_ref()
    global_sections.rs  -- GlobalAccumulator logic
```

**`ParamDef`**:
```rust
pub struct ParamDef {
    pub name: String,
    pub default: Option<Value>,
    pub param_type: Option<String>,
    pub allowed_values: Option<Vec<Value>>,
    pub allowed_pattern: Option<String>,
    pub schema: Option<Value>,
    pub is_global: bool,
}
```

**`TemplateInfo`** (stored on TagContext):
```rust
pub struct TemplateInfo {
    pub params: Vec<ParamDef>,
    pub body: Value,           // the full imported document
    pub location: String,      // source URI for error messages
}
```

**`TagContext` additions**:
```rust
pub struct TagContext {
    // existing fields...
    pub variables: HashMap<String, Value>,
    pub input_uri: Option<String>,
    pub global_accumulator: Option<GlobalAccumulator>,  // already exists
    pub scope_context: Option<ScopeContext>,

    // new fields:
    pub template_defs: HashMap<String, TemplateInfo>,   // imported templates with $params
    pub prefix: Option<String>,                         // active name prefix (None outside expansion)
    pub global_refs: HashSet<String>,                   // refs exempt from prefixing
}
```

### Integration points

**1. `engine.rs` -- `load_imports_and_defs`**

After processing an imported document, check if it has a `$params` key.
If so, parse the `$params` array into `Vec<ParamDef>` and store a
`TemplateInfo` entry keyed by the import name. Still store the document
value in `env_values` for the resolver (templates can reference their
own `$envValues`/`$defs`).

**2. `parser.rs` -- `parse_preprocessing_tag`**

Add `"!$expand"` case that parses `{template: <name>, params: <mapping>}`
into `PreprocessingTag::Expand(ExpandTag)`.

**3. `resolver.rs` -- `resolve_mapping`**

In the slow path, after resolving keys, check if `path_tracker` indicates
we're at `Root.Resources` (segments == `["Root", "Resources"]` or just
`["Resources"]`). If so, for each resource entry:
- Resolve the `Type` value
- If Type is a string not starting with `AWS::` or `Custom::`, look it
  up in `context.template_defs`
- If found, delegate to `custom_resources::expand_single_resource()`
- Collect expanded resources into the output mapping with prefixed names
- Accumulate global sections into `context.global_accumulator`

**4. `resolver.rs` -- `resolve_cloudformation_tag`**

For `Ref`, `GetAtt`, and `Sub` variants, after resolving the inner value,
check `context.prefix`. If set and `should_rewrite_ref(name, context)`,
prepend the prefix.

**5. `engine.rs` -- `process()` post-resolution**

After `resolve_ast`, if `global_accumulator` has accumulated sections,
merge them into the resolved output Value. This is the equivalent of
the JS `transformPostImports` merge at lines 692-699.

### Expansion flow for a single custom resource

Given:
```yaml
Resources:
  OrderEvents:
    Type: MonitoredQueue
    Properties:
      QueueLabel: OrderEvents
```

1. Detect `Type: MonitoredQueue` is not AWS/Custom, look up in
   `template_defs`
2. Get `TemplateInfo` for `MonitoredQueue`
3. Merge params: template `$params` defaults + provided `Properties`
4. Validate merged params against `ParamDef` constraints
5. Set `prefix = "OrderEvents"` (or `NamePrefix` if provided)
6. Build `global_refs` from items with `$global: true`
7. Create child context via `context.with_bindings(merged_params)` with
   `prefix` and `global_refs` set
8. Resolve the template's `Resources` section with the child context
9. Prefix each output resource name: `Queue` -> `OrderEventsQueue`
10. Collect template's `Parameters`, `Outputs`, etc. into
    `global_accumulator` with prefixed names (except `$global`)
11. Return the expanded resources for insertion into the parent mapping

## Implementation Order

1. **`ParamDef` and validation** -- types, parsing, validation logic.
   Pure data, fully testable in isolation.

2. **`!$expand` tag** -- parser + resolver. Simpler form: no prefixing,
   no global sections. Exercises `$params` parsing, default merging,
   validation, and template body resolution with param bindings.

3. **Custom resource detection** -- `resolve_mapping` hook at
   `Resources` level. Detect non-AWS types, look up templates. Error
   on unknown types.

4. **Name prefixing and ref rewriting** -- `prefix` on TagContext,
   `should_rewrite_ref`, rewriting in `Ref`/`GetAtt`/`Sub` resolution.
   `Condition` and `DependsOn` rewriting on resource entries.

5. **`$global` flag** -- suppress prefixing for marked items. Build
   `global_refs` set.

6. **Global section promotion** -- `GlobalAccumulator` collects
   Parameters/Conditions/Mappings/Outputs from templates. Post-resolution
   merge into root document.

7. **`Overrides` and `NamePrefix`** -- deep-merge Overrides into template
   before expansion. NamePrefix override for the prefix.

Each step produces a working, testable increment. Steps 1-2 can be
merged in isolation. Steps 3-6 build on each other but each has a clear
test surface using the example templates in
`example-templates/custom-resource-templates/`.

## Test Strategy

- Unit tests for `ParamDef` validation (all constraint types)
- Unit tests for ref rewriting (prefix application, `$global` exemption,
  `AWS::` exemption)
- Snapshot tests using the four example templates
- Integration test: `queue-consumers.yaml` -> verify 6 queues + 12
  alarms with correct prefixed names
- Integration test: `multi-role-stack.yaml` -> verify prefixed role
  names and correct Output refs
- Edge cases: nested custom resources (template uses another template),
  circular template references, missing required params, type validation
  failures

## Open Questions

1. **Nested custom resources**: Can a template's `Resources` section use
   another custom resource type? The JS version supports this via
   recursive `visitResourceNode`. We should support it.

2. **`$defs` semantics divergence**: The Rust engine uses `let*`
   (sequential) for `$defs` while JS uses parallel. Should we align
   before adding more features that depend on `$defs` behavior? This
   RFC does not depend on this choice.

3. **`$envValues` gap**: Templates in JS carry `$envValues` which are
   merged into the expansion context. The Rust engine currently ignores
   `$envValues`. For custom resource templates, we need the template's
   own `$defs`/`$imports` to be available during expansion. Since
   imported documents are already recursively preprocessed (their
   `$defs`/`$imports` are resolved before being stored), this may
   work as-is. Needs verification.

4. **Path detection robustness**: Detecting `Resources` via
   `path_tracker.segments()` works for standard CFN documents but
   could false-positive on a non-CFN document with a `Resources` key.
   Should we gate this on `AWSTemplateFormatVersion` presence? The JS
   version does (`const isCFNDoc = root.AWSTemplateFormatVersion ||
   root.Resources`).
