# Comprehensive Test Coverage Implementation Plan

## Overview

This plan addresses critical test coverage gaps in the YAML parsing system to ensure safe removal of the old parser. Based on the comprehensive analysis, we need to add ~50 new test cases across 4 categories.

## Execution Strategy

**Approach**: Incremental implementation with validation at each step
- Add test categories one at a time
- Run full test suite after each category
- Commit working increments to maintain stability

## Phase 1: Block-Style Tag Parsing Tests ⭐ **CRITICAL**

**Files to modify**: `src/yaml/parsing_w_loc/test.rs`

### Test Cases to Add:

1. **Basic block-style preprocessing tags**
```yaml
config: !$let
  database_url: "postgres://localhost"
  in: "{{database_url}}/myapp"
```

2. **Nested block-style tags**
```yaml
resources: !$map
  items: !$ servers
  template: !$merge
    - !$ baseTemplate
    - region: "{{item.name}}"
```

3. **Mixed flow and block styles**
```yaml
env: !$if { test: !$ isProd, then: "production", else: !$let { debug: true, in: "{{debug}}" } }
```

4. **Complex indentation scenarios**
```yaml
data: !$mapValues
  items:
    server1: { type: "web", port: 80 }
    server2: { type: "api", port: 8080 }
  template: "{{item.type}}:{{item.port}}"
```

**Success Criteria**: All block-style tags correctly parse with proper content association

## Phase 2: YAML Specification Compatibility Tests

**Files to modify**: `src/yaml/parsing_w_loc/diagnostic_tests.rs` (avoid creating new files)

### Test Categories:

1. **YAML 1.1 Boolean Forms**
   - Test: `yes/no/on/off/YES/NO/On/Off/true/false/TRUE/FALSE`
   - Ensure CloudFormation compatibility

2. **Numeric Format Compatibility**
   - Octal: `0o10` → 8
   - Hexadecimal: `0x10` → 16  
   - Float forms: `10.`, `.5`, `1e10`, `1.5e-3`

3. **String Escaping Edge Cases**
   - Unicode escapes: `"\u0041"` → `"A"`
   - Control characters: `"\n\t\r"`
   - Quote escaping in different contexts

4. **Anchor and Alias Handling** (when supported)
   - Basic anchors: `&anchor` and `*anchor`
   - Complex reference scenarios

**Success Criteria**: 100% compatibility with YAML 1.1 spec (CloudFormation standard)

## Phase 3: Tree-Sitter Robustness Tests

**Files to modify**: `src/yaml/parsing_w_loc/diagnostic_tests.rs`

### Error Recovery Test Cases:

1. **Malformed Structure Handling**
   - Unclosed brackets: `key: [\n  item`
   - Indentation errors: Mixed tabs/spaces
   - Unclosed quotes: `key: "unclosed`

2. **Unicode and Character Encoding**
   - Unicode keys: `unicode_key_🚀: value`
   - Emoji values: `key: "🎉"`
   - Mixed encoding scenarios

3. **Tree-Sitter Parsing Edge Cases**
   - Very long lines (>10KB)
   - Deep nesting (>100 levels)
   - Large documents (>1MB)

4. **Syntax Error Positioning**
   - Validate error locations are accurate
   - Test multi-byte character handling in positions

**Success Criteria**: Graceful error handling with precise location reporting

## Phase 4: Complex Integration Scenarios

**Files to create**: `src/yaml/parsing_w_loc/integration_tests.rs`

### Real-World Pattern Tests:

1. **CloudFormation Template Patterns**
   - AWS SAM templates with preprocessing
   - Nested stack references
   - Complex parameter substitution

2. **Deep Preprocessing Chains**
```yaml
result: !$map
  items: !$groupBy
    items: !$map
      items: !$ rawData  
      template: !$merge
        - !$ item
        - computed: !$if
            test: !$eq [!$ item.type, "special"]
            then: !$let
              factor: 2.5
              in: !$ "{{multiply item.value factor}}"
            else: !$ item.value
    key: !$ item.category
  template: !$merge
    - category: !$ item.key
    - items: !$ item.value
    - summary: !$map
        items: !$ item.value
        template: !$ item.computed
```

3. **Import System Edge Cases**
   - Circular import detection
   - Relative path resolution
   - Security boundary validation

4. **Performance Stress Tests**
   - Large file parsing (>1000 resources)
   - Memory usage validation
   - Parsing time benchmarks

**Success Criteria**: Handle realistic CloudFormation complexity without degradation

## Phase 5: Compatibility Validation Suite

**Files to modify**: `src/yaml/parsing_w_loc/compatibility_test.rs`

### Enhanced Compatibility Tests:

1. **Error Message Consistency**
   - Compare error messages between parsers
   - Validate error codes match
   - Check error position accuracy

2. **AST Structure Validation**
   - Deep comparison utilities
   - Metadata preservation checks
   - Edge case handling comparison

3. **Performance Benchmarking**
   - Parse time comparison
   - Memory usage analysis  
   - Scalability testing

**Success Criteria**: <5% performance difference, identical functional behavior

## Implementation Order

1. **Day 1**: Phase 1 - Block-style tag parsing (highest risk)
2. **Day 2**: Phase 2 - YAML spec compatibility  
3. **Day 3**: Phase 3 - Robustness and error handling
4. **Day 4**: Phase 4 - Complex integration scenarios
5. **Day 5**: Phase 5 - Enhanced compatibility validation

## Success Metrics

- **Coverage**: Add 50+ new test cases
- **Compatibility**: 100% pass rate on all existing functionality
- **Performance**: <5% deviation from original parser
- **Robustness**: Graceful handling of all malformed input
- **Documentation**: Clear test categories and maintenance guide

## Risk Mitigation

- **Incremental commits** after each phase
- **Full test suite execution** before proceeding to next phase  
- **Rollback plan** if any phase breaks existing functionality
- **Performance monitoring** throughout implementation

## Post-Implementation

1. **Update CI pipeline** to include new test categories
2. **Document test maintenance** procedures
3. **Create test coverage report** for release notes
4. **Plan old parser removal** based on validation results

## Files to Create/Modify

- `src/yaml/parsing_w_loc/yaml_spec_tests.rs` (new)
- `src/yaml/parsing_w_loc/integration_tests.rs` (new)  
- `src/yaml/parsing_w_loc/test.rs` (modify)
- `src/yaml/parsing_w_loc/diagnostic_tests.rs` (modify)
- `src/yaml/parsing_w_loc/compatibility_test.rs` (modify)

Total estimated effort: 5 days of focused implementation with validation.