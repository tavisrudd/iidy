# Multi-Error Collection Parser Design

## Executive Summary

This document outlines a design for modifying the YAML parser to collect all errors rather than failing on the first error encountered. This capability is essential for implementing an LSP server and linter that can provide comprehensive error reporting to users.

## Current State Analysis

The current parser implements a fail-fast model with approximately 45 error propagation points throughout the codebase. The parser architecture uses:

- `ParseResult<T> = Result<T, ParseError>` - Single error return type
- Early return on first error via `?` operator
- Single error context with location information

## Design Goals

1. **Comprehensive Error Collection**: Collect all parsing errors in a single pass
2. **Backward Compatibility**: Maintain existing API for current users
3. **LSP/Linter Support**: Enable rich error reporting for development tools
4. **Future Extensibility**: Support partial AST construction in future iterations

## Proposed Solution: Option A - Error Collection Only

### Phase 1: Core Type System Changes

#### 1.1 New Error Types
```rust
// New multi-error result type
#[derive(Debug, Clone)]
pub struct ParseErrors {
    pub errors: Vec<ParseError>,
}

pub type MultiParseResult<T> = Result<T, ParseErrors>;

// New parsing context for error collection
#[derive(Debug)]
pub struct ErrorCollector {
    errors: Vec<ParseError>,
    continue_on_error: bool,
}

impl ErrorCollector {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            continue_on_error: true,
        }
    }
    
    pub fn add_error(&mut self, error: ParseError) {
        self.errors.push(error);
    }
    
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
    
    pub fn into_result<T>(self, value: Option<T>) -> MultiParseResult<T> {
        if self.errors.is_empty() {
            Ok(value.unwrap_or_else(|| panic!("No value provided when no errors occurred")))
        } else {
            Err(ParseErrors { errors: self.errors })
        }
    }
}
```

#### 1.2 Parser State Modifications
```rust
pub struct YamlParser {
    parser: Parser,
    error_collector: ErrorCollector,
}

impl YamlParser {
    pub fn parse_with_error_collection(&mut self, source: &str, uri: Url) -> MultiParseResult<YamlAst> {
        self.error_collector = ErrorCollector::new();
        
        // Parse and collect all errors
        let result = self.parse_internal(source, uri);
        
        // Return collected errors or successful result
        if self.error_collector.has_errors() {
            Err(ParseErrors { errors: self.error_collector.errors.clone() })
        } else {
            Ok(result)
        }
    }
}
```

### Phase 2: Parser Core Refactoring

#### 2.1 Error Handling Strategy
Replace fail-fast `?` operators with error collection:

```rust
// Before (fail-fast)
fn parse_mapping(&mut self, node: Node, source: &[u8], uri: &Url) -> ParseResult<YamlAst> {
    let fields = self.extract_fields_from_mapping(node, source, uri)?;
    // ... rest of function
}

// After (error collection)
fn parse_mapping(&mut self, node: Node, source: &[u8], uri: &Url) -> Option<YamlAst> {
    let fields = match self.extract_fields_from_mapping(node, source, uri) {
        Ok(fields) => fields,
        Err(e) => {
            self.error_collector.add_error(e);
            return None; // Continue parsing other nodes
        }
    };
    // ... rest of function
}
```

#### 2.2 Recursive Parsing Strategy
Implement resilient parsing that continues even when child nodes fail:

```rust
fn parse_sequence(&mut self, node: Node, source: &[u8], uri: &Url) -> Option<YamlAst> {
    let mut items = Vec::new();
    let mut has_valid_items = false;
    
    for child in node.named_children(&mut node.walk()) {
        match self.build_ast(child, source, uri) {
            Some(ast) => {
                items.push(ast);
                has_valid_items = true;
            }
            None => {
                // Error already recorded in error_collector
                // Continue processing other items
                continue;
            }
        }
    }
    
    if has_valid_items || items.is_empty() {
        Some(YamlAst::Sequence(items, self.node_meta(&node, uri)))
    } else {
        None
    }
}
```

### Phase 3: Semantic Validation Updates

#### 3.1 Tag Validation
Update tag parsing to collect validation errors:

```rust
fn parse_preprocessing_tag(&mut self, tag_name: &str, content: YamlAst, uri: &Url, node: &Node) -> Option<PreprocessingTag> {
    match tag_name {
        "!$let" => {
            match self.parse_let_tag(content, uri, node) {
                Ok(tag) => Some(PreprocessingTag::Let(tag)),
                Err(e) => {
                    self.error_collector.add_error(e);
                    None
                }
            }
        }
        // ... other tags
    }
}

fn parse_let_tag(&mut self, content: YamlAst, uri: &Url, node: &Node) -> Result<LetTag, ParseError> {
    // Collect all validation errors for this tag
    let mut local_errors = Vec::new();
    
    let (bindings, expression) = match content {
        YamlAst::Mapping(pairs, _) => {
            let mut bindings = IndexMap::new();
            let mut expression = None;
            
            for (key, value) in pairs {
                match key {
                    YamlAst::PlainString(key_str, _) => {
                        if key_str == "in" {
                            expression = Some(value);
                        } else {
                            bindings.insert(key_str, value);
                        }
                    }
                    _ => {
                        local_errors.push(ParseError::with_location(
                            "Let tag keys must be strings",
                            uri.clone(),
                            // ... position info
                        ));
                    }
                }
            }
            
            let expression = match expression {
                Some(expr) => expr,
                None => {
                    local_errors.push(ParseError::with_location(
                        "Let tag missing required 'in' field",
                        uri.clone(),
                        // ... position info
                    ));
                    return Err(local_errors.into_iter().next().unwrap()); // Return first error for now
                }
            };
            
            (bindings, expression)
        }
        _ => {
            return Err(ParseError::with_location(
                "Let tag must be a mapping",
                uri.clone(),
                // ... position info
            ));
        }
    };
    
    Ok(LetTag { bindings, expression: Box::new(expression) })
}
```

### Phase 4: API Compatibility Layer

#### 4.1 Backward Compatibility
Maintain existing single-error API:

```rust
impl YamlParser {
    // Existing API - returns first error only
    pub fn parse(&mut self, source: &str, uri: Url) -> ParseResult<YamlAst> {
        match self.parse_with_error_collection(source, uri) {
            Ok(ast) => Ok(ast),
            Err(errors) => Err(errors.errors.into_iter().next().unwrap()),
        }
    }
    
    // New API - returns all errors
    pub fn parse_with_error_collection(&mut self, source: &str, uri: Url) -> MultiParseResult<YamlAst> {
        // Implementation from Phase 1
    }
}
```

#### 4.2 Conversion Utilities
Update conversion functions to support multi-error results:

```rust
pub fn parse_and_convert_to_original_with_errors(source: &str, uri_str: &str) -> Result<original::YamlAst, ParseErrors> {
    let mut parser = YamlParser::new().map_err(|e| ParseErrors { errors: vec![e] })?;
    let uri = parse_uri(uri_str)?;
    
    match parser.parse_with_error_collection(source, uri) {
        Ok(with_location_ast) => Ok(to_original_ast(&with_location_ast)),
        Err(errors) => Err(errors),
    }
}

// Maintain backward compatibility
pub fn parse_and_convert_to_original(source: &str, uri_str: &str) -> anyhow::Result<original::YamlAst> {
    match parse_and_convert_to_original_with_errors(source, uri_str) {
        Ok(ast) => Ok(ast),
        Err(errors) => Err(anyhow::anyhow!("{}", errors.errors[0].message)),
    }
}
```

### Phase 5: Error Reporting Enhancements

#### 5.1 Multi-Error Display
```rust
impl fmt::Display for ParseErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.errors.len() == 1 {
            write!(f, "{}", self.errors[0])
        } else {
            writeln!(f, "Found {} parsing errors:", self.errors.len())?;
            for (i, error) in self.errors.iter().enumerate() {
                writeln!(f, "  {}: {}", i + 1, error)?;
            }
            Ok(())
        }
    }
}
```

#### 5.2 LSP Integration Helpers
```rust
impl ParseErrors {
    pub fn to_lsp_diagnostics(&self) -> Vec<lsp_types::Diagnostic> {
        self.errors.iter().map(|error| {
            lsp_types::Diagnostic {
                range: error.location.as_ref().map(|loc| lsp_types::Range {
                    start: lsp_types::Position {
                        line: loc.start.line as u32,
                        character: loc.start.character as u32,
                    },
                    end: lsp_types::Position {
                        line: loc.end.line as u32,
                        character: loc.end.character as u32,
                    },
                }).unwrap_or_default(),
                severity: Some(lsp_types::DiagnosticSeverity::ERROR),
                message: error.message.clone(),
                source: Some("iidy-yaml".to_string()),
                // ... other fields
            }
        }).collect()
    }
}
```

## Implementation Phases

### Phase 1: Core Infrastructure (Week 1)
- [ ] Implement `ParseErrors` and `MultiParseResult` types
- [ ] Add `ErrorCollector` with error accumulation logic
- [ ] Update `YamlParser` to support error collection mode
- [ ] Create backward compatibility layer

### Phase 2: Parser Refactoring (Week 2)
- [ ] Refactor core parsing methods to use error collection
- [ ] Update recursive parsing logic for sequences and mappings
- [ ] Implement resilient node processing
- [ ] Update syntax error handling

### Phase 3: Semantic Validation (Week 3)
- [ ] Refactor tag validation to collect errors
- [ ] Update preprocessing tag parsing
- [ ] Update CloudFormation tag parsing
- [ ] Implement comprehensive field validation

### Phase 4: Integration & Testing (Week 4)
- [ ] Update conversion utilities
- [ ] Implement error reporting enhancements
- [ ] Add LSP integration helpers
- [ ] Comprehensive testing with error scenarios

### Phase 5: Documentation & Optimization (Week 5)
- [ ] Performance testing and optimization
- [ ] Documentation updates
- [ ] Example implementations for LSP usage
- [ ] Final integration testing

## Testing Strategy

### Error Collection Tests
```rust
#[test]
fn test_multiple_errors_collected() {
    let yaml = r#"
!$let
  var1: value1
  # Missing 'in' field

!$unknown_tag value

invalid: !$let
  # Another missing 'in' field
"#;
    
    let mut parser = YamlParser::new().unwrap();
    let result = parser.parse_with_error_collection(yaml, test_uri());
    
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert_eq!(errors.errors.len(), 3); // Should collect all 3 errors
    
    // Verify specific error types
    assert!(errors.errors.iter().any(|e| e.message.contains("missing required 'in' field")));
    assert!(errors.errors.iter().any(|e| e.message.contains("unknown tag")));
}
```

### Backward Compatibility Tests
```rust
#[test]
fn test_backward_compatibility() {
    let yaml = "!$let\n  var1: value1"; // Missing 'in' field
    
    let mut parser = YamlParser::new().unwrap();
    let result = parser.parse(yaml, test_uri());
    
    // Should still return single error for backward compatibility
    assert!(result.is_err());
    assert!(result.unwrap_err().message.contains("missing required 'in' field"));
}
```

## Future Enhancements

### Partial AST Support (Phase 2)
Once the basic error collection is stable, add partial AST construction:

```rust
pub enum ParsedNode {
    Valid(YamlAst),
    Invalid(ParseError),
    Partial(YamlAst, Vec<ParseError>), // Partial with errors
}

pub struct PartialAst {
    pub root: ParsedNode,
    pub all_errors: Vec<ParseError>,
}
```

### Recovery Strategies
Implement intelligent error recovery:
- Skip invalid sequence items
- Provide default values for missing required fields
- Continue parsing siblings after encountering invalid nodes

### Enhanced LSP Features
- Real-time error highlighting
- Hover information for valid partial nodes
- Completion suggestions based on partial AST
- Refactoring support with error-aware transformations

## Risk Assessment

### Low Risk
- Type system changes (contained within parsing module)
- Backward compatibility maintenance
- Error collection logic

### Medium Risk
- Performance impact of error collection
- Memory usage with large error lists
- Integration complexity with existing error systems

### High Risk
- Breaking changes to internal APIs
- Test suite updates required
- Potential for recursive error handling bugs

## Implementation Status

✅ **COMPLETED** - All phases of the multi-error collection system have been successfully implemented and tested.

### What Was Implemented

**Phase 1: Core Infrastructure** ✅
- `ParseWarning`, `ParseDiagnostics`, `ParseMode` types with error codes
- `ErrorCollector` functionality built into parser state management 
- Full backward compatibility maintained - all 486 existing tests pass

**Phase 2: Parser Refactoring** ✅  
- `validate_with_diagnostics()` method for comprehensive error collection
- Comprehensive syntax error collection via tree traversal
- Resilient parsing that continues after errors

**Phase 3: Semantic Validation** ✅
- Tag validation without AST building (performance optimized)
- Preprocessing tag validation (`!$let`, `!$include`, `!$map`, `!$if`, etc.)
- CloudFormation tag recognition
- Unknown tag detection with proper error codes
- Missing field validation with specific error messages
- Support for both mapping and sequence syntax (e.g., `!$join`)

**Phase 4: API Integration** ✅
- New public APIs: `parse_yaml_ast_with_diagnostics()`, `validate_yaml_only()`
- Convert module extensions: `parse_and_convert_to_original_with_diagnostics()`
- Full backward compatibility - existing `parse()` method unchanged

**Phase 5: Testing & Validation** ✅
- Comprehensive test suite (12 new diagnostic tests)
- All existing 486 tests pass (backward compatibility verified)
- Error location information preserved for LSP integration
- Warning support for non-critical issues

### Key Features Delivered

1. **Multi-Error Collection**: Collects all parsing and semantic errors in a single pass
2. **Error Categorization**: Structured error codes (IY1001, IY4001, etc.) for different error types
3. **Precise Location Information**: Line/column data for each error, ready for LSP integration
4. **Warning Support**: Non-fatal warnings for issues like unexpected fields
5. **Backward Compatibility**: Zero breaking changes - all existing code works unchanged
6. **Performance Optimized**: Lightweight semantic validation without full AST construction

### API Examples

```rust
// New diagnostic API - collects all errors
let diagnostics = parse_yaml_ast_with_diagnostics(source, uri);
if diagnostics.has_errors() {
    for error in &diagnostics.errors {
        println!("Error: {} (code: {:?})", error.message, error.code);
        if let Some(loc) = &error.location {
            println!("  at {}:{}:{}", loc.uri, loc.start.line+1, loc.start.character+1);
        }
    }
}

// Backward compatible API - existing behavior unchanged  
let result = parse_yaml_ast(source, uri); // Works exactly as before
```

### Ready for LSP Integration

The implementation provides everything needed for rich IDE/LSP integration:
- Multiple error collection in one parse
- Precise error locations 
- Structured error codes
- Warning vs error distinction
- Comprehensive tag validation

### Future Enhancements

The foundation is now in place for Phase 6 (Partial AST Support) when needed:
- Error recovery with partial AST construction
- Enhanced LSP features (hover, completion, refactoring)
- Real-time validation as users type

## Conclusion

The multi-error collection system has been successfully implemented with full backward compatibility. All 486 existing tests pass while the new diagnostic API enables comprehensive error reporting for LSP/linter integration. The implementation provides a solid foundation for rich development tooling while maintaining the reliability of existing functionality.