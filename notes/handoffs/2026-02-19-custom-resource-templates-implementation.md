# Custom Resource Templates -- Implementation Plan

**Date**: 2026-02-19
**RFC**: `notes/2026-02-17-custom-resource-templates-rfc.md`
**JS Reference**: `../iidy-js/src/preprocess/visitor.ts` (visitCustomResource, lines 747-827)

## Context

The Rust iidy rewrite lacks the custom resource template system -- the feature that
lets imported YAML documents with `$params` serve as synthetic resource types in CFN
`Resources:` sections. This is the primary remaining feature gap.

**Example templates** in `example-templates/custom-resource-templates/`:
- `monitored-queue-template.yaml` -- template with $params, $global on Parameters
- `queue-consumers.yaml` -- uses MonitoredQueue type, 6 instances -> 18+ expanded resources
- `deploy-role-template.yaml` -- template with $global on params themselves
- `multi-role-stack.yaml` -- uses DeployRole type, references expanded resources by prefixed name

**How it works end-to-end**: An import like `MonitoredQueue: monitored-queue-template.yaml`
loads a document with `$params`. When a resource has `Type: MonitoredQueue`, the template
body is re-parsed and resolved with the resource's Properties as param values. Each resource
in the template gets prefixed (e.g., `Queue` -> `OrderEventsQueue`) and all `!Ref`/`!GetAtt`/
`!Sub` references are rewritten to use the prefixed names. Global sections (Parameters, etc.)
with `$global: true` entries get promoted to the outer template.

---

## Key Architecture Decisions (Finalized)

### Template body storage: raw YAML string

TemplateInfo stores the raw YAML string (`import_data.data`) rather than the parsed Value.
During expansion, we re-parse to AST with our tree-sitter parser and resolve with the
sub-context. This reuses all existing resolution machinery (handlebars, `!$`, `!Ref`, etc.).

Why: The import loader produces a `serde_yaml::Value` which preserves YAML tags but
not our preprocessing semantics. Our tree-sitter parser + AST resolver handles `!$`,
`{{handlebars}}`, and all preprocessing tags correctly. Re-parsing is cheap (templates
are small documents).

### Ref rewriting: post-expansion Value walk

After resolving a template's Resources section, we walk the resulting Value tree and
rewrite refs. CloudFormation tags resolve to `Value::Tagged(TaggedValue { tag, value })`:
- `!Ref Queue` -> `Tagged { tag: "Ref", value: String("Queue") }`
- `!GetAtt Queue.QueueName` -> `Tagged { tag: "GetAtt", value: String("Queue.QueueName") }`
- `!Sub "${Queue}"` -> `Tagged { tag: "Sub", value: String("${Queue}") }`

The post-walk finds these Tagged values and prepends the prefix to the string inside.
This is a pure function on Values -- easy to test and keeps the resolver unchanged.

### Global section accumulation: Rc<RefCell<...>>

`accumulated_globals: Rc<RefCell<HashMap<String, serde_yaml::Mapping>>>` on TagContext.
Only written during custom resource expansion, read once post-resolution by the engine.
`Rc::clone` in `with_bindings` so all child contexts share the same accumulator.

---

## Chunks

Three self-contained chunks. Each compiles and passes all existing tests.

### Chunk 1: Foundation modules (new files only)
- `src/yaml/custom_resources/mod.rs` -- TemplateInfo struct
- `src/yaml/custom_resources/params.rs` -- ParamDef, parse/validate/merge
- `src/yaml/custom_resources/ref_rewriting.rs` -- rewrite_refs post-walk
- `src/yaml/mod.rs` -- add `pub mod custom_resources;`

### Chunk 2: Plumbing (thread through existing code)
- `src/yaml/resolution/context.rs` -- TagContext new fields
- `src/yaml/resolution/resolver.rs` -- skip list ($params, $global)
- `src/yaml/engine.rs` -- template detection + transfer to context

### Chunk 3: Expansion + wiring
- `src/yaml/custom_resources/expansion.rs` -- expand_custom_resource
- `src/yaml/resolution/resolver.rs` -- Resources-level detection
- `src/yaml/engine.rs` -- global section promotion
- Snapshot acceptance for example templates

---

## Chunk 1 Details

### 1a. `src/yaml/mod.rs`

Add after line 15 (`pub mod resolution;`):
```rust
pub mod custom_resources;
```

### 1b. `src/yaml/custom_resources/mod.rs`

```rust
pub mod params;
pub mod ref_rewriting;

use params::ParamDef;

/// A parsed custom resource template (imported document with $params).
/// Stores the raw YAML string for re-parsing during expansion.
#[derive(Debug, Clone)]
pub struct TemplateInfo {
    pub params: Vec<ParamDef>,
    pub raw_body: String,
    pub location: String,
}
```

### 1c. `src/yaml/custom_resources/params.rs`

**Input format** -- `$params` is a `Value::Sequence` of `Value::Mapping` entries:
```yaml
$params:
  - Name: QueueLabel          # required, String
    Type: String              # optional
  - Name: AlarmPeriod
    Type: Number
    Default: 60               # optional, any Value
  - Name: RoleName
    Default: !Ref "AWS::StackName"  # can be a Tagged value
    $global: true             # optional, bool
```

**Struct and functions**:
```rust
use anyhow::{Result, anyhow};
use serde_yaml::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ParamDef {
    pub name: String,
    pub default: Option<Value>,
    pub param_type: Option<String>,
    pub allowed_values: Option<Vec<Value>>,
    pub allowed_pattern: Option<String>,
    pub schema: Option<Value>,
    pub is_global: bool,
}

/// Parse $params from Value::Sequence of mappings
pub fn parse_params(value: &Value) -> Result<Vec<ParamDef>>
// Extract Name (required), Default, Type, AllowedValues, AllowedPattern, Schema, $global
// from each mapping entry. Error if Name is missing or value is not a sequence.

/// Merge param defaults with provided values
pub fn merge_params(defs: &[ParamDef], provided: &HashMap<String, Value>) -> HashMap<String, Value>
// For each ParamDef: use provided[name] if present, else default if present.
// Skip params with no default and no provided value (validate_params catches these).

/// Validate merged params. Call after merge_params.
pub fn validate_params(defs: &[ParamDef], merged: &HashMap<String, Value>, resource_name: &str) -> Result<()>
// Checks (matching JS index.ts:551-642):
// 1. Required: if param not in merged -> error "Missing required parameter {name} in {resource_name}"
// 2. AllowedValues: if set, check merged[name] is in the list
// 3. AllowedPattern: if set, compile regex, check merged[name] (must be string)
// 4. Type: "String"/"string" -> Value::String, "Number"/"number" -> Value::Number,
//    "Object"/"object" -> Value::Mapping. AWS types (CommaDelimitedList, AWS::*) -> skip.
//    Unknown type that doesn't start with "AWS:" or "List<" -> error.
// Note: Schema validation skipped (would need json-schema crate).
```

**Tests**: parse with all fields, minimal entry, $global flag, merge with defaults/overrides,
validate missing required, type mismatch, allowed values pass/fail, allowed pattern pass/fail.

### 1d. `src/yaml/custom_resources/ref_rewriting.rs`

Pure function that walks a `Value` tree and rewrites CFN refs by prepending prefix.

```rust
use serde_yaml::Value;
use serde_yaml::value::{Tag, TaggedValue};
use std::collections::HashSet;

/// Walk Value tree, rewrite CFN refs by prepending prefix.
pub fn rewrite_refs(value: &Value, prefix: &str, global_refs: &HashSet<String>) -> Value

/// Should this ref name be rewritten?
fn should_rewrite(ref_name: &str, global_refs: &HashSet<String>) -> bool
// false if starts with "AWS:" (single colon, matching JS)
// false if in global_refs
// true otherwise
```

**Rewriting rules** (from JS visitor.ts:456-547):

1. `Tagged("Ref", String(name))` -- if should_rewrite(name.trim()) -> prepend prefix
2. `Tagged("GetAtt", String(dotted))` -- split on `.`, check first segment, prepend to whole
3. `Tagged("Sub", String(template))` -- regex `\$\{([^!].*?)\}`, check first `.`-segment
4. `Tagged("Sub", Sequence([String(template), Mapping(vars)]))` -- vars keys added to globals
5. Resource entry `Condition: String(name)` -- rewrite if should_rewrite
6. Resource entry `DependsOn: String(name)` or `DependsOn: Sequence([...])` -- rewrite each

**Recursive walk**: Mapping -> recurse values (+ check Condition/DependsOn on resource entries).
Sequence -> recurse elements. Tagged -> check tag, rewrite, recurse. Scalars -> return as-is.

A "resource entry" is a Mapping that contains a "Type" key (heuristic to detect CFN resources
within the resolved Resources section).

**Tests**: ref rewrite, AWS:: no-rewrite, global no-rewrite, GetAtt dotted, Sub template,
Sub with vars, Condition, DependsOn scalar+array, nested structures.

---

## Chunk 2 Details

### 2a. `src/yaml/resolution/context.rs` -- TagContext additions

**Current struct** (lines 23-32):
```rust
#[derive(Debug, Default)]
pub struct TagContext {
    pub variables: HashMap<String, Value>,
    pub input_uri: Option<String>,
    pub global_accumulator: Option<GlobalAccumulator>,
    pub scope_context: Option<ScopeContext>,
}
```

**Add two fields**:
```rust
pub custom_template_defs: HashMap<String, crate::yaml::custom_resources::TemplateInfo>,
pub accumulated_globals: std::rc::Rc<std::cell::RefCell<HashMap<String, serde_yaml::Mapping>>>,
```

**Derive(Default) won't work** -- need manual Default impl because Rc<RefCell<...>>
isn't derived. Actually `Rc::default()` works for `Rc<RefCell<HashMap>>`, so it should
be fine. But `HashMap<String, TemplateInfo>` needs TemplateInfo to not be in Default.
Actually HashMap::new() is Default. Check if derive still works, if not write manual impl.

**Update all methods that construct TagContext**:
Every place that creates `Self { ... }` needs the two new fields. These are:
- `with_bindings()` (line 203) -- clone custom_template_defs, Rc::clone accumulated_globals
- `with_bindings_ref()` (line 215) -- both branches, same propagation
- `with_variable()` (line 245) -- just `mut self`, fields already present
- `with_input_uri()` (line 251) -- just `mut self`, fields already present
- `with_scope_tracking()` (line 257) -- add empty defaults to constructed Self
- `#[cfg(test)] from_file_location()` (line 172) -- add empty defaults
- `#[cfg(test)] from_location_and_vars()` (line 183) -- add empty defaults
- `new_with_cfn_accumulator()` (line 193) -- add empty defaults

### 2b. `src/yaml/resolution/resolver.rs` -- skip list

Line 757, change:
```rust
if matches!(key_str, "$imports" | "$defs" | "$envValues") {
```
to:
```rust
if matches!(key_str, "$imports" | "$defs" | "$envValues" | "$params" | "$global") {
```

### 2c. `src/yaml/engine.rs` -- template detection

**Add import** at top:
```rust
use crate::yaml::custom_resources::{TemplateInfo, params::parse_params};
```

**Add field to YamlPreprocessor** (line 95-101):
```rust
custom_template_defs: std::collections::HashMap<String, TemplateInfo>,
```

**Initialize in new()** (line 106): `custom_template_defs: std::collections::HashMap::new()`

**In process_imports()**, after line 288 (`env_values.insert(import_key.clone(), processed_doc);`):
```rust
// Detect custom resource templates (imported docs with $params)
if let Value::Mapping(ref map) = processed_doc {
    if let Some(params_value) = map.get(&Value::String("$params".to_string())) {
        let params = parse_params(params_value)?;
        self.custom_template_defs.insert(
            import_key.clone(),
            TemplateInfo {
                params,
                raw_body: import_data.data.clone(),
                location: import_data.resolved_location.clone(),
            },
        );
    }
}
```

Note: `import_data.data` is the raw YAML string from disk. `import_data.resolved_location`
is the resolved file path. Both available in the same scope (look at lines 270-305).

**In process()**, after the env var loop (after line 148), transfer custom_template_defs:
```rust
context.custom_template_defs = std::mem::take(&mut self.custom_template_defs);
```

**Verification**: `make check-fast`, then `make test`. No behavior change -- templates
are detected and stored but not yet expanded.

---

## Chunk 3 Details

### 3a. `src/yaml/custom_resources/expansion.rs`

Add `pub mod expansion;` to `src/yaml/custom_resources/mod.rs`.

```rust
use anyhow::{Result, anyhow};
use serde_yaml::{Mapping, Value};
use std::collections::{HashMap, HashSet};

use crate::yaml::custom_resources::TemplateInfo;
use crate::yaml::custom_resources::params::{merge_params, validate_params};
use crate::yaml::custom_resources::ref_rewriting::rewrite_refs;
use crate::yaml::parsing;
use crate::yaml::path_tracker::PathTracker;
use crate::yaml::resolution::context::TagContext;
use crate::yaml::resolution::resolve_ast;

const GLOBAL_SECTION_NAMES: &[&str] = &[
    "Parameters", "Metadata", "Mappings", "Conditions", "Transform", "Outputs",
];

pub struct ExpandedResources {
    pub resources: Vec<(String, Value)>,
    pub global_sections: HashMap<String, Mapping>,
}
```

**`expand_custom_resource` function**:

```rust
pub fn expand_custom_resource(
    name: &str,
    resource_value: &Value,
    template_info: &TemplateInfo,
    context: &TagContext,
    path_tracker: &mut PathTracker,
) -> Result<ExpandedResources>
```

**Flow** (matching JS `visitCustomResource` lines 747-827):

1. **Extract fields** from resource_value (must be a Mapping):
   - `Type` (already known to be in custom_template_defs)
   - `Properties` (optional Mapping -- the param values)
   - `NamePrefix` (optional String -- override prefix)
   - `Overrides` (optional Mapping -- deep-merge over template)

2. **Determine prefix**: `NamePrefix` value if present, else `name`

3. **Deep-merge Overrides** (if present):
   - Resolve Overrides with outer context (current context, not sub-context)
   - Clone template body and deep-merge Overrides over it
   - If no Overrides, just use template body as-is

4. **Collect $globalRefs**: Scan Parameters, Resources, Mappings, Conditions
   sections of merged template for entries with `$global: true`. Also add params
   with `is_global: true`. Build `HashSet<String>`.

5. **Resolve Properties** with outer context to get provided_params:
   - Properties is already a resolved Value (resolved during the resource entry processing)
   - Convert Properties Mapping to `HashMap<String, Value>`

6. **Merge params**: `merge_params(&template_info.params, &provided_params)`

7. **Validate params**: `validate_params(&template_info.params, &merged, name)?`

8. **Build sub-context**: `context.with_bindings(merged_params_map)` where merged_params_map
   includes `Prefix` binding, plus any `$envValues` from the template

9. **Re-parse and resolve template body**:
   ```rust
   let ast = parsing::parse_yaml_from_file(&template_info.raw_body, &template_info.location)?;
   let resolved = resolve_ast(&ast, &sub_context)?;
   ```
   This resolves the entire template with params in scope. `$params` and `$global` are
   skipped by the resolver's skip list (Chunk 2).

10. **Extract Resources section** from resolved output (Mapping with "Resources" key)

11. **Apply ref rewriting**: `rewrite_refs(&resources_value, &prefix, &global_refs)`

12. **Prefix resource names** and strip $global:
    For each (key, value) in Resources mapping:
    - If value is Mapping with `$global: true` -> keep original key, delete $global from value
    - Else -> key becomes `format!("{}{}", prefix, key)`

13. **Collect global sections**: For each GLOBAL_SECTION_NAMES, if present in resolved
    template output, resolve and prefix keys (same $global logic), accumulate into
    `context.accumulated_globals` via `Rc<RefCell<...>>`.borrow_mut()

14. **Return** `ExpandedResources { resources, global_sections }`

### 3b. `src/yaml/resolution/resolver.rs` -- Resources-level detection

In `resolve_mapping` (line 698), at the START of the slow path (line 713), add:

```rust
// Custom resource expansion at the CFN Resources level
if !context.custom_template_defs.is_empty() && path_tracker.segments() == ["Resources"] {
    return self.resolve_resources_mapping(pairs, context, path_tracker);
}
```

**New method** `resolve_resources_mapping` on `impl TagResolver for Resolver`:

```rust
fn resolve_resources_mapping(
    &self,
    pairs: &[(YamlAst, YamlAst)],
    context: &TagContext,
    path_tracker: &mut PathTracker,
) -> Result<Value> {
    let mut result = serde_yaml::Mapping::new();

    for (key_ast, value_ast) in pairs {
        let key_value = self.resolve_ast(key_ast, context, path_tracker)?;
        let key_str = key_value.as_str().unwrap_or("");

        // Skip preprocessing directives
        if key_str.starts_with('$') {
            continue;
        }

        path_tracker.push(key_str);
        let resolved_value = self.resolve_ast(value_ast, context, path_tracker)?;
        path_tracker.pop();

        // Check if this resource uses a custom resource type
        let resource_type = resolved_value
            .as_mapping()
            .and_then(|m| m.get(&Value::String("Type".into())))
            .and_then(|v| v.as_str());

        if let Some(type_name) = resource_type {
            if let Some(template_info) = context.custom_template_defs.get(type_name) {
                // Expand custom resource
                let expanded = expand_custom_resource(
                    key_str, &resolved_value, template_info, context, path_tracker,
                )?;
                for (res_name, res_value) in expanded.resources {
                    result.insert(Value::String(res_name), res_value);
                }
                // Global sections go into accumulated_globals via Rc<RefCell<...>>
                // (handled inside expand_custom_resource)
                continue;
            }
        }

        // Regular resource -- pass through
        result.insert(key_value, resolved_value);
    }

    Ok(Value::Mapping(result))
}
```

Add import: `use crate::yaml::custom_resources::expansion::expand_custom_resource;`

**Note**: `resolve_resources_mapping` does NOT need to be added to the `TagResolver` trait.
It can be a private method on `impl Resolver` (non-trait impl block), or an inherent method.
Check if Resolver has an inherent impl block or if all methods are on the trait.

Actually, looking at the code: all methods are on `impl TagResolver for Resolver`. The
simplest approach is to add it there (it's private use anyway since the trait is sealed).

### 3c. `src/yaml/engine.rs` -- global section promotion

After line 150 (`let result = resolve_ast(&ast, &context)?;`), add:

```rust
// Promote accumulated global sections from custom resource expansion
let result = promote_global_sections(result, &context.accumulated_globals);
```

**New free function** (or method on YamlPreprocessor):

```rust
fn promote_global_sections(
    mut result: Value,
    accumulated: &std::rc::Rc<std::cell::RefCell<HashMap<String, serde_yaml::Mapping>>>,
) -> Value {
    let globals = accumulated.borrow();
    if globals.is_empty() {
        return result;
    }

    if let Value::Mapping(ref mut result_map) = result {
        for (section_name, section_entries) in globals.iter() {
            let section_key = Value::String(section_name.clone());
            let existing = result_map
                .entry(section_key)
                .or_insert_with(|| Value::Mapping(serde_yaml::Mapping::new()));
            if let Value::Mapping(ref mut existing_map) = existing {
                for (k, v) in section_entries {
                    existing_map.insert(k.clone(), v.clone());
                }
            }
        }
    }

    result
}
```

### 3d. Verification

1. `make check-fast` after each file
2. `make test` -- all existing tests must pass
3. Review new snapshots for the 4 example templates in custom-resource-templates/
4. User accepts snapshots if correct

**Expected output for queue-consumers.yaml**:
- Resources section has: OrderEventsQueue, OrderEventsQueueDepthAlarm, OrderEventsQueueAgeAlarm,
  PaymentEventsQueue, PaymentEventsQueueDepthAlarm, PaymentEventsQueueAgeAlarm, ... (6*3=18
  expanded resources) + DeadLetterQueue (AWS::SQS::Queue) + EventTopic (AWS::SNS::Topic)
- Parameters section has: Environment, AlertTopicArn (promoted with $global)
- All !Ref and !GetAtt inside expanded resources reference prefixed names

**Expected output for multi-role-stack.yaml**:
- Resources: AppDeployRoleDeployRole, DataPipelineDeployRoleDeployRole, AppDeployRoleS3Policy
- Outputs reference prefixed names: AppDeployRoleDeployRole.Arn, DataPipelineDeployRoleDeployRole.Arn

---

## Codebase Reference (for quick lookup)

| What | Where |
|------|-------|
| YAML module declarations | `src/yaml/mod.rs` (lines 6-16) |
| PreprocessingTag enum | `src/yaml/parsing/ast.rs` (lines 257-300) |
| Tag parsing dispatch | `src/yaml/parsing/parser.rs` (search `parse_preprocessing_tag`) |
| TagContext struct | `src/yaml/resolution/context.rs` (lines 23-32) |
| with_bindings() | `src/yaml/resolution/context.rs` (lines 203-212) |
| with_bindings_ref() | `src/yaml/resolution/context.rs` (lines 215-237) |
| Resolver struct | `src/yaml/resolution/resolver.rs` (line 282) |
| resolve_mapping() | `src/yaml/resolution/resolver.rs` (lines 698-781) |
| Skip list ($imports etc) | `src/yaml/resolution/resolver.rs` (line 757) |
| resolve_cloudformation_tag() | `src/yaml/resolution/resolver.rs` (line 1759) |
| resolve_preprocessing_tag() | `src/yaml/resolution/resolver.rs` (line 822) |
| Public resolve_ast() | `src/yaml/resolution/resolver.rs` (line 2202) |
| YamlPreprocessor struct | `src/yaml/engine.rs` (lines 95-101) |
| process() | `src/yaml/engine.rs` (lines 114-158) |
| process_imports() | `src/yaml/engine.rs` (lines 242-310) |
| process_imported_document() | `src/yaml/engine.rs` (lines 344-406) |
| ImportData (data + doc) | `src/yaml/imports/mod.rs` (lines 168-173) |
| JS visitCustomResource | `../iidy-js/src/preprocess/visitor.ts` (lines 747-827) |
| JS shouldRewriteRef | `../iidy-js/src/preprocess/visitor.ts` (lines 456-460) |
| JS validateTemplateParameter | `../iidy-js/src/preprocess/index.ts` (lines 551-642) |
| JS _mapCustomResourceToGlobalSections | `../iidy-js/src/preprocess/visitor.ts` (lines 829-853) |

## Build/Test Commands

- `make check-fast` (~3s) -- call directly, not via run-quiet. Quick type checking.
- `make test` -- always via `~/.claude/bin/run-quiet 'make test'`. All 608+ tests must pass.
- Snapshots via insta -- only user may accept changes unless told otherwise.
- When multiple new snapshots block the test suite (insta stops on the first new snapshot),
  use `INSTA_FORCE_PASS=1` to generate all `.snap.new` files in one run:
  `INSTA_FORCE_PASS=1 cargo test --test <test_file> <test_name>`
  Then read the `.snap.new` files to verify correctness before asking user to accept.

---

## Workflow Instructions for Each Iteration

**IMPORTANT**: Each context window should complete ONE chunk. After completing a chunk:

1. Run `make check-fast` and `make test` to verify everything passes
2. Update the Progress section below with what was done
3. Add a "Chunk N Handoff Notes" section at the bottom with:
   - Any deviations from the plan and why
   - Gotchas or surprises encountered
   - Anything the next iteration needs to know
   - Updated line numbers if files shifted significantly
4. If the plan for the NEXT chunk needs changes based on what you learned, update
   the relevant Chunk Details section inline (don't just note it -- fix the plan)
5. Tell the user the chunk is done and ready for review

**Starting a new iteration**: Read THIS file first. Check the Progress section to see
which chunk to work on next. Read any Handoff Notes from previous chunks. Then execute
the next chunk's details.

---

## Progress

- [x] Codebase analysis complete
- [x] Chunk 1: Foundation modules (params.rs, ref_rewriting.rs, mod.rs)
- [x] Chunk 2: Plumbing (TagContext, skip list, engine template detection)
- [x] Chunk 3: Expansion + wiring (expansion.rs, resolver, global promotion)
- [ ] Snapshot acceptance (user review required)

---

## Handoff Notes

### Chunk 1 Handoff (2026-02-19)

**Completed**: All three files created, wired into `src/yaml/mod.rs`. Zero warnings, 589 tests pass.

**Files created**:
- `src/yaml/custom_resources/mod.rs` -- TemplateInfo struct, re-exports
- `src/yaml/custom_resources/params.rs` -- ParamDef, parse_params, merge_params, validate_params + 14 unit tests
- `src/yaml/custom_resources/ref_rewriting.rs` -- rewrite_refs post-walk + 12 unit tests
- `src/yaml/mod.rs` -- added `pub mod custom_resources;`

**Deviations from plan**: None. Implemented exactly as specified.

**Notes for Chunk 2**:
- `regex` crate is already in dependencies (used in ref_rewriting for Sub template rewriting)
- `serde_yaml::value::{Tag, TaggedValue}` import paths confirmed working
- `Tag::new(name)` automatically adds `!` prefix -- `Tag::new("Ref")` creates `!Ref`
- `TaggedValue.tag.to_string()` returns `!Ref` (with `!` prefix), so strip it for matching
- All existing tests continue to pass -- the new module is purely additive

### Chunk 2 Handoff (2026-02-19)

**Completed**: All three files modified, zero warnings, 589 tests pass.

**Files modified**:
- `src/yaml/resolution/context.rs` -- Added `custom_template_defs` and `accumulated_globals` fields to TagContext, updated 6 constructor sites
- `src/yaml/resolution/resolver.rs` -- Added `$params` and `$global` to skip list (line 757)
- `src/yaml/engine.rs` -- Added `custom_template_defs` field to YamlPreprocessor, template detection in process_imports, transfer to context in process

**Deviations from plan**:
- Template detection checks `import_data.doc` (raw Value) BEFORE `process_imported_document` consumes it, rather than checking `processed_doc` after. This is necessary because the skip list now strips `$params` during resolution, so templates with nested `$imports`/`$defs` would lose their `$params` key in the processed output.
- Used `..Self::default()` pattern for test-only constructors (`from_file_location`, `from_location_and_vars`, `new_with_cfn_accumulator`, `with_scope_tracking`) to avoid listing the two new fields explicitly in every constructor. The `with_bindings` and `with_bindings_ref` methods explicitly propagate `custom_template_defs` (clone) and `accumulated_globals` (Rc::clone).

**Naming convention -- avoid "template" overloading**:
The word "template" is heavily used in this codebase for CFN templates (the documents iidy
deploys). Custom resource templates are a different concept (imported docs with `$params` that
act as synthetic resource types). All variables, fields, and functions related to the custom
resource template system MUST use the `custom_template` prefix to avoid confusion. Examples:
`custom_template_defs`, `custom_template_params`, `TemplateInfo` (already in the
`custom_resources` module so the module path disambiguates). Do not introduce bare `template_*`
names for this feature.

**Notes for Chunk 3**:
- `context.custom_template_defs` is populated and available during resolution -- check with `!context.custom_template_defs.is_empty()`
- `context.accumulated_globals` is an `Rc<RefCell<HashMap<String, Mapping>>>` -- use `borrow_mut()` to write
- All child contexts share the same `accumulated_globals` Rc (via `Rc::clone` in `with_bindings`)
- `resolve_ast` public function is at resolver.rs line 2202 -- it creates a Resolver and PathTracker internally
- The `resolve_resources_mapping` method should be added as an inherent method on `Resolver` (separate `impl Resolver` block), not on the `TagResolver` trait
- `$params` keys are stripped from resolved output by the skip list (but NOT `$global` -- see Chunk 3 notes)
- Updated line numbers: skip list is at resolver.rs:757, resolve_mapping starts at :698

### Chunk 3 Handoff (2026-02-19)

**Completed**: All files created/modified, zero warnings, 600 tests pass (with INSTA_FORCE_PASS).

**Files created**:
- `src/yaml/custom_resources/expansion.rs` -- `expand_custom_resource` function + 9 unit tests

**Files modified**:
- `src/yaml/custom_resources/mod.rs` -- added `pub mod expansion;`
- `src/yaml/resolution/resolver.rs` -- added `resolve_resources_mapping` inherent method on `Resolver`,
  added Resources-level dispatch in `resolve_mapping`
- `src/yaml/engine.rs` -- added `promote_global_sections` function, called after `resolve_ast`
- `tests/example_templates_snapshots.rs` -- removed `custom-resource-templates` from skip list,
  added `-template.yaml` suffix skip for standalone template files

**Deviations from plan**:
1. `$global` was REMOVED from the resolver skip list (both `resolve_mapping` and
   `resolve_resources_mapping`). The plan had it in the skip list, but `$global` flags
   appear inside section entries (e.g., `Parameters.Environment.$global: true`) and the
   skip list operates on ALL mappings at all nesting levels. Stripping `$global` prevented
   `collect_global_refs` and `accumulate_globals` from seeing the flags. The `$global` key
   is instead stripped from output by `strip_global_key` in the expansion code.
2. `promote_global_sections` uses skip-if-exists instead of overwrite. The outer template's
   definitions (e.g., `Parameters.Environment` with `AllowedValues`) are more complete than
   the promoted versions from templates. Matching JS `_.merge` semantics where the outer
   template's keys are preserved.
3. `expand_custom_resource` does NOT take a `PathTracker` parameter (plan had it). The function
   calls `resolve_ast` which creates its own PathTracker internally. The outer PathTracker
   is not needed during template expansion since the template is a separate document.
4. Auto-discovery test: template files (`*-template.yaml`) are skipped since they can't be
   processed standalone (they reference `$params` variables not in scope).

**New snapshots pending user review**:
- `tests/snapshots/example_templates_snapshots__auto_discovered_custom_resource_templates_queue_consumers.snap.new`
- `tests/snapshots/example_templates_snapshots__auto_discovered_custom_resource_templates_multi_role_stack.snap.new`

**Verified correct output**:
- queue-consumers.yaml: 18 expanded resources (6 custom x 3 template resources) + 2 regular.
  Parameters section has Environment (with AllowedValues preserved) and AlertTopicArn.
  All !Ref/!GetAtt/!Sub correctly rewritten with prefixes. Global refs (Environment,
  AlertTopicArn) correctly NOT prefixed.
- multi-role-stack.yaml: AppDeployRoleDeployRole, DataPipelineDeployRoleDeployRole (expanded),
  AppDeployRoleS3Policy (regular). Outputs reference prefixed names correctly.

**Lesson learned -- `$global` is not a top-level directive**:
The hardest part of this chunk was the `$global` skip list interaction. Chunk 2 added
`$global` to the resolver's skip list alongside `$imports`, `$defs`, etc. But the skip
list runs on *every* mapping at *every* nesting level, so `$global: true` inside entries
like `Parameters.Environment.$global` got stripped before the expansion code could see it.
This caused all promoted Parameters to be wrongly prefixed (e.g., `OrderEventsEnvironment`
instead of `Environment`).

After removing `$global` from the skip list, a second issue appeared: the promoted
Parameters (with just `{ Type: String }`) overwrote the outer template's more complete
definitions (which included `AllowedValues`). Fixed by changing `promote_global_sections`
from overwrite to skip-if-exists.

The root design issue: `$global` behaves differently from `$imports`/`$defs`/`$params`.
Those are top-level-only directives that should be stripped everywhere. `$global` is
metadata on individual entries within sections that must survive resolution and be consumed
later by the expansion code. This distinction wasn't visible until expansion ran against
real templates. Future work should be careful about adding keys to the resolver skip list
-- consider whether the key appears only at document root or at arbitrary nesting depths.

### Follow-up: Schema validation and additional examples (2026-02-19)

**Added**: `jsonschema` crate (0.28) for JSON Schema validation on `$params`.
Schema validation implemented in `validate_schema` in params.rs. Gracefully skips
values containing CFN intrinsic functions (tagged values like `!Sub`, `!Ref`) since
those can't be converted to JSON for validation.

**New example templates**:
- `lambda-worker-template.yaml` + `event-processors.yaml`: Mappings promotion (`$global`),
  Schema validation (Tags param with `{required: [Key, Value]}`, Permissions with IAM
  statement structure), AllowedPattern, AllowedValues, Type validation.
- `tagged-bucket-template.yaml` + `data-lake.yaml`: Parameters promotion (`$global`),
  demonstrating `Environment` and `CostCenter` stitched into outer template.
- Updated `monitored-queue-template.yaml` with Outputs (QueueUrl, QueueArn).
- Updated `deploy-role-template.yaml` with Outputs (ARN with Export).
- Moved Outputs from `multi-role-stack.yaml` into the template.
- `overrides-demo.yaml`: Demonstrates `Overrides` deep-merge for tweaking template
  internals not exposed as `$params` (e.g., adding FifoQueue/ContentBasedDeduplication
  to a queue template).

**Overrides**: Implemented `deep_merge` in expansion.rs. The `Overrides` key on a custom
resource entry is deep-merged into the resolved template document before ref rewriting
and global section promotion. Already resolved by the outer context, so it can reference
outer `$defs`/`$imports`. Use sparingly -- prefer `$params` for anything that varies
regularly between instances.

**Conditions not demonstrated**: The `$global` flag can't be placed on Conditions entries
because a Condition's value IS the expression (e.g., `!Equals [...]`), not a mapping.
To support `$global` on Conditions, a wrapper syntax would be needed (e.g.,
`IsProduction: { Fn::Equals: [...], $global: true }`). This is a design gap carried over
from the JS implementation -- Conditions promotion via `$global` was theoretically
supported but not practically usable.

### Follow-up task: error reporting consistency

All errors raised by the custom resource expansion code (params.rs, expansion.rs) need
to be reviewed for consistency with the project's user-facing error reporting style.
Currently they use plain `anyhow!()` strings. They should be checked against the error
formatting patterns in `src/yaml/errors/` to ensure they include file paths, YAML paths,
and helpful context as other preprocessing errors do.
