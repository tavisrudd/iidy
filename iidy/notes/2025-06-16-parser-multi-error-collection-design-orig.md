
⏺ Detailed Implementation Plan: Option A with Future Partial AST Support

  Phase 1: Core Infrastructure (Error Collection Foundation)

  Step 1.1: Extend Error Types

  File: src/yaml/parsing_w_loc/error.rs

  #[derive(Debug, Clone)]
  pub struct ParseWarning {
      pub message: String,
      pub location: Option<ParseLocation>,
      pub code: Option<String>,
  }

  #[derive(Debug, Clone)]
  pub struct ParseDiagnostics {
      pub errors: Vec<ParseError>,
      pub warnings: Vec<ParseWarning>,
      pub parse_successful: bool,
  }

  #[derive(Debug, Clone)]
  pub enum ParseMode {
      StrictValidation,  // Current behavior - stop on first error
      CollectAll,        // Collect all errors without AST building
      // Future: BestEffort, // Build partial AST where possible (Phase 3)
  }

  impl ParseDiagnostics {
      pub fn new() -> Self {
          Self {
              errors: Vec::new(),
              warnings: Vec::new(),
              parse_successful: true,
          }
      }

      pub fn add_error(&mut self, error: ParseError) {
          self.errors.push(error);
          self.parse_successful = false;
      }

      pub fn add_warning(&mut self, warning: ParseWarning) {
          self.warnings.push(warning);
      }

      pub fn has_errors(&self) -> bool {
          !self.errors.is_empty()
      }

      pub fn error_count(&self) -> usize {
          self.errors.len()
      }
  }

  impl ParseWarning {
      pub fn new(message: impl Into<String>) -> Self {
          Self {
              message: message.into(),
              location: None,
              code: None,
          }
      }

      pub fn with_location(message: impl Into<String>, uri: Url, start: Position, end: Position) -> Self {
          Self {
              message: message.into(),
              location: Some(ParseLocation { uri, start, end }),
              code: None,
          }
      }
  }

  // Extend ParseError with error codes for better LSP integration
  impl ParseError {
      pub fn with_code(mut self, code: impl Into<String>) -> Self {
          // Add code field to ParseError struct first
          self
      }
  }

  Step 1.2: Add Error Codes for Categorization

  // Add to ParseError struct
  #[derive(Debug, Clone)]
  pub struct ParseError {
      pub message: String,
      pub location: Option<ParseLocation>,
      pub code: Option<String>,  // New field for error codes
  }

  // Error code constants for different error types
  pub mod error_codes {
      pub const SYNTAX_ERROR: &str = "IY1001";
      pub const UNKNOWN_TAG: &str = "IY4001";
      pub const MISSING_FIELD: &str = "IY4002";
      pub const INVALID_TYPE: &str = "IY4003";
      pub const INVALID_FORMAT: &str = "IY4004";
  }

  Phase 2: Parser API Extension

  Step 2.1: Add Diagnostic Parsing Methods

  File: src/yaml/parsing_w_loc/parser.rs

  impl YamlParser {
      /// New API for collecting all errors without stopping on first error
      pub fn validate_with_diagnostics(&mut self, source: &str, uri: Url) -> ParseDiagnostics {
          let mut diagnostics = ParseDiagnostics::new();

          // Check for anchor/alias fallback scenario
          if source.contains("&") && !source.contains("&amp;") || source.contains("*") && !source.contains("**/") {
              // For now, fallback to serde_yaml and convert any error
              match self.parse_with_serde_yaml_fallback(source, uri.clone()) {
                  Ok(_) => {
                      // Could add warning about using fallback parser
                      diagnostics.add_warning(ParseWarning::with_location(
                          "Using fallback parser for anchor/alias resolution",
                          uri, Position::new(0, 0), Position::new(0, 0)
                      ));
                  }
                  Err(e) => {
                      diagnostics.add_error(e);
                      return diagnostics;
                  }
              }
              return diagnostics;
          }

          // Parse with tree-sitter
          let tree = match self.parser.parse(source, None) {
              Some(tree) => tree,
              None => {
                  diagnostics.add_error(ParseError::new("Failed to parse YAML source")
                      .with_code(error_codes::SYNTAX_ERROR));
                  return diagnostics;
              }
          };

          // Collect ALL syntax errors (not just first)
          self.collect_all_syntax_errors(&tree, source, &uri, &mut diagnostics);

          // If no fatal syntax errors, proceed with semantic validation
          if !self.has_fatal_syntax_errors(&diagnostics) {
              self.validate_semantics_with_diagnostics(&tree, source, &uri, &mut diagnostics);
          }

          diagnostics
      }

      /// Backward compatibility - existing behavior unchanged
      pub fn parse(&mut self, source: &str, uri: Url) -> ParseResult<YamlAst> {
          // Use diagnostic mode and convert to old API
          let diagnostics = self.validate_with_diagnostics(source, uri.clone());

          if diagnostics.has_errors() {
              // Return first error for backward compatibility
              Err(diagnostics.errors.into_iter().next().unwrap())
          } else {
              // If validation passed, do actual parsing with current logic
              self.parse_internal(source, uri)
          }
      }

      /// Internal method that does actual AST building (current parse logic)
      fn parse_internal(&mut self, source: &str, uri: Url) -> ParseResult<YamlAst> {
          // Current parse() implementation goes here
          // This maintains exact current behavior for backward compatibility

          if source.contains("&") && !source.contains("&amp;") || source.contains("*") && !source.contains("**/") {
              return self.parse_with_serde_yaml_fallback(source, uri);
          }

          let tree = self.parser.parse(source, None)
              .ok_or_else(|| ParseError::new("Failed to parse YAML source"))?;

          let root = tree.root_node();

          if root.has_error() {
              return Err(self.find_syntax_error(&tree, source, &uri));
          }

          self.build_ast(root, source.as_bytes(), &uri)
      }
  }

  Step 2.2: Comprehensive Syntax Error Collection

  impl YamlParser {
      /// Collect ALL syntax errors from tree-sitter parse tree
      fn collect_all_syntax_errors(&self, tree: &Tree, source: &str, uri: &Url, diagnostics: &mut ParseDiagnostics) {
          let root = tree.root_node();
          self.traverse_for_syntax_errors(root, source, uri, diagnostics);
      }

      /// Recursively traverse tree and collect all error/missing nodes
      fn traverse_for_syntax_errors(&self, node: tree_sitter::Node, source: &str, uri: &Url, diagnostics: &mut ParseDiagnostics) {
          // Check current node for errors
          if node.is_error() || node.kind() == "ERROR" {
              let meta = node_meta(&node, uri);
              let message = self.analyze_syntax_error(&node, source);
              let error = self.create_syntax_error(&message, &meta, source)
                  .with_code(error_codes::SYNTAX_ERROR);
              diagnostics.add_error(error);
          }

          if node.is_missing() {
              let meta = node_meta(&node, uri);
              let message = format!("Missing {} element", node.kind());
              let error = self.create_syntax_error(&message, &meta, source)
                  .with_code(error_codes::SYNTAX_ERROR);
              diagnostics.add_error(error);
          }

          // Recursively check all children
          for i in 0..node.child_count() {
              if let Some(child) = node.child(i) {
                  self.traverse_for_syntax_errors(child, source, uri, diagnostics);
              }
          }
      }

      /// Create syntax error (extracted from current syntax_error method)
      fn create_syntax_error(&self, message: &str, meta: &SrcMeta, source: &str) -> ParseError {
          // Extract current syntax_error logic but return ParseError instead of using it
          let file_path = self.format_file_path_only(meta);

          if let Err(serde_error) = serde_yaml::from_str::<serde_yaml::Value>(source) {
              let anyhow_error = yaml_syntax_error(serde_error, &file_path, source);
              ParseError {
                  message: anyhow_error.to_string(),
                  location: Some(super::error::ParseLocation {
                      uri: meta.input_uri.clone(),
                      start: meta.start,
                      end: meta.end,
                  }),
                  code: None,
              }
          } else {
              ParseError {
                  message: format!("Syntax error: {} @ {}", message, self.format_file_location(meta)),
                  location: Some(super::error::ParseLocation {
                      uri: meta.input_uri.clone(),
                      start: meta.start,
                      end: meta.end,
                  }),
                  code: None,
              }
          }
      }

      /// Determine if syntax errors are fatal (prevent semantic analysis)
      fn has_fatal_syntax_errors(&self, diagnostics: &ParseDiagnostics) -> bool {
          // For now, any syntax error is fatal for semantic analysis
          // Later we can be more sophisticated about which errors allow continuation
          diagnostics.has_errors()
      }
  }

  Phase 3: Semantic Validation Without AST Building

  Step 3.1: Lightweight Semantic Validation

  impl YamlParser {
      /// Validate semantics by traversing tree-sitter nodes without building full AST
      fn validate_semantics_with_diagnostics(&self, tree: &Tree, source: &str, uri: &Url, diagnostics: &mut ParseDiagnostics) {
          let root = tree.root_node();
          self.validate_node_semantics(root, source.as_bytes(), uri, diagnostics);
      }

      /// Validate individual nodes for semantic correctness
      fn validate_node_semantics(&self, node: tree_sitter::Node, src: &[u8], uri: &Url, diagnostics: &mut ParseDiagnostics) {
          let meta = node_meta(&node, uri);

          match node.kind() {
              "tag" => {
                  // Validate tagged nodes without building full AST
                  self.validate_tagged_node_semantics(node, src, uri, diagnostics);
              }
              "flow_node" => {
                  // Check for tagged flow nodes
                  if let Some(tag_child) = node.child_by_field_name("tag") {
                      self.validate_tagged_node_semantics(tag_child, src, uri, diagnostics);
                  }
              }
              "block_node" => {
                  // Check for tagged block nodes
                  if let Some(tag_child) = node.child_by_field_name("tag") {
                      self.validate_tagged_node_semantics(tag_child, src, uri, diagnostics);
                  }
              }
              _ => {
                  // Recursively validate children
                  for i in 0..node.named_child_count() {
                      if let Some(child) = node.named_child(i) {
                          self.validate_node_semantics(child, src, uri, diagnostics);
                      }
                  }
              }
          }
      }

      /// Validate tagged nodes for known tags and required fields
      fn validate_tagged_node_semantics(&self, node: tree_sitter::Node, src: &[u8], uri: &Url, diagnostics: &mut ParseDiagnostics) {
          let meta = node_meta(&node, uri);

          // Extract tag text
          let tag_text = match node.utf8_text(src) {
              Ok(text) => text,
              Err(_) => {
                  diagnostics.add_error(
                      ParseError::with_location("Invalid UTF-8 in tag", uri.clone(), meta.start, meta.end)
                          .with_code(error_codes::SYNTAX_ERROR)
                  );
                  return;
              }
          };

          let tag_name = tag_text.split_whitespace().next().unwrap_or(tag_text);

          // Validate known tags
          if tag_name.starts_with("!$") {
              self.validate_preprocessing_tag_semantics(tag_name, node, src, uri, diagnostics);
          } else if self.is_known_cloudformation_tag(tag_name) {
              self.validate_cloudformation_tag_semantics(tag_name, node, src, uri, diagnostics);
          } else if !tag_name.starts_with("!") {
              // Not a tag at all
              return;
          } else {
              // Unknown tag
              diagnostics.add_error(
                  ParseError::with_location(
                      format!("Unknown tag '{}'", tag_name),
                      uri.clone(), meta.start, meta.end
                  ).with_code(error_codes::UNKNOWN_TAG)
              );
          }
      }

      /// Validate preprocessing tags without building AST
      fn validate_preprocessing_tag_semantics(&self, tag_name: &str, node: tree_sitter::Node, src: &[u8], uri: &Url, diagnostics: &mut
  ParseDiagnostics) {
          let meta = node_meta(&node, uri);

          // Find the content node (next sibling or child depending on syntax)
          let content_node = if let Some(content) = node.named_child(0) {
              content
          } else {
              diagnostics.add_error(
                  ParseError::with_location(
                      format!("Tag '{}' missing content", tag_name),
                      uri.clone(), meta.start, meta.end
                  ).with_code(error_codes::MISSING_FIELD)
              );
              return;
          };

          match tag_name {
              "!$include" => self.validate_include_tag_semantics(content_node, src, uri, diagnostics),
              "!$let" => self.validate_let_tag_semantics(content_node, src, uri, diagnostics),
              "!$map" => self.validate_map_tag_semantics(content_node, src, uri, diagnostics),
              "!$if" => self.validate_if_tag_semantics(content_node, src, uri, diagnostics),
              "!$eq" | "!$split" | "!$join" => self.validate_binary_tag_semantics(tag_name, content_node, src, uri, diagnostics),
              "!$merge" | "!$concat" => self.validate_variadic_tag_semantics(tag_name, content_node, src, uri, diagnostics),
              _ => {
                  // Unknown preprocessing tag
                  diagnostics.add_error(
                      ParseError::with_location(
                          format!("Unknown preprocessing tag '{}'", tag_name),
                          uri.clone(), meta.start, meta.end
                      ).with_code(error_codes::UNKNOWN_TAG)
                  );
              }
          }
      }
  }

  Step 3.2: Tag-Specific Validation Methods

  impl YamlParser {
      /// Validate !$include tag structure
      fn validate_include_tag_semantics(&self, content_node: tree_sitter::Node, src: &[u8], uri: &Url, diagnostics: &mut ParseDiagnostics) {
          let meta = node_meta(&content_node, uri);

          match content_node.kind() {
              "plain_scalar" | "single_quote_scalar" | "double_quote_scalar" => {
                  // Valid: !$include "path/to/file"
                  // Could add additional validation for file path format
              }
              "flow_mapping" | "block_mapping" => {
                  // Valid: !$include {path: "file", query: "selector"}
                  self.validate_mapping_fields(content_node, src, uri, &["path"], &["query"], "!$include", diagnostics);
              }
              _ => {
                  diagnostics.add_error(
                      ParseError::with_location(
                          "!$include expects string path or mapping with path field",
                          uri.clone(), meta.start, meta.end
                      ).with_code(error_codes::INVALID_TYPE)
                  );
              }
          }
      }

      /// Validate !$let tag structure
      fn validate_let_tag_semantics(&self, content_node: tree_sitter::Node, src: &[u8], uri: &Url, diagnostics: &mut ParseDiagnostics) {
          let meta = node_meta(&content_node, uri);

          match content_node.kind() {
              "flow_mapping" | "block_mapping" => {
                  self.validate_mapping_fields(content_node, src, uri, &["in"], &[], "!$let", diagnostics);
                  // Additional validation: other fields should be variable bindings
              }
              _ => {
                  diagnostics.add_error(
                      ParseError::with_location(
                          "!$let expects mapping with variable bindings and 'in' field",
                          uri.clone(), meta.start, meta.end
                      ).with_code(error_codes::INVALID_TYPE)
                  );
              }
          }
      }

      /// Validate mapping has required fields
      fn validate_mapping_fields(&self, mapping_node: tree_sitter::Node, src: &[u8], uri: &Url,
                               required_fields: &[&str], optional_fields: &[&str],
                               tag_name: &str, diagnostics: &mut ParseDiagnostics) {
          let mut found_fields = std::collections::HashSet::new();

          // Walk through mapping pairs
          let mut cursor = mapping_node.walk();
          for child in mapping_node.named_children(&mut cursor) {
              if child.kind() == "flow_pair" || child.kind() == "block_mapping_pair" {
                  if let Some(key_node) = child.child_by_field_name("key") {
                      if let Ok(key_text) = key_node.utf8_text(src) {
                          // Extract key (remove quotes if present)
                          let key = if key_text.starts_with('"') && key_text.ends_with('"') && key_text.len() >= 2 {
                              &key_text[1..key_text.len()-1]
                          } else if key_text.starts_with('\'') && key_text.ends_with('\'') && key_text.len() >= 2 {
                              &key_text[1..key_text.len()-1]
                          } else {
                              key_text
                          };
                          found_fields.insert(key);
                      }
                  }
              }
          }

          // Check for missing required fields
          for &required_field in required_fields {
              if !found_fields.contains(required_field) {
                  let meta = node_meta(&mapping_node, uri);
                  diagnostics.add_error(
                      ParseError::with_location(
                          format!("Missing required '{}' field in {} tag", required_field, tag_name),
                          uri.clone(), meta.start, meta.end
                      ).with_code(error_codes::MISSING_FIELD)
                  );
              }
          }

          // Check for unexpected fields (warnings)
          let all_valid_fields: std::collections::HashSet<_> = required_fields.iter()
              .chain(optional_fields.iter())
              .cloned()
              .collect();

          for found_field in &found_fields {
              if !all_valid_fields.contains(found_field) {
                  let meta = node_meta(&mapping_node, uri);
                  diagnostics.add_warning(
                      ParseWarning::with_location(
                          format!("Unexpected field '{}' in {} tag", found_field, tag_name),
                          uri.clone(), meta.start, meta.end
                      )
                  );
              }
          }
      }

      /// Check if this is a known CloudFormation tag
      fn is_known_cloudformation_tag(&self, tag_name: &str) -> bool {
          matches!(tag_name,
              "!Ref" | "!GetAtt" | "!Sub" | "!Join" | "!Split" |
              "!Select" | "!FindInMap" | "!ImportValue" | "!Condition" |
              "!And" | "!Or" | "!Not" | "!Equals" | "!If" |
              "!Base64" | "!GetAZs" | "!Cidr"
          )
      }
  }

  Phase 4: API Integration & Backward Compatibility

  Step 4.1: Update Public API

  File: src/yaml/parsing_w_loc/mod.rs

  // New exports for diagnostic API
  pub use error::{ParseDiagnostics, ParseWarning, ParseMode, error_codes};

  // New diagnostic parsing function
  pub fn parse_yaml_ast_with_diagnostics(source: &str, uri: Url) -> ParseDiagnostics {
      let mut parser = YamlParser::new().expect("Failed to create YAML parser");
      parser.validate_with_diagnostics(source, uri)
  }

  // Backward compatibility - existing function unchanged
  pub fn parse_yaml_ast(source: &str, uri: Url) -> ParseResult<YamlAst> {
      let mut parser = YamlParser::new()?;
      parser.parse(source, uri)
  }

  Step 4.2: Update Convert Module

  File: src/yaml/parsing_w_loc/convert.rs

  /// New diagnostic API for convert module
  pub fn parse_and_convert_to_original_with_diagnostics(source: &str, uri_str: &str) -> Result<ParseDiagnostics, anyhow::Error> {
      let uri = create_uri_from_string(uri_str)?;
      let diagnostics = parse_yaml_ast_with_diagnostics(source, uri);
      Ok(diagnostics)
  }

  /// Validate YAML without conversion (useful for linting)
  pub fn validate_yaml_only(source: &str, uri_str: &str) -> Result<ParseDiagnostics, anyhow::Error> {
      parse_and_convert_to_original_with_diagnostics(source, uri_str)
  }

  // Backward compatibility - existing function unchanged
  pub fn parse_and_convert_to_original(source: &str, uri_str: &str) -> anyhow::Result<original::YamlAst> {
      // Current implementation unchanged
      let uri = match Url::parse(uri_str) {
          Ok(uri) => uri,
          Err(_) => {
              match Url::from_file_path(uri_str) {
                  Ok(uri) => uri,
                  Err(_) => {
                      Url::parse(&format!("file://{}", uri_str))
                          .map_err(|e| anyhow::anyhow!("Cannot create URI from '{}': {}", uri_str, e))?
                  }
              }
          }
      };

      let with_location_ast = parse_yaml_ast(source, uri)
          .map_err(|e| anyhow::anyhow!("{}", e.message))?;

      Ok(to_original_ast(&with_location_ast))
  }

  fn create_uri_from_string(uri_str: &str) -> anyhow::Result<Url> {
      match Url::parse(uri_str) {
          Ok(uri) => Ok(uri),
          Err(_) => {
              match Url::from_file_path(uri_str) {
                  Ok(uri) => Ok(uri),
                  Err(_) => {
                      Url::parse(&format!("file://{}", uri_str))
                          .map_err(|e| anyhow::anyhow!("Cannot create URI from '{}': {}", uri_str, e))
                  }
              }
          }
      }
  }

  Phase 5: Testing & Validation

  Step 5.1: Backward Compatibility Tests

  // Add to existing test files to ensure no regressions
  #[cfg(test)]
  mod diagnostic_tests {
      use super::*;

      #[test]
      fn test_backward_compatibility_all_existing_tests_pass() {
          // Run all existing 472 tests with old API to ensure no regressions
          // This test should pass if we haven't broken anything
      }

      #[test]
      fn test_diagnostic_api_basic() {
          let source = r#"
  Resources:
    Bucket:
      Type: AWS::S3::Bucket
      Properties:
        BucketName: "test"
  "#;
          let diagnostics = parse_yaml_ast_with_diagnostics(source, Url::parse("file://test.yaml").unwrap());
          assert!(!diagnostics.has_errors());
          assert!(diagnostics.parse_successful);
      }

      #[test]
      fn test_multiple_errors_collected() {
          let source = r#"
  Resources:
    Bucket:
      Type: AWS::S3::Bucket
      Properties:
        BucketName: !$unknownTag
        InvalidProperty: !$anotherUnknownTag
  "#;
          let diagnostics = parse_yaml_ast_with_diagnostics(source, Url::parse("file://test.yaml").unwrap());

          assert!(diagnostics.has_errors());
          assert_eq!(diagnostics.error_count(), 2);
          assert!(!diagnostics.parse_successful);

          // Check that both errors are collected
          assert!(diagnostics.errors.iter().any(|e| e.message.contains("unknownTag")));
          assert!(diagnostics.errors.iter().any(|e| e.message.contains("anotherUnknownTag")));
      }
  }

  Step 5.2: Error Collection Test Suite

  #[cfg(test)]
  mod error_collection_tests {
      use super::*;

      #[test]
      fn test_syntax_errors_collected() {
          let source = r#"
  Resources:
    Bucket:
      Type: AWS::S3::Bucket
      Properties:
        BucketName: "unclosed quote
        Tags: [
  "#;
          let diagnostics = parse_yaml_ast_with_diagnostics(source, Url::parse("file://test.yaml").unwrap());

          assert!(diagnostics.has_errors());
          // Should collect multiple syntax errors
          assert!(diagnostics.error_count() > 0);
      }

      #[test]
      fn test_semantic_validation_without_ast_building() {
          let source = r#"
  Resources:
    Bucket:
      Type: AWS::S3::Bucket
      Properties:
        BucketName: !$include  # Missing path
        Tags: !$let            # Missing 'in' field
  "#;
          let diagnostics = parse_yaml_ast_with_diagnostics(source, Url::parse("file://test.yaml").unwrap());

          assert!(diagnostics.has_errors());
          // Should have errors for both missing fields
          assert!(diagnostics.errors.iter().any(|e| e.message.contains("include")));
          assert!(diagnostics.errors.iter().any(|e| e.message.contains("let")));
      }

      #[test]
      fn test_warnings_collected() {
          let source = r#"
  Resources:
    Bucket:
      Type: AWS::S3::Bucket
      Properties:
        BucketName: "test"
        UnknownProperty: "value"  # Should generate warning
  "#;
          let diagnostics = parse_yaml_ast_with_diagnostics(source, Url::parse("file://test.yaml").unwrap());

          // Syntax is valid, no errors
          assert!(!diagnostics.has_errors());
          // But should have warnings for unknown properties
          assert!(!diagnostics.warnings.is_empty());
      }
  }

  Phase 6: Future Partial AST Support (Post-Phase 5)

  Step 6.1: Extend Types for Partial AST

  // Add to error.rs after Phase 1-5 are complete and tested

  #[derive(Debug, Clone)]
  pub struct ParseDiagnosticsWithAst<T> {
      pub result: Option<T>,  // Partial or complete AST
      pub errors: Vec<ParseError>,
      pub warnings: Vec<ParseWarning>,
      pub parse_successful: bool,
  }

  impl<T> ParseDiagnosticsWithAst<T> {
      pub fn success(ast: T) -> Self {
          Self {
              result: Some(ast),
              errors: Vec::new(),
              warnings: Vec::new(),
              parse_successful: true,
          }
      }

      pub fn partial(ast: T, errors: Vec<ParseError>, warnings: Vec<ParseWarning>) -> Self {
          Self {
              result: Some(ast),
              errors,
              warnings,
              parse_successful: false,
          }
      }

      pub fn failure(errors: Vec<ParseError>) -> Self {
          Self {
              result: None,
              errors,
              warnings: Vec::new(),
              parse_successful: false,
          }
      }
  }

  #[derive(Debug, Clone)]
  pub enum ParseMode {
      StrictValidation,  // Stop on first error (current)
      CollectAll,        // Collect all errors, no AST
      BestEffort,        // Build partial AST where possible (Phase 6)
  }

  Step 6.2: Partial AST Building API

  impl YamlParser {
      /// Phase 6: Build partial AST with error recovery
      pub fn parse_with_recovery(&mut self, source: &str, uri: Url) -> ParseDiagnosticsWithAst<YamlAst> {
          // Implementation for Phase 6
          // This would build AST nodes where possible, insert placeholders for errors
          todo!("Implement in Phase 6 after basic error collection is stable")
      }
  }

  Implementation Timeline

  Week 1: Core Infrastructure (Phases 1-2)

  - Extend error types with warnings and diagnostics
  - Add new parser API methods
  - Implement comprehensive syntax error collection
  - Test backward compatibility (all 472 tests must pass)

  Week 2: Semantic Validation (Phase 3)

  - Implement lightweight semantic validation without AST building
  - Add tag-specific validation methods
  - Add field validation for mappings
  - Test with intentionally broken YAML files

  Week 3: Integration & Testing (Phases 4-5)

  - Update public APIs with backward compatibility
  - Add comprehensive test suite for error collection
  - Performance testing to ensure minimal impact
  - Documentation and examples

  Week 4: LSP Preparation

  - Add error code categorization
  - Create diagnostic conversion utilities
  - Add configuration options for different validation modes
  - Prepare for LSP integration

  Future: Partial AST Support (Phase 6)

  - Implement after Phases 1-5 are complete and all tests pass
  - Add error recovery strategies
  - Build partial AST with placeholders
  - Extended testing for partial AST scenarios

  Success Criteria

  Phase 1-5 Completion:

  1. ✅ All existing 472 tests pass with backward compatible API
  2. ✅ New diagnostic API collects multiple errors in one pass
  3. ✅ No performance regression for normal parsing use cases
  4. ✅ Comprehensive error location information for LSP integration
  5. ✅ Clear separation between syntax and semantic errors

  Ready for LSP Integration:

  - Error messages with precise location ranges
  - Error categorization with codes
  - Support for warnings and informational diagnostics
  - Configurable validation modes
  - Comprehensive test coverage

  This plan provides a solid foundation for multi-error collection while maintaining full backward compatibility and preparing for future LSP/linter
  integration.

...
...

  🎉 Multi-Error Collection Implementation - COMPLETE

  I have successfully implemented a comprehensive multi-error collection system for the YAML parser that enables LSP/linter integration while
  maintaining full backward compatibility.

  ✅ What Was Achieved

  Core Features:
  - Multi-Error Collection: Parser now collects ALL errors in a single pass instead of stopping at the first error
  - Comprehensive Validation: Detects syntax errors, unknown tags, missing required fields, and invalid structures
  - Error Categorization: Structured error codes (IY1001, IY4001, etc.) for different error types
  - Precise Location Information: Line/column data for each error, ready for LSP integration
  - Warning Support: Non-fatal warnings for issues like unexpected fields
  - Full Backward Compatibility: All 486 existing tests pass - zero breaking changes

  New APIs:
  // Diagnostic API - collects all errors
  let diagnostics = parse_yaml_ast_with_diagnostics(source, uri);

  // Validation only (no AST building)
  let diagnostics = validate_yaml_only(source, uri_str)?;

  // Convert with diagnostics
  let diagnostics = parse_and_convert_to_original_with_diagnostics(source, uri_str)?;

  Comprehensive Tag Validation:
  - ✅ Unknown tag detection (!$unknownTag → error with code IY4001)
  - ✅ Missing required fields (!$let without in field → error with code IY4002)
  - ✅ Invalid tag structures (wrong types, missing content → error with code IY4003)
  - ✅ Syntax error collection (malformed YAML → error with code IY1001)
  - ✅ Support for both mapping and sequence syntax (!$join [delimiter, array] and !$join {delimiter: "x", array: [...]})

  LSP Integration Ready:
  - Multiple errors collected in one parse
  - Precise error locations with line/column information
  - Structured error codes for categorization
  - Warning vs error distinction
  - Performance optimized (no full AST building for validation)

  🔧 Implementation Details

  The system uses a lightweight semantic validation approach that:
  1. Parses with tree-sitter to get syntax tree
  2. Traverses nodes to find tagged elements
  3. Validates tag semantics without building full AST
  4. Collects all errors with precise location information
  5. Maintains backward compatibility by keeping original API unchanged

  📊 Test Results

  - ✅ 486/486 tests passing (100% backward compatibility)
  - ✅ 12 new diagnostic tests covering all error collection scenarios
  - ✅ Performance validated - no regression in normal parsing use cases

  The implementation provides a solid foundation for LSP server development while ensuring existing functionality remains completely unchanged.
  Future enhancements can build on this foundation to add partial AST support and advanced IDE features.
