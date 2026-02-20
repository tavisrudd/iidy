# cfn: Import Subtypes -- Implementation Plan

**Date**: 2026-02-19
**Session**: `42cba5c8-603a-4a7b-90bd-c33096a88fdb`
**References**: `../iidy-js/src/preprocess/index.ts:320-377`, `docs/import-types.md:181`

## Context

The cfn import loader supports two patterns:
- `cfn:stack-name.OutputKey` -- stack output lookup (Rust-only dot syntax)
- `cfn:export:ExportName` -- named export lookup

The JS version uses a **different syntax** for outputs and supports
additional subtypes that are missing entirely:

| Subtype | JS syntax | Rust syntax (current) |
|---------|-----------|----------------------|
| output | `cfn:output:stack-name/OutputKey` | `cfn:stack-name.OutputKey` (incompatible) |
| export | `cfn:export:ExportName` | `cfn:export:ExportName` (matches) |
| parameter | `cfn:parameter:stack-name/Key` | not implemented |
| tag | `cfn:tag:stack-name/Key` | not implemented |
| resource | `cfn:resource:stack-name/LogicalId` | not implemented |
| stack | `cfn:stack:stack-name` | not implemented |

**Syntax incompatibility**: The Rust `cfn:stack.Output` dot syntax was
invented during the port and does not match JS `cfn:output:stack/Output`.
Both the subtype prefix (`output:`) and the separator (`.` vs `/`) differ.

**Decision**: Add the JS-compatible `cfn:output:stack/Key` syntax. Keep
the existing `cfn:stack.Output` dot syntax as a backward-compatible alias
(it's documented in `docs/import-types.md` and `docs/command-reference.md`).
Document the JS syntax as canonical and the dot syntax as a shorthand.

JS README ref: `../iidy-js/README.md:243-248`
(Note: line 244 has a copy-paste typo showing `cfn:export:` for outputs --
the code at `preprocess/index.ts:345` confirms the subtype is `output`.)

All missing subtypes documented in `docs/import-types.md:181`.

## JS Behavior (verbatim from preprocess/index.ts:320-377)

The JS parser splits on `:` to get `[_, field, ...resolvedLocationParts]`.
Then `resolvedLocation = resolvedLocationParts.join(':')` and for non-export
cases, `[StackName, fieldKey] = resolvedLocation.split('/')`.

```
cfn:parameter:my-stack/DbPassword
     ^field   ^stack   ^fieldKey
```

For `output`, `parameter` and `tag`: calls `describeStacks`, builds a
key->value map from the relevant Stack field, returns either the full map
(no fieldKey) or a single value (with fieldKey).

For `resource`: calls `describeStackResources`, builds a LogicalResourceId
-> full resource object map. With fieldKey, returns the specific resource
object (not just the PhysicalResourceId -- the whole StackResource struct).

For `stack`: returns the entire Stack object from describeStacks.

Without a fieldKey, all subtypes return the full map/object.

JS also supports `?region=us-east-1` query parameter on cfn imports to
query cross-region stacks. This is out of scope for this handoff.

## Implementation

### Syntax Mapping

The Rust `parse_cfn_location` currently handles:
- `cfn:stack-name.OutputKey` -> `("stack", "stack-name", "OutputKey")`
- `cfn:export:ExportName` -> `("export", "", "ExportName")`

Add JS-compatible colon syntax for all subtypes:
- `cfn:output:stack-name/Key` -> `("stack", "stack-name", "Key")`
- `cfn:parameter:stack-name/Key` -> `("parameter", "stack-name", "Key")`
- `cfn:tag:stack-name/Key` -> `("tag", "stack-name", "Key")`
- `cfn:resource:stack-name/LogicalId` -> `("resource", "stack-name", "LogicalId")`

Keep existing dot syntax as alias:
- `cfn:stack-name.OutputKey` -> `("stack", "stack-name", "OutputKey")` (unchanged)

Support omitting the fieldKey to return all values as a mapping
(e.g., `cfn:parameter:stack-name` returns all parameters as a YAML map).

### CfnClient Trait Changes

The `parameter` and `tag` subtypes need stack metadata which comes from
`describe_stacks` -- same API call that `get_stack_outputs` already makes.
Two options:

**Option A** (minimal): Add `get_stack_parameters` and `get_stack_tags`
methods that each call `describe_stacks` independently. Simple but makes
redundant API calls if a template imports both outputs and parameters from
the same stack.

**Option B** (clean): Add a single `describe_stack` method returning a
struct with outputs, parameters, and tags. Then `get_stack_outputs` becomes
a thin wrapper. This avoids redundant API calls.

For `resource`: Add `get_stack_resources` method that calls
`describe_stack_resources`. This is a different API endpoint, no overlap.

Recommend **Option A** for simplicity. The redundant API calls are
negligible in practice (import resolution already makes many AWS calls).
If needed, caching can be added later at the CfnClient level.

### New Structs

```rust
// In cfn.rs, alongside CfnOutput and CfnExport:

#[derive(Debug, Clone)]
pub struct CfnParameter {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct CfnTag {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct CfnResource {
    pub logical_resource_id: String,
    pub physical_resource_id: Option<String>,
    pub resource_type: String,
    pub resource_status: String,
}
```

### CfnClient Trait Additions

```rust
async fn get_stack_parameters(&self, stack_name: &str) -> Result<Vec<CfnParameter>>;
async fn get_stack_tags(&self, stack_name: &str) -> Result<Vec<CfnTag>>;
async fn get_stack_resources(&self, stack_name: &str) -> Result<Vec<CfnResource>>;
```

### parse_cfn_location Changes

Rewrite the parser to check for known subtype prefixes first (`export:`,
`output:`, `parameter:`, `tag:`, `resource:`, `stack:`), then fall back
to the existing dot syntax for backward compatibility.

```rust
fn parse_cfn_location(location: &str) -> Result<(String, String, String)> {
    let path = location.strip_prefix("cfn:")
        .ok_or_else(|| anyhow!("Invalid cfn location: {}", location))?;

    // Check for explicit subtype prefix (JS-compatible syntax)
    if let Some(rest) = path.strip_prefix("export:") {
        // cfn:export:ExportName (existing)
        ...
    } else if let Some(rest) = path.strip_prefix("output:") {
        // cfn:output:stack-name/OutputKey
        let (stack, field) = split_stack_field(rest, location)?;
        Ok(("stack".to_string(), stack, field))  // reuse "stack" type
    } else if let Some(rest) = path.strip_prefix("parameter:") {
        let (stack, field) = split_stack_field(rest, location)?;
        Ok(("parameter".to_string(), stack, field))
    } else if let Some(rest) = path.strip_prefix("tag:") {
        let (stack, field) = split_stack_field(rest, location)?;
        Ok(("tag".to_string(), stack, field))
    } else if let Some(rest) = path.strip_prefix("resource:") {
        let (stack, field) = split_stack_field(rest, location)?;
        Ok(("resource".to_string(), stack, field))
    } else if let Some(rest) = path.strip_prefix("stack:") {
        // cfn:stack:stack-name -- returns entire stack object
        Ok(("full_stack".to_string(), rest.to_string(), String::new()))
    } else {
        // Legacy dot syntax: cfn:stack-name.OutputKey (backward compat)
        let parts: Vec<&str> = path.splitn(2, '.').collect();
        ...
    }
}
```

Helper for subtype prefix parsing (allows optional fieldKey):
```rust
fn split_stack_field(rest: &str, location: &str) -> Result<(String, String)> {
    match rest.split_once('/') {
        Some((stack, field)) if !stack.is_empty() && !field.is_empty() => {
            Ok((stack.to_string(), field.to_string()))
        }
        _ if !rest.is_empty() && !rest.contains('/') => {
            // No field key -- return all values (e.g., cfn:parameter:stack-name)
            Ok((rest.to_string(), String::new()))
        }
        _ => Err(anyhow!("Invalid cfn import format: {}", location)),
    }
}
```

### load_cfn_import_with_client Changes

Add match arms for the three new types:

```rust
"parameter" => {
    let params = client.get_stack_parameters(&stack_name).await?;
    if output_key.is_empty() {
        // Return all parameters as a mapping
        let map: serde_yaml::Mapping = params.iter()
            .map(|p| (Value::String(p.key.clone()), Value::String(p.value.clone())))
            .collect();
        // ...
    } else {
        let param = params.iter().find(|p| p.key == output_key)
            .ok_or_else(|| anyhow!("Parameter {} not found in stack {}", output_key, stack_name))?;
        param.value.clone()
    }
}
```

Similar for `tag` and `resource`. For `resource`, the fieldKey extracts
a specific resource from the map; the value is the full resource object
serialized as a YAML mapping (not just the PhysicalResourceId), matching
JS behavior.

### MockCfnClient Updates

Add mock data storage and trait implementations for the three new methods.
Follow the existing `with_stack_outputs` / `with_exports` builder pattern.

### Documentation

Update `docs/import-types.md:161-181`:
- Add syntax rows to the table for parameter, tag, resource
- Add examples
- Remove the "not yet implemented" note

## Codebase Reference

| What | Where |
|------|-------|
| CFN import loader | `src/yaml/imports/loaders/cfn.rs` |
| CfnClient trait | `src/yaml/imports/loaders/cfn.rs:28-32` |
| AwsCfnClient impl | `src/yaml/imports/loaders/cfn.rs:47-110` |
| parse_cfn_location | `src/yaml/imports/loaders/cfn.rs:171-210` |
| load_cfn_import_with_client | `src/yaml/imports/loaders/cfn.rs:122-164` |
| MockCfnClient | `src/yaml/imports/loaders/cfn.rs:218-272` |
| Import types doc | `docs/import-types.md:161-181` |
| JS implementation | `../iidy-js/src/preprocess/index.ts:320-377` |

## Delegation Strategy

- **Can delegate?** Yes
- **Sub-agent type**: Sonnet
- **Why**: Isolated module, clear pattern to extend (existing outputs/exports
  serve as template), mock infrastructure already in place, no cross-module
  changes needed

## Workflow Instructions

1. Read this file and `src/yaml/imports/loaders/cfn.rs`
2. Add new structs (CfnParameter, CfnTag, CfnResource)
3. Extend CfnClient trait with three new methods
4. Implement in AwsCfnClient (describe_stacks for params/tags, describe_stack_resources for resources)
5. Extend parse_cfn_location with new subtype prefixes
6. Add match arms in load_cfn_import_with_client
7. Update MockCfnClient with new builder methods and trait impls
8. Add tests for each new subtype (parse + load, success + error cases)
9. Update `docs/import-types.md`
10. `make check-fast` + `make test`

## Progress

- [ ] Add CfnParameter, CfnTag, CfnResource structs
- [ ] Extend CfnClient trait and AwsCfnClient impl
- [ ] Add cfn:output: syntax (JS-compatible alias for existing dot syntax)
- [ ] Add cfn:parameter:, cfn:tag:, cfn:resource:, cfn:stack: parsing
- [ ] Add load match arms for new subtypes
- [ ] Update MockCfnClient and add tests
- [ ] Update docs/import-types.md (add new syntax, keep dot syntax as shorthand)
- [ ] make check-fast + make test pass
