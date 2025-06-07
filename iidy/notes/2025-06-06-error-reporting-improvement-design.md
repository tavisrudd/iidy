# Error Reporting Improvement Design

## Overview

This document analyzes current error reporting in iidy's YAML preprocessing system and designs comprehensive improvements inspired by Rust's excellent diagnostic messages.

## Current Error Message Analysis

### Testing Results Summary

| Error Type | Current Message Quality | Key Issues |
|------------|------------------------|------------|
| YAML Syntax | Poor | Technical parser language, no source context |
| Variable Not Found | Good | Has file/path info, helpful guidance |
| Import Errors | Fair | Vague language, raw OS errors |
| Type Mismatch | Poor | No context, no location |
| Handlebars | Good | Line/col info, shows template |
| Tag Syntax | Poor | No location, generic messages |

### Specific Current Examples

**YAML Syntax Error:**
```
did not find expected ',' or ']' at line 3 column 18, while parsing a flow sequence at line 2 column 7
```

**Variable Not Found (Best Current Example):**
```
Variable 'nonexistent_variable' not found in environment in file 'test_errors/variable_not_found.yaml' at path '<root>.result'
Only variables from $defs, $imports, and local scoped variables (like 'item' in !$map) are available.
```

**Type Mismatch (Worst Current Example):**
```
Map items must be a sequence
```

## Improved Error Message Design

Inspired by Rust's diagnostic format: `error: description --> file:line:col`

### Target Format Template
```
error: {error_description}
  --> {file}:{line}:{column}
   |
{line_num} | {source_line}
   |       {highlight_caret} {inline_description}
{line_num} | {next_line_if_relevant}
   |
   = note: {explanatory_note}
   = help: {suggestion_or_fix}
   = help: {additional_suggestion}
```

### Example Improvements

**1. YAML Syntax Error - Improved:**
```
error: invalid YAML syntax
  --> config.yaml:2:7
   |
 2 | key2: [invalid, syntax
   |       ^^^^^^^^^^^^^^^^ expected closing bracket ']'
 3 |   missing_bracket: true
   |
   = help: YAML flow sequences must be properly closed
   = note: try adding ']' at the end of line 2
```

**2. Variable Reference Error - Improved:**
```
error: variable 'nonexistent_variable' not found
  --> template.yaml:4:9
   |
 4 | result: !$ nonexistent_variable
   |            ^^^^^^^^^^^^^^^^^^^^ variable not defined
   |
   = note: only variables from $defs, $imports, and local scoped variables are available
   = help: available variables in this scope: existing_var
   = help: did you mean 'existing_var'?
```

**3. Type Mismatch Error - Improved:**
```
error: type mismatch in map operation
  --> template.yaml:5:10
   |
 5 |   items: !$ not_an_array
   |          ^^^^^^^^^^^^^^^ expected array, found string
 6 |   template: "{{item}}"
   |
   = note: !$map requires 'items' to be an array or sequence
   = help: the variable 'not_an_array' contains: "I am a string"
   = help: try using !$split to convert a string to an array
```

## Implementation Options Analysis

### Core Infrastructure Requirements

#### 1. Source Location Tracking
**Option A: Tree-sitter Integration** ⭐ (Recommended)
- **Pros**: Precise byte-level positioning, robust parsing, industry standard
- **Cons**: Additional dependency, learning curve
- **Implementation**: Use tree-sitter-yaml for location tracking alongside serde_yaml for logic
- **Libraries**: `tree-sitter`, `tree-sitter-yaml`

**Option B: Custom Source Maps**
- **Pros**: Lightweight, integrated with existing parser
- **Cons**: Complex to maintain, potential accuracy issues
- **Implementation**: Track byte positions during serde_yaml parsing

**Option C: Hybrid Approach**
- **Pros**: Best of both worlds, fallback capabilities
- **Cons**: Most complex implementation
- **Implementation**: Primary tree-sitter with serde_yaml fallback

#### 2. Error Context System
**Option A: Enhanced ProcessingEnv** ⭐ (Recommended)
- **Pros**: Builds on existing stack frame system, minimal disruption
- **Cons**: May need some refactoring
- **Implementation**: Extend ProcessingEnv with source position tracking

**Option B: Separate Error Context**
- **Pros**: Clean separation, easier to test
- **Cons**: Parallel tracking complexity
- **Implementation**: New ErrorContext struct tracking file/line/column

#### 3. Source Code Display
**Option A: Source Cache** ⭐ (Recommended)
- **Pros**: Fast error display, no I/O during errors
- **Cons**: Higher memory usage
- **Implementation**: Cache source content during initial parsing

**Option B: On-Demand Source Reading**
- **Pros**: Lower memory usage
- **Cons**: I/O overhead, risk of file changes

### Feature-Specific Implementation Options

#### 4. Suggestion System
**Option A: Rule-Based Suggestions** ⭐ (Phase 1)
- **Implementation**: Hard-coded suggestion rules for common error patterns
- **Examples**:
  - Variable name typos → fuzzy matching against available variables
  - Missing tag fields → suggest required fields with examples
  - Import path errors → suggest common path patterns

**Option B: Fuzzy Matching** (Phase 2)
- **Libraries**: `strsim` for Levenshtein distance
- **Use Cases**: Variable name suggestions, tag name corrections

**Option C: Context-Aware Suggestions** (Phase 3)
- **Implementation**: Different suggestion strategies based on error context
- **Examples**: 
  - In CloudFormation context, suggest CF-specific patterns
  - In import context, suggest relative vs absolute path alternatives

#### 5. Error Message Formatting
**Option A: Structured Error Types** ⭐ (Recommended)
- **Implementation**: Enum-based error types with custom Display traits
- **Benefits**: Type-safe, compile-time checked, consistent formatting
- **Example Structure**:
```rust
pub enum PreprocessingError {
    VariableNotFound { 
        variable: String, 
        location: SourceLocation, 
        available_vars: Vec<String> 
    },
    TypeMismatch { 
        expected: String, 
        found: String, 
        location: SourceLocation,
        context: String 
    },
    // ... other variants
}
```

**Option B: Template System**
- **Implementation**: Use handlebars for error message templates
- **Pros**: Easy to modify, consistent formatting
- **Cons**: Performance overhead, complexity

#### 6. Advanced Features
**Option A: Multiple Error Collection** (Phase 2)
- **Implementation**: ErrorCollector pattern that continues parsing after recoverable errors
- **Benefits**: Users can fix multiple issues in one cycle
- **Challenges**: Error recovery logic complexity

**Option B: Interactive Help System** (Phase 3)
- **Implementation**: Error codes with `--explain` flag like Rust
- **Example**: `iidy explain E0001` for detailed error explanations
- **Benefits**: Rich help without cluttering basic error messages

### Technology Stack Recommendations

#### Required Dependencies
- **tree-sitter**: `~0.20`
- **tree-sitter-yaml**: `~0.5` 
- **strsim**: `~0.10` (for fuzzy matching)

#### Optional Dependencies
- **annotate-snippets**: `~0.9` (for Rust-style error display)
- **console**: `~0.15` (for colored terminal output)
- **similar**: `~2.2` (for diff-style suggestions)

## Implementation Roadmap

### Phase 1: Foundation (High Impact, Medium Effort)
**Estimated Effort**: 2-3 days
1. **Enhanced Error Types**
   - Create structured error enum with source locations
   - Implement Display traits for Rust-style formatting
   - Update all error creation sites

2. **Basic Source Location Tracking**
   - Integrate tree-sitter-yaml for position tracking
   - Map AST nodes to source positions
   - Update error creation to include locations

3. **Rule-Based Suggestions**
   - Implement fuzzy matching for variable names
   - Add common fix suggestions for tag syntax errors
   - Suggest available variables in scope

### Phase 2: Rich Diagnostics (High Impact, High Effort)
**Estimated Effort**: 4-5 days
1. **Source Code Context Display**
   - Implement source line display with highlighting
   - Add caret pointing to specific error locations
   - Support multi-line error contexts

2. **Multiple Error Collection**
   - Implement error recovery in parser
   - Collect and display multiple errors in single run
   - Prioritize errors by severity

3. **Advanced Suggestions**
   - Context-aware suggestion system
   - Import path resolution suggestions
   - CloudFormation-specific help

### Phase 3: Interactive Features (Medium Impact, High Effort)
**Estimated Effort**: 3-4 days
1. **Error Code System**
   - Assign unique codes to error types
   - Implement `--explain` functionality
   - Create comprehensive error documentation

2. **Error Recovery and Auto-Fix**
   - Suggest automatic fixes where safe
   - Implement basic error recovery for continued parsing
   - Preview mode for suggested changes

### Success Metrics

#### User Experience Metrics
- **Error Clarity**: Users can understand what went wrong without external help
- **Fix Guidance**: Error messages provide actionable steps to resolve issues
- **Context Awareness**: Users can quickly locate the problematic code
- **Learning Support**: Error messages help users learn correct syntax

#### Technical Metrics
- **Performance Impact**: Error reporting overhead <5% of total processing time
- **Memory Usage**: Source caching memory overhead <10% of document size
- **Coverage**: >90% of error scenarios have helpful suggestions

#### Comparison Benchmarks
- **Before**: "Map items must be a sequence"
- **After**: Detailed error with location, context, available variables, and fix suggestions

## Conclusion

## Error ID System Design

### Recommended Error ID Format: `IY` + Category + Number

**Benefits:**
- **IY prefix**: Clear tool identification (iidy YAML)
- **Category-based**: Logical grouping for documentation and debugging
- **Numeric**: Stable, language-agnostic, easy to reference in discussions
- **Expandable**: Can add new categories without ID conflicts

### Category Hierarchy

**1xxx - YAML Syntax & Parsing**
- IY1001: Invalid YAML syntax
- IY1002: YAML version mismatch  
- IY1003: Unsupported YAML feature
- IY1004: Malformed YAML structure
- IY1005: YAML merge key usage (not supported in 1.2)

**2xxx - Variable & Scope Errors**
- IY2001: Variable not found
- IY2002: Variable name collision
- IY2003: Invalid variable name
- IY2004: Circular variable reference
- IY2005: Variable access out of scope

**3xxx - Import & Loading Errors**
- IY3001: Import file not found
- IY3002: Import URL unreachable
- IY3003: Import authentication failure
- IY3004: Import circular dependency
- IY3005: Import format not supported
- IY3006: Environment variable not found
- IY3007: Git command failure
- IY3008: S3 access denied
- IY3009: SSM parameter not found
- IY3010: CloudFormation stack not found

**4xxx - Tag Syntax & Structure Errors**
- IY4001: Unknown preprocessing tag
- IY4002: Missing required tag field
- IY4003: Invalid tag field value
- IY4004: Incompatible tag combination
- IY4005: Tag syntax error

**5xxx - Type & Validation Errors**
- IY5001: Type mismatch in operation
- IY5002: Invalid array operation on non-array
- IY5003: Invalid object operation on non-object
- IY5004: Division by zero
- IY5005: Invalid comparison operation
- IY5006: String operation on non-string

**6xxx - Template & Handlebars Errors**
- IY6001: Handlebars syntax error
- IY6002: Unknown handlebars helper
- IY6003: Handlebars helper argument error
- IY6004: Template compilation failure
- IY6005: Template execution error

**7xxx - CloudFormation Specific**
- IY7001: Invalid CloudFormation intrinsic function
- IY7002: CloudFormation reference error
- IY7003: CloudFormation resource dependency issue
- IY7004: CloudFormation template size limit

**8xxx - Configuration & Setup**
- IY8001: Invalid command line argument
- IY8002: Missing required configuration
- IY8003: Configuration file not found
- IY8004: AWS credentials not configured
- IY8005: Unsupported file format

**9xxx - Internal & System Errors**
- IY9001: Internal processing error
- IY9002: Memory allocation failure
- IY9003: File system permission denied
- IY9004: Network connectivity issue
- IY9005: Unexpected system error

### Enhanced Error Format with IDs

```
error[IY2001]: variable 'nonexistent_variable' not found
  --> template.yaml:4:9
   |
 4 | result: !$ nonexistent_variable
   |            ^^^^^^^^^^^^^^^^^^^^ variable not defined
   |
   = note: only variables from $defs, $imports, and local scoped variables are available
   = help: available variables in this scope: existing_var
   = help: did you mean 'existing_var'?
   = help: for more information about this error, try `iidy explain IY2001`
```

### CLI Integration

**Error Explanation Command:**
```bash
$ iidy explain IY2001
Error IY2001: Variable Not Found

This error occurs when you reference a variable that hasn't been defined
in the current scope.

COMMON CAUSES:
• Typo in variable name
• Variable defined in different scope
• Missing $defs or $imports section

EXAMPLES:
[Detailed examples with correct/incorrect usage]

SEE ALSO:
• IY2002: Variable name collision
• IY3001: Import file not found
```

## Deep Implementation Challenges

### 1. Error ID Granularity Balance

**Challenge**: Finding the right level of specificity for error IDs.

**Too Broad Problem:**
```rust
IY2001: VariableError  // Could mean "not found", "collision", "scope issue"
```

**Too Narrow Problem:**
```rust
IY2001: VariableNotFoundInDefs
IY2002: VariableNotFoundInImports  
IY2003: VariableNotFoundInMapScope
// ... 50 different "not found" variants
```

**Recommended Solution: Context-Aware Same ID**
```rust
IY2001: VariableNotFound  // Same ID, context determines specific message
```

### 2. Error Chaining & Causation

**Challenge**: Many errors are chains (import fails → variable unavailable → processing fails).

**Example Scenario:**
```yaml
$imports:
  config: file:missing.yaml
result: !$ config.database_url
```

**Solution Options:**

**Primary + Secondary Display:**
```
error[IY3001]: import file not found
  --> template.yaml:2:12
   |
 2 |   config: file:missing.yaml
   |           ^^^^^^^^^^^^^^^^^ file not found

note[IY2001]: this also caused variable 'config' to be unavailable
  --> template.yaml:3:9
```

**Root Cause Focus:**
```
error[IY3001]: import file not found (this will cause variable errors)
  --> template.yaml:2:12

subsequent error[IY2001]: variable 'config' not found (caused by IY3001)
  --> template.yaml:3:9
```

### 3. Error ID Evolution & Versioning

**Challenge**: Maintaining backward compatibility as error semantics evolve.

**Scenario**: Need to split a broad error into specific cases:
```rust
// v1.0: Broad error
IY4001: InvalidTagSyntax

// v2.0: More specific (breaking change?)
IY4001: InvalidTagSyntax        // Now means "malformed structure"  
IY4002: MissingRequiredField    // New specific case
IY4003: InvalidFieldValue       // New specific case
```

**Solutions:**

**Never Change Approach (Rust-style):**
- Once assigned, error ID meanings never change
- New errors get new IDs always
- Perfect compatibility, but potential ID space bloat

**Deprecation + Migration:**
```rust
IY4001: InvalidTagSyntax [DEPRECATED: use IY4002 for missing fields]
IY4002: MissingRequiredField    // New specific error
```

**Versioned IDs:**
```rust
IY4001@v1: InvalidTagSyntax     // Old broad meaning
IY4001@v2: InvalidTagSyntax     // New narrow meaning
```

### 4. Multiple Error Aggregation

**Challenge**: How to display multiple errors with IDs effectively.

**Sequential Display (Simple):**
```
error[IY2001]: variable 'var1' not found
error[IY2001]: variable 'var2' not found  
error[IY4002]: missing 'template' field
```

**Grouped by Error ID (Efficient):**
```
error[IY2001]: variables not found (2 occurrences)
  --> template.yaml:4:11 (var1)
  --> template.yaml:5:11 (var2)

error[IY4002]: missing required field
  --> template.yaml:6:3 (missing 'template')
```

**Summary + Details (User Choice):**
```
Found 3 errors: IY2001 (2×), IY4002 (1×)
For details: iidy explain IY2001 IY4002

=== Detailed Errors ===
[Full error details if requested]
```

### 5. Source Location Precision

**Challenge**: What exactly should the error location point to in complex nested structures?

**Complex Example:**
```yaml
computed: !$join
  array: 
    - !$ app_name          # Variable error here
    - !$if                 # Conditional with error
        condition: !$ production_mode
        then: "prod"
        else: "dev"
```

**Location Options:**
- **Precise Point**: Exact `!$ app_name` location
- **Operation Context**: Point to `!$join` that contains the error
- **Full Path**: Show the complete evaluation path

**Recommended: Precise Point + Context**
```
error[IY2001]: variable 'app_name' not found
  --> template.yaml:3:7
   |
 1 | computed: !$join
   |           ------ in join operation
 2 |   array:
 3 |     - !$ app_name
   |          ^^^^^^^^ variable not defined
   |
   = note: error path: computed → !$join → array[0] → !$ app_name
```

### 6. Performance vs. Precision Tradeoffs

**Challenge**: Rich error reporting has significant overhead.

**Overhead Sources:**
- Source position tracking: Memory overhead for every AST node
- Multiple error collection: Complex error recovery, continued parsing
- Suggestion generation: Fuzzy matching, context analysis
- Source content caching: Memory usage for error display

**Mitigation Strategies:**

**Error Detail Levels:**
```bash
iidy render --error-level=basic    # Fast, minimal details
iidy render --error-level=detailed # Rich diagnostics (default)
iidy render --error-level=debug    # Maximum verbosity
```

**Lazy Error Enhancement:**
```rust
struct LazyError {
    error_id: ErrorId,
    location: SourceLocation,
    context: Box<dyn FnOnce() -> ErrorContext>, // Generated on demand
}
```

### 7. Internationalization Complexity

**Challenge**: Error IDs must remain stable across languages while content adapts.

**What Stays the Same:**
- ✅ Error IDs: Always `IY2001`
- ✅ Source locations: File:line:col
- ✅ Code examples: YAML syntax is universal

**What Must Translate:**
- ❌ Error descriptions and messages
- ❌ Help suggestions and guidance
- ❌ Documentation and explanations

**Complex Localization Issues:**

**Pluralization Complexity:**
```rust
// English: "1 error", "2 errors"
// Polish: "1 błąd", "2 błędy", "5 błędów" (3 different forms!)
// Russian: Even more complex pluralization rules
```

**Cultural Context:**
```rust
// Western: Point with ^^^^ caret
// Arabic/Hebrew: Right-to-left text layout
// CJK: Wide characters affect visual alignment
```

**Technical Term Translation:**
- "variable" → Spanish: "variable" vs "valor" vs "identificador"?
- "array" → Spanish: "matriz" vs "arreglo" vs "lista"?
- Context determines best translation choice

### 8. Documentation Maintenance Challenges

**Challenge**: Keeping error documentation synchronized with evolving code.

**Problems:**
- Code adds new error conditions → Docs must update
- Error message improvements → Examples become stale  
- Cross-references between errors → Link maintenance

**Solutions:**

**Code-Generated Documentation:**
```rust
#[error_doc(
    id = "IY4002",
    category = "Tag Syntax",
    examples = include_str!("../docs/examples/IY4002.md"),
    see_also = ["IY4001", "IY4003"]
)]
pub struct MapTagError { ... }
```

**Automated Validation:**
```rust
#[test]
fn validate_all_error_docs_exist() {
    for error_id in ErrorId::all() {
        assert!(error_doc_exists(error_id));
        assert!(error_doc_has_required_sections(error_id));
    }
}
```

### 9. CLI UX Edge Cases

**Error Code Input Flexibility:**
```bash
iidy explain IY2001        # Standard format
iidy explain iy2001        # Case insensitive?
iidy explain 2001          # Auto-prefix with IY?
iidy explain "variable"    # Natural language lookup?
```

**Context-Aware Help:**
```bash
# After getting an error:
$ iidy explain --last      # Explain the last error encountered
$ iidy explain --context=template.yaml  # Context-specific help
```

**Batch Operations:**
```bash
iidy explain IY2001 IY4002  # Multiple errors at once
iidy explain IY2*           # Category wildcards
iidy explain --category=2   # All 2xxx errors
```

## Implementation Roadmap Revision

### Phase 1: Error ID Infrastructure (2-3 days)
1. **Error ID Enum & Display**
   - Define comprehensive ErrorId enum with categories
   - Implement Display traits for Rust-style formatting
   - Update all error creation sites to include IDs

2. **Basic CLI Integration**
   - Add `iidy explain <error_id>` command
   - Implement basic error explanations
   - Add help hints to error messages

3. **Foundation Testing**
   - Test error ID stability across error scenarios
   - Validate error ID assignment consistency
   - Test CLI explain functionality

### Phase 2: Rich Diagnostics (4-5 days)
1. **Source Location Enhancement**
   - Integrate tree-sitter for precise positioning
   - Implement source context display with highlighting
   - Add multi-line error context support

2. **Error Chaining & Aggregation**
   - Implement error cause tracking
   - Design multiple error display strategies
   - Add error recovery for continued parsing

3. **Advanced Suggestions**
   - Fuzzy matching for variable names
   - Context-aware suggestion system
   - CloudFormation-specific guidance

### Phase 3: Production Polish (3-4 days)
1. **Documentation System**
   - Create comprehensive error documentation
   - Build automated doc validation
   - Implement online error reference

2. **Performance Optimization**
   - Implement lazy error detail generation
   - Add configurable error detail levels
   - Optimize memory usage for large files

3. **Internationalization Foundation**
   - Design translation-friendly error architecture
   - Implement locale-aware error formatting
   - Create translation validation framework

## Conclusion

This comprehensive error reporting improvement will significantly enhance the user experience of iidy's YAML preprocessing system. By implementing Rust-style diagnostics with stable error IDs, source context, helpful suggestions, and clear explanations, we can transform error messages from cryptic technical details into helpful guides that educate users and accelerate problem resolution.

The error ID system adds professional polish and provides stable references for documentation, community discussions, and tooling integration. The detailed analysis of implementation challenges ensures we can build a robust system that scales with the project's growth while maintaining excellent user experience.

The phased approach allows for incremental improvement while maintaining system stability, with each phase providing immediate user value before moving to more advanced features.

## Implementation Progress (2025-06-06)

### Feature Flag Implementation ✅

Successfully implemented enhanced error reporting behind a feature flag `enhanced-errors` for safe testing:

**1. Cargo.toml Feature Definition:**
```toml
[features]
default = []
enhanced-errors = []
```

**2. Error Wrapper Module (`src/yaml/error_wrapper.rs`):**
- Provides wrapper functions that switch between basic and enhanced error reporting
- `variable_not_found_error()` - switches based on feature flag
- `type_mismatch_error()` - switches based on feature flag  
- `missing_required_field_error()` - switches based on feature flag

**3. Conditional Module Loading:**
- Enhanced error modules only compiled when feature is enabled
- `#[cfg(feature = "enhanced-errors")]` guards on modules and imports
- Maintains backward compatibility when feature is disabled

**4. CLI Integration:**
- Added `explain` command (only available with feature flag)
- `iidy explain IY2001` - shows detailed error documentation
- Supports multiple error codes: `iidy explain IY2001 IY4002`

**5. Integration Points:**
- Updated `src/yaml/tags.rs` to use error wrapper for variable not found errors
- Maintains existing error format when feature disabled
- Collects available variables for enhanced suggestions when enabled

### Testing Strategy

**To test enhanced errors:**
```bash
# Build with enhanced errors feature
cargo build --features enhanced-errors

# Test the explain command
cargo run --features enhanced-errors -- explain IY2001

# Run spike tests
cargo test --features enhanced-errors error_spike_tests
```

**To ensure backward compatibility:**
```bash
# Build without feature (default)
cargo build

# Run all tests - should pass with existing error format
cargo test
```

### Green Commit Safety ✅

This implementation is safe for a green commit because:

1. **Feature flag protection** - Enhanced errors only active when explicitly enabled
2. **Backward compatible** - Default behavior unchanged
3. **No breaking changes** - Existing error handling preserved
4. **Minimal intrusion** - Only one integration point updated (variable not found)
5. **Tested isolation** - Can be tested independently without affecting production

### Next Steps

1. **Gradual Integration:**
   - Add more error wrapper usage points one at a time
   - Test each integration thoroughly
   - Monitor performance impact

2. **Documentation:**
   - Create more error documentation files
   - Add examples and common solutions
   - Build comprehensive error reference

3. **Source Location Tracking:**
   - Investigate tree-sitter integration
   - Add line/column tracking to errors
   - Implement source context display

4. **Production Readiness:**
   - Performance testing with feature enabled
   - Memory usage analysis
   - User feedback on error quality