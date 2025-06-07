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

### AST Changes (`src/yaml/ast.rs`)
- [x] ✅ STARTED: Updated SplitTag to use array format
- [x] ✅ STARTED: Updated GroupByTag to use "items", "var", add "template"
- [ ] Complete LetTag changes for flat structure
- [ ] Ensure all delegation tags use same format as MapTag

### Parser Changes (`src/yaml/parser.rs`)
- [ ] Fix `parse_split_tag()` to expect array format `[delimiter, string]`
- [ ] Fix `parse_let_tag()` to expect flat object with "in" field
- [ ] Fix `parse_group_by_tag()` to use "items", "var", "template"
- [ ] Ensure `parse_merge_map_tag()`, `parse_concat_map_tag()`, `parse_map_list_to_hash_tag()` all use !$map format

### Tag Resolution (`src/yaml/tags.rs`)  
- [ ] Update `resolve_split_tag()` for new array format
- [ ] Update `resolve_let_tag()` for new flat structure
- [ ] Update `resolve_group_by_tag()` for new field names and optional template
- [ ] Update delegation tag resolvers

### Example Files
- [ ] Update `example-templates/yaml-iidy-syntax/split.yaml` to use array format
- [ ] Update `example-templates/yaml-iidy-syntax/let.yaml` to use flat format 
- [ ] Update `example-templates/yaml-iidy-syntax/groupby.yaml` to use correct field names
- [ ] Update `example-templates/yaml-iidy-syntax/mergemap.yaml` to use !$map format
- [ ] Update `example-templates/yaml-iidy-syntax/maplisttohash.yaml` to use !$map format

## Implementation Priority

1. **High Priority**: !$split and !$let (basic functionality used frequently)
2. **Medium Priority**: !$groupBy field name changes
3. **Low Priority**: Delegation tag format consistency (functionality works, just inconsistent)

## Testing Strategy

After fixes:
1. Run `cargo test --test example_templates_snapshots` 
2. Accept new snapshots with `cargo insta accept`
3. Ensure all 21 tags have working examples
4. Verify compatibility with iidy-js behavior patterns

## Notes

- iidy-js visitor.ts shows that !$mergeMap, !$concatMap, and !$mapListToHash all delegate to !$map internally
- This means they should all accept the same parameters: `{items, template, var, filter}`
- Our current separate parsing approach works but isn't compatible with the original