# iidy-js Compatibility Issues Found

Based on analysis of `/iidy-js-for-reference/src/preprocess/visitor.ts`, several compatibility issues were found between our Rust implementation and the original iidy-js behavior.

## Critical Issues to Fix

### 1. **!$split** - Wrong Format
- **iidy-js**: `!$split [delimiter, string]` (array with 2 elements)
- **Our implementation**: `!$split {string: ..., delimiter: ...}` (object format)
- **Fix needed**: Change parser to expect array format, update AST field order

### 2. **!$let** - Wrong Field Names  
- **iidy-js**: `!$let {var1: value1, var2: value2, in: expression}` (flat structure)
- **Our implementation**: `!$let {bindings: {var1: value1}, expression: ...}` (nested structure)
- **Fix needed**: Change parser to expect flat structure with "in" field

### 3. **!$groupBy** - Wrong Field Names
- **iidy-js**: Uses `node.data.items` and `node.data.var` (line 391)
- **Our implementation**: Uses "source" and "var_name"
- **Fix needed**: Change to use "items" and "var", add optional "template" field

### 4. **!$mergeMap** and **!$concatMap** - Delegation Issue
- **iidy-js**: Both tags delegate to `new yaml.$map(node.data)` - they use the same format as !$map
- **Our implementation**: We changed !$mergeMap to use "items"/"template" but kept separate parsing
- **Fix needed**: These should use identical format to !$map (items, template, var, filter)

### 5. **!$mapListToHash** - Delegation Issue  
- **iidy-js**: Uses `new yaml.$map(node.data)` (line 453) - same format as !$map
- **Our implementation**: Uses "source" field
- **Fix needed**: Should use !$map format (items, template, var, filter)

## Files That Need Updates

### AST Changes (`src/yaml/parsing/ast.rs`)
- [x] ✅ COMPLETED: Updated SplitTag to use array format (delimiter, string fields)
- [x] ✅ COMPLETED: Updated GroupByTag to use "items", "var", "key", add "template"
- [ ] ❌ TODO: Complete LetTag changes for flat structure (still uses nested bindings)
- [x] ✅ COMPLETED: All delegation tags use same format as MapTag (items, template, var, filter)

### Parser Changes (`src/yaml/parsing/parser.rs`)
- [ ] ❌ TODO: Fix `parse_split_tag()` to expect array format `[delimiter, string]` (currently object format)
- [ ] ❌ TODO: Fix `parse_let_tag()` to expect flat object with "in" field
- [x] ✅ COMPLETED: Fixed `parse_group_by_tag()` to use "items", "var", "template"
- [x] ✅ COMPLETED: `parse_merge_map_tag()`, `parse_concat_map_tag()`, `parse_map_list_to_hash_tag()` all use !$map format

### Tag Resolution (`src/yaml/resolution/resolver.rs`)  
- [ ] ❌ TODO: Update `resolve_split_tag()` for new array format
- [ ] ❌ TODO: Update `resolve_let_tag()` for new flat structure
- [x] ✅ COMPLETED: Updated `resolve_group_by_tag()` for new field names and optional template
- [x] ✅ COMPLETED: Updated delegation tag resolvers to use Map format

### Example Files
- [ ] ❌ TODO: Update `example-templates/yaml-iidy-syntax/split.yaml` to use array format `!$split [delimiter, string]`
- [ ] ❌ TODO: Update `example-templates/yaml-iidy-syntax/let.yaml` to use flat format with "in" field
- [x] ✅ COMPLETED: `example-templates/yaml-iidy-syntax/groupby.yaml` uses correct field names
- [x] ✅ COMPLETED: `example-templates/yaml-iidy-syntax/mergemap.yaml` uses !$map format
- [x] ✅ COMPLETED: `example-templates/yaml-iidy-syntax/maplisttohash.yaml` uses !$map format

## Implementation Status Summary

### ✅ COMPLETED (Major Progress!)
- **!$groupBy**: All field name changes completed (items, var, key, template)
- **!$mergeMap, !$concatMap, !$mapListToHash**: All now use consistent !$map format
- **All delegation tags**: Proper format consistency implemented
- **Most example files**: Updated to use correct field names

### ❌ REMAINING WORK  
- **!$split**: Parser and examples need array format `!$split [delimiter, string]` instead of object format
- **!$let**: Parser and examples need flat format `{var1: value1, var2: value2, in: expression}` instead of nested bindings

## Implementation Priority

1. **High Priority**: !$split and !$let (these are the only remaining compatibility issues)
2. ~~**Medium Priority**: !$groupBy field name changes~~ ✅ COMPLETED
3. ~~**Low Priority**: Delegation tag format consistency~~ ✅ COMPLETED

## Testing Strategy

After fixes:
1. Run `cargo test --test example_templates_snapshots` 
2. Accept new snapshots with `cargo insta accept`
3. Ensure all 21 tags have working examples
4. Verify compatibility with iidy-js behavior patterns

## Notes

- ✅ iidy-js visitor.ts shows that !$mergeMap, !$concatMap, and !$mapListToHash all delegate to !$map internally
- ✅ These now all accept the same parameters: `{items, template, var, filter}`
- ✅ Fixed the separate parsing approach to be compatible with the original
- ⚠️ **Only !$split and !$let remain incompatible** - these are the final two tags needing format changes

## Update History

- **2025-06-08**: Major compatibility work completed during YAML preprocessing modernization
  - Fixed all delegation tag formats to match !$map
  - Updated GroupBy tag field names 
  - Fixed field name consistency (source→items, transform→template, var_name→var)
  - Only !$split array format and !$let flat format remain as TODO items