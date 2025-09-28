# $defs Variable Resolution Bug Analysis

**Date**: 2025-09-27

## The Bug

Variables defined in `$defs` that contain handlebars templates or reference other `$defs` variables are not properly resolved.

### Broken Example

**Input** (`example-templates/yaml-iidy-syntax/defs-using-handlebars-or-vars.yaml`):
```yaml
$defs:
  a: 1234
  b: 999
  c: !$ a
  handlebars_a: "a-is-{{ a }}"
  direct_a: !$ a

output:
  handlebars_a: !$ handlebars_a
  direct_a: !$ direct_a
  c: !$ c
```

**Expected Output**:
```yaml
output:
  handlebars_a: a-is-1234
  direct_a: 1234
  c: 1234
```

**Actual Output**:
```yaml
output:
  handlebars_a: a-is-{{ a }}
  direct_a: __PREPROCESSING_TAG_1__
  c: __PREPROCESSING_TAG_2__
```

## Root Cause

**The problem is NOT with the `EnvValues` type.** The issue is in the `$defs` processing logic.

### Current Broken Flow

```rust
// In process_defs() - src/yaml/engine.rs:220
for (key, value_ast) in defs_pairs {
    // WRONG: Store raw AST without resolving cross-references
    env_values.insert(key_str.clone(), value.clone());
}
```

**What happens**:
1. `a: 1234` → stored as `YamlAst::Number(1234)` ✅
2. `handlebars_a: "a-is-{{ a }}"` → stored as `YamlAst::TemplatedString("a-is-{{ a }}")` ❌ **Unresolved**
3. `direct_a: !$ a` → stored as `YamlAst::PreprocessingTag(Include("a"))` ❌ **Unresolved**

Later when `!$ handlebars_a` is resolved, it gets the unresolved AST and has no way to resolve the cross-references.

## The Correct Solution: Sequential Resolution in `$defs`

### Pattern from `!$let` Implementation

The `!$let` tag already implements sequential resolution correctly:

```rust
// From resolve_let() - src/yaml/resolution/resolver.rs:1061-1064
for (var_name, var_expr) in &tag.bindings {
    let var_value = self.resolve_ast(var_expr, context, path_tracker)?;  // ← Resolve first
    bindings.insert(var_name.clone(), var_value);                       // ← Then store
}
```

### Apply Same Pattern to `$defs`

```rust
// In process_defs() - CORRECTED VERSION
for (key, value_ast) in defs_pairs {
    // Create context with variables defined so far
    let mut context = TagContext::default();
    for (existing_key, existing_value) in &env_values {
        context.variables.insert(existing_key.clone(), existing_value.clone());
    }

    // Resolve current variable using existing variables
    let resolved_value = self.resolve_ast(&value_ast, &context, &mut path_tracker)?;

    // Store resolved value (not raw AST)
    env_values.insert(key.clone(), resolved_value);
}
```

### Why This Works

1. **Sequential Resolution**: Each variable is resolved using previously defined variables
2. **Cross-References Handled**: `handlebars_a: "a-is-{{ a }}"` can access resolved `a: 1234`
3. **Circular Dependencies Caught**: If `a: !$ b` and `b: !$ a`, the second fails immediately
4. **No Type Changes Needed**: `EnvValues` remains `HashMap<String, Value>`
5. **Natural Ordering**: Lexical, ordered scoping enforced automatically

## Scoping Analysis

### Constructs That Need Sequential Resolution (let* semantics)

1. **`$defs` processing** ❌ **BROKEN** - Currently stores raw AST
2. **`!$let` bindings** ✅ **WORKING** - Already implements sequential resolution

### Constructs That Don't Need Sequential Resolution

All map-style tags introduce simple runtime values (no cross-references):
- `!$map`, `!$concatMap`, `!$mergeMap`, `!$mapListToHash`, `!$mapValues`, `!$groupBy`
- `$imports` (standalone processed documents)

### Dynamic Scoping Still Works

```yaml
$defs:
  prefix: "item"      # ← Resolved to Value::String("item") during $defs processing

output: !$map
  items: [1, 2, 3]
  var: n              # ← Creates runtime variable
  template: "{{ prefix }}-{{ n }}"  # ← Runtime template uses both resolved and runtime vars
```

This works because:
1. `prefix` is resolved to `Value::String("item")` during `$defs` processing
2. Template `"{{ prefix }}-{{ n }}"` is processed at runtime with context `{prefix: "item", n: 1}`
3. No need for late resolution of `$defs` variables

## Implementation Changes

### Core Fix

**File**: `src/yaml/engine.rs` in `process_defs()` method

**Change**: Replace raw AST storage with sequential resolution pattern

### No Type Changes Needed

- ✅ **Keep**: `EnvValues = HashMap<String, Value>`
- ✅ **Keep**: `TagContext.variables: HashMap<String, Value>`
- ✅ **Keep**: All existing resolution logic

### Error Handling

- **Circular dependencies**: Naturally caught during resolution
- **Undefined variables**: Clear error messages with source location
- **Type mismatches**: Existing error handling continues to work

## Success Criteria

- `example-templates/yaml-iidy-syntax/defs-using-handlebars-or-vars.yaml` produces expected output
- Cross-references within `$defs` work correctly
- Circular dependencies are detected with helpful error messages
- All existing functionality continues to work
- No performance regression
- Clean, maintainable implementation that matches `!$let` pattern

## Additional Test Cases Needed

### Cross-Reference Tests ❌ (Currently Missing)
```yaml
# Basic cross-reference
$defs:
  a: 1234
  b: !$ a
output: !$ b
# Expected: output: 1234

# Handlebars cross-reference
$defs:
  a: 1234
  b: "value-is-{{ a }}"
output: !$ b
# Expected: output: value-is-1234

# Mixed references
$defs:
  base: 1234
  template: "value-is-{{ base }}"
  reference: !$ template
output: !$ reference
# Expected: output: value-is-1234
```

### Import + `$defs` Cross-References ❌ (Currently Missing)
```yaml
# Test file: parent.yaml
$imports:
  shared: "shared-vars.yaml"  # Contains: shared_value: "imported"

$defs:
  local: "local-value"
  combined: "{{ shared.shared_value }}-{{ local }}"
  reference: !$ shared.shared_value

output:
  combined: !$ combined
  reference: !$ reference
# Expected:
# output:
#   combined: imported-local-value
#   reference: imported
```

### Dynamic Scoping Tests ❌ (Currently Missing)
```yaml
# Parent scope + dynamic scope
$defs:
  items: [1, 2, 3]
  prefix: "item"

output: !$map
  items: !$ items
  var: n
  template: "{{ prefix }}-{{ n }}"
# Expected:
# output: ["item-1", "item-2", "item-3"]

# Nested dynamic scopes
$defs:
  outer_var: "outer"

outer: !$let
  vars:
    middle_var: "middle"
  in: !$map
    items: [1, 2]
    var: inner
    template: "{{ outer_var }}-{{ middle_var }}-{{ inner }}"
# Expected:
# outer: ["outer-middle-1", "outer-middle-2"]
```

### Deep Nesting Tests ❌ (Currently Missing)
```yaml
# Chain of multiple references
$defs:
  level1: !$ level2
  level2: !$ level3
  level3: "{{ level4 }}"
  level4: "deep-value"
output: !$ level1
# Expected: output: deep-value

# Mixed nesting with complex handlebars
$defs:
  a: 1234
  b: "{{ a }}"
  c: "prefix-{{ b }}-suffix"
  d: !$ c
  e: "final-{{ d }}"
output: !$ e
# Expected: output: final-prefix-1234-suffix
```

### Circular Dependency Detection ❌ (Currently Missing)
```yaml
# Direct circular reference
$defs:
  a: !$ b
  b: !$ a
output: !$ a
# Expected: Error showing cycle: a → b → a

# Handlebars circular reference
$defs:
  a: "{{ b }}"
  b: "{{ a }}"
output: !$ a
# Expected: Error with helpful message

# Complex circular reference
$defs:
  a: !$ b
  b: "value-{{ c }}"
  c: !$ a
output: !$ a
# Expected: Error showing cycle: a → b → c → a
```