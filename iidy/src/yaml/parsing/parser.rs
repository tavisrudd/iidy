use serde_yaml::Number;
use std::collections::HashMap;
use std::str::FromStr;
use tree_sitter::{Node, Parser, Point, Tree};
use tree_sitter_yaml::LANGUAGE;
use url::Url;

use super::ast::{
    CloudFormationTag, ConcatMapTag, ConcatTag, EqTag, EscapeTag, FromPairsTag, GroupByTag, IfTag,
    IncludeTag, JoinTag, LetTag, MapListToHashTag, MapTag, MapValuesTag, MergeMapTag, MergeTag,
    NotTag, ParseJsonTag, ParseYamlTag, Position, PreprocessingTag, SplitTag, SrcMeta,
    ToJsonStringTag, ToYamlStringTag, UnknownTag, YamlAst,
};
use super::error::{ParseDiagnostics, ParseError, ParseResult, error_codes};
use crate::yaml::errors::{missing_required_field_error, tag_parsing_error, yaml_syntax_error};

/// YAML chomping indicator for block scalars
#[derive(Debug, Clone, Copy)]
enum ChompingIndicator {
    /// Clip (default): single trailing newline
    Clip,
    /// Strip (-): remove all trailing newlines
    Strip,
    /// Keep (+): preserve all trailing newlines
    Keep,
}

/// Remove common leading indentation from all content lines
fn remove_common_indentation<'a>(lines: &[&'a str]) -> Vec<&'a str> {
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .iter()
        .map(|&line| {
            if line.len() >= min_indent {
                &line[min_indent..]
            } else {
                line.trim_start()
            }
        })
        .collect()
}

/// Fold lines for folded scalars (>): join consecutive non-empty lines with spaces
fn fold_lines(lines: &[&str]) -> String {
    // Estimate capacity: total chars + spaces between lines
    let estimated_capacity = lines.iter().map(|l| l.len()).sum::<usize>() + lines.len();
    let mut result = String::with_capacity(estimated_capacity);
    let mut pending_paragraph_break = false;

    for &line in lines {
        if line.trim().is_empty() {
            // Empty line creates paragraph break before next content
            pending_paragraph_break = true;
        } else {
            // Non-empty line
            if !result.is_empty() {
                if pending_paragraph_break {
                    result.push_str("\n\n"); // Paragraph break
                    pending_paragraph_break = false;
                } else {
                    result.push(' '); // Fold with space
                }
            }
            result.push_str(line);
        }
    }
    result
}

/// Apply YAML chomping indicator to handle trailing newlines
fn apply_chomping(mut content: String, chomping: ChompingIndicator, node: Node, src: &[u8]) -> String {
    if content.is_empty() {
        return content;
    }

    match chomping {
        ChompingIndicator::Strip => content,
        ChompingIndicator::Clip => {
            content.push('\n');
            content
        }
        ChompingIndicator::Keep => {
            let trailing_newlines = count_trailing_newlines_in_source(node, src);
            for _ in 0..trailing_newlines {
                content.push('\n');
            }
            content
        }
    }
}

/// Count trailing newlines for keep indicator by examining source
/// Tree-sitter may not include trailing blank lines in node boundaries for literal scalars
fn count_trailing_newlines_in_source(node: Node, src: &[u8]) -> usize {
    let node_end = node.end_byte();
    let node_start = node.start_byte();

    // Helper to check if byte is whitespace (but not newline)
    let is_whitespace = |b: u8| b == b'\r' || b == b' ' || b == b'\t';

    // Count trailing newlines within the node (scan backwards)
    let count_in_node = src[node_start..node_end]
        .iter()
        .rev()
        .take_while(|&&b| b == b'\n' || is_whitespace(b))
        .filter(|&&b| b == b'\n')
        .count();

    // Look ahead past node boundary for literal scalars
    let count_after_node = src[node_end..]
        .iter()
        .take_while(|&&b| b == b'\n' || is_whitespace(b))
        .filter(|&&b| b == b'\n')
        .count();

    count_in_node + count_after_node
}

pub struct YamlParser {
    parser: Parser,
}

impl YamlParser {
    pub fn new() -> ParseResult<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&LANGUAGE.into())
            .map_err(|_| ParseError::new("Failed to set YAML language for tree-sitter parser"))?;

        Ok(Self { parser })
    }
    
    pub fn parse(&mut self, source: &str, uri: Url) -> ParseResult<YamlAst> {
        let tree = self
            .parser
            .parse(source, None)
            .ok_or_else(|| ParseError::new("Failed to parse YAML source"))?;

        let root = tree.root_node();

        if root.has_error() {
            return Err(self.find_syntax_error(&tree, source, &uri));
        }

        let mut anchor_map = HashMap::new();
        self.build_ast(root, source.as_bytes(), &uri, &mut anchor_map)
    }

    /// New API for collecting all errors without stopping on first error
    pub fn validate_with_diagnostics(&mut self, source: &str, uri: Url) -> ParseDiagnostics {
        let mut diagnostics = ParseDiagnostics::new();

        // Parse with tree-sitter
        let tree = match self.parser.parse(source, None) {
            Some(tree) => tree,
            None => {
                diagnostics.add_error(
                    ParseError::new("Failed to parse YAML source")
                        .with_code(error_codes::SYNTAX_ERROR),
                );
                return diagnostics;
            }
        };

        // Collect ALL syntax errors (not just first)
        self.collect_all_syntax_errors(&tree, source, &uri, &mut diagnostics);

        // If no fatal syntax errors, proceed with semantic validation
        if !self.has_fatal_syntax_errors(&diagnostics) {
            let mut anchor_map = HashMap::new();
            self.validate_semantics_with_diagnostics(&tree, source, &uri, &mut diagnostics, &mut anchor_map);
        }

        diagnostics
    }

    /// Collect ALL syntax errors from tree-sitter parse tree
    fn collect_all_syntax_errors(
        &self,
        tree: &Tree,
        source: &str,
        uri: &Url,
        diagnostics: &mut ParseDiagnostics,
    ) {
        let root = tree.root_node();
        self.traverse_for_syntax_errors(root, source, uri, diagnostics);
    }

    /// Recursively traverse tree and collect all error/missing nodes
    fn traverse_for_syntax_errors(
        &self,
        node: tree_sitter::Node,
        source: &str,
        uri: &Url,
        diagnostics: &mut ParseDiagnostics,
    ) {
        // Check current node for errors
        if node.is_error() || node.kind() == "ERROR" {
            let meta = node_meta(&node, uri);
            let message = self.analyze_syntax_error(&node, source);
            let error = self
                .create_syntax_error(&message, &meta, source)
                .with_code(error_codes::SYNTAX_ERROR);
            diagnostics.add_error(error);
        }

        if node.is_missing() {
            let meta = node_meta(&node, uri);
            let message = format!("Missing {} element", node.kind());
            let error = self
                .create_syntax_error(&message, &meta, source)
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
                message: format!(
                    "Syntax error: {} @ {}",
                    message,
                    self.format_file_location(meta)
                ),
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

    /// Validate semantics by building AST in error-tolerant mode
    fn validate_semantics_with_diagnostics(
        &self,
        tree: &Tree,
        source: &str,
        uri: &Url,
        diagnostics: &mut ParseDiagnostics,
        anchor_map: &mut HashMap<String, YamlAst>,
    ) {
        let root = tree.root_node();

        // Use a modified version of build_ast that collects errors instead of stopping
        self.build_ast_with_error_collection(root, source.as_bytes(), uri, diagnostics, anchor_map);
    }

    /// Build AST but collect all errors instead of stopping on first error
    fn build_ast_with_error_collection(
        &self,
        node: Node,
        src: &[u8],
        uri: &Url,
        diagnostics: &mut ParseDiagnostics,
        anchor_map: &mut HashMap<String, YamlAst>,
    ) {
        match node.kind() {
            "stream" => {
                // Process all document children
                for i in 0..node.named_child_count() {
                    if let Some(child) = node.named_child(i) {
                        if child.kind() == "document" {
                            self.build_ast_with_error_collection(child, src, uri, diagnostics, anchor_map);
                        }
                    }
                }
            }
            "document" => {
                // Process all children in document
                for i in 0..node.named_child_count() {
                    if let Some(child) = node.named_child(i) {
                        self.build_ast_with_error_collection(child, src, uri, diagnostics, anchor_map);
                    }
                }
            }
            "block_mapping" | "flow_mapping" => {
                // Process mapping pairs
                let mut cursor = node.walk();
                for pair_node in node.named_children(&mut cursor) {
                    if matches!(pair_node.kind(), "block_mapping_pair" | "flow_pair") {
                        self.build_ast_with_error_collection(pair_node, src, uri, diagnostics, anchor_map);
                    }
                }
            }
            "block_mapping_pair" | "flow_pair" => {
                // Process key and value
                let mut pair_cursor = node.walk();
                for child in node.named_children(&mut pair_cursor) {
                    self.build_ast_with_error_collection(child, src, uri, diagnostics, anchor_map);
                }
            }
            "flow_node" => {
                // Check if this flow_node contains a tag - if so, validate it
                let has_tag = (0..node.named_child_count())
                    .any(|i| node.named_child(i).map_or(false, |child| child.kind() == "tag"));
                
                if has_tag {
                    // This is a tagged flow node - validate it
                    match self.build_ast(node, src, uri, anchor_map) {
                        Ok(_) => {
                            // Success, no error to collect
                        }
                        Err(parse_error) => {
                            // Collect this error and continue
                            diagnostics.add_error(parse_error);
                        }
                    }
                } else {
                    // Not a tagged node, recurse into children
                    for i in 0..node.named_child_count() {
                        if let Some(child) = node.named_child(i) {
                            self.build_ast_with_error_collection(child, src, uri, diagnostics, anchor_map);
                        }
                    }
                }
            }
            "block_node" => {
                // Check if this block_node contains a tag - if so, validate it
                let has_tag = (0..node.named_child_count())
                    .any(|i| node.named_child(i).map_or(false, |child| child.kind() == "tag"));

                if has_tag {
                    // This is a tagged block node - validate it
                    match self.build_ast(node, src, uri, anchor_map) {
                        Ok(_) => {
                            // Success, no error to collect
                        }
                        Err(parse_error) => {
                            // Collect this error and continue
                            diagnostics.add_error(parse_error);
                        }
                    }
                } else {
                    // Not a tagged node, recurse into children
                    for i in 0..node.named_child_count() {
                        if let Some(child) = node.named_child(i) {
                            self.build_ast_with_error_collection(child, src, uri, diagnostics, anchor_map);
                        }
                    }
                }
            }
            _ => {
                // For other nodes, recurse into children
                for i in 0..node.named_child_count() {
                    if let Some(child) = node.named_child(i) {
                        self.build_ast_with_error_collection(child, src, uri, diagnostics, anchor_map);
                    }
                }
            }
        }
    }

    fn find_syntax_error(&self, tree: &Tree, source: &str, uri: &Url) -> ParseError {
        // Traverse the tree to find the first error node
        let root = tree.root_node();

        if let Some(error_node) = self.find_error_node(root) {
            let meta = node_meta(&error_node, uri);

            // Try to determine the specific type of syntax error
            let error_message = self.analyze_syntax_error(&error_node, source);

            // Use the actual source for serde_yaml error detection
            self.syntax_error(&error_message, &meta, source)
        } else {
            // Fallback if no specific error node found
            let meta = SrcMeta {
                input_uri: uri.clone(),
                start: Position::new(0, 0),
                end: Position::new(0, 0),
            };
            self.syntax_error("Syntax error in YAML", &meta, source)
        }
    }

    /// Recursively find the first error node in the tree
    fn find_error_node<'a>(&self, node: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        // Check if this node is an error
        if node.is_error() || node.kind() == "ERROR" {
            return Some(node);
        }

        // Check if this node is missing (indicates a syntax error)
        if node.is_missing() {
            return Some(node);
        }

        // Recursively check children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if let Some(error_node) = self.find_error_node(child) {
                    return Some(error_node);
                }
            }
        }

        None
    }

    /// Analyze the syntax error to provide a more specific message
    fn analyze_syntax_error(&self, error_node: &tree_sitter::Node<'_>, source: &str) -> String {
        let node_text = error_node.utf8_text(source.as_bytes()).unwrap_or("");

        // Analyze common syntax error patterns
        if node_text.contains('"') && !node_text.ends_with('"') {
            "unexpected end of file".to_string()
        } else if error_node.is_missing() {
            "missing syntax element".to_string()
        } else if error_node.kind() == "ERROR" {
            "invalid syntax".to_string()
        } else {
            "syntax error".to_string()
        }
    }

    #[inline]
    fn build_ast(&self, node: Node, src: &[u8], uri: &Url, anchor_map: &mut HashMap<String, YamlAst>) -> ParseResult<YamlAst> {
        // Cache the node kind to avoid repeated string comparisons
        let node_kind = node.kind();
        let meta = node_meta(&node, uri);

        match node_kind {
            "stream" => {
                // Stream is the root node, process its document children
                for i in 0..node.named_child_count() {
                    if let Some(child) = node.named_child(i) {
                        if child.kind() == "document" {
                            match self.build_ast(child, src, uri, anchor_map) {
                                Ok(result) => {
                                    if !matches!(result, YamlAst::Null(_)) {
                                        return Ok(result);
                                    }
                                }
                                Err(e) => {
                                    // Only continue for syntax errors, not validation errors
                                    // Validation errors should propagate up
                                    return Err(e);
                                }
                            }
                        }
                        // Skip comment nodes and other non-document children
                    }
                }
                // If no valid documents found, return null
                Ok(YamlAst::Null(meta))
            }
            "document" => {
                // Skip document wrapper and process its content
                // Try each child until we find non-null content
                for i in 0..node.named_child_count() {
                    if let Some(child) = node.named_child(i) {
                        match self.build_ast(child, src, uri, anchor_map) {
                            Ok(result) => {
                                // Skip null results and try the next child
                                if !matches!(result, YamlAst::Null(_)) {
                                    return Ok(result);
                                }
                            }
                            Err(e) => {
                                // Only continue for syntax errors, not validation errors
                                // Validation errors should propagate up
                                return Err(e);
                            }
                        }
                    }
                }
                // If all children are null or no children, return null
                Ok(YamlAst::Null(meta))
            }
            "block_mapping" | "flow_mapping" => self.build_mapping(node, src, uri, meta, anchor_map),
            "block_sequence" | "flow_sequence" => self.build_sequence(node, src, uri, meta, anchor_map),
            "block_sequence_item" | "flow_sequence_item" => {
                // Unwrap sequence items
                if let Some(child) = node.named_child(0) {
                    self.build_ast(child, src, uri, anchor_map)
                } else {
                    Ok(YamlAst::Null(meta))
                }
            }
            "plain_scalar" => self.build_scalar(node, src, meta, false),
            "single_quote_scalar" | "double_quote_scalar" => {
                self.build_scalar(node, src, meta, true)
            }
            "literal" | "folded" | "block_scalar" => {
                // Handle multiline block scalars (| and >)
                self.build_block_scalar(node, src, meta)
            }
            "flow_node" => {
                // Handle tagged flow nodes (e.g., "!Ref MyResource")
                self.build_flow_node(node, src, uri, meta, anchor_map)
            }
            "block_node" => {
                // Handle block nodes which may contain tags with block content
                self.build_block_node(node, src, uri, meta, anchor_map)
            }
            "tag" => self.build_tagged_node(node, src, uri, meta, anchor_map),
            "alias" => {
                // Extract alias name and look up in anchor map
                let alias_name = self.extract_alias_name(node, src, &meta)?;
                anchor_map
                    .get(&alias_name)
                    .cloned()
                    .ok_or_else(|| self.undefined_alias_error(&alias_name, &meta))
            }
            "anchor" => {
                // Extract anchor name and value, store in map, return value
                let anchor_name = self.extract_anchor_name(node, src, &meta)?;
                let value = if let Some(child) = node.named_child(0) {
                    self.build_ast(child, src, uri, anchor_map)?
                } else {
                    YamlAst::Null(meta.clone())
                };
                anchor_map.insert(anchor_name, value.clone());
                Ok(value)
            }
            _ => {
                // Handle any remaining node types as null or attempt to parse as text
                if node.named_child_count() > 0 {
                    if let Some(child) = node.named_child(0) {
                        self.build_ast(child, src, uri, anchor_map)
                    } else {
                        Ok(YamlAst::Null(meta))
                    }
                } else {
                    Ok(YamlAst::Null(meta))
                }
            }
        }
    }

    fn build_mapping(
        &self,
        node: Node,
        src: &[u8],
        uri: &Url,
        meta: SrcMeta,
        anchor_map: &mut HashMap<String, YamlAst>,
    ) -> ParseResult<YamlAst> {
        // More accurate sizing for CloudFormation documents
        let total_children = node.named_child_count();
        let estimated_pairs = if total_children > 100 {
            // For large documents, assume most children are pairs
            total_children
        } else {
            total_children / 2
        };
        
        let mut pairs = Vec::with_capacity(estimated_pairs);
        let mut cursor = node.walk();

        // Direct processing without intermediate collection for better cache locality
        for pair_node in node.named_children(&mut cursor) {
            match pair_node.kind() {
                "block_mapping_pair" | "flow_pair" => {
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

                    // Look for value node, skipping any comments
                    let mut value = YamlAst::Null(node_meta(&pair_node, uri));
                    while let Some(val_node) = children.next() {
                        match val_node.kind() {
                            "comment" => continue, // Skip comments between key and value
                            _ => {
                                value = self.build_ast(val_node, src, uri, anchor_map)?;
                                break;
                            }
                        }
                    }

                    pairs.push((key, value));
                }
                "comment" => continue, // Skip comments in mappings
                _ => {} // Skip other structural nodes
            }
        }

        Ok(YamlAst::Mapping(pairs, meta))
    }

    fn build_sequence(
        &self,
        node: Node,
        src: &[u8],
        uri: &Url,
        meta: SrcMeta,
        anchor_map: &mut HashMap<String, YamlAst>,
    ) -> ParseResult<YamlAst> {
        let child_count = node.named_child_count();
        let mut items = Vec::with_capacity(child_count);
        let mut cursor = node.walk();

        // Direct iteration without collecting all children first
        for child in node.named_children(&mut cursor) {
            let child_kind = child.kind();
            match child_kind {
                "comment" => continue, // Skip comments
                "block_sequence_item" => {
                    // For block sequence items, process their content
                    if let Some(content_child) = child.named_child(0) {
                        let item = self.build_ast(content_child, src, uri, anchor_map)?;
                        items.push(item);
                    }
                }
                _ => {
                    let item = self.build_ast(child, src, uri, anchor_map)?;
                    // Fast path: most items are not null
                    if !matches!(item, YamlAst::Null(_)) {
                        items.push(item);
                    } else {
                        // Check if this is a legitimate null or spurious
                        let child_text = child.utf8_text(src).unwrap_or("");
                        if !child_text.trim().is_empty() || child_kind == "null" {
                            items.push(item);
                        }
                        // Otherwise skip spurious nulls from empty structural nodes
                    }
                }
            }
        }

        Ok(YamlAst::Sequence(items, meta))
    }

    #[inline]
    fn build_scalar(
        &self,
        node: Node,
        src: &[u8],
        meta: SrcMeta,
        is_quoted: bool,
    ) -> ParseResult<YamlAst> {
        let text = self.extract_utf8_text(node, src, &meta, "scalar")?;

        // Fast path for simple unquoted strings
        if !is_quoted {
            // Early check for special values before string processing using bytes comparison
            let text_bytes = text.as_bytes();
            match text_bytes {
                b"true" => return Ok(YamlAst::Bool(true, meta)),
                b"false" => return Ok(YamlAst::Bool(false, meta)),
                b"null" | b"~" | b"" => return Ok(YamlAst::Null(meta)),
                _ => {}
            }

            // Check if it's templated before number parsing
            if text_bytes.windows(2).any(|w| w == b"{{") {
                return Ok(YamlAst::TemplatedString(text, meta));
            }

            // Try to parse as number (only if it looks like a number)
            if !text.is_empty() && (text_bytes[0].is_ascii_digit() || text_bytes[0] == b'-' || text_bytes[0] == b'+') {
                if let Ok(num) = Number::from_str(&text) {
                    return Ok(YamlAst::Number(num, meta));
                }
            }

            return Ok(YamlAst::PlainString(text, meta));
        }

        // Handle quoted strings - remove quotes and process escape sequences
        let content = if text.len() >= 2 {
            let inner = &text[1..text.len() - 1];
            // Process escape sequences for double-quoted strings
            if text.starts_with('"') {
                self.unescape_string(inner)
            } else {
                // Single quotes don't process escape sequences
                inner.to_string()
            }
        } else {
            text
        };

        // Check if it's a templated string
        if content.contains("{{") && content.contains("}}") {
            Ok(YamlAst::TemplatedString(content, meta))
        } else {
            Ok(YamlAst::PlainString(content, meta))
        }
    }

    fn build_block_scalar(&self, node: Node, src: &[u8], meta: SrcMeta) -> ParseResult<YamlAst> {
        let text = self.extract_utf8_text(node, src, &meta, "block scalar")?;

        // Parse scalar type and chomping indicator from first line
        let first_char = text.chars().next().unwrap_or(' ');
        let is_folded = first_char == '>';
        let chomping = match text.chars().nth(1) {
            Some('-') => ChompingIndicator::Strip,
            Some('+') => ChompingIndicator::Keep,
            _ => ChompingIndicator::Clip,
        };

        // Split into lines and skip the indicator line (use slice to avoid allocation)
        let lines: Vec<&str> = text.lines().collect();
        if lines.len() <= 1 {
            return Ok(YamlAst::PlainString(String::new(), meta));
        }
        let content_lines = &lines[1..];

        // Remove common indentation from all lines
        let stripped_lines = remove_common_indentation(content_lines);

        // Build content: fold lines for folded scalars, preserve for literal
        let content = if is_folded {
            fold_lines(&stripped_lines)
        } else {
            stripped_lines.join("\n")
        };

        // Apply chomping indicator to add/remove trailing newlines
        let final_content = apply_chomping(content, chomping, node, src);

        // Detect templated strings
        if final_content.contains("{{") && final_content.contains("}}") {
            Ok(YamlAst::TemplatedString(final_content, meta))
        } else {
            Ok(YamlAst::PlainString(final_content, meta))
        }
    }

    fn build_block_node(
        &self,
        node: Node,
        src: &[u8],
        uri: &Url,
        meta: SrcMeta,
        anchor_map: &mut HashMap<String, YamlAst>,
    ) -> ParseResult<YamlAst> {
        // Block nodes can contain:
        // 1. An anchor (optional) followed by content
        // 2. A tag (optional) followed by content
        // 3. Just content

        let mut anchor_node = None;
        let mut tag_node = None;
        let mut content_node = None;

        // Examine children to find anchor, tag, and content
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                match child.kind() {
                    "anchor" => anchor_node = Some(child),
                    "tag" => tag_node = Some(child),
                    "block_mapping" | "block_sequence" | "flow_mapping" | "flow_sequence"
                    | "literal" | "folded" | "block_scalar" => {
                        content_node = Some(child);
                    }
                    _ => {
                        // If no tag/anchor found yet, try to parse other children as content
                        if tag_node.is_none() && anchor_node.is_none() {
                            content_node = Some(child);
                        }
                    }
                }
            }
        }

        // Build the result (handling tags if present)
        let result = if let Some(tag) = tag_node {
            // This is a block-style tagged node
            let tag_text = self.extract_utf8_text(tag, src, &meta, "tag")?;

            let tag_name = tag_text.trim();

            // Parse the content
            let tagged_content = if let Some(content) = content_node {
                self.build_ast(content, src, uri, anchor_map)?
            } else {
                YamlAst::Null(meta.clone())
            };

            // Classify the tag type based on naming convention - optimize the check
            if tag_name.as_bytes().get(0) == Some(&b'!') && tag_name.as_bytes().get(1) == Some(&b'$') {
                // Preprocessing tag: !$include, !$if, !$let, etc.
                match self.parse_preprocessing_tag(tag_name, tagged_content.clone(), &meta) {
                    Ok(preprocessing_tag) => Ok(YamlAst::PreprocessingTag(preprocessing_tag, meta.clone())),
                    Err(e) => {
                        // All preprocessing tag errors should propagate (including unknown tags)
                        // This ensures typos in tag names are caught
                        Err(e)
                    }
                }
            } else if let Some(cf_tag) =
                CloudFormationTag::from_tag_name(tag_name, tagged_content.clone())
            {
                // CloudFormation tag: !Ref, !GetAtt, !Sub, etc.
                Ok(YamlAst::CloudFormationTag(cf_tag, meta.clone()))
            } else {
                // Unknown tag
                let unknown_tag = UnknownTag {
                    tag: tag_name.to_string(),
                    value: Box::new(tagged_content),
                };
                Ok(YamlAst::UnknownYamlTag(unknown_tag, meta.clone()))
            }
        } else if let Some(content) = content_node {
            // No tag, just regular block content
            self.build_ast(content, src, uri, anchor_map)
        } else {
            // Empty block node
            Ok(YamlAst::Null(meta.clone()))
        }?;

        // If there's an anchor, store the result in the anchor map
        if let Some(anchor) = anchor_node {
            let anchor_name = self.extract_anchor_name(anchor, src, &meta)?;
            anchor_map.insert(anchor_name, result.clone());
        }

        Ok(result)
    }

    fn build_flow_node(
        &self,
        node: Node,
        src: &[u8],
        uri: &Url,
        meta: SrcMeta,
        anchor_map: &mut HashMap<String, YamlAst>,
    ) -> ParseResult<YamlAst> {
        // Flow nodes can contain tags with their values
        let mut tag_node = None;
        let mut value_node = None;

        // Look for tag and value children
        for i in 0..node.named_child_count() {
            if let Some(child) = node.named_child(i) {
                match child.kind() {
                    "tag" => tag_node = Some(child),
                    "plain_scalar"
                    | "single_quote_scalar"
                    | "double_quote_scalar"
                    | "literal"
                    | "folded"
                    | "block_scalar" => value_node = Some(child),
                    _ => {
                        // Try to parse other children as potential values
                        if tag_node.is_some() && value_node.is_none() {
                            value_node = Some(child);
                        }
                    }
                }
            }
        }

        if let Some(tag) = tag_node {
            let tag_text = self.extract_utf8_text(tag, src, &meta, "tag")?;

            let tag_name = tag_text.trim();

            // Parse the value
            let tagged_content = if let Some(val_node) = value_node {
                self.build_ast(val_node, src, uri, anchor_map)?
            } else {
                YamlAst::Null(meta.clone())
            };

            // Classify the tag type based on naming convention - optimize the check
            if tag_name.as_bytes().get(0) == Some(&b'!') && tag_name.as_bytes().get(1) == Some(&b'$') {
                // Preprocessing tag: !$include, !$if, !$let, etc.
                match self.parse_preprocessing_tag(tag_name, tagged_content.clone(), &meta) {
                    Ok(preprocessing_tag) => Ok(YamlAst::PreprocessingTag(preprocessing_tag, meta)),
                    Err(e) => {
                        // All preprocessing tag errors should propagate (including unknown tags)
                        // This ensures typos in tag names are caught
                        Err(e)
                    }
                }
            } else if let Some(cf_tag) =
                CloudFormationTag::from_tag_name(tag_name, tagged_content.clone())
            {
                // CloudFormation tag: !Ref, !GetAtt, !Sub, etc.
                Ok(YamlAst::CloudFormationTag(cf_tag, meta))
            } else {
                // Unknown tag
                let unknown_tag = UnknownTag {
                    tag: tag_name.to_string(),
                    value: Box::new(tagged_content),
                };
                Ok(YamlAst::UnknownYamlTag(unknown_tag, meta))
            }
        } else {
            // No tag found, process as regular flow node
            if let Some(child) = node.named_child(0) {
                self.build_ast(child, src, uri, anchor_map)
            } else {
                Ok(YamlAst::Null(meta))
            }
        }
    }

    fn build_tagged_node(
        &self,
        node: Node,
        src: &[u8],
        uri: &Url,
        meta: SrcMeta,
        anchor_map: &mut HashMap<String, YamlAst>,
    ) -> ParseResult<YamlAst> {
        let tag_text = self.extract_utf8_text(node, src, &meta, "tag")?;

        // Extract the tag name (everything before the first space or newline)
        let tag_name = tag_text.split_whitespace().next().unwrap_or(&tag_text);

        // Find the tagged content
        let tagged_content = if let Some(content_node) = node.named_child(0) {
            self.build_ast(content_node, src, uri, anchor_map)?
        } else {
            // Some tags might not have explicit content, treat as null
            YamlAst::Null(meta.clone())
        };

        // Classify the tag type based on naming convention
        if tag_name.starts_with("!$") {
            // Preprocessing tag: !$include, !$if, !$let, etc.
            match self.parse_preprocessing_tag(tag_name, tagged_content.clone(), &meta) {
                Ok(preprocessing_tag) => Ok(YamlAst::PreprocessingTag(preprocessing_tag, meta)),
                Err(_) => {
                    // If parsing fails, fall back to unknown tag
                    let unknown_tag = UnknownTag {
                        tag: tag_name.to_string(),
                        value: Box::new(tagged_content),
                    };
                    Ok(YamlAst::UnknownYamlTag(unknown_tag, meta))
                }
            }
        } else if let Some(cf_tag) =
            CloudFormationTag::from_tag_name(tag_name, tagged_content.clone())
        {
            // CloudFormation tag: !Ref, !GetAtt, !Sub, etc.
            Ok(YamlAst::CloudFormationTag(cf_tag, meta))
        } else {
            // Unknown tag
            let unknown_tag = UnknownTag {
                tag: tag_name.to_string(),
                value: Box::new(tagged_content),
            };
            Ok(YamlAst::UnknownYamlTag(unknown_tag, meta))
        }
    }

    /// Parse preprocessing tags into proper PreprocessingTag enum variants
    /// Helper to unwrap single-element sequences (for block-style YAML compatibility)
    #[inline(always)]
    fn unwrap_single_sequence(&self, content: YamlAst) -> YamlAst {
        match content {
            YamlAst::Sequence(ref items, _) if items.len() == 1 => items[0].clone(),
            _ => content,
        }
    }

    fn parse_preprocessing_tag(
        &self,
        tag_name: &str,
        content: YamlAst,
        meta: &SrcMeta,
    ) -> ParseResult<PreprocessingTag> {
        match tag_name {
            "!$not" => {
                // Negation tag: !$not <expression> or !$not [<expression>]
                // If it's an array with exactly one element, unwrap it
                let expr = match content {
                    YamlAst::Sequence(ref items, _) if items.len() == 1 => items[0].clone(),
                    _ => content,
                };
                Ok(PreprocessingTag::Not(NotTag {
                    expression: Box::new(expr),
                }))
            }
            "!$" | "!$include" => {
                // Include tag: !$ <path> or !$include <path>
                // Can be either a string or an object with path and query fields
                match content {
                    YamlAst::PlainString(s, _) | YamlAst::TemplatedString(s, _) => {
                        // Simple string form: !$ path or !$ path?query
                        // Don't parse query syntax - keep the whole string as the path
                        Ok(PreprocessingTag::Include(IncludeTag {
                            path: s,
                            query: None,
                        }))
                    }
                    YamlAst::Mapping(pairs, _) => {
                        // Object form: !$ { path: "...", query: "..." }
                        let mut path = None;
                        let mut query = None;

                        for (key, value) in pairs {
                            match key {
                                YamlAst::PlainString(k, _) if k == "path" => {
                                    path = match value {
                                        YamlAst::PlainString(s, _)
                                        | YamlAst::TemplatedString(s, _) => Some(s),
                                        _ => {
                                            return Err(self.tag_error(
                                                "!$include",
                                                "path must be a string",
                                                Some("use string path"),
                                                &meta,
                                            ));
                                        }
                                    };
                                }
                                YamlAst::PlainString(k, _) if k == "query" => {
                                    query = match value {
                                        YamlAst::PlainString(s, _)
                                        | YamlAst::TemplatedString(s, _) => Some(s),
                                        _ => {
                                            return Err(self.tag_error(
                                                "!$include",
                                                "query must be a string",
                                                Some("use string query"),
                                                &meta,
                                            ));
                                        }
                                    };
                                }
                                _ => {}
                            }
                        }

                        match path {
                            Some(p) => Ok(PreprocessingTag::Include(IncludeTag { path: p, query })),
                            None => Err(self.missing_field_error("!$include", "path", &meta)),
                        }
                    }
                    _ => Err(self.tag_error(
                        "!$include",
                        "invalid format - must be string variable name",
                        Some("use string variable name"),
                        &meta,
                    )),
                }
            }
            "!$eq" => {
                // Equality tag: !$eq [left, right]
                match content {
                    YamlAst::Sequence(items, _) if items.len() == 2 => {
                        Ok(PreprocessingTag::Eq(EqTag {
                            left: Box::new(items[0].clone()),
                            right: Box::new(items[1].clone()),
                        }))
                    }
                    YamlAst::Sequence(_items, _) => {
                        // It's a sequence but wrong count
                        Err(self.tag_error(
                            "!$eq",
                            "must have exactly 2 elements to compare",
                            Some("use format: [value1, value2]"),
                            &meta,
                        ))
                    }
                    _ => {
                        // Not a sequence at all
                        Err(self.tag_error(
                            "!$eq",
                            "must be a sequence with exactly 2 elements",
                            Some("use format: [value1, value2]"),
                            &meta,
                        ))
                    }
                }
            }
            "!$split" => {
                // Split tag: !$split [delimiter, string]
                match content {
                    YamlAst::Sequence(items, _) if items.len() == 2 => {
                        Ok(PreprocessingTag::Split(SplitTag {
                            delimiter: Box::new(items[0].clone()),
                            string: Box::new(items[1].clone()),
                        }))
                    }
                    _ => Err(self.tag_error(
                        "!$split",
                        "must be a sequence with format [delimiter, string]",
                        Some("use format: [delimiter, string]"),
                        &meta,
                    )),
                }
            }
            "!$join" => {
                // Join tag: !$join [delimiter, array]
                match content {
                    YamlAst::Sequence(items, _) if items.len() == 2 => {
                        Ok(PreprocessingTag::Join(JoinTag {
                            delimiter: Box::new(items[0].clone()),
                            array: Box::new(items[1].clone()),
                        }))
                    }
                    _ => Err(self.tag_error(
                        "!$join",
                        "must be a sequence with format [delimiter, array]",
                        Some("use format: [delimiter, array]"),
                        &meta,
                    )),
                }
            }
            "!$merge" => {
                // Merge tag: !$merge [source1, source2, ...]
                match content {
                    YamlAst::Sequence(items, _) => {
                        Ok(PreprocessingTag::Merge(MergeTag { sources: items }))
                    }
                    _ => Err(self.tag_error(
                        "!$merge",
                        "must be a sequence of objects to merge",
                        Some("use format: [object1, object2, ...]"),
                        &meta,
                    )),
                }
            }
            "!$concat" => {
                // Concat tag: !$concat [source1, source2, ...]
                match content {
                    YamlAst::Sequence(items, _) => {
                        Ok(PreprocessingTag::Concat(ConcatTag { sources: items }))
                    }
                    _ => Err(self.tag_error(
                        "!$concat",
                        "must be a sequence of arrays to concatenate",
                        Some("use format: [array1, array2, ...]"),
                        &meta,
                    )),
                }
            }
            "!$escape" => {
                // Escape tag: !$escape <content>
                Ok(PreprocessingTag::Escape(EscapeTag {
                    content: Box::new(self.unwrap_single_sequence(content)),
                }))
            }
            "!$fromPairs" => {
                // FromPairs tag: !$fromPairs <source>
                Ok(PreprocessingTag::FromPairs(FromPairsTag {
                    source: Box::new(self.unwrap_single_sequence(content)),
                }))
            }
            "!$toYamlString" => {
                // ToYamlString tag: !$toYamlString <data>
                Ok(PreprocessingTag::ToYamlString(ToYamlStringTag {
                    data: Box::new(self.unwrap_single_sequence(content)),
                }))
            }
            "!$parseYaml" => {
                // ParseYaml tag: !$parseYaml <yaml_string>
                Ok(PreprocessingTag::ParseYaml(ParseYamlTag {
                    yaml_string: Box::new(self.unwrap_single_sequence(content)),
                }))
            }
            "!$toJsonString" => {
                // ToJsonString tag: !$toJsonString <data>
                Ok(PreprocessingTag::ToJsonString(ToJsonStringTag {
                    data: Box::new(self.unwrap_single_sequence(content)),
                }))
            }
            "!$parseJson" => {
                // ParseJson tag: !$parseJson <json_string>
                Ok(PreprocessingTag::ParseJson(ParseJsonTag {
                    json_string: Box::new(self.unwrap_single_sequence(content)),
                }))
            }
            "!$mapValues" => {
                // MapValues tag: !$mapValues {items: ..., template: ..., var?: ...}
                self.parse_map_values_tag(content, meta)
            }
            "!$map" => {
                // Map tag: !$map {items: ..., template: ..., var?: ..., filter?: ...}
                self.parse_map_tag(content, meta)
            }
            "!$concatMap" => {
                // ConcatMap tag: !$concatMap {items: ..., template: ..., var?: ..., filter?: ...}
                self.parse_concat_map_tag(content, meta)
            }
            "!$mergeMap" => {
                // MergeMap tag: !$mergeMap {items: ..., template: ..., var?: ...}
                self.parse_merge_map_tag(content, meta)
            }
            "!$mapListToHash" => {
                // MapListToHash tag: !$mapListToHash {items: ..., template: ..., var?: ..., filter?: ...}
                self.parse_map_list_to_hash_tag(content, meta)
            }
            "!$groupBy" => {
                // GroupBy tag: !$groupBy {items: ..., key: ..., var?: ..., template?: ...}
                self.parse_group_by_tag(content, meta)
            }
            "!$if" => {
                // If tag: !$if {test: ..., then: ..., else?: ...}
                self.parse_if_tag(content, meta)
            }
            "!$let" => {
                // Let tag: !$let {...bindings..., in: ...}
                self.parse_let_tag(content, meta)
            }
            _ => {
                // Unknown preprocessing tag
                Err(self.tag_error(
                    "unknown tag",
                    &format!("'{}' is not a valid iidy tag", tag_name),
                    Some("check tag spelling or see documentation for valid tags"),
                    &meta,
                ))
            }
        }
    }

    /// Helper method to extract a field from a mapping content
    #[inline]
    fn extract_field_from_mapping(&self, content: &YamlAst, field_name: &str) -> Option<YamlAst> {
        if let YamlAst::Mapping(pairs, _) = content {
            for (key, value) in pairs {
                if let YamlAst::PlainString(key_str, _) = key {
                    if key_str == field_name {
                        return Some(value.clone());
                    }
                }
            }
        }
        None
    }

    /// Extract multiple fields from a mapping in a single traversal
    fn extract_fields_from_mapping<'a>(
        &self,
        content: &YamlAst,
        field_names: &[&'a str],
    ) -> std::collections::HashMap<&'a str, YamlAst> {
        let mut result = std::collections::HashMap::with_capacity(field_names.len());
        if let YamlAst::Mapping(pairs, _) = content {
            // Early exit optimization: stop when we've found all fields
            let mut found_count = 0;
            for (key, value) in pairs {
                if let YamlAst::PlainString(key_str, _) = key {
                    for &field_name in field_names {
                        if key_str == field_name {
                            result.insert(field_name, value.clone());
                            found_count += 1;
                            // Happy path optimization: stop early if we found all fields
                            if found_count == field_names.len() {
                                return result;
                            }
                            break;
                        }
                    }
                }
            }
        }
        result
    }

    /// Comprehensive field validation for preprocessing tags
    fn validate_tag_fields(
        &self,
        content: &YamlAst,
        tag_name: &str,
        required_fields: &[&str],
        optional_fields: &[&str],
        meta: &SrcMeta,
    ) -> ParseResult<()> {
        if let YamlAst::Mapping(pairs, _) = content {
            let mut present_fields = std::collections::HashSet::with_capacity(pairs.len());

            // Collect all present fields
            for (key, _) in pairs {
                if let YamlAst::PlainString(key_str, _) = key {
                    present_fields.insert(key_str.as_str());
                }
            }

            // Check for missing required fields
            for &required_field in required_fields {
                if !present_fields.contains(required_field) {
                    // Generate simple error message matching original parser format
                    return Err(self.missing_field_error(tag_name, required_field, &meta));
                }
            }

            // Check for unexpected fields
            let mut all_valid_fields = std::collections::HashSet::with_capacity(required_fields.len() + optional_fields.len());
            for &field in required_fields {
                all_valid_fields.insert(field);
            }
            for &field in optional_fields {
                all_valid_fields.insert(field);
            }

            for present_field in &present_fields {
                if !all_valid_fields.contains(present_field) {
                    let valid_fields_str = {
                        let mut fields = Vec::with_capacity(required_fields.len() + optional_fields.len());
                        for &field in required_fields {
                            fields.push(field.to_string());
                        }
                        for &field in optional_fields {
                            fields.push(format!("{} (optional)", field));
                        }
                        fields.join(", ")
                    };

                    return Err(self.tag_error(
                        tag_name,
                        &format!(
                            "unexpected field '{}'\n\nValid fields are: {}",
                            present_field, valid_fields_str
                        ),
                        Some("check field spelling and tag documentation"),
                        &meta,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Parse MapValues tag content
    fn parse_map_values_tag(
        &self,
        content: YamlAst,
        meta: &SrcMeta,
    ) -> ParseResult<PreprocessingTag> {
        // Fast path for simple cases - extract fields directly without validation
        if let YamlAst::Mapping(ref pairs, _) = content {
            let mut items = None;
            let mut template = None;
            let mut var = None;
            
            for (key, value) in pairs {
                if let YamlAst::PlainString(key_str, _) = key {
                    match key_str.as_str() {
                        "items" => items = Some(value.clone()),
                        "template" => template = Some(value.clone()),
                        "var" => {
                            var = if let YamlAst::PlainString(s, _) = value {
                                Some(s.clone())
                            } else {
                                None
                            };
                        }
                        _ => {} // Ignore unknown fields for performance
                    }
                }
            }
            
            let items = items.ok_or_else(|| self.missing_field_error("!$mapValues", "items", &meta))?;
            let template = template.ok_or_else(|| self.missing_field_error("!$mapValues", "template", &meta))?;
            
            return Ok(PreprocessingTag::MapValues(MapValuesTag {
                items: Box::new(items),
                template: Box::new(template),
                var,
            }));
        }

        Err(self.tag_error("!$mapValues", "must be a mapping", Some("use mapping format"), meta))
    }

    /// Parse Map tag content
    fn parse_map_tag(&self, content: YamlAst, meta: &SrcMeta) -> ParseResult<PreprocessingTag> {
        // Validate fields first
        self.validate_tag_fields(
            &content,
            "!$map",
            &["items", "template"],
            &["var", "filter"],
            meta,
        )?;

        let fields =
            self.extract_fields_from_mapping(&content, &["items", "template", "var", "filter"]);

        let items = fields
            .get("items")
            .cloned()
            .ok_or_else(|| self.missing_field_error("!$map", "items", &meta))?;

        let template = fields
            .get("template")
            .cloned()
            .ok_or_else(|| self.missing_field_error("!$map", "template", &meta))?;

        let var = fields.get("var").and_then(|v| {
            if let YamlAst::PlainString(s, _) = v {
                Some(s.clone())
            } else {
                None
            }
        });

        let filter = fields.get("filter").cloned().map(Box::new);

        Ok(PreprocessingTag::Map(MapTag {
            items: Box::new(items),
            template: Box::new(template),
            var,
            filter,
        }))
    }

    /// Parse ConcatMap tag content
    fn parse_concat_map_tag(
        &self,
        content: YamlAst,
        meta: &SrcMeta,
    ) -> ParseResult<PreprocessingTag> {
        let items = self
            .extract_field_from_mapping(&content, "items")
            .ok_or_else(|| self.missing_field_error("!$concatMap", "items", &meta))?;

        let template = self
            .extract_field_from_mapping(&content, "template")
            .ok_or_else(|| self.missing_field_error("!$concatMap", "template", &meta))?;

        let var = self
            .extract_field_from_mapping(&content, "var")
            .and_then(|v| {
                if let YamlAst::PlainString(s, _) = v {
                    Some(s)
                } else {
                    None
                }
            });

        let filter = self
            .extract_field_from_mapping(&content, "filter")
            .map(Box::new);

        Ok(PreprocessingTag::ConcatMap(ConcatMapTag {
            items: Box::new(items),
            template: Box::new(template),
            var,
            filter,
        }))
    }

    /// Parse MergeMap tag content
    fn parse_merge_map_tag(
        &self,
        content: YamlAst,
        meta: &SrcMeta,
    ) -> ParseResult<PreprocessingTag> {
        let items = self
            .extract_field_from_mapping(&content, "items")
            .ok_or_else(|| self.missing_field_error("!$mergeMap", "items", &meta))?;

        let template = self
            .extract_field_from_mapping(&content, "template")
            .ok_or_else(|| self.missing_field_error("!$mergeMap", "template", &meta))?;

        let var = self
            .extract_field_from_mapping(&content, "var")
            .and_then(|v| {
                if let YamlAst::PlainString(s, _) = v {
                    Some(s)
                } else {
                    None
                }
            });

        Ok(PreprocessingTag::MergeMap(MergeMapTag {
            items: Box::new(items),
            template: Box::new(template),
            var,
        }))
    }

    /// Parse MapListToHash tag content
    fn parse_map_list_to_hash_tag(
        &self,
        content: YamlAst,
        meta: &SrcMeta,
    ) -> ParseResult<PreprocessingTag> {
        // Validate fields first
        self.validate_tag_fields(
            &content,
            "!$mapListToHash",
            &["items", "template"],
            &["var", "filter"],
            meta,
        )?;

        let items = self.extract_field_from_mapping(&content, "items").unwrap(); // Safe due to validation

        let template = self
            .extract_field_from_mapping(&content, "template")
            .unwrap(); // Safe due to validation

        let var = self
            .extract_field_from_mapping(&content, "var")
            .and_then(|v| {
                if let YamlAst::PlainString(s, _) = v {
                    Some(s)
                } else {
                    None
                }
            });

        let filter = self
            .extract_field_from_mapping(&content, "filter")
            .map(Box::new);

        Ok(PreprocessingTag::MapListToHash(MapListToHashTag {
            items: Box::new(items),
            template: Box::new(template),
            var,
            filter,
        }))
    }

    /// Parse GroupBy tag content
    fn parse_group_by_tag(
        &self,
        content: YamlAst,
        meta: &SrcMeta,
    ) -> ParseResult<PreprocessingTag> {
        // Check if content is a mapping first
        if !matches!(content, YamlAst::Mapping(_, _)) {
            return Err(self.tag_error("!$groupBy", "must be a mapping with 'items' and 'key' fields", Some("use format: {items: array, key: grouping_key, var: item_name, template: result_template}"), &meta));
        }

        let items = self
            .extract_field_from_mapping(&content, "items")
            .ok_or_else(|| self.missing_field_error("!$groupBy", "items", &meta))?;

        let key = self
            .extract_field_from_mapping(&content, "key")
            .ok_or_else(|| self.missing_field_error("!$groupBy", "key", &meta))?;

        let var = self
            .extract_field_from_mapping(&content, "var")
            .and_then(|v| {
                if let YamlAst::PlainString(s, _) = v {
                    Some(s)
                } else {
                    None
                }
            });

        let template = self
            .extract_field_from_mapping(&content, "template")
            .map(Box::new);

        Ok(PreprocessingTag::GroupBy(GroupByTag {
            items: Box::new(items),
            key: Box::new(key),
            var,
            template,
        }))
    }

    /// Parse If tag content
    fn parse_if_tag(&self, content: YamlAst, meta: &SrcMeta) -> ParseResult<PreprocessingTag> {
        // Fast path for simple cases - extract fields directly
        if let YamlAst::Mapping(ref pairs, _) = content {
            let mut test = None;
            let mut then_value = None;
            let mut else_value = None;
            
            for (key, value) in pairs {
                if let YamlAst::PlainString(key_str, _) = key {
                    match key_str.as_str() {
                        "test" => test = Some(value.clone()),
                        "then" => then_value = Some(value.clone()),
                        "else" => else_value = Some(value.clone()),
                        _ => {} // Ignore unknown fields for performance
                    }
                }
            }
            
            let test = test.ok_or_else(|| self.missing_field_error("!$if", "test", &meta))?;
            let then_value = then_value.ok_or_else(|| self.missing_field_error("!$if", "then", &meta))?;
            
            return Ok(PreprocessingTag::If(IfTag {
                test: Box::new(test),
                then_value: Box::new(then_value),
                else_value: else_value.map(Box::new),
            }));
        }

        Err(self.tag_error(
            "!$if",
            "must be a mapping with required 'test' and 'then' fields",
            Some("use format: {test: condition, then: value, else: alternative}"),
            &meta,
        ))
    }

    /// Parse Let tag content
    fn parse_let_tag(&self, content: YamlAst, meta: &SrcMeta) -> ParseResult<PreprocessingTag> {
        // Check if content is a mapping first
        if !matches!(content, YamlAst::Mapping(_, _)) {
            return Err(self.tag_error(
                "!$let",
                "must be a mapping with variable bindings and 'in' field",
                Some("use format: {var1: value1, var2: value2, in: expression}"),
                &meta,
            ));
        }

        let in_expr = self
            .extract_field_from_mapping(&content, "in")
            .ok_or_else(|| {
                self.tag_error(
                    "!$let",
                    "missing required 'in' field",
                    Some("add 'in' field containing the expression to evaluate"),
                    &meta,
                )
            })?;

        // Extract all other fields as bindings
        let mut bindings = Vec::new(); // Size unknown, will grow as needed
        if let YamlAst::Mapping(pairs, _) = content {
            for (key, value) in pairs {
                if let YamlAst::PlainString(key_str, _) = key {
                    if key_str != "in" {
                        bindings.push((key_str, value));
                    }
                }
            }
        }

        Ok(PreprocessingTag::Let(LetTag {
            bindings,
            expression: Box::new(in_expr),
        }))
    }

    /// Process escape sequences in double-quoted strings
    fn unescape_string(&self, s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars();

        while let Some(ch) = chars.next() {
            if ch == '\\' {
                match chars.next() {
                    Some('n') => result.push('\n'),
                    Some('t') => result.push('\t'),
                    Some('r') => result.push('\r'),
                    Some('\\') => result.push('\\'),
                    Some('"') => result.push('"'),
                    Some('\'') => result.push('\''),
                    Some('0') => result.push('\0'),
                    Some(escaped) => {
                        // If we don't recognize the escape sequence, keep both the backslash and the character
                        result.push('\\');
                        result.push(escaped);
                    }
                    None => {
                        // Trailing backslash
                        result.push('\\');
                    }
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    // Helper functions to generate properly formatted errors

    /// Extract UTF-8 text from a node with proper error handling
    #[inline(always)]
    fn extract_utf8_text(
        &self,
        node: Node,
        src: &[u8],
        meta: &SrcMeta,
        context: &str,
    ) -> ParseResult<String> {
        // Optimized path - avoid format! allocation in common case
        match node.utf8_text(src) {
            Ok(s) => Ok(s.to_string()),
            Err(_) => {
                // Only allocate error string when there's actually an error
                Err(self.syntax_error(&format!("Invalid UTF-8 in {}", context), meta, ""))
            }
        }
    }

    /// Extract anchor name from anchor node (removes '&' prefix)
    fn extract_anchor_name(
        &self,
        node: Node,
        src: &[u8],
        meta: &SrcMeta,
    ) -> ParseResult<String> {
        // The anchor node has the full text including children,
        // but the anchor name itself is just the first line/token
        // Extract only the bytes for this specific node (not children)
        let start = node.start_byte();
        let end = node.end_byte();

        // Get just the first part (the anchor identifier, not the content)
        // The anchor name ends at the first whitespace or newline
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

    /// Extract alias name from alias node (removes '*' prefix)
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

    /// Generate error for undefined alias reference
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

    /// Generate error for YAML 1.1 merge keys
    fn merge_key_error(&self, meta: &SrcMeta) -> ParseError {
        let file_path = self.format_file_path_only(meta);
        ParseError {
            message: format!(
                "YAML merge keys ('<<') are not supported in YAML 1.2\n\
                 in file '{}'\n\n\
                 Consider using iidy's !$merge tag instead:\n\n\
                 result: !$merge\n\
                   - *base\n\
                   - override_key: override_value",
                file_path
            ),
            location: Some(super::error::ParseLocation {
                uri: meta.input_uri.clone(),
                start: meta.start,
                end: meta.end,
            }),
            code: Some("MERGE_KEY_NOT_SUPPORTED".to_string()),
        }
    }

    /// Extract the file path from a URI for error display
    fn extract_file_path(&self, meta: &SrcMeta) -> String {
        if meta.input_uri.scheme() == "file" {
            // Try to convert URI to file path
            if let Ok(path) = meta.input_uri.to_file_path() {
                // Get the current working directory to compute relative path
                if let Ok(cwd) = std::env::current_dir() {
                    if let Ok(rel_path) = path.strip_prefix(&cwd) {
                        rel_path.to_string_lossy().to_string()
                    } else {
                        path.to_string_lossy().to_string()
                    }
                } else {
                    path.to_string_lossy().to_string()
                }
            } else {
                // Fallback for URIs that don't convert to file paths properly
                // This handles cases like file://test.yaml (without proper file path structure)
                let uri_str = meta.input_uri.as_str();
                if let Some(file_part) = uri_str.strip_prefix("file://") {
                    // Handle file://test.yaml -> test.yaml
                    if !file_part.starts_with('/') {
                        file_part.trim_end_matches('/').to_string()
                    } else {
                        // Handle file:///absolute/path cases
                        let uri_path = meta.input_uri.path();
                        if uri_path.starts_with('/')
                            && uri_path.chars().filter(|&c| c == '/').count() == 1
                        {
                            // URI like file:///test.yaml becomes /test.yaml, extract just test.yaml
                            uri_path.trim_start_matches('/').to_string()
                        } else if let Some(rel_index) = uri_path.find("example-templates/") {
                            // For paths like /some/path/example-templates/errors/file.yaml
                            uri_path[rel_index..].to_string()
                        } else {
                            // Remove leading slash for display
                            uri_path.trim_start_matches('/').to_string()
                        }
                    }
                } else {
                    // Fallback to just the path part
                    meta.input_uri.path().to_string()
                }
            }
        } else {
            // For non-file URIs, use the URI as-is
            meta.input_uri.to_string()
        }
    }

    /// Convert URI to file path for error display with line:col
    #[inline]
    fn format_file_location(&self, meta: &SrcMeta) -> String {
        let path_str = self.extract_file_path(meta);
        format!(
            "{}:{}:{}",
            path_str,
            meta.start.line + 1,
            meta.start.character + 1
        )
    }

    /// Convert URI to just file path for error display (without line:col)
    #[inline]
    fn format_file_path_only(&self, meta: &SrcMeta) -> String {
        self.extract_file_path(meta)
    }

    /// Generate a missing required field error
    fn missing_field_error(&self, tag_name: &str, field_name: &str, meta: &SrcMeta) -> ParseError {
        let file_path = self.format_file_location(meta);
        let anyhow_error = missing_required_field_error(
            tag_name,
            field_name,
            &file_path,
            "",     // yaml_path
            vec![], // required_fields
        );
        ParseError {
            message: anyhow_error.to_string(),
            location: Some(super::error::ParseLocation {
                uri: meta.input_uri.clone(),
                start: meta.start,
                end: meta.end,
            }),
            code: Some(error_codes::MISSING_FIELD.to_string()),
        }
    }

    /// Generate a tag parsing error
    fn tag_error(
        &self,
        tag_name: &str,
        message: &str,
        suggestion: Option<&str>,
        meta: &SrcMeta,
    ) -> ParseError {
        let file_path = self.format_file_location(meta);
        let anyhow_error = tag_parsing_error(tag_name, message, &file_path, suggestion);
        
        // Determine the appropriate error code based on the message content
        let error_code = if message.contains("missing required") {
            error_codes::MISSING_FIELD
        } else if message.contains("is not a valid iidy tag") {
            error_codes::UNKNOWN_TAG
        } else if message.contains("invalid format") || message.contains("must be") {
            error_codes::INVALID_FORMAT
        } else {
            error_codes::INVALID_TYPE // fallback
        };
        
        ParseError {
            message: anyhow_error.to_string(),
            location: Some(super::error::ParseLocation {
                uri: meta.input_uri.clone(),
                start: meta.start,
                end: meta.end,
            }),
            code: Some(error_code.to_string()),
        }
    }

    /// Generate a YAML syntax error
    fn syntax_error(&self, message: &str, meta: &SrcMeta, source: &str) -> ParseError {
        // For syntax errors, extract just the file path without line:col since yaml_syntax_error adds its own
        let file_path = self.format_file_path_only(meta);

        // Try parsing with serde_yaml to get a proper error for the wrapper function
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
            // Fallback to simple error format
            let full_location = self.format_file_location(meta);
            ParseError {
                message: format!("Syntax error: {} @ {}", message, full_location),
                location: Some(super::error::ParseLocation {
                    uri: meta.input_uri.clone(),
                    start: meta.start,
                    end: meta.end,
                }),
                code: None,
            }
        }
    }
}

impl Default for YamlParser {
    fn default() -> Self {
        Self::new().expect("Failed to create default YAML parser")
    }
}

#[inline(always)]
fn point_to_position(p: Point) -> Position {
    Position::new(p.row as u32, p.column as u32)
}

#[inline(always)]
fn node_meta(node: &Node, uri: &Url) -> SrcMeta {
    SrcMeta {
        input_uri: uri.clone(),
        start: point_to_position(node.start_position()),
        end: point_to_position(node.end_position()),
    }
}

#[allow(dead_code)]
pub fn parse_yaml_ast(source: &str, uri: Url) -> ParseResult<YamlAst> {
    let mut parser = YamlParser::new()?;
    parser.parse(source, uri)
}
