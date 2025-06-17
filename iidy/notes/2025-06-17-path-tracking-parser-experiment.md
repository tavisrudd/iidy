# Path Tracking Parser Experiment Results

**Date:** 2025-06-17  
**Experiment:** Adding PathTracker to parser for enhanced error location reporting  
**Result:** ❌ Not viable due to performance impact  

## Objective

Investigate whether adding path tracking during YAML parsing would improve error reporting by providing full YAML paths (e.g., `Resources.MyBucket.Properties.BucketName`) directly in AST nodes, eliminating the need for heuristic-based location finding in error reporting.

## Implementation Details

### Changes Made

1. **Added `path` field to `SrcMeta` struct**
   ```rust
   pub struct SrcMeta {
       pub input_uri: Url,
       pub start: Position,
       pub end: Position,
       pub path: Vec<String>,  // ← New field
   }
   ```

2. **Modified `YamlParser` to include `PathTracker`**
   - Added `path_tracker: PathTracker` field to parser struct
   - Reset path tracker for each new parse operation

3. **Updated all parser methods to be mutable** (`&self` → `&mut self`)
   - Required to allow path tracker mutations during parsing

4. **Added path tracking in AST building**
   - **Mappings**: Push/pop key names during key-value pair processing
   - **Sequences**: Push/pop array indices (`[0]`, `[1]`, etc.) during item processing
   - **Meta creation**: Clone current path into every `SrcMeta` instance

### Key Implementation Points

- Path segments stored as strings (key names, `[index]` for arrays)
- Path cloned from `PathTracker.segments()` for every AST node
- Tracking occurs during tree-sitter AST traversal in `build_ast()` methods

## Performance Benchmark Results

### Baseline vs. Path Tracking Performance

| Test Case | Baseline Time | After Path Tracking | Performance Impact |
|-----------|---------------|---------------------|-------------------|
| **Simple Cases** |
| `custom_parser_simple` | ~16.1µs | 17.3µs | **+7.7%** ⚠️ |
| `custom_parser_simple_preprocessing` | ~47.7µs | 55.5µs | **+16.3%** ⚠️ |
| **CloudFormation Cases** |
| `custom_parser_cloudformation` | ~51.9µs | 61.0µs | **+17.7%** ⚠️ |
| **Complex Cases** |
| `custom_parser_complex_preprocessing` | ~139.4µs | 271.5µs | **+94.7%** 🚨 |
| **Large Document Cases** |
| `custom_parser_large_cloudformation` | ~5.26ms | 6.37ms | **+21.1%** ⚠️ |
| `custom_parser_large_preprocessing` | ~1.03ms | 1.42ms | **+37.8%** 🚨 |
| **Structural Cases** |
| `deep_nesting` | ~102.9µs | 180.5µs | **+75.4%** 🚨 |
| `mapping_heavy_parsing` | ~145.7µs | 177.0µs | **+21.4%** ⚠️ |
| `array_syntax_parsing` | ~46.1µs | 54.7µs | **+18.9%** ⚠️ |

### Performance Impact Analysis

- **Minimum impact**: +7.7% (simple cases)
- **Maximum impact**: +94.7% (complex preprocessing - nearly 2x slower!)
- **Average impact**: ~30-40% across most realistic workloads

## Root Causes of Performance Impact

1. **String allocations**: Converting keys and array indices to strings for every path segment
2. **Vec cloning**: Cloning the entire path vector for every AST node's `SrcMeta`
3. **Push/pop overhead**: PathTracker operations on every mapping key and sequence item
4. **Memory pressure**: Storing full paths in every AST node increases memory usage

## Decision: Experiment Rejected

The performance cost (8% to 95% slower) is too high to justify the benefits. Path tracking during parsing is not viable for production use.

## Recommended Alternative Approach

Instead of eager path tracking during parsing, use **lazy path computation** during error reporting:

### 1. Use Existing Infrastructure
- **PathTracker in `resolve.rs`**: Already exists and works well for resolution-time errors
- **Tree-sitter navigation**: Use tree-sitter's node traversal capabilities for parsing errors

### 2. On-Demand Path Computation for Parse Errors
```rust
// When a parse error occurs, compute path from tree-sitter node
fn compute_parse_error_path(error_node: &Node, source: &str) -> Vec<String> {
    // Walk up the tree-sitter AST to build path segments
    // Only pay this cost when errors actually occur
}
```

### 3. Benefits of Lazy Approach
- **Zero performance impact** on successful parsing (99.9% of cases)
- **Precise paths** computed only when needed for error reporting
- **Uses existing tree-sitter capabilities** rather than duplicating tracking
- **Lower memory usage** - paths not stored in every AST node

## Related Work

This experiment supports the implementation plan in `notes/2025-06-16-use-src-meta-in-errors.md`:

> **Phase 1**: The YAML parsing system now has precise line:col information via tree-sitter and SrcMeta, but error reporting still uses old heuristic-based location finding.

The plan correctly identifies that we should use SrcMeta's existing position information rather than adding redundant path tracking during parsing.

## Lessons Learned

1. **Measure early**: Performance experiments should be done before full implementation
2. **Tree-sitter is sufficient**: The existing tree-sitter AST provides enough navigation capabilities
3. **Lazy > Eager**: Computing paths on-demand is much more efficient than storing them everywhere
4. **PathTracker works well**: The existing PathTracker in resolution is the right tool for the job

## Files Modified (Experimental - Not Committed)

- `src/yaml/parsing/ast.rs` - Added path field to SrcMeta
- `src/yaml/parsing/parser.rs` - Added PathTracker integration
- `src/yaml/resolution/resolver.rs` - Updated test SrcMeta creation

## Benchmark Data

- **Before**: `tmp/parsing_optimization_benchmark-2025-06-16-before-path-tracking.txt`
- **After**: `tmp/parsing_optimization_benchmark-2025-06-16-after-path-tracking.txt`