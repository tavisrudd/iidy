# Native Anchor/Alias Support in Tree-sitter Parser

**Date**: 2025-09-29
**Status**: ✅ COMPLETE
**Commit**: 5390279f03436661e32f07201f621eb2ac01ca1a
**Goal**: Remove serde_yaml fallback and implement native YAML anchor/alias resolution in tree-sitter parser

## Problem Statement

Currently, the parser uses a crude heuristic to detect anchors/aliases and falls back to serde_yaml for parsing:

```rust
// parser.rs:32-36
if source.contains("&") && !source.contains("&amp;")
    || source.contains("*") && !source.contains("**/")
{
    return self.parse_with_serde_yaml_fallback(source, uri);
}
```

This approach has several issues:
- **Two parsers**: Maintains both tree-sitter and serde_yaml code paths
- **Loss of precision**: serde_yaml parse results lose exact source locations
- **Inconsistent metadata**: Fallback creates generic metadata instead of precise positions
- **Architecture complexity**: Requires conversion from serde_yaml::Value to YamlAst
- **Poor error messages**: Can't point to exact location of undefined aliases

## YAML Anchor/Alias Semantics

**Critical constraint**: YAML spec requires anchors to be defined **before** their aliases. Forward references are not allowed.

```yaml
# Valid: anchor before alias
config: &base
  timeout: 30
service:
  settings: *base  # ✓ references previously defined anchor

# Invalid: alias before anchor
service:
  settings: *base  # ✗ error - anchor not yet defined
config: &base
  timeout: 30
```

This ordering constraint enables **single-pass resolution** during parsing.

## Tree-sitter Support

Tree-sitter's YAML grammar already parses anchors and aliases as distinct node types:
- `anchor` nodes: `&anchor_name value`
- `alias` nodes: `*alias_name`

Currently, the parser handles these incorrectly:
```rust
// parser.rs:535-546
"alias" => {
    let text = self.extract_utf8_text(node, src, &meta, "alias")?;
    Ok(YamlAst::PlainString(text, meta))  // ✗ returns literal "*alias"
}
"anchor" => {
    if let Some(child) = node.named_child(0) {
        self.build_ast(child, src, uri)  // ✗ ignores the anchor name
    } else {
        Ok(YamlAst::Null(meta))
    }
}
```

## Proposed Solution

### Single-Pass Resolution During Parsing

Maintain an anchor map during AST construction and resolve aliases immediately:

1. **Add anchor tracking**: `HashMap<String, YamlAst>` passed through recursive calls
2. **On anchor node**: Extract name, parse value, store in map, return value
3. **On alias node**: Extract name, lookup in map, return clone (or error)
4. **No AST changes**: Anchors/aliases are invisible in final `YamlAst`

### Implementation Discovery: Anchor Location in Tree-sitter

**Critical**: Tree-sitter represents anchors as **child nodes of `block_node`**, not as standalone nodes at the mapping/sequence level. This means anchors must be handled inside `build_block_node` (and potentially `build_flow_node`), not just in the main `build_ast` match.

Example tree structure:
```
"block_node" at 5:16
  children=["anchor@5", "block_mapping@6"]
```

The anchor wraps the content in a `block_node` structure, requiring special handling in that method.

### Implementation Steps

#### 1. Update Parser Method Signature

```rust
impl YamlParser {
    pub fn parse(&mut self, source: &str, uri: Url) -> ParseResult<YamlAst> {
        // Remove anchor/alias detection heuristic (lines 32-36)

        let tree = self.parser.parse(source, None)
            .ok_or_else(|| ParseError::new("Failed to parse YAML source"))?;

        let root = tree.root_node();

        if root.has_error() {
            return Err(self.find_syntax_error(&tree, source, &uri));
        }

        // Parse with anchor tracking
        let mut anchor_map = HashMap::new();
        self.build_ast_with_anchors(root, source.as_bytes(), &uri, &mut anchor_map)
    }
}
```

#### 2. Update build_ast Signature (Keep Name)

**Note**: Keep the function name as `build_ast` - no need to rename to `build_ast_with_anchors`.

```rust
fn build_ast(
    &self,
    node: Node,
    src: &[u8],
    uri: &Url,
    anchor_map: &mut HashMap<String, YamlAst>,  // Added parameter
) -> ParseResult<YamlAst> {
    let node_kind = node.kind();
    let meta = node_meta(&node, uri);

    match node_kind {
        "alias" => {
            // Extract: *alias_name -> "alias_name"
            let alias_name = self.extract_alias_name(node, src, &meta)?;

            // Look up the anchor and clone its value
            anchor_map
                .get(&alias_name)
                .cloned()
                .ok_or_else(|| self.undefined_alias_error(&alias_name, &meta))
        }

        "anchor" => {
            // Fallback handling for standalone anchor nodes (if any exist)
            let anchor_name = self.extract_anchor_name(node, src, &meta)?;
            let value = if let Some(child) = node.named_child(0) {
                self.build_ast(child, src, uri, anchor_map)?
            } else {
                YamlAst::Null(meta.clone())
            };
            anchor_map.insert(anchor_name, value.clone());
            Ok(value)
        }

        // All other cases: pass anchor_map through recursive calls
        "stream" => { /* ... update recursive calls ... */ }
        "document" => { /* ... update recursive calls ... */ }
        "block_mapping" | "flow_mapping" => {
            self.build_mapping(node, src, uri, meta, anchor_map)
        }
        "block_sequence" | "flow_sequence" => {
            self.build_sequence(node, src, uri, meta, anchor_map)
        }
        "block_node" => {
            // CRITICAL: Most anchors are handled here!
            self.build_block_node(node, src, uri, meta, anchor_map)
        }
        // ... etc for all other node types
    }
}
```

#### 3. Helper Methods for Name Extraction

**Note**: `extract_anchor_name` is more complex than initially expected because tree-sitter's anchor node includes the entire content, not just the name. We need to extract only the first token.

```rust
fn extract_anchor_name(
    &self,
    node: Node,
    src: &[u8],
    meta: &SrcMeta,
) -> ParseResult<String> {
    // The anchor node has full text including children,
    // but anchor name is just the first token
    let start = node.start_byte();
    let end = node.end_byte();

    let full_text = std::str::from_utf8(&src[start..end])
        .map_err(|_| self.syntax_error("Invalid UTF-8 in anchor", meta, ""))?;

    // Extract just the anchor name (first word after &)
    let anchor_part = full_text
        .lines()
        .next()
        .unwrap_or(full_text)
        .split_whitespace()
        .next()
        .unwrap_or(full_text);

    // Remove '&' prefix: "&myanchor" -> "myanchor"
    Ok(anchor_part.trim_start_matches('&').to_string())
}

fn extract_alias_name(
    &self,
    node: Node,
    src: &[u8],
    meta: &SrcMeta,
) -> ParseResult<String> {
    let text = self.extract_utf8_text(node, src, meta, "alias")?;
    // Remove '*' prefix and whitespace: "*myalias " -> "myalias"
    Ok(text.trim_start_matches('*').trim().to_string())
}
```

#### 4. Error Handling

```rust
fn undefined_alias_error(&self, alias_name: &str, meta: &SrcMeta) -> ParseError {
    let file_location = self.format_file_location(meta);
    ParseError {
        message: format!(
            "Undefined YAML alias: *{}\n\
             @ {}\n\n\
             Aliases must reference anchors defined earlier in the document.",
            alias_name, file_location
        ),
        location: Some(super::error::ParseLocation {
            uri: meta.input_uri.clone(),
            start: meta.start,
            end: meta.end,
        }),
        code: Some("UNDEFINED_ALIAS".to_string()),
    }
}
```

#### 5. Critical: Anchor Handling in build_block_node

**This is the primary location for anchor handling** - most anchors appear as children of block_node:

```rust
fn build_block_node(
    &self,
    node: Node,
    src: &[u8],
    uri: &Url,
    meta: SrcMeta,
    anchor_map: &mut HashMap<String, YamlAst>,
) -> ParseResult<YamlAst> {
    let mut anchor_node = None;
    let mut tag_node = None;
    let mut content_node = None;

    // Examine children to find anchor, tag, and content
    for i in 0..node.named_child_count() {
        if let Some(child) = node.named_child(i) {
            match child.kind() {
                "anchor" => anchor_node = Some(child),
                "tag" => tag_node = Some(child),
                "block_mapping" | "block_sequence" | /* ... */ => {
                    content_node = Some(child);
                }
                _ => { /* ... */ }
            }
        }
    }

    // Build the result (handling tags if present)
    let result = if let Some(tag) = tag_node {
        // ... tag handling ...
    } else if let Some(content) = content_node {
        self.build_ast(content, src, uri, anchor_map)
    } else {
        Ok(YamlAst::Null(meta.clone()))
    }?;

    // If there's an anchor, store the result in the anchor map
    if let Some(anchor) = anchor_node {
        let anchor_name = self.extract_anchor_name(anchor, src, &meta)?;
        anchor_map.insert(anchor_name, result.clone());
    }

    Ok(result)
}
```

#### 6. Merge Key Detection

Add detection in `build_mapping`:

```rust
fn build_mapping(
    &self,
    node: Node,
    src: &[u8],
    uri: &Url,
    meta: SrcMeta,
    anchor_map: &mut HashMap<String, YamlAst>,
) -> ParseResult<YamlAst> {
    let mut pairs = Vec::new();
    let mut cursor = node.walk();

    for pair_node in node.named_children(&mut cursor) {
        if matches!(pair_node.kind(), "block_mapping_pair" | "flow_pair") {
            let mut pair_cursor = pair_node.walk();
            let mut children = pair_node.named_children(&mut pair_cursor);

            let key = if let Some(key_node) = children.next() {
                self.build_ast(key_node, src, uri, anchor_map)?
            } else {
                return Err(self.syntax_error("Missing key in mapping pair", &meta, ""));
            };

            // Detect YAML 1.1 merge keys
            if let YamlAst::PlainString(ref key_str, ref key_meta) = key {
                if key_str == "<<" {
                    return Err(self.merge_key_error(key_meta));
                }
            }

            // Parse value...
            let value = /* ... */;
            pairs.push((key, value));
        }
    }

    Ok(YamlAst::Mapping(pairs, meta))
}

fn merge_key_error(&self, meta: &SrcMeta) -> ParseError {
    let file_location = self.format_file_location(meta);
    ParseError {
        message: format!(
            "YAML merge keys ('<<') are not supported in YAML 1.2\n\
             in file '{}'\n\n\
             Consider using iidy's !$merge tag instead:\n\n\
             result: !$merge\n\
               - *base\n\
               - override_key: override_value",
            file_location
        ),
        location: Some(super::error::ParseLocation {
            uri: meta.input_uri.clone(),
            start: meta.start,
            end: meta.end,
        }),
        code: Some("MERGE_KEY_NOT_SUPPORTED".to_string()),
    }
}
```

#### 7. Update All Recursive Calls

Every method that calls `build_ast` needs to accept and pass through `anchor_map` parameter. **Function names remain unchanged**:

- `build_mapping` - add `anchor_map` parameter
- `build_sequence` - add `anchor_map` parameter
- `build_flow_node` - add `anchor_map` parameter
- `build_block_node` - add `anchor_map` parameter (and handle anchors as children)
- `build_tagged_node` - add `anchor_map` parameter
- All match arms in `build_ast` itself
- `build_ast_with_error_collection` - add `anchor_map` parameter
- `validate_semantics_with_diagnostics` - add `anchor_map` parameter

#### 8. Remove Fallback Code

Delete these methods entirely:
- `parse_with_serde_yaml_fallback`
- `convert_serde_value_to_ast`

Remove unused import:
- `ParseWarning` (no longer used after removing fallback warning)

#### 9. Update validate_with_diagnostics

The diagnostic collection API also needs anchor support:

```rust
pub fn validate_with_diagnostics(&mut self, source: &str, uri: Url) -> ParseDiagnostics {
    let mut diagnostics = ParseDiagnostics::new();

    // Remove anchor/alias fallback check that was here

    // Parse with tree-sitter
    let tree = match self.parser.parse(source, None) { /* ... */ };

    // Collect syntax errors
    self.collect_all_syntax_errors(&tree, source, &uri, &mut diagnostics);

    // Semantic validation with anchor support
    if !self.has_fatal_syntax_errors(&diagnostics) {
        let mut anchor_map = HashMap::new();
        self.validate_semantics_with_diagnostics(&tree, source, &uri, &mut diagnostics, &mut anchor_map);
    }

    diagnostics
}
```

## Testing Strategy

### Existing Tests

All tests in `tests/yaml_anchors_aliases_tests.rs` should continue to pass:

1. **test_basic_yaml_anchor_and_alias** - Basic anchor definition and alias usage
2. **test_yaml_merge_key_detection_and_error** - Merge key error with helpful message
3. **test_suggested_alternative_to_merge_keys** - Using `!$merge` instead of `<<`
4. **test_anchors_aliases_with_iidy_preprocessing_tags** - Anchors + iidy tags
5. **test_nested_anchors_and_iidy_merge_alternative** - Nested structures
6. **test_anchors_in_sequences_and_arrays** - Anchors in arrays
7. **test_anchor_scope_and_ordering** - Cross-section anchor references

### New Error Cases to Test

Add tests for error conditions:

```rust
#[tokio::test]
async fn test_undefined_alias_error() -> Result<()> {
    let yaml_input = r#"
service:
  config: *undefined_anchor
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("Undefined YAML alias: *undefined_anchor"));
    assert!(error.contains("test.yaml"));
    Ok(())
}

#[tokio::test]
async fn test_forward_reference_error() -> Result<()> {
    let yaml_input = r#"
service:
  config: *base

base: &base
  timeout: 30
"#;

    let result = preprocess_yaml_v11(yaml_input, "test.yaml").await;
    assert!(result.is_err());
    let error = result.unwrap_err().to_string();
    assert!(error.contains("Undefined YAML alias: *base"));
    Ok(())
}
```

## Benefits

1. **No AST pollution** - `YamlAst` enum remains unchanged, anchors are invisible after parsing
2. **Single parser** - Remove serde_yaml dependency from parser module
3. **Better error messages** - Precise line/column for undefined aliases
4. **Consistent metadata** - All nodes have accurate source locations from tree-sitter
5. **Cleaner architecture** - Anchors/aliases are purely a parsing concern, not AST concern
6. **Simpler code** - No fallback logic, no serde_yaml conversion code
7. **Correct semantics** - Single-pass resolution matches YAML spec ordering requirement

## Implementation Summary

### Completed Changes

- [x] Add `use std::collections::HashMap` to parser.rs imports
- [x] Remove anchor/alias detection heuristic from `parse()` and `validate_with_diagnostics()`
- [x] Add `anchor_map` parameter to `build_ast` (kept original name, no rename)
- [x] Implement `extract_anchor_name` helper (complex version to extract first token)
- [x] Implement `extract_alias_name` helper
- [x] Update "anchor" match arm to store in map (fallback handler)
- [x] Update "alias" match arm to lookup and clone from map
- [x] Add `undefined_alias_error` method
- [x] Add `merge_key_error` method
- [x] **Critical**: Update `build_block_node` to handle anchor children (primary path)
- [x] Update `build_mapping` to detect merge keys and pass `anchor_map`
- [x] Update `build_sequence` to pass `anchor_map`
- [x] Update `build_flow_node` to pass `anchor_map`
- [x] Update `build_tagged_node` to pass `anchor_map`
- [x] Update all match arms in `build_ast` to pass `anchor_map`
- [x] Update `validate_with_diagnostics` to remove fallback and create anchor_map
- [x] Update `validate_semantics_with_diagnostics` signature to accept `anchor_map`
- [x] Update `build_ast_with_error_collection` to accept and pass `anchor_map`
- [x] Delete `parse_with_serde_yaml_fallback` method
- [x] Delete `convert_serde_value_to_ast` method
- [x] Remove unused `ParseWarning` import
- [x] Run all tests: 6/7 passing (merge key error message format needs adjustment)
- [x] Verify no compiler warnings: `cargo check --all` passes

### Remaining Work

- [x] Fix merge key error message format (use file path only, not file:line:col)
- [x] Verify all 7 tests pass
- [x] Run full test suite to ensure no regressions (590 tests passing)
- [x] Optional: Add new error case tests for undefined aliases and forward references

## Implementation Complete (2025-09-29)

Successfully implemented in commit 5390279f03436661e32f07201f621eb2ac01ca1a:

✅ **Native anchor/alias resolution** - Single-pass HashMap-based resolution during parsing
✅ **Removed serde_yaml fallback** - No more dual parser code paths
✅ **Error detection** - Undefined aliases and YAML 1.1 merge keys properly detected
✅ **Bonus: Folded scalars** - Implemented proper YAML folded scalar (>) processing
✅ **Bonus: Chomping indicators** - All YAML 1.2 chomping indicators (|-, >-, |+, >+)
✅ **Test coverage** - All 590 tests passing with new comprehensive tests
✅ **No compiler warnings** - Clean build

## Migration Notes

- **No breaking changes** - Public API remains identical
- **Better errors** - Users get more precise error messages for undefined aliases
- **Merge key errors** - Clearer guidance to use `!$merge` instead of `<<`
- **Performance** - Slightly faster (one parser instead of two)
- **Dependencies** - Could potentially remove serde_yaml if only used for fallback

## Future Considerations

- Consider if serde_yaml is still needed elsewhere in the codebase
- Could add warning for complex anchor patterns that might have performance implications
- Consider caching anchor resolution for frequently-used anchors (premature optimization)