# Plan: Use SrcMeta in YAML Error Reporting

## Executive Summary

The YAML parsing system now has precise line:col information via tree-sitter and SrcMeta, but error reporting still uses old heuristic-based location finding. This plan outlines steps to properly use SrcMeta throughout error construction and remove redundant location-finding code.

## Current State Analysis

### 1. SrcMeta Availability
- Every `YamlAst` node contains a `SrcMeta` field with:
  - `input_uri: Url` - Source file URL
  - `start: Position` - Start position (line, column)
  - `end: Position` - End position (line, column)
- The AST has a `meta()` method to access SrcMeta from any node

### 2. Current Error System Problems
- **Location Heuristics**: `src/yaml/errors/location.rs` and `src/yaml/errors/tree_sitter_location.rs` search for positions after the fact
- **Ignored SrcMeta**: Resolution code in `src/yaml/resolution/resolver_split_args.rs` has access to SrcMeta but doesn't pass it to error constructors
- **Redundant Modules**: `src/yaml/errors/wrapper.rs` and `src/yaml/errors/enhanced.rs` duplicate functionality
- **File Path Parsing**: Error functions in `wrapper.rs` extract line numbers from file paths like "file.yaml:10:5"
- **Panic Potential**: Multiple unsafe array accesses and string slicing operations in `wrapper.rs`

### 3. Where SrcMeta Is Available But Ignored

#### In Resolver Error Creation (`src/yaml/resolution/resolver_split_args.rs`):
```rust
// Current (ignores available SrcMeta):
let file_path = context.input_uri.as_deref().unwrap_or("unknown");
Err(cloudformation_validation_error_with_path_tracker(...))

// SrcMeta is available in the AST node but not used:
let meta = ast.meta(); // Available but ignored
```

#### In Error Wrapper Functions (`src/yaml/errors/wrapper.rs`):
- Functions derive line numbers by searching through file content (see `wrapper.rs:240-280`)
- SrcMeta with precise positions is available from callers but not passed

#### In Tag Resolution Methods:
- All methods in `src/yaml/resolution/` have access to SrcMeta via AST nodes
- None pass this information to error construction functions
- Location information is lost and later derived through heuristics in `enhanced.rs:290-320`

## Requirements for Final Error Reporting System

### 1. Human-Readable Error Reports

#### Visual Requirements
- **Precise Location**: Show exact line:column with visual pointer
- **Context**: 3-5 lines of surrounding code with the error line highlighted
- **Color Coding**: Error severity indicated by color (red for errors, yellow for warnings)
- **Structured Format**:
  ```
  Error[E001]: Invalid CloudFormation function syntax
  --> templates/stack.yaml:45:12
     |
  43 |   Resources:
  44 |     MyBucket:
  45 |       Type: !Ref InvalidRef
     |             ^^^^^^^^^^^^^^^^ Expected AWS::S3::Bucket or similar
  46 |       Properties:
  47 |         BucketName: my-bucket
     |
  Help: Did you mean "AWS::S3::Bucket"?
  ```

#### Content Requirements
- **Clear Error Messages**: Explain what went wrong in plain language
- **Actionable Suggestions**: Provide fixes or alternatives
- **Error Codes**: Consistent identifiers for documentation/search
- **Stack Traces**: Optional verbose mode showing resolution path

### 2. API Usage Requirements (LSP, LLM Helpers)

**Note**: A Language Server Protocol (LSP) server will be implemented later to provide diagnostic information about all errors in a document. The parser already supports collecting all errors rather than stopping at the first one, which enables comprehensive document analysis for IDEs and editors.

#### Structured Data
```rust
pub struct ErrorReport {
    // Core identification
    pub error_code: ErrorId,         // e.g., ErrorId::ERR_7001
    // severity: always Error (derives from error_code)
    
    // Location information
    pub location: ErrorLocation {
        pub file_uri: Url,           // File URL
        pub range: Range {           // LSP-compatible range
            pub start: Position { line: u32, character: u32 },
            pub end: Position { line: u32, character: u32 },
        },
        pub snippet: Option<String>, // Affected code snippet
    },
    
    // Content
    pub message: String,             // Main error message
    pub details: Option<String>,     // Extended explanation
    pub suggestions: Vec<Suggestion>,// Possible fixes
    
    // Navigation
    pub related_locations: Vec<RelatedLocation>, // Other relevant positions
    pub tags: Vec<DiagnosticTag>,   // Currently unused (all errors are blocking)
}
```

#### API Methods
```rust
pub trait ErrorReporter {
    // Serialization for different consumers
    fn to_json(&self) -> serde_json::Value;
    fn to_lsp_diagnostic(&self) -> lsp_types::Diagnostic;
    fn to_sarif(&self) -> sarif::Result;  // Static analysis format
    
    // Rendering
    fn format_human(&self, style: OutputStyle) -> String;
    fn format_github_annotation(&self) -> String;
    fn format_junit(&self) -> String;
}

// ErrorId should be serializable for APIs
impl ErrorId {
    pub fn as_str(&self) -> &'static str {
        // Return the string representation (e.g., "ERR_7001")
    }
    
    pub fn description(&self) -> &'static str {
        // Return human-readable description
    }
    
    pub fn severity(&self) -> Severity {
        // All current errors prevent successful template processing
        Severity::Error
    }
}
```

### 3. Performance Requirements
- No file re-reading during error construction
- Cache source content during parsing for error context
- Lazy evaluation of expensive operations (fuzzy matching, suggestions)

## Step-by-Step Implementation Plan

### Phase 1: Refactor Error Types

1. **Consolidate Error Modules**
   - Merge `src/yaml/errors/wrapper.rs` functionality into `src/yaml/errors/enhanced.rs`
   - Create a single, clean API for error construction
   - Remove `EnhancedErrorWrapper` struct from `wrapper.rs`

2. **Update Error Structure**
   ```rust
   pub struct EnhancedPreprocessingError {
       pub error_code: ErrorId,      // Use ErrorId enum, not String
       pub location: SourceLocation, // Now from SrcMeta, not derived
       pub message: String,
       pub details: Option<String>,
       pub suggestions: Vec<String>,
       pub source_context: SourceContext,
       pub related_info: Vec<RelatedInfo>,
   }
   ```

3. **Update ErrorId Prefix**
   - Change error code prefix from `IY` to `ERR_` in `src/yaml/errors/ids.rs`
   - Update all error codes: `IY1001` → `ERR_1001`, `IY4002` → `ERR_4002`, etc.
   - Update `explain` command in `src/explain.rs` to handle new prefix
   - Update error message formatting in `enhanced.rs` to show new prefix

4. **Fix Panic Potentials**
   - Add bounds checking for all array accesses in `wrapper.rs:240-280`
   - Validate string slicing operations in `wrapper.rs:300-350`
   - Use saturating arithmetic where needed

5. **Verify Phase 1 Completion**
   ```bash
   # Run comprehensive checks
   cargo check --lib --tests --bins --benches
   cargo nextest r --color=never --hide-progress-bar
   
   # Handle snapshot test changes (will likely be many due to error format changes)
   # Review and accept valid snapshot changes - USER APPROVAL REQUIRED
   cargo insta review  # Only accept if changes are valid, not regressions
   
   # If all tests pass, stage and commit changes - USER APPROVAL REQUIRED
   git add .
   git status  # Review staged changes with user
   # ASK USER: Ready to commit Phase 1 changes?
   git commit -m "refactor: consolidate error modules and update ErrorId prefix
   
   - Merge wrapper.rs functionality into enhanced.rs
   - Change error code prefix from IY to ERR_
   - Fix panic potentials in error handling
   - Remove EnhancedErrorWrapper struct
   
   🤖 Generated with [Claude Code](https://claude.ai/code)
   
   Co-Authored-By: Claude <noreply@anthropic.com>"
   ```

### Phase 2: Simplify Error Construction API

1. **Ultra-Simple 1-Liner API**
   
   Make `ErrorType` responsible for creating errors - no builders, no complex APIs:

   ```rust
   // Main API - 1 liner for all cases
   impl ErrorType {
       pub fn at_node(self, node: &YamlAst, msg: Option<impl Into<String>>) -> EnhancedPreprocessingError {
           let message = msg.map(|m| m.into())
               .unwrap_or_else(|| self.auto_generate_message(node));
           EnhancedPreprocessingError::new(self, node, message)
       }
       
       // For non-parsing errors without YamlAst nodes
       pub fn without_node(self, msg: impl Into<String>) -> EnhancedPreprocessingError {
           EnhancedPreprocessingError::new_without_location(self, msg.into())
       }
   }
   
   impl EnhancedPreprocessingError {
       fn new(error_type: ErrorType, node: &YamlAst, message: String) -> Self {
           let meta = node.meta();
           let mut error = Self {
               error_type,
               location: Some(SourceLocation::from_src_meta(meta)),
               message,
               suggestions: Vec::new(),
               examples: Vec::new(),
               context: ErrorContext::from_node(node),
           };
           
           // Auto-generate suggestions and examples based on error type and node
           error.auto_populate();
           error
       }
       
       fn new_without_location(error_type: ErrorType, message: String) -> Self {
           let mut error = Self {
               error_type,
               location: None, // No source location available
               message,
               suggestions: Vec::new(),
               examples: Vec::new(),
               context: ErrorContext::empty(),
           };
           
           // Auto-generate suggestions and examples based on error type only
           error.auto_populate();
           error
       }
       
       fn auto_populate(&mut self) {
           // Automatically add suggestions based on error type
           self.suggestions = self.error_type.generate_suggestions(&self.context);
           
           // Automatically add examples from central registry
           self.examples = self.error_type.get_examples();
       }
       
       // Override methods for rare cases needing customization
       pub fn with_custom_suggestion(mut self, suggestion: String) -> Self {
           self.suggestions.push(suggestion);
           self
       }
       
       pub fn with_custom_example(mut self, example: String) -> Self {
           self.examples.push(example);
           self
       }
   }
   
   // Helper for parse failures
   impl YamlAst {
       pub fn dummy_for_parse_error(
           file_uri: Url,
           line: u32,
           column: u32,
           error_text: Option<String>,
       ) -> Self {
           let src_meta = SrcMeta {
               input_uri: file_uri,
               start: Position { line, character: column },
               end: Position { line, character: column + 1 },
           };
           
           YamlAst::Scalar(
               error_text.unwrap_or_else(|| "<parse error>".to_string()),
               src_meta
           )
       }
   }
   ```

   **Auto-Message Generation:**
   ```rust
   impl ErrorType {
       fn auto_generate_message(&self, node: &YamlAst) -> String {
           match self {
               ErrorType::MapMissingItems => "'items' field missing in !$map tag".to_string(),
               ErrorType::MapMissingTemplate => "'template' field missing in !$map tag".to_string(),
               ErrorType::IfMissingTest => "'test' field missing in !$if tag".to_string(),
               ErrorType::RefInvalidType => format!("!Ref expects a string, found {}", 
                   node.type_description()),
               ErrorType::VariableNotFound(name) => format!("variable '{}' not found", name),
               // ... precise messages for each ErrorType
           }
       }
   }
   ```

   **Usage Examples:**
   ```rust
   // Auto-generated messages (most common case)
   return Err(ErrorType::MapMissingTemplate.at_node(&node, None));
   // → "'template' field missing in !$map tag"
   
   return Err(ErrorType::RefInvalidType.at_node(&node, None));
   // → "!Ref expects a string, found array"
   
   // Custom message when needed
   return Err(ErrorType::VariableNotFound.at_node(&node, Some("variable 'config.database' not found in current scope")));
   
   // Non-parsing errors without nodes
   return Err(ErrorType::FileNotFound.without_node("template file 'missing.yaml' not found"));
   
   // Parse error with dummy node
   let dummy_node = YamlAst::dummy_for_parse_error(file_uri, line, column, None);
   return Err(ErrorType::UnexpectedEof.at_node(&dummy_node, None));
   // → Auto-generated: "unexpected end of file"
   ```

   **Enhanced Error Context for Explain Command:**
   ```rust
   impl EnhancedPreprocessingError {
       pub fn to_explain_reference(&self) -> String {
           let context = ExplainContext {
               tag_name: self.extract_tag_name(),
               error_variant: self.error_variant(),
               src_meta: Some(self.location.src_meta.clone()),
               invalid_value: self.extract_invalid_value(),
               expected_type: self.extract_expected_type(),
           };
           
           let encoded = base64::encode(serde_json::to_string(&context).unwrap());
           format!("For more info: iidy explain {}:{}", self.error_id().as_str(), encoded)
       }
   }
   ```

   **Auto-Generation Logic:**
   ```rust
   impl ErrorType {
       fn generate_suggestions(&self, context: &ErrorContext) -> Vec<String> {
           match self {
               ErrorType::TypeMismatch => self.type_mismatch_suggestions(context),
               ErrorType::MissingField(field) => vec![format!("Add '{field}' field to the tag")],
               ErrorType::UnknownTag => self.unknown_tag_suggestions(context),
               ErrorType::CloudFormationValidation => self.cf_suggestions(context),
               // ... etc for all error types
           }
       }
       
       fn get_examples(&self) -> Vec<String> {
           TAG_DOCS.get(self.tag_name())
               .map(|doc| doc.examples.iter().map(|ex| ex.yaml.to_string()).collect())
               .unwrap_or_default()
       }
   }
   ```

2. **Verify Phase 2 Completion**
   ```bash
   # Run comprehensive checks
   cargo check --lib --tests --bins --benches
   cargo nextest r --color=never --hide-progress-bar
   
   # Handle snapshot test changes - USER APPROVAL REQUIRED
   cargo insta review  # Many snapshots will change due to new error API
   
   # If all tests pass, stage and commit changes - USER APPROVAL REQUIRED
   git add .
   git status  # Review staged changes with user
   # ASK USER: Ready to commit Phase 2 changes?
   git commit -m "feat: implement ultra-simple 1-liner error construction API
   
   - Add ErrorType::at_node() and ErrorType::without_node() methods
   - Implement auto-generated error messages with optional override
   - Add support for non-parsing errors without YamlAst nodes
   - Create enhanced error context for explain command
   - Add YamlAst::dummy_for_parse_error() helper method
   
   🤖 Generated with [Claude Code](https://claude.ai/code)
   
   Co-Authored-By: Claude <noreply@anthropic.com>"
   ```

### Phase 3: Update All Error Call Sites

1. **Update Resolver Methods**
   - Replace all error construction with simple 1-liner calls
   - Example changes in `src/yaml/resolution/resolver_split_args.rs`:
   ```rust
   // Before
   Err(cloudformation_validation_error_with_path_tracker(
       tag_name, msg, file_path, path_tracker
   ))
   
   // After  
   Err(ErrorType::CloudFormationValidation.at_node(&ast_node, msg))
   ```

2. **Update Parser Error Handling**
   - Create dummy nodes for parse failures and use same 1-liner API
   ```rust
   // Parse error handling
   let dummy_node = YamlAst::dummy_for_parse_error(file_uri, line, column, error_text);
   Err(ErrorType::SyntaxError.at_node(&dummy_node, "malformed YAML structure"))
   ```

3. **Simplify PathTracker Integration**
   - PathTracker information can be included in the auto-generated context
   - No need for special handling since it's part of the error context analysis

4. **Verify Phase 3 Completion**
   ```bash
   # Run comprehensive checks
   cargo check --lib --tests --bins --benches
   cargo nextest r --color=never --hide-progress-bar
   
   # Handle snapshot test changes - USER APPROVAL REQUIRED
   cargo insta review  # All error call sites changed, many snapshots affected
   
   # If all tests pass, stage and commit changes - USER APPROVAL REQUIRED
   git add .
   git status  # Review staged changes with user
   # ASK USER: Ready to commit Phase 3 changes?
   git commit -m "refactor: update all error call sites to use new SrcMeta-based API
   
   - Replace all error construction with 1-liner ErrorType::at_node() calls
   - Update parser error handling to use dummy nodes
   - Simplify PathTracker integration
   - All error locations now come directly from SrcMeta
   
   🤖 Generated with [Claude Code](https://claude.ai/code)
   
   Co-Authored-By: Claude <noreply@anthropic.com>"
   ```

### Phase 4: Remove Legacy Code

1. **Delete Old Location Finding**
   - Remove `src/yaml/errors/location.rs`
   - Remove `src/yaml/errors/tree_sitter_location.rs`
   - Remove location derivation from file paths in `wrapper.rs`

2. **Clean Up Dependencies**
   - Remove unused imports
   - Update error module exports

3. **Verify Phase 4 Completion**
   ```bash
   # Run comprehensive checks
   cargo check --lib --tests --bins --benches
   cargo nextest r --color=never --hide-progress-bar
   
   # Handle snapshot test changes if any - USER APPROVAL REQUIRED
   cargo insta review  # Should be minimal changes since legacy code removal
   
   # If all tests pass, stage and commit changes - USER APPROVAL REQUIRED
   git add .
   git status  # Review staged changes with user
   # ASK USER: Ready to commit Phase 4 changes?
   git commit -m "refactor: remove legacy location-finding code
   
   - Delete location.rs and tree_sitter_location.rs
   - Remove location derivation from file paths
   - Clean up unused imports and dependencies
   - Complete migration to SrcMeta-based error locations
   
   🤖 Generated with [Claude Code](https://claude.ai/code)
   
   Co-Authored-By: Claude <noreply@anthropic.com>"
   ```

### Phase 5: Add API Support

1. **Implement Serialization**
   ```rust
   #[derive(Serialize, Deserialize)]
   pub struct SerializableError {
       // LSP-compatible structure
   }
   
   impl From<EnhancedPreprocessingError> for SerializableError {
       // Conversion logic
   }
   ```

2. **Add Format Methods**
   - `to_lsp_diagnostic()` for Language Server Protocol
   - `to_json()` for general API usage
   - `to_github_annotation()` for CI integration

3. **Create Error Registry**
   - Document all error codes
   - Provide machine-readable error catalog
   - Support error code lookups

4. **Centralize Tag Examples and Auto-Generation**
   
   Create a central registry that powers the auto-generation:
   
   ```rust
   pub struct TagDocumentation {
       pub tag_name: &'static str,
       pub description: &'static str,
       pub required_fields: Vec<&'static str>,
       pub optional_fields: Vec<&'static str>,
       pub examples: Vec<TagExample>,
       pub common_mistakes: Vec<CommonMistake>,
   }
   
   pub struct TagExample {
       pub description: &'static str,
       pub yaml: &'static str,
   }
   
   pub struct CommonMistake {
       pub pattern: &'static str,
       pub suggestion: &'static str,
   }
   
   lazy_static! {
       static ref TAG_DOCS: HashMap<&'static str, TagDocumentation> = {
           let mut m = HashMap::new();
           
           m.insert("!$map", TagDocumentation {
               tag_name: "!$map",
               description: "Transform each item in a list",
               required_fields: vec!["items", "template"],
               optional_fields: vec!["var"],
               examples: vec![
                   TagExample {
                       description: "Basic mapping",
                       yaml: "!$map\n  items: [1, 2, 3]\n  template: \"{{item}}\"",
                   },
               ],
               common_mistakes: vec![
                   CommonMistake {
                       pattern: "source",
                       suggestion: "Use 'items' instead of 'source'",
                   },
                   CommonMistake {
                       pattern: "transform", 
                       suggestion: "Use 'template' instead of 'transform'",
                   },
               ],
           });
           
           // Add all other tags...
           m
       };
   }
   ```
   
   **Auto-generation uses this registry:**
   - Examples are automatically pulled from `TAG_DOCS`
   - Suggestions are generated based on `common_mistakes` and node analysis
   - Error messages include appropriate context based on the error type
   - Will also power LSP diagnostics for comprehensive document analysis
   - Powers context-aware explanations for the `explain` command

5. **Centralize Current Example Logic**
   
   Based on analysis of the current implementation in `wrapper.rs` and `enhanced.rs`:
   
   **Current Problems:**
   - Examples are hardcoded in two different locations with different formats
   - `src/yaml/errors/wrapper.rs:408-463` has elaborate multi-line examples in match statements
   - `src/yaml/errors/enhanced.rs:410-477` has concise inline examples for fields and CloudFormation tags
   - No validation that examples are syntactically correct
   - Adding new tags requires updating multiple match statements
   - Existing `example-templates/yaml-iidy-syntax/` directory has quality examples but isn't integrated
   
   **Solution:**
   - Move all examples from hardcoded match statements to the central `TAG_DOCS` registry
   - Include both iidy tags (`!$map`, `!$if`, etc.) and CloudFormation tags (`!Ref`, `!Sub`, etc.)
   - Reuse existing examples from `example-templates/yaml-iidy-syntax/` as the source
   - Single example format that works for both human-readable errors and LSP diagnostics

6. **Integrate with 'iidy explain' Command**
   
   Analysis of the current `iidy explain` implementation:
   
   **Current State:**
   - Uses the same `ErrorId` enum as the error reporting system
   - Only 4 out of 45+ error codes have detailed documentation (ERR_1001, ERR_2001, ERR_4002, ERR_5001)
   - Documentation stored as embedded markdown files in `src/docs/errors/` (IY1001.md, IY2001.md, etc.)
   - Error messages in `enhanced.rs:344` include "For more info: iidy explain ERR_####" footer
   - Command implementation in `src/explain.rs` supports explaining multiple error codes in one command
   
   **Problems with Current Generic Approach:**
   - `explain ERR_4002` just says "Tag Syntax & Structure Error" 
   - Doesn't know if it's a missing `template` field in `!$map` vs missing `test` field in `!$if`
   - Can't provide specific examples for the actual tag that failed
   - No context about what the user was trying to do
   
   **Enhanced Context-Aware Explain:**
   ```bash
   # Instead of just: iidy explain ERR_4002
   # Support shell-safe base64 encoded context:
   
   # Base64 encoded context after ErrorId
   iidy explain ERR_4002:eyJ0YWciOiIhJG1hcCIsImZpZWxkIjoidGVtcGxhdGUiLCJmaWxlIjoiZmlsZS55YW1sIiwibGluZSI6MTUsImNvbCI6M30=
   
   # Still support simple codes for backward compatibility
   iidy explain ERR_4002
   
   # Multiple context-aware codes
   iidy explain ERR_4002:eyJ0YWciOiIhJG1hcCJ9 ERR_5001:eyJ0YWciOiIhJHNwbGl0In0=
   ```
   
   **Base64 Context Format:**
   ```rust
   #[derive(Serialize, Deserialize)]
   pub struct ExplainContext {
       pub tag_name: Option<String>,           // e.g., "!$map"
       pub error_variant: Option<String>,      // e.g., "missing_template"
       pub src_meta: Option<SrcMeta>,          // Full location info
       pub invalid_value: Option<String>,      // What was actually provided
       pub expected_type: Option<String>,      // What was expected
   }
   ```
   
   **Implementation Approach:**
   ```rust
   impl EnhancedPreprocessingError {
       pub fn to_explain_reference(&self) -> String {
           let context = ExplainContext {
               tag_name: self.extract_tag_name(),
               error_variant: self.error_variant(),
               src_meta: Some(self.location.src_meta.clone()),
               invalid_value: self.extract_invalid_value(),
               expected_type: self.extract_expected_type(),
           };
           
           let encoded = base64::encode(serde_json::to_string(&context).unwrap());
           format!("For more info: iidy explain {}:{}", self.error_id().as_str(), encoded)
       }
   }
   
   // Explain command parsing
   impl ExplainCommand {
       fn parse_code(&self, input: &str) -> Result<(ErrorId, Option<ExplainContext>), Error> {
           if let Some((error_id, context_b64)) = input.split_once(':') {
               let error_id = ErrorId::from_code(error_id)?;
               let context = base64::decode(context_b64)
                   .and_then(|bytes| serde_json::from_slice(&bytes))
                   .ok();
               Ok((error_id, context))
           } else {
               Ok((ErrorId::from_code(input)?, None))
           }
       }
   }
   ```
   
   **Benefits with SrcMeta:**
   - `explain` can reference the exact file and line number where error occurred
   - Can show the actual source code around the error with highlighting
   - Knows exactly what value was provided vs what was expected
   - Can suggest specific fixes based on the actual content
   - Shell-safe: base64 encoding handles all special characters and spaces
   - Backward compatible: plain error codes still work

   **Future: LLM-Based Auto-Fix:**
   ```bash
   # Rich context enables automated fixing
   iidy fix ERR_4002:eyJ0YWciOiIhJG1hcCIsImZpZWxkIjoidGVtcGxhdGUiLCJmaWxlIjoiZmlsZS55YW1sIiwibGluZSI6MTUsImNvbCI6M30=
   
   # Or explain with fix suggestion
   iidy explain --with-fix ERR_4002:eyJ0YWciOiIhJG1hcCJ9
   ```
   
   **LLM Integration Possibilities:**
   - **Complete Context**: LLM gets exact error location, surrounding code, what was expected
   - **Precise Edits**: Can generate exact line/column edits rather than vague suggestions
   - **Validation**: Can validate proposed fixes against iidy syntax rules
   - **Multiple Options**: Generate several fix alternatives with explanations
   - **Learning**: Learn from successful fixes to improve future suggestions
   
   The rich error context makes this feasible because the LLM would have:
   - Exact source location and surrounding context
   - Precise error type and what went wrong
   - Tag documentation and examples from central registry
   - Type information about what was provided vs expected

7. **Verify Phase 5 Completion**
   ```bash
   # Run comprehensive checks
   cargo check --lib --tests --bins --benches
   cargo nextest r --color=never --hide-progress-bar
   
   # Test the new explain command with context
   cargo run -- explain ERR_4002  # Should work with backward compatibility
   
   # Handle snapshot test changes - USER APPROVAL REQUIRED
   cargo insta review  # New explain command output and centralized examples
   
   # If all tests pass, stage and commit changes - USER APPROVAL REQUIRED
   git add .
   git status  # Review staged changes with user
   # ASK USER: Ready to commit Phase 5 changes?
   git commit -m "feat: add comprehensive API support and centralized documentation
   
   - Implement serialization for LSP, JSON, and other formats
   - Create central TAG_DOCS registry with examples and suggestions
   - Enhance explain command with base64-encoded context support
   - Add auto-generation for examples and suggestions
   - Prepare foundation for future LLM integration
   
   🤖 Generated with [Claude Code](https://claude.ai/code)
   
   Co-Authored-By: Claude <noreply@anthropic.com>"
   ```

### Phase 6: Testing and Documentation

1. **Update Tests**
   - Fix tests expecting old error format
   - Add tests for SrcMeta preservation
   - Test all error formatting modes

2. **Document Error System**
   - API documentation for error types
   - Usage examples for different consumers
   - Migration guide from old system

3. **Verify Phase 6 Completion**
   ```bash
   # Run comprehensive checks
   cargo check --lib --tests --bins --benches
   cargo nextest r --color=never --hide-progress-bar
   
   # Test all error examples from example-templates/errors/
   for F in example-templates/errors/*yaml; do 
       echo "Testing $F"
       cargo run -- render "$F" || echo "Expected error from $F"
   done
   
   # Handle snapshot test changes - USER APPROVAL REQUIRED
   cargo insta review  # Final test updates and documentation changes
   
   # If all tests pass, stage and commit changes - USER APPROVAL REQUIRED
   git add .
   git status  # Review staged changes with user
   # ASK USER: Ready to commit Phase 6 changes? This completes core SrcMeta implementation.
   git commit -m "docs: update tests and documentation for new error system
   
   - Fix tests expecting old error format
   - Add tests for SrcMeta preservation
   - Test all error formatting modes
   - Update API documentation and usage examples
   - Complete core SrcMeta implementation
   
   🤖 Generated with [Claude Code](https://claude.ai/code)
   
   Co-Authored-By: Claude <noreply@anthropic.com>"
   ```

### Phase 7: ErrorId Cleanup and Reorganization

**Note**: This is a later phase after the core SrcMeta implementation is complete.

1. **Current ErrorId Analysis**
   
   Based on analysis of `src/yaml/errors/ids.rs` (84 total error variants defined):
   
   **Current Categories:**
   - 1xxx: YAML Syntax & Parsing (ERR_1001, etc.)
   - 2xxx: Variable & Scope Errors (ERR_2001, etc.) 
   - 3xxx: Import & Loading Errors
   - 4xxx: Tag Syntax & Structure Errors (needs review)
   - 5xxx: Type & Validation Errors (needs review)
   - 6xxx: Template & Handlebars Errors
   - 7xxx: CloudFormation Specific (needs review)
   - 8xxx: Configuration & Setup
   - 9xxx: Internal & System Errors (remove entirely)

2. **Remove Unused ErrorIds**
   - Audit which ErrorId variants are actually used in the codebase (currently only 4 out of 84 are used)
   - Remove any ErrorIds that are defined but never referenced from `src/yaml/errors/ids.rs`
   - Remove entire 9xxx category (Internal & System Errors)

3. **Tag-Specific Error Reorganization**
   
   **Current Problem**: Generic errors like ERR_4002 "Tag Syntax Error" for all tags
   
   **New Approach**: Precise ErrorId ranges for each tag type:
   
   ```rust
   // !$ (include/variable) errors: 4000-4099
   ERR_4001, // Variable not found
   ERR_4002, // Invalid variable syntax
   ERR_4003, // Circular variable reference
   
   // !$if errors: 4100-4199  
   ERR_4101, // Missing 'test' field
   ERR_4102, // Missing 'then' field
   ERR_4103, // Invalid test expression
   
   // !$map errors: 4200-4299
   ERR_4201, // Missing 'items' field
   ERR_4202, // Missing 'template' field
   ERR_4203, // Invalid items type
   ERR_4204, // Template evaluation failed
   
   // !$let errors: 4300-4399
   ERR_4301, // Missing 'in' field
   ERR_4302, // Invalid variable binding
   
   // CloudFormation !Ref errors: 7000-7099
   ERR_7001, // Invalid reference type
   ERR_7002, // Reference not found
   
   // CloudFormation !Sub errors: 7100-7199
   ERR_7101, // Invalid substitution syntax
   ERR_7102, // Missing substitution parameter
   ```

4. **Auto-Generated Messages with Precise ErrorIds**
   
   With precise ErrorIds, most error messages can be auto-generated:
   
   ```rust
   impl ErrorType {
       fn auto_generate_message(&self, node: Option<&YamlAst>) -> String {
           match self {
               // Tag-specific errors with context
               ErrorType::MapMissingItems => "'items' field missing in !$map tag",
               ErrorType::MapMissingTemplate => "'template' field missing in !$map tag", 
               ErrorType::IfMissingTest => "'test' field missing in !$if tag",
               ErrorType::LetMissingIn => "'in' field missing in !$let tag",
               
               // Type errors with node analysis
               ErrorType::RefInvalidType => {
                   let found_type = node.map(|n| n.type_description()).unwrap_or("unknown");
                   format!("!Ref expects a string, found {}", found_type)
               },
               
               // Variable errors
               ErrorType::VariableNotFound(name) => format!("variable '{}' not found", name),
               
               // File system errors (no node context)
               ErrorType::FileNotFound(path) => format!("file '{}' not found", path),
               ErrorType::PermissionDenied(path) => format!("permission denied: '{}'", path),
           }
       }
   }
   ```

5. **Benefits of Precise ErrorIds + Auto-Generation**
   - **Consistent Messages**: All similar errors have identical, well-crafted messages
   - **Less Boilerplate**: `ErrorType::MapMissingTemplate.at_node(&node, None)` instead of manually writing messages
   - **Specific Documentation**: Each ErrorId has precise explanation for that exact failure
   - **Better Explain Command**: `explain ERR_4201` shows `!$map` missing items examples
   - **Targeted Fixes**: LLM tools can provide highly specific solutions
   - **Error Analytics**: Can track which specific errors are most common
   - **Maintainability**: Clear mapping between error and documentation
   - **Override When Needed**: Can still provide custom messages for special cases

6. **Migration Strategy**
   - Implement new ErrorId scheme alongside existing one
   - Update error construction sites to use precise ErrorIds
   - Remove generic ErrorIds after all specific ones are in place
   - Update central documentation registry to match new scheme

7. **Verify Phase 7 Completion**
   ```bash
   # Run comprehensive checks
   cargo check --lib --tests --bins --benches
   cargo nextest r --color=never --hide-progress-bar
   
   # Test precise error IDs with explain command
   cargo run -- explain ERR_4201  # Should show specific !$map missing items error
   cargo run -- explain ERR_4202  # Should show specific !$map missing template error
   
   # Test error example templates to ensure all precise errors work
   for F in example-templates/errors/*yaml; do 
       echo "Testing precise ErrorIds with $F"
       cargo run -- render "$F" || echo "Expected specific error from $F"
   done
   
   # Handle snapshot test changes - USER APPROVAL REQUIRED
   cargo insta review  # All error messages now use precise ErrorIds
   
   # If all tests pass, stage and commit changes - USER APPROVAL REQUIRED
   git add .
   git status  # Review staged changes with user
   # ASK USER: Ready to commit Phase 7 changes? This completes the entire implementation.
   git commit -m "feat: implement precise ErrorId scheme with tag-specific ranges
   
   - Remove 80 unused ErrorId variants from ids.rs
   - Implement tag-specific ErrorId ranges (4000-4099 for variables, etc.)
   - Add auto-generated messages for all precise ErrorIds
   - Remove entire 9xxx category (Internal & System Errors)
   - Complete ErrorId cleanup and reorganization
   
   🤖 Generated with [Claude Code](https://claude.ai/code)
   
   Co-Authored-By: Claude <noreply@anthropic.com>"
   ```

## Migration Strategy

### Order of Implementation
1. Phase 1 first (consolidate modules)
2. Phase 2 next (simple API)
3. Phases 3-4 together (update call sites and remove old code)
4. Phase 5-6 can be done in parallel with 3-4

### User Approval Requirements

**IMPORTANT**: This implementation requires frequent user approval for two critical areas:

1. **Snapshot Test Changes**: 
   - Error format changes will trigger many `insta` snapshot updates
   - Each phase will likely require `cargo insta review` and user approval
   - Only accept snapshot changes that are valid improvements, not regressions
   - User must manually review each snapshot change before accepting

2. **Git Commits**:
   - All commits require user approval before execution
   - Agent will stage changes and ask: "Ready to commit Phase X changes?"
   - User must review `git status` output and approve each commit
   - User can reject commits if changes look incorrect

### Expected Snapshot Changes by Phase
- **Phase 1**: Error code prefix changes (IY → ERR_), module consolidation
- **Phase 2**: New error construction API, different error message formats  
- **Phase 3**: All error call sites updated, widespread message changes
- **Phase 4**: Minimal changes (legacy code removal)
- **Phase 5**: New explain command output, centralized examples
- **Phase 6**: Test updates and documentation
- **Phase 7**: Precise ErrorId messages, highly specific error output

### Backward Compatibility
- Old error functions can be implemented as thin wrappers over new API
- Easy migration path: just change error construction calls
- Remove old functions after all call sites updated

## Success Criteria

1. **No Location Heuristics**: All error locations come directly from SrcMeta
2. **Ultra-Simple API**: `ErrorType::SomeError.at_node(&node, "message")` for all cases
3. **Auto-Everything**: Suggestions and examples generated automatically unless overridden
4. **No Redundancy**: One module for error handling, not multiple
5. **API Ready**: Errors serializable for LSP/tool consumption
6. **Performance**: No file re-reading, no expensive operations during error construction
7. **Safety**: No panic potential in error handling code
8. **1-Liner Errors**: 95% of error construction should be a single line of code

## Notes

- Consider caching source file content during parsing to avoid re-reading
- Ensure error messages remain helpful when serialized (no ANSI codes in JSON)
- Keep performance in mind - error construction should be fast even for many errors
- Consider adding error recovery information for better IDE experience

## Example Error Templates

The project includes 40+ error example templates in `example-templates/errors/` that demonstrate the current error reporting capabilities:

### Current Error Format Examples

1. **Type Errors** - Show expected vs actual types with suggestions:
   ```
   Type error: expected string, found number @ file.yaml:5:20 (errno: IY5001)
     -> data type mismatch
   
      4 | 
      5 | test_split: !$split [123, "hello,world"]
        |                    ^^^^^^^^ expected string
      6 |
   
      expected string, found number
   ```

2. **Tag Errors** - Show missing fields with examples:
   ```
   Tag error: 'template' missing in !$map tag @ file.yaml:5:9 (errno: IY4002)
     -> add 'template' field to !$map tag
   
      4 | 
      5 | result: !$map
      6 |   items: !$ data
   
      example:
      !$map
        items: [1, 2, 3]
        template: "{{item}}"
   ```

3. **CloudFormation Validation** - Show AWS-specific requirements:
   ```
   CloudFormation error: !Ref expects a string (resource or parameter name), found array @ file.yaml:11:24 (errno: IY7001)
     -> invalid CloudFormation intrinsic function
   
     10 |     Properties:
     11 |       BucketName: !Ref []  # Invalid - empty array
        |                        ^^^^ invalid CloudFormation tag
     12 |       
   
      !Ref expects a string (resource or parameter name)
      example: BucketName: !Ref MyBucket
      example: Environment: !Ref EnvironmentParam
   ```

### Error Categories Covered

- **Syntax Errors**: YAML parsing failures, malformed structures
- **Type Errors**: Wrong types passed to functions
- **Tag Errors**: Missing required fields, unknown tags
- **CloudFormation Errors**: Invalid intrinsic function usage
- **Variable Errors**: Undefined variables, missing includes
- **Resolution Errors**: Template evaluation failures

These examples serve as both test cases and documentation of the error system's capabilities. The new SrcMeta-based implementation must maintain or improve upon this level of detail and helpfulness.